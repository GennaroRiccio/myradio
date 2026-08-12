//! Macchina a stati e gestione dell'input dell'applicazione TUI.

use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use image::RgbaImage;
use ratatui::layout::{Position, Rect};

use crate::artwork::ArtworkStore;
use crate::audio::{DEFAULT_VOLUME, EngineHandle, PlaybackState};
use crate::error::AppError;
use crate::favorites::FavoritesStore;
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

/// Aree interattive della UI aggiornate a ogni frame, in coordinate schermo.
#[derive(Debug, Clone, Copy, Default)]
pub struct UiAreas {
    /// Riga del campo di ricerca per nome.
    pub query: Rect,
    /// Riga del campo di filtro per tag.
    pub tag: Rect,
    /// Area interna della tabella dei risultati.
    pub results: Rect,
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

    /// Lista mostrata nella tabella (risultati di ricerca o preferiti).
    pub stations: Vec<Station>,
    /// Stazioni salvate nei preferiti.
    pub favorites: Vec<Station>,
    /// Ultimi risultati di ricerca, per tornare alla lista risultati.
    pub search_results: Vec<Station>,
    /// `true` se la tabella mostra i preferiti invece dei risultati.
    pub showing_favorites: bool,
    /// Indice della stazione selezionata.
    pub selected: usize,
    /// `true` mentre una ricerca è in corso.
    pub loading: bool,

    /// Persistenza dei preferiti su disco.
    store: FavoritesStore,

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

    /// Aree interattive della UI, aggiornate a ogni frame dal rendering.
    pub areas: UiAreas,
    /// Prima riga visibile della tabella risultati (per mappare click -> stazione).
    pub results_offset: usize,

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
            favorites: Vec::new(),
            search_results: Vec::new(),
            showing_favorites: false,
            store: FavoritesStore::new(),
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
            areas: UiAreas::default(),
            results_offset: 0,
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

    /// Restituisce `true` se la stazione è nei preferiti.
    #[must_use]
    pub fn is_favorite(&self, station: &Station) -> bool {
        self.favorites
            .iter()
            .any(|favorite| favorite.id == station.id)
    }

    /// Carica i preferiti dal disco e, se non vuoti, li mostra all'avvio.
    pub fn load_favorites(&mut self) {
        self.favorites = self.store.load();
        if !self.favorites.is_empty() {
            self.show_favorites();
        }
    }

    /// Mostra la lista dei preferiti nella tabella dei risultati.
    pub fn show_favorites(&mut self) {
        self.showing_favorites = true;
        self.loading = false;
        self.stations = self.favorites.clone();
        self.selected = self.selected.min(self.stations.len().saturating_sub(1));
    }

    /// Alterna la tabella tra preferiti e ultimi risultati di ricerca.
    pub fn toggle_favorites_view(&mut self) {
        if self.showing_favorites {
            self.showing_favorites = false;
            self.stations = self.search_results.clone();
            self.selected = self.selected.min(self.stations.len().saturating_sub(1));
        } else {
            self.show_favorites();
        }
    }

    /// Aggiunge o rimuove la stazione selezionata dai preferiti, salvando su disco.
    pub fn toggle_favorite(&mut self) {
        let Some(station) = self.stations.get(self.selected).cloned() else {
            return;
        };
        let name = station.name.clone();
        let message = if let Some(position) = self
            .favorites
            .iter()
            .position(|favorite| favorite.id == station.id)
        {
            self.favorites.remove(position);
            format!("Rimossa dai preferiti: {name}")
        } else {
            self.favorites.push(station);
            format!("Aggiunta ai preferiti: {name}")
        };

        if let Err(error) = self.store.save(&self.favorites) {
            self.status = Some(format!("errore salvataggio preferiti: {error}"));
        } else {
            self.status = Some(message);
        }

        // Se stiamo mostrando i preferiti, la lista cambia subito.
        if self.showing_favorites {
            self.show_favorites();
            if self.favorites.is_empty() {
                self.showing_favorites = false;
                self.stations = self.search_results.clone();
            }
        }
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
            KeyCode::Char('f') => self.toggle_favorite(),
            KeyCode::Char('F') => self.toggle_favorites_view(),
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

    /// Elabora un evento del mouse.
    pub fn handle_mouse(&mut self, event: MouseEvent) {
        let position = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.handle_click(position),
            MouseEventKind::ScrollUp if self.areas.results.contains(position) => {
                self.move_selection(true);
            }
            MouseEventKind::ScrollDown if self.areas.results.contains(position) => {
                self.move_selection(false);
            }
            _ => {}
        }
    }

    /// Gestisce un click con il tasto sinistro su un elemento interattivo.
    fn handle_click(&mut self, position: Position) {
        if self.areas.query.contains(position) {
            self.focus = Focus::Query;
        } else if self.areas.tag.contains(position) {
            self.focus = Focus::Tag;
        } else if self.areas.results.contains(position) {
            self.focus = Focus::Results;
            if self.select_row_at(position.y, self.areas.results) {
                self.play_on_click();
            }
        }
    }

    /// Seleziona la stazione corrispondente alla riga di schermo `row` della tabella.
    ///
    /// Restituisce `true` se il click ha colpito una riga con una stazione valida.
    fn select_row_at(&mut self, row: u16, results: Rect) -> bool {
        if row < results.y || row >= results.y + results.height {
            return false;
        }
        let local = row - results.y;
        if local == 0 {
            return false;
        }
        let index = self.results_offset + usize::from(local - 1);
        if index < self.stations.len() {
            self.selected = index;
            true
        } else {
            false
        }
    }

    /// Avvia la riproduzione della stazione appena selezionata col mouse, senza
    /// riavviare uno stream già attivo sulla stessa stazione.
    fn play_on_click(&mut self) {
        let Some(station) = self.stations.get(self.selected).cloned() else {
            return;
        };
        let is_current = self
            .now_playing
            .as_ref()
            .is_some_and(|now| now.id == station.id);
        if !is_current {
            self.play_selected();
        }
    }

    /// Avvia una ricerca in un thread di lavoro.
    pub fn run_search(&mut self) {
        let query = self.query.trim().to_string();
        let tag = self.tag.trim();

        self.loading = true;
        self.status = None;
        self.selected = 0;
        self.showing_favorites = false;

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
                    self.search_results = stations.clone();
                    self.stations = stations;
                    self.showing_favorites = false;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::MouseEvent;
    use ratatui::layout::Rect;
    use std::sync::mpsc;

    fn station(id: &str) -> Station {
        Station {
            id: id.to_string(),
            name: id.to_string(),
            url_resolved: "http://example/stream".to_string(),
            url: "http://example/stream".to_string(),
            favicon: String::new(),
            homepage: String::new(),
            country: "IT".to_string(),
            state: String::new(),
            language: String::new(),
            codec: "MP3".to_string(),
            bitrate: 128,
            tags: Vec::new(),
            votes: 0,
            hls: false,
        }
    }

    fn app_with_stations(n: usize) -> App {
        let (tx, rx) = mpsc::channel();
        let mut app = App::new(tx, rx, EngineHandle::broken());
        app.dismiss_splash();
        app.stations = (0..n).map(|i| station(&format!("s{i}"))).collect();
        app
    }

    fn click_at(app: &mut App, x: u16, y: u16) {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::empty(),
        });
    }

    #[test]
    fn mouse_click_focuses_query_field() {
        let mut app = app_with_stations(0);
        app.focus = Focus::Results;
        app.areas.query = Rect::new(2, 4, 40, 1);
        app.areas.tag = Rect::new(2, 5, 40, 1);
        click_at(&mut app, 5, 4);
        assert_eq!(app.focus, Focus::Query);
    }

    #[test]
    fn mouse_click_focuses_tag_field() {
        let mut app = app_with_stations(0);
        app.focus = Focus::Query;
        app.areas.query = Rect::new(2, 4, 40, 1);
        app.areas.tag = Rect::new(2, 5, 40, 1);
        click_at(&mut app, 5, 5);
        assert_eq!(app.focus, Focus::Tag);
    }

    #[test]
    fn mouse_click_selects_row_in_results() {
        let mut app = app_with_stations(5);
        app.areas.results = Rect::new(0, 10, 40, 10);
        app.results_offset = 2;
        app.focus = Focus::Results;
        click_at(&mut app, 3, 12);
        assert_eq!(app.selected, 3);
    }

    #[test]
    fn mouse_click_starts_playback_of_row() {
        let mut app = app_with_stations(3);
        app.areas.results = Rect::new(0, 10, 40, 10);
        app.focus = Focus::Results;
        click_at(&mut app, 3, 12);
        assert_eq!(app.selected, 1);
        assert_eq!(app.playback, PlaybackState::Connecting);
        assert_eq!(app.now_playing.as_ref().map(|s| s.id.as_str()), Some("s1"));
    }

    #[test]
    fn mouse_click_on_active_station_does_not_restart() {
        let mut app = app_with_stations(3);
        app.areas.results = Rect::new(0, 10, 40, 10);
        app.selected = 1;
        app.now_playing = Some(station("s1"));
        app.playback = PlaybackState::Playing;
        click_at(&mut app, 3, 12);
        assert_eq!(app.playback, PlaybackState::Playing);
        assert_eq!(app.now_playing.as_ref().map(|s| s.id.as_str()), Some("s1"));
    }

    #[test]
    fn mouse_click_on_header_keeps_selection() {
        let mut app = app_with_stations(5);
        app.areas.results = Rect::new(0, 10, 40, 10);
        app.selected = 4;
        click_at(&mut app, 3, 10);
        assert_eq!(app.selected, 4);
    }

    #[test]
    fn mouse_scroll_moves_selection() {
        let mut app = app_with_stations(5);
        app.areas.results = Rect::new(0, 10, 40, 10);
        app.selected = 0;
        let scroll = |app: &mut App, down: bool| {
            app.handle_mouse(MouseEvent {
                kind: if down {
                    MouseEventKind::ScrollDown
                } else {
                    MouseEventKind::ScrollUp
                },
                column: 3,
                row: 11,
                modifiers: KeyModifiers::empty(),
            });
        };
        scroll(&mut app, true);
        assert_eq!(app.selected, 1);
        scroll(&mut app, true);
        assert_eq!(app.selected, 2);
        scroll(&mut app, false);
        assert_eq!(app.selected, 1);
    }

    fn press(app: &mut App, key: char) {
        app.focus = Focus::Results;
        app.handle_input(KeyEvent::new(KeyCode::Char(key), KeyModifiers::empty()));
    }

    fn temp_store() -> FavoritesStore {
        let uniq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("orologio di sistema non valido")
            .as_nanos();
        FavoritesStore::with_path(std::env::temp_dir().join(format!("myradio-app-fav-{uniq}.json")))
    }

    #[test]
    fn favorite_toggle_adds_and_saves() {
        let mut app = app_with_stations(1);
        app.store = temp_store();
        press(&mut app, 'f');
        assert_eq!(app.favorites.len(), 1);
        assert_eq!(app.favorites[0].id, "s0");
        assert!(app.is_favorite(&station("s0")));
        assert_eq!(
            app.store.load().len(),
            1,
            "i preferiti devono persistere su disco"
        );
    }

    #[test]
    fn favorite_toggle_removes() {
        let mut app = app_with_stations(1);
        app.store = temp_store();
        press(&mut app, 'f');
        press(&mut app, 'f');
        assert!(app.favorites.is_empty());
        assert!(!app.is_favorite(&station("s0")));
    }

    #[test]
    fn load_favorites_shows_when_non_empty() {
        let store = temp_store();
        store.save(&[station("s0"), station("s1")]).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut app = App::new(tx, rx, EngineHandle::broken());
        app.store = store;
        app.load_favorites();
        assert!(app.showing_favorites);
        assert_eq!(app.stations.len(), 2);
        assert_eq!(app.stations[0].id, "s0");
    }

    #[test]
    fn load_favorites_empty_keeps_list_hidden() {
        let (tx, rx) = mpsc::channel();
        let mut app = App::new(tx, rx, EngineHandle::broken());
        app.store = temp_store();
        app.load_favorites();
        assert!(!app.showing_favorites);
        assert!(app.stations.is_empty());
    }

    #[test]
    fn key_shift_f_toggles_favorites_view() {
        let mut app = app_with_stations(0);
        app.favorites = vec![station("pref1")];
        app.search_results = vec![station("ris1")];
        press(&mut app, 'F');
        assert!(app.showing_favorites);
        assert_eq!(app.stations[0].id, "pref1");
        press(&mut app, 'F');
        assert!(!app.showing_favorites);
        assert_eq!(app.stations[0].id, "ris1");
    }

    #[test]
    fn removing_last_favorite_hides_favorites_view() {
        let mut app = app_with_stations(0);
        app.favorites = vec![station("solo")];
        app.search_results = vec![station("ris")];
        app.show_favorites();
        assert!(app.showing_favorites);
        press(&mut app, 'f');
        assert!(app.favorites.is_empty());
        assert!(!app.showing_favorites);
        assert_eq!(app.stations[0].id, "ris");
    }
}
