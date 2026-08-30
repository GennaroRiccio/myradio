//! Persistence of favorites on disk.
//!
//! Favorites are saved as a list of [`Station`] in a JSON file in the
//! application data folder:
//!
//! - Linux: `$XDG_DATA_HOME/myradio/favorites.json` (default `~/.local/share/…`)
//! - macOS: `~/Library/Application Support/myradio/favorites.json`
//! - Windows: `%APPDATA%\myradio\favorites.json`
//!
//! Loading is tolerant: missing or corrupted files produce an empty list,
//! never an error that blocks startup.

use std::io;
use std::path::PathBuf;

use crate::radio::Station;

/// Name of the application data subfolder.
const APP_DIR: &str = "myradio";

/// Name of the favorites file.
const FILE_NAME: &str = "favorites.json";

/// Access to the favorites file.
///
/// If it's not possible to determine a data folder (systems without HOME),
/// saving is a no-op and loading returns an empty list.
#[derive(Debug, Clone)]
pub struct FavoritesStore {
    path: Option<PathBuf>,
}

impl FavoritesStore {
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

    /// Path of the favorites file, if a data folder is available.
    #[must_use]
    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// Load favorites from disk.
    ///
    /// Missing or non-decodable file → empty list.
    #[must_use]
    pub fn load(&self) -> Vec<Station> {
        let Some(path) = &self.path else {
            return Vec::new();
        };
        let Ok(json) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        let Ok(stations) = serde_json::from_str(&json) else {
            tracing::warn!(path = %path.display(), "invalid favorites file");
            return Vec::new();
        };
        stations
    }

    /// Save favorites to disk.
    ///
    /// Creates missing folders. The write is atomic: data goes to a temporary
    /// file in the same folder which is then renamed over the target, so an
    /// interrupted write never leaves a corrupted file. A write error is
    /// returned to be shown to the user without crashing the application.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the folder cannot be created or the file is
    /// not writable.
    pub fn save(&self, stations: &[Station]) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(stations)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

impl Default for FavoritesStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine the favorites file path for the current platform.
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
        let _ = ();
        None
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::FavoritesStore;
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
    fn save_and_load_roundtrip() {
        let path = temp_path("roundtrip");
        let store = FavoritesStore::with_path(path.clone());
        store.save(&[station("a"), station("b")]).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "a");
        assert_eq!(loaded[1].id, "b");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let store = FavoritesStore::with_path(temp_path("missing"));
        assert!(store.load().is_empty());
    }

    #[test]
    fn load_corrupted_file_returns_empty() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "non-json{{").unwrap();
        let store = FavoritesStore::with_path(path.clone());
        assert!(store.load().is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_creates_parent_directories() {
        let path = temp_path("nested").join("sub").join("favorites.json");
        let store = FavoritesStore::with_path(path.clone());
        store.save(&[station("x")]).unwrap();
        assert!(path.exists());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_file() {
        let path = temp_path("atomic");
        let store = FavoritesStore::with_path(path.clone());
        store.save(&[station("x")]).unwrap();
        assert!(path.exists());
        assert!(
            !path.with_extension("tmp").exists(),
            "temp file must have been renamed away"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_partial_station_json_uses_defaults() {
        let path = temp_path("partial");
        std::fs::write(&path, r#"[{"id":"a","name":"A"}]"#).unwrap();
        let store = FavoritesStore::with_path(path.clone());
        let loaded = store.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "a");
        assert_eq!(loaded[0].codec, "");
        std::fs::remove_file(&path).ok();
    }
}
