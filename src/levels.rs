//! Computation and sharing of audio levels for the visualizer.
//!
//! The audio thread periodically produces an RMS value (in dBFS) of the decoded
//! signal and records it in a shared area; the TUI loop reads a snapshot every
//! frame without blocking sample production.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Meter full scale, in dBFS. Levels below this are considered silence.
const MIN_DB: f64 = -60.0;

/// Capacity of the levels history (in points).
const HISTORY_CAPACITY: usize = 240;

/// Convert a dBFS level to a 0..100 percentage.
#[must_use]
pub fn db_to_pcent(db: f64) -> f64 {
    let db = db.clamp(MIN_DB, 0.0);
    ((db - MIN_DB) / (-MIN_DB)) * 100.0
}

/// History and current level of decoded samples.
#[derive(Debug)]
struct Levels {
    history: VecDeque<f64>,
    current_db: f64,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            history: VecDeque::new(),
            current_db: MIN_DB,
        }
    }
}

impl Levels {
    /// Record a new RMS value (dB) and update history.
    fn push_db(&mut self, db: f64) {
        self.current_db = db.clamp(MIN_DB, 0.0);
        if self.history.len() >= HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back(db_to_pcent(self.current_db));
    }
}

/// Shared area between the audio thread and the TUI loop.
#[derive(Debug, Clone, Default)]
pub struct SharedLevels(Arc<Mutex<Levels>>);

impl SharedLevels {
    /// Create a new shared area.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset current level and history.
    pub fn reset(&self) {
        if let Ok(mut levels) = self.0.lock() {
            levels.history.clear();
            levels.current_db = MIN_DB;
        }
    }

    /// Record a new RMS level (in dBFS).
    pub fn push_db(&self, db: f64) {
        if let Ok(mut levels) = self.0.lock() {
            levels.push_db(db);
        }
    }

    /// Returns `(current level in percentage, history in percentage)`.
    #[must_use]
    pub fn snapshot(&self) -> (f64, Vec<u64>) {
        let Ok(levels) = self.0.lock() else {
            return (0.0, Vec::new());
        };
        let current = db_to_pcent(levels.current_db);
        let history = levels
            .history
            .iter()
            .map(|p| {
                let value = p.clamp(0.0, 100.0).round() as i64;
                u64::try_from(value).unwrap_or(0)
            })
            .collect();
        (current, history)
    }
}

#[cfg(test)]
mod tests {
    use super::{SharedLevels, db_to_pcent};

    #[test]
    fn db_mapping_is_monotonic() {
        assert!(db_to_pcent(-60.0) <= 0.0001);
        assert!((db_to_pcent(0.0) - 100.0).abs() < 0.0001);
        assert!(db_to_pcent(-30.0) > db_to_pcent(-45.0));
        assert!(db_to_pcent(-90.0) >= 0.0);
    }

    #[test]
    fn history_is_capped() {
        let levels = SharedLevels::new();
        for db in (0..500).map(|i| -60.0 + i as f64 / 5.0) {
            levels.push_db(db);
        }
        let (_, history) = levels.snapshot();
        assert_eq!(history.len(), 240);
    }

    #[test]
    fn reset_clears_history() {
        let levels = SharedLevels::new();
        for db in (-20..0).map(f64::from) {
            levels.push_db(db);
        }
        levels.reset();
        let (current, history) = levels.snapshot();
        assert!(current.abs() < 1e-9, "expected level 0, got {current}");
        assert!(history.is_empty());
    }
}
