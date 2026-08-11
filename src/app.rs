//! Macchina a stati e gestione dell'input dell'applicazione TUI.

use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use image::RgbaImage;

use crate::artwork::ArtworkStore;
use crate::audio::{DEFAULT_VOLUME, EngineHandle, PlaybackState};
use crate::error::AppError;
use crate::levels::SharedLevels;
use crate::radio::{RadioBrowserProvider, Station, StationProvider};

/// Messaggi asincroni inviati al loop principale da thread di lavoro.
#[derive(Debug)]
pub enum Msg {
    /// Risultato di una ricerca di stazioni.
    SearchFinished(Result<Vec<Station>, AppError>),
    /// Cambio di stato della riproduzione.
    Playback(PlaybackState),
    /// Errore di riproduzione non recuperabile.
    PlaybackError(String),
    /// Artwork di una stazione scaricato (o fallito).
    Artwork {
        /// Identificativo della stazione.
        id: String,
        /// Immagine decodificata, se il download è riuscito.
        image: Option<RgbaImage>,
    },
}

/// Campo della UI attualmente in primo piano.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Campo di ricerca per nome.
    Query,
    /// Campo di filtro per tag.
    Tag,
    /// Tabella dei risultati.
    Results,
}

/// Stato complessivo dell'applicazione.
pub struct App {
    msg_tx: Sender<Msg>,
    msg_rx: Receiver<Msg>,
    engine: EngineHandle,

    /// Testo del campo di ricerca per nome.
    pub query: String,
    /// Testo del campo di filtro per tag.
    pub tag: String,
    /// Campo della UI attualmente in focus.
    pub focus: Focus,

    /// Risultati dell'ultima ricerca.
    pub stations: Vec<Station>,
    /// Indice della stazione selezionata.
    pub selected: usize,
    /// `true` mentre una ricerca è in corso.
    pub loading: bool,

    /// Stazione in riproduzione, se presente.
    pub now_playing: Option<Station>,
    /// Stato corrente della riproduzione.
    pub playback: PlaybackState,
    /// Volume corrente (0.0..1.0).
    pub volume: f32,

    /// Mostra il visualizzatore audio.
    pub visualizer: bool,
    /// Livelli audio condivisi col thread di riproduzione.
    pub levels: SharedLevels,

    /// Messaggio di stato da mostrare nella status bar.
    pub status: Option<String>,

    /// Artwork delle stazioni (download, cache su disco e memoria).
    pub artworks: ArtworkStore,

    /// `true` mentre è visibile il banner di avvio.
    pub splash: bool,
    /// Istante di avvio del banner (per l'animazione).
    pub splash_started: Instant,

    should_exit: bool,
}

impl App {
    /// Crea una nuova applicazione con lo stato iniziale.
    #[must_use]
    pub fn new(msg_tx: Sender<Msg>, msg_rx: Receiver<Msg>, engine: EngineHandle) -> Self {
        let app = Self {
            msg_tx,
            msg_rx,
            engine,
            query: String::new(),
            tag: String::new(),
            focus: Focus::Query,
            stations: Vec::new(),
            selected: 0,
            loading: false,
            now_playing: None,
            playback: PlaybackState::Stopped,
            volume: DEFAULT_VOLUME,
            visualizer: true,
            levels: SharedLevels::new(),
            status: None,
            artworks: ArtworkStore::default(),
            splash: true,
            splash_started: Instant::now(),
            should_exit: false,
        };
        app.engine.set_volume(app.volume);
        app
    }

    /// Indica se il loop principale deve terminare.
    #[must_use]
    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    /// Chiude il banner di avvio.
    pub fn dismiss_splash(&mut self) {
        self.splash = false;
    }

    /// Restituisce la stazione selezionata, se presente.
    #[must_use]
    pub fn selected_station(&self) -> Option<&Station> {
        self.stations.get(self.selected)
    }

    /// Restituisce `true` se un campo di ricerca è in editing.
    #[must_use]
    pub fn editing(&self) -> bool {
        matches!(self.focus, Focus::Query | Focus::Tag)
    }

    /// Elabora un evento da tastiera.
    pub fn handle_input(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_exit = true;
            return;
        }
        if self.editing() {
            self.handle_editing_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('q' | 'Q') => self.should_exit = true,
            KeyCode::Char('/' | 'i') | KeyCode::Esc => self.focus = Focus::Query,
            KeyCode::Char('t') => self.focus = Focus::Tag,
            KeyCode::Char('r') => self.run_search(),
            KeyCode::Char('v') => self.visualizer = !self.visualizer,
            KeyCode::Char('p' | ' ') => self.toggle_play_or_play_selected(),
            KeyCode::Char('s') => self.stop(),
            KeyCode::Char('+' | '=') => self.volume_up(),
            KeyCode::Char('-' | '_') => self.volume_down(),
            KeyCode::Enter => self.play_selected(),
            KeyCode::Tab => self.focus = self.next_focus(),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(true),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(false),
            _ => {}
        }
    }

    /// Gestisce i tasti durante l'editing dei campi di ricerca.
    fn handle_editing_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.focus = Focus::Results,
            KeyCode::Backspace => {
                let _ = match self.focus {
                    Focus::Query => self.query.pop(),
                    Focus::Tag => self.tag.pop(),
                    Focus::Results => None,
                };
            }
            KeyCode::Enter => self.run_search(),
            KeyCode::Tab => self.focus = self.next_focus(),
            KeyCode::Char(ch) => match self.focus {
                Focus::Query => self.query.push(ch),
                Focus::Tag => self.tag.push(ch),
                Focus::Results => {}
            },
            _ => {}
        }
    }

    /// Passa al campo successivo nel ciclo Query -> Tag -> Risultati.
    #[must_use]
    fn next_focus(&self) -> Focus {
        match self.focus {
            Focus::Query => Focus::Tag,
            Focus::Tag => Focus::Results,
            Focus::Results => Focus::Query,
        }
    }

    /// Sposta la selezione nella lista dei risultati.
    fn move_selection(&mut self, up: bool) {
        if self.stations.is_empty() {
            return;
        }
        let len = self.stations.len();
        self.selected = if up {
            self.selected.saturating_sub(1)
        } else {
            (self.selected + 1).min(len - 1)
        };
    }

    /// Avvia una ricerca in un thread di lavoro.
    pub fn run_search(&mut self) {
        let query = self.query.trim().to_string();
        let tag = self.tag.trim();

        self.loading = true;
        self.status = None;
        self.selected = 0;

        let tx = self.msg_tx.clone();
        let tag = (!tag.is_empty()).then(|| tag.to_string());
        thread::spawn(move || {
            let provider = RadioBrowserProvider;
            let result = provider.search(&query, tag.as_deref());
            let _ = tx.send(Msg::SearchFinished(result));
        });
    }

    /// Avvia la riproduzione della stazione selezionata.
    pub fn play_selected(&mut self) {
        let Some(station) = self.stations.get(self.selected).cloned() else {
            return;
        };
        self.now_playing = Some(station.clone());
        self.playback = PlaybackState::Connecting;
        self.status = None;
        self.levels.reset();
        self.engine.play(station, self.levels.clone());
    }

    /// Alterna riproduzione/pausa, o avvia la stazione selezionata se ferma.
    fn toggle_play_or_play_selected(&mut self) {
        match self.playback {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused => self.resume(),
            _ => self.play_selected(),
        }
    }

    /// Mette in pausa la riproduzione corrente.
    pub fn pause(&mut self) {
        self.engine.toggle_pause();
        self.playback = PlaybackState::Paused;
    }

    /// Riprende la riproduzione in pausa.
    pub fn resume(&mut self) {
        self.engine.toggle_pause();
        self.playback = PlaybackState::Playing;
    }

    /// Ferma la riproduzione corrente.
    pub fn stop(&mut self) {
        self.engine.stop();
        self.now_playing = None;
        self.playback = PlaybackState::Stopped;
        self.levels.reset();
    }

    /// Incrementa il volume.
    pub fn volume_up(&mut self) {
        self.volume = (self.volume + 0.1).min(1.0);
        self.engine.set_volume(self.volume);
    }

    /// Decrementa il volume.
    pub fn volume_down(&mut self) {
        self.volume = (self.volume - 0.1).max(0.0);
        self.engine.set_volume(self.volume);
    }

    /// Processa i messaggi asincroni in coda.
    pub fn process_messages(&mut self) {
        while let Ok(msg) = self.msg_rx.try_recv() {
            match msg {
                Msg::SearchFinished(Ok(stations)) => {
                    self.stations = stations;
                    self.selected = 0;
                    self.loading = false;
                }
                Msg::SearchFinished(Err(e)) => {
                    self.loading = false;
                    self.status = Some(e.to_string());
                }
                Msg::Playback(state) => {
                    self.playback = state;
                    if state == PlaybackState::Stopped {
                        self.now_playing = None;
                        self.levels.reset();
                    }
                }
                Msg::PlaybackError(message) => {
                    self.playback = PlaybackState::Error;
                    self.status = Some(message);
                }
                Msg::Artwork { id, image } => {
                    self.artworks.store(id, image);
                }
            }
        }

        // Avvia man mano i download di artwork mancanti (al più MAX_IN_FLIGHT
        // in volo): i risultati arrivano sul canale e vengono processati sopra.
        self.artworks.request_missing(&self.stations, &self.msg_tx);
    }
}
