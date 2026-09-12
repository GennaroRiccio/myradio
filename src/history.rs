//! Persistence of listen history on disk.
//!
//! History stores the last 20 played stations in a JSON file in the
//! application data folder (same as favorites). Each entry includes a
//! timestamp. Loading is tolerant: missing or corrupted files produce an
//! empty list.

use std::io;
use std::path::PathBuf;

use crate::radio::Station;

/// Name of the application data subfolder.
const APP_DIR: &str = "myradio";

/// Name of the history file.
const FILE_NAME: &str = "history.json";

/// Maximum number of history entries kept.
const MAX_HISTORY: usize = 20;

/// A single history entry: station data + timestamp.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    /// The station that was played.
    #[serde(flatten)]
    pub station: Station,
    /// Unix timestamp (seconds) when playback started.
    pub played_at: u64,
}

/// Access to the history file.
#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: Option<PathBuf>,
}

impl HistoryStore {
    /// Create the store with the system's default data path.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: default_path(),
        }
    }

    /// Create the store with an explicit path (used by tests).
    #[must_use]
    pub fn with_path(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    /// Path of the history file, if a data folder is available.
    #[must_use]
    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// Load history from disk (most recent first).
    ///
    /// Missing or non-decodable file → empty list.
    #[must_use]
    pub fn load(&self) -> Vec<HistoryEntry> {
        let Some(path) = &self.path else {
            return Vec::new();
        };
        let Ok(json) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        let Ok(entries) = serde_json::from_str::<Vec<HistoryEntry>>(&json) else {
            tracing::warn!(path = %path.display(), "invalid history file");
            return Vec::new();
        };
        entries
    }

    /// Record a station play, prepending to history. Deduplicates by station
    /// id (moves the most recent play to the top). Truncates to
    /// [`MAX_HISTORY`] entries.
    pub fn record(&mut self, station: &Station) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());

        let mut entries = self.load();
        entries.retain(|e| e.station.id != station.id);
        entries.insert(
            0,
            HistoryEntry {
                station: station.clone(),
                played_at: now,
            },
        );
        entries.truncate(MAX_HISTORY);
        let _ = self.save(&entries);
    }

    /// Save history entries to disk (atomic write).
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the folder cannot be created or the file is
    /// not writable.
    pub fn save(&self, entries: &[HistoryEntry]) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(entries)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine the history file path for the current platform.
fn default_path() -> Option<PathBuf> {
    let base = data_dir()?;
    Some(base.join(APP_DIR).join(FILE_NAME))
}

/// Return the base user data folder for the current platform.
fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg));
            }
        }
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
        })
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::HistoryStore;
    use crate::radio::Station;

    fn station(id: &str) -> Station {
        Station {
            id: id.to_string(),
            name: format!("Stazione {id}"),
            url_resolved: format!("http://example.com/{id}"),
            url: format!("http://example.com/{id}"),
            favicon: String::new(),
            homepage: String::new(),
            country: "IT".to_string(),
            countrycode: "IT".to_string(),
            geo_lat: None,
            geo_long: None,
            state: String::new(),
            language: "italiano".to_string(),
            codec: "MP3".to_string(),
            bitrate: 128,
            tags: vec!["jazz".to_string()],
            votes: 42,
            hls: false,
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("orologio di sistema non valido")
            .as_nanos();
        std::env::temp_dir().join(format!("myradio-test-{name}-{uniq}.json"))
    }

    #[test]
    fn record_and_load_roundtrip() {
        let path = temp_path("roundtrip");
        let mut store = HistoryStore::with_path(path.clone());
        store.record(&station("a"));
        store.record(&station("b"));
        let loaded = store.load();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].station.id, "b");
        assert_eq!(loaded[1].station.id, "a");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn record_deduplicates_by_id() {
        let path = temp_path("dedup");
        let mut store = HistoryStore::with_path(path.clone());
        store.record(&station("a"));
        store.record(&station("b"));
        store.record(&station("a"));
        let loaded = store.load();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].station.id, "a");
        assert_eq!(loaded[1].station.id, "b");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let store = HistoryStore::with_path(temp_path("missing"));
        assert!(store.load().is_empty());
    }

    #[test]
    fn save_creates_parent_directories() {
        let path = temp_path("nested").join("sub").join("history.json");
        let mut store = HistoryStore::with_path(path.clone());
        store.record(&station("x"));
        assert!(path.exists());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_file() {
        let path = temp_path("atomic");
        let mut store = HistoryStore::with_path(path.clone());
        store.record(&station("x"));
        assert!(path.exists());
        assert!(
            !path.with_extension("tmp").exists(),
            "temp file must have been renamed away"
        );
        std::fs::remove_file(&path).ok();
    }
}
