//! Client TUI per l'ascolto di radio internet via [Radio Browser](https://radio-browser.info).
//!
//! L'applicazione permette di cercare stazioni radio per nome e tag, selezionarne
//! una per riprodurla in streaming e visualizzare in tempo reale l'andamento del
//! volume attraverso un meter e uno storico dei livelli.

pub mod app;
pub mod artwork;
pub mod audio;
pub mod error;
pub mod favorites;
pub mod levels;
pub mod radio;
pub mod ui;

pub use audio::{EngineHandle, PlaybackState};
pub use error::AppError;
pub use radio::{RadioBrowserProvider, Station, StationProvider};
