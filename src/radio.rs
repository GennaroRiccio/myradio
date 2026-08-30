//! Searching radio stations via Radio Browser.
//!
//! The client talks directly to the Radio Browser JSON API with the blocking
//! client of `reqwest` (no extra resolver/async-std dependencies). The
//! [`StationProvider`] trait lets tests swap the real provider for a mock.

use std::time::Duration;

use crate::error::AppError;

/// Number of stations returned by a single search (page size).
pub const SEARCH_LIMIT: usize = 200;

/// Static Radio Browser mirror used as the API base URL.
const API_BASE: &str = "https://de1.api.radio-browser.info";

/// Timeout for a single search request.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(20);

/// Internal model of a radio station.
///
/// Every field has `#[serde(default)]` so a favorites file written by another
/// version (with missing or extra fields) still loads: deserialization never
/// fails, preventing an app update from wiping the favorites.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Station {
    /// Unique station identifier (stationuuid).
    #[serde(default)]
    pub id: String,
    /// Station name.
    #[serde(default)]
    pub name: String,
    /// Stream URL (normalized after following redirects).
    #[serde(default)]
    pub url_resolved: String,
    /// Declared stream URL.
    #[serde(default)]
    pub url: String,
    /// Station favicon/artwork URL.
    #[serde(default)]
    pub favicon: String,
    /// Station homepage.
    #[serde(default)]
    pub homepage: String,
    /// Country of origin.
    #[serde(default)]
    pub country: String,
    /// ISO 3166-1 alpha-2 country code.
    #[serde(default)]
    pub countrycode: String,
    /// Latitude of the station (if provided by API).
    #[serde(default)]
    pub geo_lat: Option<f64>,
    /// Longitude of the station (if provided by API).
    #[serde(default)]
    pub geo_long: Option<f64>,
    /// Region/State of origin.
    #[serde(default)]
    pub state: String,
    /// Primary language.
    #[serde(default)]
    pub language: String,
    /// Stream codec (e.g. `MP3`, `AAC`).
    #[serde(default)]
    pub codec: String,
    /// Declared bitrate in kbps.
    #[serde(default)]
    pub bitrate: u32,
    /// Station labels/genres.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Number of votes received.
    #[serde(default)]
    pub votes: i32,
    /// `true` if the stream uses HLS (not supported).
    #[serde(default)]
    pub hls: bool,
}

impl Station {
    /// Returns the string to display in the results table.
    #[must_use]
    pub fn bitrate_label(&self) -> String {
        if self.bitrate > 0 {
            format!("{} kbps", self.bitrate)
        } else {
            "-".to_string()
        }
    }
}

/// Data source for station search.
pub trait StationProvider: Send + Sync {
    /// Search stations by name (substring) and optional tag.
    ///
    /// `offset` is the pagination offset (0 for first page).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Search`] if the server is unreachable or the
    /// response is invalid.
    fn search(
        &self,
        query: &str,
        tag: Option<&str>,
        offset: usize,
    ) -> Result<Vec<Station>, AppError>;
}

/// Real provider based on the Radio Browser API.
#[derive(Debug)]
pub struct RadioBrowserProvider {
    client: reqwest::blocking::Client,
}

impl Default for RadioBrowserProvider {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::builder()
                .user_agent(concat!("myradio/", env!("CARGO_PKG_VERSION")))
                .timeout(SEARCH_TIMEOUT)
                .build()
                .expect("valid HTTP client"),
        }
    }
}

impl StationProvider for RadioBrowserProvider {
    fn search(
        &self,
        query: &str,
        tag: Option<&str>,
        offset: usize,
    ) -> Result<Vec<Station>, AppError> {
        let tag = tag.and_then(|t| {
            let trimmed = t.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        });

        let query = query.trim();
        let mut params: Vec<(&str, String)> = vec![
            ("hidebroken", "true".to_string()),
            ("order", "clickcount".to_string()),
            ("reverse", "true".to_string()),
            ("limit", SEARCH_LIMIT.to_string()),
            ("offset", offset.to_string()),
        ];
        if !query.is_empty() {
            params.push(("name", query.to_string()));
        }
        if let Some(tag) = tag {
            params.push(("tag", tag));
        }

        let response = self
            .client
            .get(format!("{API_BASE}/json/stations/search"))
            .query(&params)
            .send()
            .map_err(|e| AppError::Search(e.to_string()))?;

        let stations: Vec<ApiStation> = response
            .json()
            .map_err(|e| AppError::Search(e.to_string()))?;

        Ok(stations.into_iter().filter_map(station_from_api).collect())
    }
}

/// A station record as returned by the Radio Browser JSON API (subset of
/// fields we use). All fields default so partial records still map.
#[derive(Debug, Clone, Default, serde::Deserialize)]
struct ApiStation {
    #[serde(default)]
    stationuuid: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    url_resolved: String,
    #[serde(default)]
    favicon: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    countrycode: String,
    #[serde(default)]
    geo_lat: Option<f64>,
    #[serde(default)]
    geo_long: Option<f64>,
    #[serde(default)]
    state: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    codec: String,
    #[serde(default)]
    bitrate: u32,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    votes: i32,
    #[serde(default)]
    hls: u8,
}

/// Maps an API record to the internal model, dropping non-playable stations
/// (HLS or without a resolved URL).
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
        countrycode: station.countrycode,
        geo_lat: station.geo_lat,
        geo_long: station.geo_long,
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
    use super::{ApiStation, station_from_api};

    fn sample_api_station() -> ApiStation {
        ApiStation {
            stationuuid: "s1".to_string(),
            name: "Jazz 24".to_string(),
            url: "http://example/jazz".to_string(),
            url_resolved: "http://example/jazz/live.mp3".to_string(),
            homepage: "http://example".to_string(),
            favicon: String::new(),
            tags: "jazz, live, usa".to_string(),
            country: "United States".to_string(),
            countrycode: "US".to_string(),
            geo_lat: Some(39.0),
            geo_long: Some(-98.0),
            state: "California".to_string(),
            language: "English".to_string(),
            votes: 42,
            codec: "MP3".to_string(),
            bitrate: 128,
            hls: 0,
        }
    }

    #[test]
    fn maps_record_correctly() {
        let station = station_from_api(sample_api_station()).expect("valid record");
        assert_eq!(station.name, "Jazz 24");
        assert_eq!(station.bitrate, 128);
        assert_eq!(station.tags, vec!["jazz", "live", "usa"]);
        assert_eq!(station.url_resolved, "http://example/jazz/live.mp3");
    }

    #[test]
    fn maps_favicon() {
        let mut api = sample_api_station();
        api.favicon = "http://example/favicon.png".to_string();
        let station = station_from_api(api).expect("valid record");
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
