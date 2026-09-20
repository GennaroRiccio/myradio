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

/// Advanced search filters for station queries.
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Country name or ISO 3166-1 alpha-2 code (e.g. "Italy", "IT", "Italia").
    pub country: String,
    /// Language (e.g. "Italian", "italiano", "English").
    pub language: String,
    /// Codec filter (e.g. "MP3", "AAC").
    pub codec: String,
    /// Minimum bitrate in kbps (0 = no minimum).
    pub min_bitrate: u32,
}

impl SearchFilters {
    /// Returns `true` if all filters are empty/default.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.country.is_empty()
            && self.language.is_empty()
            && self.codec.is_empty()
            && self.min_bitrate == 0
    }
}

/// Resolves user country input to either an ISO 2-letter code or a country name.
#[must_use]
pub fn resolve_country(input: &str) -> (Option<String>, Option<String>) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return (None, None);
    }
    // 2-letter ISO code
    if trimmed.chars().count() == 2 {
        let code = match trimmed.to_ascii_uppercase().as_str() {
            "UK" => "GB".to_string(),
            other => other.to_string(),
        };
        return (Some(code), None);
    }
    let lower = trimmed.to_lowercase();
    let code = match lower.as_str() {
        "italia" | "italy" => Some("IT"),
        "germania" | "germany" | "deutschland" => Some("DE"),
        "francia" | "france" => Some("FR"),
        "spagna" | "spain" | "españa" => Some("ES"),
        "stati uniti" | "united states" | "united states of america" | "usa" | "america" => {
            Some("US")
        }
        "regno unito" | "united kingdom" | "uk" | "great britain" | "england" | "inghilterra" => {
            Some("GB")
        }
        "svizzera" | "switzerland" | "schweiz" => Some("CH"),
        "austria" => Some("AT"),
        "olanda" | "paesi bassi" | "netherlands" | "holland" => Some("NL"),
        "belgio" | "belgium" => Some("BE"),
        "brasile" | "brazil" => Some("BR"),
        "canada" => Some("CA"),
        "australia" => Some("AU"),
        "giappone" | "japan" => Some("JP"),
        "cina" | "china" => Some("CN"),
        "russia" | "federazione russa" | "russian federation" => Some("RU"),
        "argentina" => Some("AR"),
        "messico" | "mexico" => Some("MX"),
        "portogallo" | "portugal" => Some("PT"),
        "grecia" | "greece" => Some("GR"),
        "polonia" | "poland" => Some("PL"),
        "svezia" | "sweden" => Some("SE"),
        "norvegia" | "norway" => Some("NO"),
        "danimarca" | "denmark" => Some("DK"),
        "finlandia" | "finland" => Some("FI"),
        "irlanda" | "ireland" => Some("IE"),
        "ucraina" | "ukraine" => Some("UA"),
        "turchia" | "turkey" => Some("TR"),
        "romania" => Some("RO"),
        "ungheria" | "hungary" => Some("HU"),
        "croazia" | "croatia" => Some("HR"),
        "serbia" => Some("RS"),
        "slovenia" => Some("SI"),
        "slovacchia" | "slovakia" => Some("SK"),
        "repubblica ceca" | "cechia" | "czechia" | "czech republic" => Some("CZ"),
        "india" => Some("IN"),
        "marocco" | "morocco" => Some("MA"),
        "egitto" | "egypt" => Some("EG"),
        "tunisia" => Some("TN"),
        "algeria" => Some("DZ"),
        "sudafrica" | "south africa" => Some("ZA"),
        "colombia" => Some("CO"),
        "cile" | "chile" => Some("CL"),
        "peru" | "perù" => Some("PE"),
        "venezuela" => Some("VE"),
        "san marino" => Some("SM"),
        "vaticano" | "vatican" | "vatican city" => Some("VA"),
        "israele" | "israel" => Some("IL"),
        "nuova zelanda" | "new zealand" => Some("NZ"),
        "albania" => Some("AL"),
        "bulgaria" => Some("BG"),
        "bosnia" | "bosnia ed erzegovina" | "bosnia and herzegovina" => Some("BA"),
        "montenegro" => Some("ME"),
        "macedonia" | "macedonia del nord" | "north macedonia" => Some("MK"),
        "islanda" | "iceland" => Some("IS"),
        "cipro" | "cyprus" => Some("CY"),
        "malta" => Some("MT"),
        "lussemburgo" | "luxembourg" => Some("LU"),
        "monaco" => Some("MC"),
        "liechtenstein" => Some("LI"),
        "andorra" => Some("AD"),
        "estonia" => Some("EE"),
        "lettonia" | "latvia" => Some("LV"),
        "lituania" | "lithuania" => Some("LT"),
        "corea del sud" | "south korea" => Some("KR"),
        "taiwan" => Some("TW"),
        "filippine" | "philippines" => Some("PH"),
        "indonesia" => Some("ID"),
        "thailandia" | "thailand" => Some("TH"),
        "vietnam" => Some("VN"),
        "uruguay" => Some("UY"),
        "paraguay" => Some("PY"),
        "ecuador" => Some("EC"),
        "cuba" => Some("CU"),
        "porto rico" | "puerto rico" => Some("PR"),
        _ => None,
    };
    if let Some(code) = code {
        (Some(code.to_string()), None)
    } else {
        (None, Some(trimmed.to_string()))
    }
}

/// Normalizes user language input to Radio Browser language name.
#[must_use]
pub fn resolve_language(input: &str) -> String {
    let lower = input.trim().to_lowercase();
    match lower.as_str() {
        "italiano" | "italian" | "it" => "italian".to_string(),
        "inglese" | "english" | "en" => "english".to_string(),
        "spagnolo" | "español" | "spanish" | "es" => "spanish".to_string(),
        "tedesco" | "deutsch" | "german" | "de" => "german".to_string(),
        "francese" | "français" | "french" | "fr" => "french".to_string(),
        "russo" | "russian" | "ru" => "russian".to_string(),
        "portoghese" | "portuguese" | "pt" => "portuguese".to_string(),
        "olandese" | "dutch" | "nl" => "dutch".to_string(),
        "greco" | "greek" | "el" => "greek".to_string(),
        "polacco" | "polish" | "pl" => "polish".to_string(),
        "arabo" | "arabic" | "ar" => "arabic".to_string(),
        "cinese" | "chinese" | "zh" => "chinese".to_string(),
        "giapponese" | "japanese" | "ja" => "japanese".to_string(),
        "turco" | "turkish" | "tr" => "turkish".to_string(),
        "ucraino" | "ukrainian" | "uk" => "ukrainian".to_string(),
        "rumeno" | "romanian" | "ro" => "romanian".to_string(),
        "ungherese" | "hungarian" | "hu" => "hungarian".to_string(),
        "croato" | "croatian" | "hr" => "croatian".to_string(),
        "svedese" | "swedish" | "sv" => "swedish".to_string(),
        "norvegese" | "norwegian" | "no" => "norwegian".to_string(),
        "danese" | "danish" | "da" => "danish".to_string(),
        "finlandese" | "finnish" | "fi" => "finnish".to_string(),
        "ceco" | "czech" | "cs" => "czech".to_string(),
        "slovacco" | "slovak" | "sk" => "slovak".to_string(),
        "bulgaro" | "bulgarian" | "bg" => "bulgarian".to_string(),
        "serbo" | "serbian" | "sr" => "serbian".to_string(),
        "hindi" | "hi" => "hindi".to_string(),
        other => other.to_string(),
    }
}

/// Data source for station search.
pub trait StationProvider: Send + Sync {
    /// Search stations by name (substring), optional tag, and advanced filters.
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
        filters: &SearchFilters,
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
        filters: &SearchFilters,
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
        if !filters.country.is_empty() {
            let (code, name) = resolve_country(&filters.country);
            if let Some(c) = code {
                params.push(("countrycode", c));
            } else if let Some(n) = name {
                params.push(("country", n));
            }
        }
        if !filters.language.is_empty() {
            params.push(("language", resolve_language(&filters.language)));
        }
        if !filters.codec.is_empty() {
            params.push(("codec", filters.codec.trim().to_lowercase()));
        }
        if filters.min_bitrate > 0 {
            params.push(("bitrateMin", filters.min_bitrate.to_string()));
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

    #[test]
    fn search_filters_is_empty() {
        use super::SearchFilters;
        let default_filters = SearchFilters::default();
        assert!(default_filters.is_empty());

        let with_country = SearchFilters {
            country: "IT".to_string(),
            ..Default::default()
        };
        assert!(!with_country.is_empty());
    }

    #[test]
    fn resolves_country_inputs() {
        use super::resolve_country;
        assert_eq!(resolve_country(""), (None, None));
        assert_eq!(resolve_country("  "), (None, None));
        assert_eq!(resolve_country("it"), (Some("IT".to_string()), None));
        assert_eq!(resolve_country("IT"), (Some("IT".to_string()), None));
        assert_eq!(resolve_country("uk"), (Some("GB".to_string()), None));
        assert_eq!(resolve_country("Italy"), (Some("IT".to_string()), None));
        assert_eq!(resolve_country("italia"), (Some("IT".to_string()), None));
        assert_eq!(resolve_country("germany"), (Some("DE".to_string()), None));
        assert_eq!(resolve_country("germania"), (Some("DE".to_string()), None));
        assert_eq!(
            resolve_country("United States"),
            (Some("US".to_string()), None)
        );
        assert_eq!(resolve_country("usa"), (Some("US".to_string()), None));
        assert_eq!(
            resolve_country("CustomCountry"),
            (None, Some("CustomCountry".to_string()))
        );
    }

    #[test]
    fn resolves_language_inputs() {
        use super::resolve_language;
        assert_eq!(resolve_language("Italian"), "italian");
        assert_eq!(resolve_language("italiano"), "italian");
        assert_eq!(resolve_language("IT"), "italian");
        assert_eq!(resolve_language("English"), "english");
        assert_eq!(resolve_language("inglese"), "english");
        assert_eq!(resolve_language("EN"), "english");
        assert_eq!(resolve_language("french"), "french");
        assert_eq!(resolve_language("francese"), "french");
    }
}
