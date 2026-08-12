//! Immagini delle stazioni: download, cache e rendering a mezzi blocchi.
//!
//! Le stazioni di Radio Browser espongono un URL `favicon`. Il modulo ne cura il
//! download in thread di lavoro (senza bloccare la TUI), la cache in memoria e
//! su disco, e il rendering dell'immagine come celle a mezzo blocco colorate,
//! che funziona su qualsiasi terminale senza protocolli speciali.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use image::RgbaImage;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::app::Msg;
use crate::radio::Station;

/// Massimo numero di download di artwork in volo contemporaneamente.
const MAX_IN_FLIGHT: usize = 8;

/// Dimensione massima accettata per un singolo artwork (byte).
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024;

/// Numero massimo di immagini conservate in memoria.
const MAX_CACHED: usize = 128;

/// Timeout per la richiesta HTTP di un artwork.
const FETCH_TIMEOUT: Duration = Duration::from_secs(8);

/// Cartella usata come cache su disco.
pub const CACHE_DIR: &str = "cache";

/// Colore di sfondo su cui comporre i pixel trasparenti.
const BACKDROP: [u8; 3] = [0, 0, 0];

/// Conserva gli artwork scaricati e tiene traccia dei download in corso.
#[derive(Default)]
pub struct ArtworkStore {
    cached: HashMap<String, RgbaImage>,
    pending: HashSet<String>,
    failed: HashSet<String>,
}

impl ArtworkStore {
    /// Returns the station image, if already available.
    #[must_use]
    pub fn get(&self, station: &Station) -> Option<&RgbaImage> {
        self.cached.get(&station.id)
    }

    /// Start missing downloads, at most [`MAX_IN_FLIGHT`] at a time.
    ///
    /// Should be called every tick of the main loop: downloads that exceed the
    /// limit remain pending and are started as soon as a slot is available.
    pub fn request_missing(&mut self, stations: &[Station], tx: &Sender<Msg>) {
        for station in stations {
            if self.pending.len() >= MAX_IN_FLIGHT {
                break;
            }
            if station.favicon.trim().is_empty()
                || self.pending.contains(&station.id)
                || self.cached.contains_key(&station.id)
                || self.failed.contains(&station.id)
            {
                continue;
            }
            self.pending.insert(station.id.clone());

            let id = station.id.clone();
            let url = station.favicon.clone();
            let tx = tx.clone();
            thread::spawn(move || {
                let image = fetch_artwork(&url, &id);
                let _ = tx.send(Msg::Artwork { id, image });
            });
        }
    }

    /// Registra il risultato di un download.
    pub fn store(&mut self, id: String, image: Option<RgbaImage>) {
        self.pending.remove(&id);
        let Some(image) = image else {
            self.failed.insert(id);
            return;
        };
        self.failed.remove(&id);
        if !self.cached.contains_key(&id) && self.cached.len() >= MAX_CACHED {
            if let Some(old) = self.cached.keys().next().cloned() {
                self.cached.remove(&old);
            }
        }
        self.cached.insert(id, image);
    }
}

/// Scarica e decodifica l'artwork di una stazione, usando la cache su disco.
fn fetch_artwork(url: &str, id: &str) -> Option<RgbaImage> {
    if let Some(image) = read_cache(id) {
        return Some(image);
    }

    let response = artwork_client()?.get(url).send().ok()?;

    if !response.status().is_success() {
        return None;
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_DOWNLOAD_BYTES)
    {
        return None;
    }

    let body = response.bytes().ok()?;
    if body.len() > MAX_DOWNLOAD_BYTES as usize {
        return None;
    }

    let image = decode(&body)?;
    let _ = write_cache(id, &body);
    Some(image)
}

/// Client HTTP condiviso per il download artwork (evita ricostruzioni ripetute).
fn artwork_client() -> Option<&'static reqwest::blocking::Client> {
    static CLIENT: OnceLock<Option<reqwest::blocking::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(FETCH_TIMEOUT)
                .user_agent(concat!("myradio/", env!("CARGO_PKG_VERSION")))
                .build()
                .ok()
        })
        .as_ref()
}

/// Decodifica i byte dell'immagine in RGBA8.
fn decode(bytes: &[u8]) -> Option<RgbaImage> {
    let image = image::load_from_memory(bytes).ok()?;
    Some(image.to_rgba8())
}

/// Legge l'artwork dalla cache su disco, se presente e decodificabile.
fn read_cache(id: &str) -> Option<RgbaImage> {
    let path = Path::new(CACHE_DIR).join(format!("{id}.png"));
    let bytes = fs::read(path).ok()?;
    decode(&bytes)
}

/// Scrive l'artwork nella cache su disco.
fn write_cache(id: &str, bytes: &[u8]) -> io::Result<()> {
    let dir = Path::new(CACHE_DIR);
    fs::create_dir_all(dir)?;
    fs::write(dir.join(format!("{id}.png")), bytes)
}

/// Compone un pixel RGBA sul colore di sfondo fisso.
fn composite(rgba: [u8; 4], backdrop: [u8; 3]) -> [u8; 3] {
    let alpha = f32::from(rgba[3]) / 255.0;
    let mix = |fg: u8, bg: u8| -> u8 {
        (f32::from(fg) * alpha + f32::from(bg) * (1.0 - alpha)).round() as u8
    };
    [
        mix(rgba[0], backdrop[0]),
        mix(rgba[1], backdrop[1]),
        mix(rgba[2], backdrop[2]),
    ]
}

/// Rendering di un'immagine come righe di celle a mezzo blocco.
///
/// Ogni cella del terminale copre due righe di pixel: `▀`/`▄`/`█` con colori
/// primo-piano/sfondo consentono di ricostruire l'immagine 1:1.
#[must_use]
pub fn art_lines(image: &RgbaImage, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(height);
    if width == 0 || height == 0 {
        return lines;
    }

    let resized = image::imageops::resize(
        image,
        width as u32,
        (height * 2) as u32,
        image::imageops::FilterType::Triangle,
    );

    for y in 0..height {
        let mut spans = Vec::with_capacity(width);
        for x in 0..width {
            let top = resized.get_pixel(x as u32, (2 * y) as u32).0;
            let bottom = resized.get_pixel(x as u32, (2 * y + 1) as u32).0;
            let top = composite(top, BACKDROP);
            let bottom = composite(bottom, BACKDROP);
            let (glyph, fg, bg) = block_cell(top, bottom);
            spans.push(Span::styled(
                glyph,
                Style::new()
                    .fg(Color::Rgb(fg[0], fg[1], fg[2]))
                    .bg(Color::Rgb(bg[0], bg[1], bg[2])),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// Sceglie il carattere e i colori per una coppia di pixel.
fn block_cell(top: [u8; 3], bottom: [u8; 3]) -> (&'static str, [u8; 3], [u8; 3]) {
    if top == bottom {
        ("█", top, top)
    } else if lightness(top) >= lightness(bottom) {
        // `▀`: metà alta col primo piano (pixel alto), metà bassa col sfondo.
        ("▀", top, bottom)
    } else {
        // `▄`: metà bassa col primo piano (pixel basso), metà alta col sfondo.
        ("▄", bottom, top)
    }
}

/// Perceived brightness of an RGB color.
fn lightness([r, g, b]: [u8; 3]) -> f32 {
    0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};

    use super::art_lines;

    #[test]
    fn produces_expected_grid() {
        let mut img = RgbaImage::new(4, 4);
        for px in img.pixels_mut() {
            *px = Rgba([255, 0, 0, 255]);
        }

        let lines = art_lines(&img, 4, 2);
        assert_eq!(lines.len(), 2);
        for line in &lines {
            assert_eq!(line.width(), 4);
            for span in &line.spans {
                assert!(!span.content.is_empty());
            }
        }
    }

    #[test]
    fn handles_zero_dimensions() {
        let img = RgbaImage::new(1, 1);
        assert!(art_lines(&img, 0, 5).is_empty());
        assert!(art_lines(&img, 5, 0).is_empty());
    }
}
