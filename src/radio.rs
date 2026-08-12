//! Ricerca delle stazioni radio tramite Radio Browser.
//!
//! Il client usa l'API bloccante del crate `radiobrowser` (feature `blocking`).
//! Il trait [`StationProvider`] permette di sostituire il provider reale con un mock
//! nei test.

use radiobrowser::blocking::RadioBrowserAPI;
use radiobrowser::{ApiStation, StationOrder};

use crate::error::AppError;

/// Numero massimo di stazioni restituite da una singola ricerca.
const SEARCH_LIMIT: usize = 200;

/// Modello interno di una stazione radio.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Station {
    /// Identificativo univoco della stazione (stationuuid).
    pub id: String,
    /// Nome della stazione.
    pub name: String,
    /// URL dello stream (standardizzato con i redirect seguiti).
    pub url_resolved: String,
    /// URL dello stream dichiarato.
    pub url: String,
    /// URL del favicon/artwork della stazione.
    pub favicon: String,
    /// Pagina web della stazione.
    pub homepage: String,
    /// Paese di origine.
    pub country: String,
    /// Regione/Stato di origine.
    pub state: String,
    /// Lingua principale.
    pub language: String,
    /// Codec dello stream (es. `MP3`, `AAC`).
    pub codec: String,
    /// Bitrate dichiarato in kbps.
    pub bitrate: u32,
    /// Etichette/generi della stazione.
    pub tags: Vec<String>,
    /// Numero di voti ricevuti.
    pub votes: i32,
    /// `true` se lo stream usa HLS (non supportato).
    pub hls: bool,
}

impl Station {
    /// Restituisce la stringa da mostrare nella tabella dei risultati.
    #[must_use]
    pub fn bitrate_label(&self) -> String {
        if self.bitrate > 0 {
            format!("{} kbps", self.bitrate)
        } else {
            "-".to_string()
        }
    }
}

/// Sorgente di dati per la ricerca delle stazioni.
pub trait StationProvider: Send + Sync {
    /// Cerca stazioni per nome (sottostringa) e tag opzionale.
    ///
    /// # Errors
    ///
    /// Restituisce [`AppError::Search`] se il server non è raggiungibile o la
    /// risposta non è valida.
    fn search(&self, query: &str, tag: Option<&str>) -> Result<Vec<Station>, AppError>;
}

/// Provider reale basato sull'API di Radio Browser.
#[derive(Debug, Default)]
pub struct RadioBrowserProvider;

impl StationProvider for RadioBrowserProvider {
    fn search(&self, query: &str, tag: Option<&str>) -> Result<Vec<Station>, AppError> {
        // `new()` risolve i server tramite DNS SRV: in ambienti con DNS filtrato
        // può fallire, per cui si usa un fallback a un server noto.
        let api = RadioBrowserAPI::new()
            .or_else(|_| RadioBrowserAPI::new_from_dns_a("de1.api.radio-browser.info"))
            .map_err(|e| AppError::Search(e.to_string()))?;

        let mut builder = api
            .get_stations()
            .hidebroken(true)
            .order(StationOrder::Clickcount)
            .reverse(true)
            .limit(SEARCH_LIMIT.to_string());

        let query = query.trim();
        if !query.is_empty() {
            builder = builder.name(query);
        }

        if let Some(tag) = tag.and_then(|t| {
            let trimmed = t.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }) {
            builder = builder.tag(tag);
        }

        let stations = builder
            .send()
            .map_err(|e| AppError::Search(e.to_string()))?;

        Ok(stations.into_iter().filter_map(station_from_api).collect())
    }
}

/// Converte un record dell'API nel modello interno, scartando le stazioni non
/// riproducibili (HLS o senza URL risolto).
fn station_from_api(station: ApiStation) -> Option<Station> {
    if station.hls != 0 || station.url_resolved.trim().is_empty() {
        return None;
    }
    Some(Station {
        id: station.stationuuid,
        name: station.name,
        url_resolved: station.url_resolved,
        url: station.url,
        favicon: station.favicon,
        homepage: station.homepage,
        country: station.country,
        state: station.state,
        language: station.language,
        codec: station.codec,
        bitrate: station.bitrate,
        tags: station
            .tags
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_string)
            .collect(),
        votes: station.votes,
        hls: station.hls != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::station_from_api;
    use radiobrowser::ApiStation;

    fn sample_api_station() -> ApiStation {
        ApiStation {
            changeuuid: "c1".to_string(),
            stationuuid: "s1".to_string(),
            serveruuid: None,
            name: "Jazz 24".to_string(),
            url: "http://example/jazz".to_string(),
            url_resolved: "http://example/jazz/live.mp3".to_string(),
            homepage: "http://example".to_string(),
            favicon: String::new(),
            tags: "jazz, live, usa".to_string(),
            country: "United States".to_string(),
            countrycode: "US".to_string(),
            iso_3166_2: None,
            state: "California".to_string(),
            language: "English".to_string(),
            languagecodes: None,
            votes: 42,
            lastchangetime_iso8601: None,
            codec: "MP3".to_string(),
            bitrate: 128,
            hls: 0,
            lastcheckok: 1,
            lastchecktime_iso8601: None,
            lastcheckoktime_iso8601: None,
            lastlocalchecktime_iso8601: None,
            clicktimestamp_iso8601: None,
            clickcount: 10,
            clicktrend: 0,
            ssl_error: None,
            geo_lat: None,
            geo_long: None,
            has_extended_info: None,
        }
    }

    #[test]
    fn maps_record_correctly() {
        let station = station_from_api(sample_api_station()).expect("record valido");
        assert_eq!(station.name, "Jazz 24");
        assert_eq!(station.bitrate, 128);
        assert_eq!(station.tags, vec!["jazz", "live", "usa"]);
        assert_eq!(station.url_resolved, "http://example/jazz/live.mp3");
    }

    #[test]
    fn maps_favicon() {
        let mut api = sample_api_station();
        api.favicon = "http://example/favicon.png".to_string();
        let station = station_from_api(api).expect("record valido");
        assert_eq!(station.favicon, "http://example/favicon.png");
    }

    #[test]
    fn rejects_hls_and_empty_url() {
        let mut hls = sample_api_station();
        hls.hls = 1;
        assert!(station_from_api(hls).is_none());

        let mut empty = sample_api_station();
        empty.url_resolved = "  ".to_string();
        assert!(station_from_api(empty).is_none());
    }
}
