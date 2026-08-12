//! Persistenza dei preferiti su disco.
//!
//! I preferiti sono salvati come lista di [`Station`] in un file JSON nella
//! cartella dati dell'applicazione:
//!
//! - Linux: `$XDG_DATA_HOME/myradio/favorites.json` (default `~/.local/share/…`)
//! - macOS: `~/Library/Application Support/myradio/favorites.json`
//! - Windows: `%APPDATA%\myradio\favorites.json`
//!
//! Il caricamento è tollerante: file mancante o corrotto producono una lista
//! vuota, mai un errore che blocca l'avvio.

use std::io;
use std::path::PathBuf;

use crate::radio::Station;

/// Nome della sottocartella dati dell'applicazione.
const APP_DIR: &str = "myradio";

/// Nome del file dei preferiti.
const FILE_NAME: &str = "favorites.json";

/// Accesso al file dei preferiti.
///
/// Se non è possibile determinare una cartella dati (sistemi senza HOME), il
/// salvataggio è un no-op e il caricamento restituisce una lista vuota.
#[derive(Debug, Clone)]
pub struct FavoritesStore {
    path: Option<PathBuf>,
}

impl FavoritesStore {
    /// Crea lo store con il percorso dati di default del sistema.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: default_path(),
        }
    }

    /// Crea lo store con un percorso esplicito (usato dai test).
    #[must_use]
    pub fn with_path(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    /// Carica i preferiti dal disco.
    ///
    /// File mancante o non decodificabile → lista vuota.
    #[must_use]
    pub fn load(&self) -> Vec<Station> {
        let Some(path) = &self.path else {
            return Vec::new();
        };
        let Ok(json) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        let Ok(stations) = serde_json::from_str(&json) else {
            tracing::warn!(path = %path.display(), "file preferiti non valido");
            return Vec::new();
        };
        stations
    }

    /// Salva i preferiti su disco.
    ///
    /// Crea le cartelle mancanti. Un errore di scrittura viene restituito per
    /// essere mostrato all'utente, senza far crashare l'applicazione.
    ///
    /// # Errors
    ///
    /// Restituisce un errore I/O se la cartella non è creabile o il file non è
    /// scrivibile.
    pub fn save(&self, stations: &[Station]) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(stations)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, json)
    }
}

impl Default for FavoritesStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Determina il percorso del file dei preferiti per la piattaforma corrente.
fn default_path() -> Option<PathBuf> {
    let base = data_dir()?;
    Some(base.join(APP_DIR).join(FILE_NAME))
}

/// Restituisce la cartella dati utente di base per la piattaforma corrente.
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
}
