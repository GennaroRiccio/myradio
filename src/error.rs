//! Tipi di errore applicativi.

use thiserror::Error;

/// Errore di dominio sollevato dall'applicazione.
#[derive(Debug, Error)]
pub enum AppError {
    /// Errore di rete durante una ricerca o durante lo streaming.
    #[error("errore di rete: {0}")]
    Network(String),
    /// La ricerca su Radio Browser è fallita.
    #[error("ricerca fallita: {0}")]
    Search(String),
    /// La stazione selezionata non può essere riprodotta.
    #[error("impossibile riprodurre la stazione: {0}")]
    Playback(String),
    /// Nessun dispositivo audio disponibile sul sistema.
    #[error("nessun dispositivo audio rilevato sull'host")]
    NoAudioDevice,
    /// Lo streaming è stato interrotto su richiesta dell'utente.
    #[error("streaming interrotto")]
    Interrupted,
}
