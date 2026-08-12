//! Audio playback engine running on a dedicated thread.
//!
//! The engine thread receives commands from the TUI via a channel and serializes
//! all operations on [`Player`] (rodio). Stream decoding is fed by a network thread
//! that reads chunks from the HTTP response: the intermediate reader is interruptible,
//! so stop/station change responds in a fraction of a second even on slow or blocked networks.
//!
//! Audio level sampling happens by wrapping the decoded source in [`LevelSource`],
//! which updates a [`SharedLevels`] shared with the TUI.

use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use crossbeam_channel as channel;
use rodio::source::Source;
use rodio::{ChannelCount, Decoder, Player, Sample, SampleRate};

use crate::app::Msg;
use crate::error::AppError;
use crate::levels::SharedLevels;
use crate::radio::Station;

/// Maximum interval at which the interruptible reader responds to a stop.
const READ_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Maximum size of the reader's rewindable cache (for seek).
///
/// The rodio decoder requires a `Read + Seek` reader. For a live stream,
/// seeking is almost always to the beginning of the stream (probing phase),
/// so it's enough to keep a limited window of already-read bytes.
const REWIND_CACHE_BYTES: usize = 16 * 1024 * 1024;

/// Default volume applied to playback.
pub const DEFAULT_VOLUME: f32 = 0.8;

/// Commands accepted by the playback thread.
pub enum EngineCmd {
    /// Start playing a station, stopping the current one if any.
    Play {
        /// Station to play.
        station: Box<Station>,
        /// Shared area to register levels.
        levels: SharedLevels,
    },
    /// Stop current playback.
    Stop,
    /// Toggle pause/resume.
    TogglePause,
    /// Set volume (0.0..1.0).
    SetVolume(f32),
    /// A stream was connected and decoded by the worker thread.
    StreamReady {
        /// Generation of the connection this result refers to.
        generation: u64,
        /// Decoder of the already connected and validated stream.
        decoder: rodio::Decoder<InterruptibleReader>,
        /// Shared levels area with the TUI.
        levels: SharedLevels,
    },
    /// Stream connection failed.
    StreamFailed {
        /// Generation of the connection this result refers to.
        generation: u64,
        /// Error message to show to the user.
        message: String,
    },
}

/// Stato di riproduzione corrente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    /// No station playing.
    #[default]
    Stopped,
    /// Stream connection/startup in progress.
    Connecting,
    /// Stream is playing.
    Playing,
    /// Playback is paused.
    Paused,
    /// Playback error occurred.
    Error,
}

impl PlaybackState {
    /// Indicates whether the state corresponds to an "active" playback
    /// (i.e., one with an associated station) for which to show the visualizer.
    #[must_use]
    pub fn is_active(self) -> bool {
        matches!(self, Self::Connecting | Self::Playing | Self::Paused)
    }
}

/// Handle sendable to the playback thread.
#[derive(Debug, Clone)]
pub struct EngineHandle {
    tx: Sender<EngineCmd>,
}

impl EngineHandle {
    /// Avvia la riproduzione della stazione indicata.
    pub fn play(&self, station: Station, levels: SharedLevels) {
        let _ = self.tx.send(EngineCmd::Play {
            station: Box::new(station),
            levels,
        });
    }

    /// Ferma la riproduzione corrente.
    pub fn stop(&self) {
        let _ = self.tx.send(EngineCmd::Stop);
    }

    /// Alterna pausa/riproduzione.
    pub fn toggle_pause(&self) {
        let _ = self.tx.send(EngineCmd::TogglePause);
    }

    /// Imposta il volume (0.0..1.0).
    pub fn set_volume(&self, volume: f32) {
        let _ = self.tx.send(EngineCmd::SetVolume(volume));
    }

    /// Crea un handle "disconnesso" che ignora ogni comando.
    #[must_use]
    pub fn broken() -> Self {
        let (tx, rx) = mpsc::channel();
        drop(rx);
        Self { tx }
    }
}

/// Segnala lo stop alla sorgente corrente, se presente.
fn stop_source(current_stop: &mut Option<Arc<AtomicBool>>) {
    if let Some(stop) = current_stop.take() {
        stop.store(true, Ordering::Relaxed);
    }
}

/// Crea il device audio di default e avvia il thread di riproduzione.
///
/// # Errors
///
/// Returns [`AppError::NoAudioDevice`] if no audio device is
/// available on the system.
pub fn spawn(msg_tx: Sender<Msg>) -> Result<EngineHandle, AppError> {
    let sink =
        rodio::DeviceSinkBuilder::open_default_sink().map_err(|_| AppError::NoAudioDevice)?;
    let player = Player::connect_new(sink.mixer());

    let (cmd_tx, cmd_rx) = mpsc::channel();
    let handle_tx = cmd_tx.clone();
    thread::spawn(move || run_engine(player, sink, cmd_tx, cmd_rx, msg_tx));
    Ok(EngineHandle { tx: handle_tx })
}

/// Mantiene vivo il device stream (`_sink`) per tutta la vita del thread e processa
/// i comandi di riproduzione.
#[allow(clippy::needless_pass_by_value)]
fn run_engine(
    player: Player,
    _sink: rodio::MixerDeviceSink,
    cmd_tx: Sender<EngineCmd>,
    cmd_rx: Receiver<EngineCmd>,
    msg_tx: Sender<Msg>,
) {
    let mut current_stop: Option<Arc<AtomicBool>> = None;
    let mut volume = DEFAULT_VOLUME;
    let mut generation: u64 = 0;

    loop {
        match cmd_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(EngineCmd::Play { station, levels }) => {
                stop_source(&mut current_stop);
                generation += 1;
                player.clear();
                player.set_volume(volume);
                let _ = msg_tx.send(Msg::Playback(PlaybackState::Connecting));

                // La connessione e la costruzione del decoder avvengono su un thread
                // di lavoro: uno stream lento o malformato non deve bloccare l'engine,
                // che resta pronto a gestire stop/cambio stazione e altri comandi.
                let stop = Arc::new(AtomicBool::new(false));
                current_stop = Some(stop.clone());
                spawn_connect(station.as_ref(), levels, stop, generation, cmd_tx.clone());
            }
            Ok(EngineCmd::Stop) => {
                stop_source(&mut current_stop);
                generation += 1;
                player.clear();
                let _ = msg_tx.send(Msg::Playback(PlaybackState::Stopped));
            }
            Ok(EngineCmd::TogglePause) => {
                if player.is_paused() {
                    player.play();
                } else {
                    player.pause();
                }
                let state = if player.is_paused() {
                    PlaybackState::Paused
                } else {
                    PlaybackState::Playing
                };
                let _ = msg_tx.send(Msg::Playback(state));
            }
            Ok(EngineCmd::SetVolume(value)) => {
                volume = value.clamp(0.0, 1.0);
                player.set_volume(volume);
            }
            Ok(EngineCmd::StreamReady {
                generation: ready_generation,
                decoder,
                levels,
            }) => {
                // Ignora risultati ormai superati da un nuovo play o da uno stop.
                if ready_generation == generation {
                    player.append(LevelSource::new(decoder, levels));
                    player.play();
                    let _ = msg_tx.send(Msg::Playback(PlaybackState::Playing));
                }
            }
            Ok(EngineCmd::StreamFailed {
                generation: failed_generation,
                message,
            }) => {
                if failed_generation == generation {
                    let _ = msg_tx.send(Msg::PlaybackError(message));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Avvia la connessione di uno stream su un thread di lavoro.
///
/// The thread downloads the HTTP stream and builds the decoder, then sends
/// the result to the engine via channel: the `generation` allows discarding
/// results that are now stale due to a stop or station change. This way,
/// a slow or malformed connection never blocks the engine thread.
fn spawn_connect(
    station: &Station,
    levels: SharedLevels,
    stop: Arc<AtomicBool>,
    generation: u64,
    cmd_tx: Sender<EngineCmd>,
) {
    let url = station.url_resolved.clone();
    let codec = station.codec.clone();
    thread::spawn(move || {
        let outcome = connect_stream(&url, &codec, &stop);
        let cmd = match outcome {
            Ok(decoder) => EngineCmd::StreamReady {
                generation,
                decoder,
                levels,
            },
            Err(error) => EngineCmd::StreamFailed {
                generation,
                message: format!("formato audio non supportato o stream non valido ({error})"),
            },
        };
        let _ = cmd_tx.send(cmd);
    });
}

/// Connette lo stream HTTP e prova a costruire il decoder, senza toccare il player.
fn connect_stream(
    url: &str,
    codec: &str,
    stop: &Arc<AtomicBool>,
) -> Result<rodio::Decoder<InterruptibleReader>, AppError> {
    let (chunk_tx, chunk_rx) = channel::unbounded::<Vec<u8>>();

    let producer_stop = stop.clone();
    let url = url.to_string();
    thread::spawn(move || {
        if let Err(error) = fetch_stream(&url, &chunk_tx, &producer_stop) {
            tracing::debug!(%url, error = %error, "fetch dello stream terminato");
        }
    });

    let reader = InterruptibleReader::new(chunk_rx, stop.clone());
    let mut builder = Decoder::builder().with_data(reader);
    if let Some(hint) = decoder_hint(codec) {
        builder = builder.with_hint(hint);
    }
    builder
        .build()
        .map_err(|error| AppError::Playback(error.to_string()))
}

/// Suggerisce al decoder un'estensione in base al codec dichiarato dalla stazione.
fn decoder_hint(codec: &str) -> Option<&'static str> {
    let codec = codec.to_ascii_lowercase();
    if codec.contains("mp3") {
        Some("mp3")
    } else if codec.contains("aac") || codec.contains("m4a") || codec.contains("mp4") {
        // AAC, AAC+ e HE-AAC in radio sono quasi sempre ADTS, non MP4: il demuxer
        // ADTS rifiuta subito i dati non validi, mentre l'hint "m4a" farebbe
        // scandire il container in attesa di un atom `moov` che su uno stream
        // live non arriva mai, bloccando la connessione.
        Some("aac")
    } else if codec.contains("vorbis") || codec.contains("ogg") {
        Some("vorbis")
    } else if codec.contains("flac") {
        Some("flac")
    } else if codec.contains("wav") {
        Some("wav")
    } else {
        None
    }
}

/// Scarica lo stream HTTP e inoltra i chunk letti al canale.
fn fetch_stream(
    url: &str,
    chunk_tx: &channel::Sender<Vec<u8>>,
    stop: &AtomicBool,
) -> Result<(), AppError> {
    use std::io::Read;

    use reqwest::header::{ACCEPT, USER_AGENT};

    const USER_AGENT_VALUE: &str = concat!("myradio/", env!("CARGO_PKG_VERSION"));

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(6))
        .build()
        .map_err(|e| AppError::Network(e.to_string()))?;

    let mut response = client
        .get(url)
        .header(ACCEPT, "*/*")
        .header(USER_AGENT, USER_AGENT_VALUE)
        .send()
        .map_err(|e| AppError::Network(e.to_string()))?;

    if !response.status().is_success() {
        return Err(AppError::Playback(format!("HTTP {}", response.status())));
    }

    let mut buffer = [0u8; 8192];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(AppError::Interrupted);
        }
        let read = response
            .read(&mut buffer)
            .map_err(|e| AppError::Network(e.to_string()))?;
        if read == 0 {
            return Ok(());
        }
        if chunk_tx.send(buffer[..read].to_vec()).is_err() {
            // Il receiver è stato chiuso (stazione terminata): interrompi.
            return Ok(());
        }
    }
}

/// Reader that consumes chunks produced by the network thread and is interruptible.
///
/// Implements [`io::Read`] and [`io::Seek`] for use as the decoder source.
/// Seek is only supported backwards, within the window of already-read bytes
/// (limited cache): this is what the decoder needs for stream *probing*.
/// If the stop flag is set, EOF is returned.
pub struct InterruptibleReader {
    rx: channel::Receiver<Vec<u8>>,
    stop: Arc<AtomicBool>,
    buffer: VecDeque<u8>,
    rewind: VecDeque<u8>,
    rewind_origin: u64,
    pos: u64,
}

impl InterruptibleReader {
    /// Create a new reader.
    #[must_use]
    pub fn new(rx: channel::Receiver<Vec<u8>>, stop: Arc<AtomicBool>) -> Self {
        Self {
            rx,
            stop,
            buffer: VecDeque::new(),
            rewind: VecDeque::new(),
            rewind_origin: 0,
            pos: 0,
        }
    }

    /// Registra un byte consumato nella cache ripercorribile, mantenendone i limiti.
    fn remember(&mut self, byte: u8) {
        if self.rewind.len() >= REWIND_CACHE_BYTES {
            if let Some(_dropped) = self.rewind.pop_front() {
                self.rewind_origin += 1;
            }
        }
        self.rewind.push_back(byte);
    }

    /// Return a byte already consumed in the read stream (rewind).
    fn unremember(&mut self) -> Option<u8> {
        self.rewind.pop_back()
    }

    /// Wait for a chunk from the network, respecting the stop flag.
    fn fill(&mut self) -> io::Result<()> {
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            match self.rx.recv_timeout(READ_POLL_INTERVAL) {
                Ok(chunk) => {
                    self.buffer.extend(chunk);
                    return Ok(());
                }
                Err(channel::RecvTimeoutError::Timeout) => {}
                Err(channel::RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }
}

impl Read for InterruptibleReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.buffer.is_empty() {
            self.fill()?;
        }
        if self.buffer.is_empty() {
            return Ok(0);
        }
        let count = buf.len().min(self.buffer.len());
        for dst in buf.iter_mut().take(count) {
            let byte = self
                .buffer
                .pop_front()
                .expect("count è calcolato dalla lunghezza del buffer");
            *dst = byte;
            self.remember(byte);
            self.pos += 1;
        }
        Ok(count)
    }
}

impl Seek for InterruptibleReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::Current(delta) => i128::from(delta) + i128::from(self.pos),
            SeekFrom::End(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "seek dalla fine non supportato su uno stream live",
                ));
            }
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "posizione di seek negativa",
            ));
        }
        let target = target as u64;

        if target < self.pos {
            let back = (self.pos - target) as usize;
            if back > self.rewind.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "rewind oltre la cache disponibile sullo stream live",
                ));
            }
            for _ in 0..back {
                if let Some(byte) = self.unremember() {
                    self.buffer.push_front(byte);
                }
            }
            self.pos = target;
        } else if target > self.pos {
            let mut forward = target - self.pos;
            while forward > 0 {
                if self.buffer.is_empty() {
                    self.fill()?;
                }
                if self.buffer.is_empty() {
                    // EOF di fatto: non si può avanzare oltre la fine dello stream.
                    break;
                }
                let take = forward.min(self.buffer.len() as u64) as usize;
                for _ in 0..take {
                    let byte = self
                        .buffer
                        .pop_front()
                        .expect("take è calcolato dalla lunghezza del buffer");
                    self.remember(byte);
                    self.pos += 1;
                }
                forward -= take as u64;
            }
        }
        Ok(self.pos)
    }
}

impl Drop for InterruptibleReader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Sorgente rodio che campiona l'ampiezza del segnale e aggiorna i livelli condivisi.
pub struct LevelSource<S: Source> {
    inner: S,
    levels: SharedLevels,
    sum_sq: f64,
    count: u64,
    window: u64,
}

impl<S: Source> LevelSource<S> {
    /// Create the source from the decoder. `window` is the number of samples between
    /// level updates (~40 times per second).
    #[must_use]
    pub fn new(inner: S, levels: SharedLevels) -> Self {
        let window = u64::from(inner.sample_rate().get()) / 40;
        Self {
            inner,
            levels,
            sum_sq: 0.0,
            count: 0,
            window: window.max(1),
        }
    }
}

impl<S: Source> Iterator for LevelSource<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let sample = self.inner.next()?;
        let value = f64::from(sample);
        self.sum_sq += value * value;
        self.count += 1;
        if self.count >= self.window {
            let rms = (self.sum_sq / self.count as f64).sqrt();
            let db = (20.0 * rms.log10()).clamp(-120.0, 0.0);
            self.levels.push_db(db);
            self.sum_sq = 0.0;
            self.count = 0;
        }
        Some(sample)
    }
}

impl<S: Source> Source for LevelSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crossbeam_channel as channel;
    use rodio::source::{SineWave, Source};

    use super::{InterruptibleReader, LevelSource, decoder_hint};
    use crate::levels::SharedLevels;

    #[test]
    fn decoder_hint_maps_codecs() {
        assert_eq!(decoder_hint("AAC+"), Some("aac"));
        assert_eq!(decoder_hint("AAC HE v1"), Some("aac"));
        assert_eq!(decoder_hint("MP4A-LATM"), Some("aac"));
        assert_eq!(decoder_hint("MP3"), Some("mp3"));
        assert_eq!(decoder_hint("OGG Vorbis"), Some("vorbis"));
        assert_eq!(decoder_hint("FLAC"), Some("flac"));
        assert_eq!(decoder_hint("WAV"), Some("wav"));
        assert_eq!(decoder_hint(""), None);
    }

    #[test]
    fn reader_disconnect_returns_eof() {
        let (tx, rx) = channel::unbounded::<Vec<u8>>();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        tx.send(vec![1, 2, 3]).unwrap();
        drop(tx);

        let mut reader = InterruptibleReader::new(rx, stop);
        let mut buf = [0u8; 4];
        let n = std::io::Read::read(&mut reader, &mut buf).unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[..3], &[1, 2, 3]);
        assert_eq!(std::io::Read::read(&mut reader, &mut buf).unwrap(), 0);
    }

    #[test]
    fn reader_stop_flag_returns_eof() {
        let (tx, rx) = channel::unbounded::<Vec<u8>>();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        std::mem::forget(tx); // mantiene il canale aperto: il reader resterebbe in attesa

        let mut reader = InterruptibleReader::new(rx, stop.clone());
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut buf = [0u8; 4];
        assert_eq!(std::io::Read::read(&mut reader, &mut buf).unwrap(), 0);
        std::mem::forget(reader);
    }

    #[test]
    fn reader_rewinds_into_consumed_bytes() {
        let (tx, rx) = channel::unbounded::<Vec<u8>>();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        tx.send(vec![1, 2, 3, 4, 5]).unwrap();

        let mut reader = InterruptibleReader::new(rx, stop);
        let mut buf = [0u8; 2];
        assert_eq!(std::io::Read::read(&mut reader, &mut buf).unwrap(), 2);
        assert_eq!(&buf, &[1, 2]);

        // Rewind al byte 0 e rilettura.
        assert_eq!(
            std::io::Seek::seek(&mut reader, std::io::SeekFrom::Start(0)).unwrap(),
            0
        );
        assert_eq!(std::io::Read::read(&mut reader, &mut buf).unwrap(), 2);
        assert_eq!(&buf, &[1, 2]);
    }

    #[test]
    fn level_source_computes_positve_levels() {
        let samples = SineWave::new(440.0).take_duration(Duration::from_secs(1));
        let levels = SharedLevels::new();
        let mut source = LevelSource::new(samples, levels.clone());

        while source.next().is_some() {}

        let (current, history) = levels.snapshot();
        assert!(current > 0.0, "livello atteso > 0, ottenuto {current}");
        assert!(!history.is_empty());
    }
}
