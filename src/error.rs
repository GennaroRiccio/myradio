//! Application error types.

use thiserror::Error;

/// Domain error raised by the application.
#[derive(Debug, Error)]
pub enum AppError {
    /// Network error during search or streaming.
    #[error("network error: {0}")]
    Network(String),
    /// Search on Radio Browser failed.
    #[error("search failed: {0}")]
    Search(String),
    /// Selected station cannot be played.
    #[error("unable to play station: {0}")]
    Playback(String),
    /// No audio device available on the system.
    #[error("no audio device detected on host")]
    NoAudioDevice,
    /// Streaming was interrupted on user request.
    #[error("streaming interrupted")]
    Interrupted,
}
