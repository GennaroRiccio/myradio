//! Rendering dell'interfaccia con ratatui.

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Paragraph, Row, Sparkline, Table, TableState, Wrap,
};

use crate::app::{App, Focus};
use crate::audio::PlaybackState;
use crate::radio::Station;

/// Altezza della sezione del visualizzatore audio.
const VISUALIZER_HEIGHT: u16 = 10;

/// Altezza massima dell'artwork nel pannello della stazione.
const MAX_ART_HEIGHT: usize = 8;

/// Larghezza massima dell'artwork nel pannello della stazione.
const MAX_ART_WIDTH: usize = 16;

/// Banner di avvio: i 7 glifi (M Y R A D I O), 5 righe × 5 colonne l'uno.
const SPLASH_LETTERS: [[&str; 5]; 7] = [
    ["█    █", "██  ██", "█ ██ █", "█  █ █", "█    █"],
    ["█   █", " █ █ ", "  █  ", "  █  ", "  █  "],
    ["████ ", "█  █ ", "███  ", "█ █  ", "█  █ "],
    [" ██  ", "█  █ ", "████ ", "█  █ ", "█  █ "],
    ["███  ", "█  █ ", "█  █ ", "█  █ ", "███  "],
    [" ██  ", " █   ", " █   ", " █   ", " ██  "],
    [" ██  ", "█  █ ", "█  █ ", "█  █ ", " ██  "],
];

/// Larghezza totale del banner: 7 lettere × 5 + 6 spazi di separazione × 2.
const SPLASH_WIDTH: usize = 7 * 5 + 6 * 2;

/// Intervallo tra la comparsa di una lettera del banner.
const SPLASH_REVEAL_STEP: Duration = Duration::from_millis(90);

/// Durata del ciclo di colori arcobaleno sul banner.
const SPLASH_COLOR_CYCLE: Duration = Duration::from_secs(3);

/// Larghezza della banale sintonizzate FM (colonne della scala).
const TUNER_WIDTH: usize = 44;

/// Periodo dello sweep automatico della sonda della sintonia.
const TUNER_SWEEP: Duration = Duration::from_millis(2500);

/// Stazioni fittizie (frazione della scala, etichetta MHz) dove la sonda si ferma.
const TUNER_STATIONS: &[(f32, &str)] = &[(0.30, "96.3"), (0.55, "106.5"), (0.80, "102.7")];

/// Disegna l'intera interfaccia nel frame corrente.
pub fn render(frame: &mut Frame<'_>, app: &mut App) {
    if app.splash {
        render_splash(frame, frame.area(), app);
        return;
    }

    let area = frame.area();
    let viz_height = if app.visualizer && app.playback.is_active() {
        VISUALIZER_HEIGHT
    } else {
        0
    };

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(6),
        Constraint::Min(8),
        Constraint::Length(viz_height),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(area);

    render_header(frame, chunks[0], app);
    render_search(frame, chunks[1], app);
    render_body(frame, chunks[2], app);
    if viz_height > 0 {
        render_visualizer(frame, chunks[3], app);
    }
    render_status(frame, chunks[4], app);
    render_help(frame, chunks[5]);
}

/// Banner di avvio animato: le lettere compaiono una a una, poi un'onda di
/// colori ruota nel tempo. Si chiude al primo tasto o dopo il timeout.
fn render_splash(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" myradio ");
    let inner = block.inner(area);

    let width = usize::from(inner.width);
    let elapsed = app.splash_started.elapsed();

    // banner + versione + riga vuota + sonda + riga vuota + suggerimento
    let total = 5 + 1 + 3 + 1 + 1;
    let top = usize::from(inner.height).saturating_sub(total) / 2;
    let base_pad = width.saturating_sub(SPLASH_WIDTH) / 2;

    let mut lines = vec![Line::default(); top];
    let cycle_ms = SPLASH_COLOR_CYCLE.as_millis();
    let shift = ((elapsed.as_millis() % cycle_ms) * 360 / cycle_ms) as u16 % 360;

    for row in 0..5 {
        let mut spans = vec![Span::raw(" ".repeat(base_pad))];
        for (index, glyph) in SPLASH_LETTERS.iter().enumerate() {
            if index > 0 {
                spans.push(Span::raw("  "));
            }
            let revealed = elapsed >= SPLASH_REVEAL_STEP * index as u32;
            if revealed {
                let hue = (index as u16 * 51 + shift) % 360;
                let color = hsv_to_rgb(hue, 0.85, 0.95);
                spans.push(Span::styled(
                    glyph[row],
                    Style::new().fg(color).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::raw("     "));
            }
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::default());
    lines.push(centered(
        &format!("v{}", app_version()),
        width,
        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    ));
    lines.extend(render_tuner(width, elapsed));
    lines.push(Line::default());
    lines.push(centered(
        "Premi un tasto per continuare",
        width,
        Style::new().dim(),
    ));

    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

/// Converte un colore HSV (0..360, 0..1, 0..1) in RGB ratatui.
#[must_use]
fn hsv_to_rgb(hue: u16, saturation: f32, value: f32) -> Color {
    let chroma = value * saturation;
    let muted = chroma * (1.0 - (((hue as f32 / 60.0) % 2.0) - 1.0).abs());
    let offset = value - chroma;
    let (red, green, blue) = match hue / 60 {
        0 => (chroma, muted, 0.0),
        1 => (muted, chroma, 0.0),
        2 => (0.0, chroma, muted),
        3 => (0.0, muted, chroma),
        4 => (muted, 0.0, chroma),
        _ => (chroma, 0.0, muted),
    };
    let to_byte = |component: f32| ((component + offset) * 255.0).round() as u8;
    Color::Rgb(to_byte(red), to_byte(green), to_byte(blue))
}

/// Scala FM animata con sonda che scorre e si ferma su stazioni.
fn render_tuner(width: usize, elapsed: Duration) -> Vec<Line<'static>> {
    let band = width.min(TUNER_WIDTH);
    let needle = tuner_needle(elapsed, band);
    let (locked_label, needle_color) = tuner_lock(elapsed);

    // scala con etichette.
    let mut scale = vec![' '; band];
    put_label(&mut scale, "88.0", 0, band);
    put_label(&mut scale, "107.9 MHz", band - 1, band);
    for (frac, label) in TUNER_STATIONS {
        let col = ((frac * (band as f32 - 1.0)).round() as usize).min(band - 1);
        put_label(&mut scale, label, col, band);
    }
    let scale_text: String = scale.iter().collect();
    let scale_line = centered_line(&scale_text, width, Style::new().dim());

    // binario con sonda.
    let mut rail = vec!['─'; band];
    for (frac, _label) in TUNER_STATIONS {
        let col = ((frac * (band as f32 - 1.0)).round() as usize).min(band - 1);
        rail[col] = '|';
    }
    rail[needle] = '▲';
    let mut spans = Vec::with_capacity(band);
    let pad = width.saturating_sub(band) / 2;
    spans.push(Span::raw(" ".repeat(pad)));
    for (col, ch) in rail.iter().enumerate() {
        let style = if col == needle {
            Style::new().fg(needle_color)
        } else {
            Style::new().dim()
        };
        spans.push(Span::styled(ch.to_string(), style));
    }
    let rail_line = Line::from(spans);

    let status = if let Some(label) = locked_label {
        format!("▶ {label} MHz")
    } else {
        "cerco stazione…".to_string()
    };
    let status_style = if locked_label.is_some() {
        Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::new().dim()
    };
    let status_line = centered_line(&status, width, status_style);

    vec![scale_line, rail_line, status_line]
}

/// Posizione della sonda in colonne (sweep unidirezionale che si resetta).
fn tuner_needle(elapsed: Duration, band_width: usize) -> usize {
    let frac =
        (elapsed.as_millis() % TUNER_SWEEP.as_millis()) as f32 / TUNER_SWEEP.as_millis() as f32;
    let col = (frac * (band_width as f32 - 1.0)).round() as usize;
    col.clamp(0, band_width.saturating_sub(1))
}

/// Eventualo "blocco" della sonda su una stazione (finestra attorno alla posizione).
fn tuner_lock(elapsed: Duration) -> (Option<&'static str>, Color) {
    let frac =
        (elapsed.as_millis() % TUNER_SWEEP.as_millis()) as f32 / TUNER_SWEEP.as_millis() as f32;
    let within = |station: f32| (frac - station).abs() < 0.05;
    for (frac_s, label) in TUNER_STATIONS {
        if within(*frac_s) {
            return (Some(label), Color::Green);
        }
    }
    (None, Color::Yellow)
}

/// Inserisce un testo centrato sulla colonna `at` (con accorgimenti di bordo).
fn put_label(buf: &mut [char], text: &str, at: usize, band: usize) {
    let center = at.min(band.saturating_sub(1));
    let start = center.saturating_sub(text.chars().count() / 2);
    for (i, ch) in text.chars().enumerate() {
        let idx = start + i;
        if idx < buf.len() {
            buf[idx] = ch;
        }
    }
}

/// Righe di testo centrata (singola stringa + stile).
fn centered_line(text: &str, width: usize, style: Style) -> Line<'static> {
    let text_len = text.chars().count();
    let pad = width.saturating_sub(text_len) / 2;
    Line::from(vec![
        Span::raw(" ".repeat(pad)),
        Span::styled(text.to_string(), style),
    ])
}

/// Crea una riga di testo centrata orizzontalmente su `width`.
fn centered(text: &str, width: usize, style: Style) -> Line<'static> {
    let text_len = text.chars().count();
    let pad = width.saturating_sub(text_len) / 2;
    Line::from(vec![
        Span::raw(" ".repeat(pad)),
        Span::styled(text.to_string(), style),
    ])
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let title = match &app.now_playing {
        Some(station) => format!(
            " myradio v{} — {} By Gennaro Riccio",
            app_version(),
            station.name
        ),
        None => format!(" myradio v{} By Gennaro Riccio ", app_version()),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .title_style(Style::new().add_modifier(Modifier::BOLD).fg(Color::Cyan));
    let line = Line::from(Span::styled(
        "Client per radio internet — cerca, seleziona e ascolta in streaming",
        Style::new().dim(),
    ));
    frame.render_widget(Paragraph::new(line).block(block), area);
}

/// Versione dell'applicazione (da `Cargo.toml`), es. `1.0.0`.
#[must_use]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Etichetta (prefisso, testo di default) per il campo in base al focus.
fn focus_label(focused: bool, label: &'static str, fallback: &'static str) -> (String, bool) {
    let text = if focused {
        format!("[{label}] ")
    } else {
        format!("{fallback} ")
    };
    (text, focused)
}

fn render_search(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Ricerca ");

    let inner = block.inner(area);
    app.areas.query = Rect::new(inner.x, inner.y, inner.width, 1);
    app.areas.tag = Rect::new(inner.x, inner.y.saturating_add(1), inner.width, 1);

    let (query_label, query_focused) = focus_label(app.focus == Focus::Query, "Nome", "Nome ");
    let (tag_label, tag_focused) = focus_label(app.focus == Focus::Tag, "Tag", "Tag  ");

    let cursor = "▊";
    let query_value = if query_focused {
        format!("{}{}", app.query, cursor)
    } else {
        app.query.clone()
    };
    let tag_value = if tag_focused {
        format!("{}{}", app.tag, cursor)
    } else {
        app.tag.clone()
    };

    let query_style = if query_focused {
        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    let tag_style = if tag_focused {
        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };

    let text = Text::from(vec![
        Line::from(vec![
            Span::styled(query_label, Style::new().fg(Color::Cyan)),
            Span::styled(query_value, query_style),
        ]),
        Line::from(vec![
            Span::styled(tag_label, Style::new().fg(Color::Cyan)),
            Span::styled(tag_value, tag_style),
        ]),
        Line::from(Span::styled(
            "Invio: cerca · Tab: campo successivo · Esc: risultati · /: focus nome · t: focus tag",
            Style::new().dim(),
        )),
    ]);

    frame.render_widget(Paragraph::new(text).block(block), area);
}

fn render_body(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let chunks =
        Layout::horizontal([Constraint::Percentage(68), Constraint::Percentage(32)]).split(area);

    render_results(frame, chunks[0], app);
    render_info(frame, chunks[1], app);
}

fn render_results(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let title = if app.loading {
        " Risultati — ricerca in corso… ".to_string()
    } else if app.showing_favorites {
        format!(" Preferiti ({}) ", app.stations.len())
    } else {
        format!(" Risultati ({}) ", app.stations.len())
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title);

    app.areas.results = block.inner(area);

    if app.stations.is_empty() {
        let hint = if app.loading {
            "Attendere…"
        } else if app.showing_favorites {
            "Nessun preferito. Premi f sulla stazione che vuoi salvare."
        } else {
            "Nessuna stazione. Digita un nome nella ricerca e premi Invio."
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, Style::new().dim()))).block(block),
            area,
        );
        return;
    }

    let header = Row::new([
        Cell::from(Span::styled(
            "Fav",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "Nome",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "Paese",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "Codec",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "Bitrate",
            Style::new().add_modifier(Modifier::BOLD),
        )),
    ]);

    let rows: Vec<Row> = app
        .stations
        .iter()
        .map(|station| {
            let fav = if app.is_favorite(station) { "★" } else { "" };
            Row::new(vec![
                Cell::from(fav),
                Cell::from(station.name.as_str()),
                Cell::from(station.country.as_str()),
                Cell::from(station.codec.as_str()),
                Cell::from(station.bitrate_label()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(3),
        Constraint::Min(18),
        Constraint::Length(14),
        Constraint::Length(7),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header.bottom_margin(0))
        .block(block)
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");

    let mut state = TableState::new().with_selected(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut state);
    app.results_offset = state.offset();
}

fn render_info(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Stazione ");

    let text = match app.selected_station() {
        Some(station) => {
            let inner = block.inner(area);
            let mut lines = Vec::new();
            if let Some(image) = app.artworks.get(station) {
                let inner_w = usize::from(inner.width);
                let (art_width, art_height) =
                    art_size_for(image, inner_w, usize::from(inner.height));
                let art = crate::artwork::art_lines(image, art_width, art_height);
                let pad = inner_w.saturating_sub(art_width) / 2;
                for line in art {
                    let mut spans = vec![Span::raw(" ".repeat(pad))];
                    spans.extend(line.spans);
                    lines.push(Line::from(spans));
                }
            }
            if app.is_favorite(station) {
                lines.push(Line::from(Span::styled(
                    "★ Preferito",
                    Style::new().fg(Color::Yellow),
                )));
            }
            lines.extend(detail_lines(station));
            Text::from(lines)
        }
        None => Text::from(Line::from(Span::styled(
            "Nessuna stazione selezionata",
            Style::new().dim(),
        ))),
    };

    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

/// Righe di dettaglio della stazione mostrate sotto l'eventuale artwork.
fn detail_lines(station: &Station) -> Vec<Line<'static>> {
    vec![
        Line::from(format!("Nome:      {}", station.name)),
        Line::from(format!("Paese:     {}", station.country)),
        Line::from(format!("Stato:     {}", station.state)),
        Line::from(format!("Lingua:    {}", station.language)),
        Line::from(format!("Codec:     {}", station.codec)),
        Line::from(format!("Bitrate:   {} kbps", station.bitrate)),
        Line::from(format!("Tags:      {}", station.tags.join(", "))),
        Line::from(format!("Voti:      {}", station.votes)),
        Line::from(format!("Homepage:  {}", station.homepage)),
        Line::from(format!("URL:       {}", station.url_resolved)),
    ]
}

/// Dimensioni della miniatura dell'artwork: larghezza limitata ([`MAX_ART_WIDTH`])
/// e altezza proporzionale al rapporto d'aspetto dell'immagine, comunque entro
/// i limiti di spazio disponibili.
fn art_size_for(
    image: &image::RgbaImage,
    inner_width: usize,
    inner_height: usize,
) -> (usize, usize) {
    let art_width = inner_width.min(MAX_ART_WIDTH);
    let (iw, ih) = image.dimensions();
    let pixels_height = if iw == 0 {
        0.0
    } else {
        art_width as f32 * (ih as f32 / iw as f32)
    };
    let rows = (pixels_height / 2.0).round() as usize;
    let rows = rows.min(MAX_ART_HEIGHT).min(art_height_for(inner_height));
    (art_width, rows)
}

/// Altezza da dedicare all'artwork, lasciando sempre spazio ai dettagli.
fn art_height_for(inner_height: usize) -> usize {
    let height = inner_height.saturating_sub(12).min(MAX_ART_HEIGHT);
    if height >= 4 { height } else { 0 }
}

fn render_visualizer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(area);

    let (current, history) = app.levels.snapshot();
    render_meter(frame, chunks[0], current);
    render_history(frame, chunks[1], &history);
}

fn render_meter(frame: &mut Frame<'_>, area: Rect, pcent: f64) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Livello segnale ");

    let label = format!("{pcent:3.0}% ");
    let cols = area.width.checked_sub(8).unwrap_or(1);
    let bar_width = usize::from(cols);
    let filled = ((pcent / 100.0) * bar_width as f64).round() as usize;
    let filled = filled.min(bar_width);

    let color = if pcent < 33.0 {
        Color::Green
    } else if pcent < 66.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_width - filled));
    let line = Line::from(vec![Span::raw(label), Span::styled(bar, color)]);

    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn render_history(frame: &mut Frame<'_>, area: Rect, history: &[u64]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Storico livello ");

    let sparkline = Sparkline::default()
        .data(history)
        .max(100)
        .block(block)
        .style(Style::new().fg(Color::Cyan).bg(Color::Black));

    frame.render_widget(sparkline, area);
}

fn render_status(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let (label, color) = match app.playback {
        PlaybackState::Playing => (" ▶ IN RIPRODUZIONE ", Color::Green),
        PlaybackState::Paused => (" ⏸ PAUSA ", Color::Yellow),
        PlaybackState::Connecting => (" ⏳ CONNESSIONE… ", Color::Cyan),
        PlaybackState::Error => (" ✖ ERRORE ", Color::Red),
        PlaybackState::Stopped => (" ■ FERMO ", Color::Gray),
    };

    let mut spans = vec![Span::styled(
        label,
        Style::new().fg(color).add_modifier(Modifier::BOLD),
    )];
    spans.push(Span::raw(format!(" Volume: {:>3.0}% ", app.volume * 100.0)));

    if let Some(station) = &app.now_playing {
        spans.push(Span::styled(
            format!("→ {}", station.name),
            Style::new().fg(Color::Cyan),
        ));
    }

    if let Some(message) = &app.status {
        spans.push(Span::styled(
            format!(" [{message}]"),
            Style::new().fg(Color::Red).italic(),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    let text = Line::from(Span::styled(
        " / i: ricerca · t: tag · f: preferito · F: lista preferiti · Invio: play · p: pausa/resume · s: stop · +/-: volume · v: visualizzatore · Tab: focus · q: esci · mouse: click = play, scroll",
        Style::new().dim(),
    ));
    frame.render_widget(Paragraph::new(text), area);
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use crate::radio::Station;
    use image::{Rgba, RgbaImage};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::app::App;
    use crate::audio::EngineHandle;

    fn test_app() -> App {
        let (tx, rx) = mpsc::channel();
        let mut app = App::new(tx, rx, EngineHandle::broken());
        app.dismiss_splash();
        app
    }

    fn render_once(app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| render(frame, app))
            .expect("render non deve fallire");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>()
    }

    #[test]
    fn renders_without_breaking() {
        let output = render_once(&mut test_app());
        assert!(output.contains("myradio"));
    }

    #[test]
    fn renders_splash_banner() {
        let (tx, rx) = mpsc::channel();
        let mut app = App::new(tx, rx, EngineHandle::broken());
        let output = render_once(&mut app);
        assert!(output.contains(&format!("v{}", super::app_version())));
        assert!(output.contains("Premi un tasto"));
        assert!(output.contains('█'), "l'ASCII art deve essere disegnata");
    }

    #[test]
    fn splash_animates_over_time() {
        let render_at = |elapsed_ms: u64| {
            let (tx, rx) = mpsc::channel();
            let mut app = App::new(tx, rx, EngineHandle::broken());
            let now = std::time::Instant::now();
            app.splash_started = now
                .checked_sub(std::time::Duration::from_millis(elapsed_ms))
                .unwrap_or(now);
            render_once(&mut app)
        };
        let revealing = render_at(0);
        let complete = render_at(700);
        assert_eq!(revealing.contains('█'), complete.contains('█'));
        assert_ne!(revealing.len(), complete.len(), "il banner deve animarsi");
    }

    #[test]
    fn renders_playing_state() {
        let mut app = test_app();
        app.playback = crate::audio::PlaybackState::Playing;
        let output = render_once(&mut app);
        assert!(output.contains("IN RIPRODUZIONE"));
    }

    #[test]
    fn renders_station_with_artwork() {
        let mut app = test_app();
        let station = Station {
            id: "s-art".to_string(),
            name: "Art Station".to_string(),
            url_resolved: "http://example/art.mp3".to_string(),
            url: "http://example/art.mp3".to_string(),
            favicon: "http://example/art.png".to_string(),
            homepage: String::new(),
            country: "IT".to_string(),
            state: String::new(),
            language: String::new(),
            codec: "MP3".to_string(),
            bitrate: 96,
            tags: Vec::new(),
            votes: 1,
            hls: false,
        };
        app.stations = vec![station.clone()];
        app.selected = 0;

        let mut img = RgbaImage::new(4, 4);
        for px in img.pixels_mut() {
            *px = Rgba([0, 0, 255, 255]);
        }
        app.artworks.store(station.id, Some(img));

        let output = render_once(&mut app);
        assert!(output.contains("Art Station"));
    }
}
