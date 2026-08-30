//! Country centroid lookup for the world map.

use crate::radio::Station;

/// Returns `(lon, lat)` for the station, preferring exact geo coords,
/// then country code centroid, then country name fallback.
#[must_use]
pub fn station_coords(station: &Station) -> Option<(f64, f64)> {
    if let (Some(lat), Some(lon)) = (station.geo_lat, station.geo_long) {
        if lat != 0.0 || lon != 0.0 {
            return Some((lon, lat));
        }
    }
    if !station.countrycode.is_empty() {
        if let Some(pos) = country_coords(&station.countrycode) {
            return Some(pos);
        }
    }
    if !station.country.is_empty() {
        if let Some(pos) = country_coords_by_name(&station.country) {
            return Some(pos);
        }
    }
    None
}

/// Lookup by ISO 3166-1 alpha-2 code (case-insensitive).
#[must_use]
pub fn country_coords(code: &str) -> Option<(f64, f64)> {
    let key = code.trim().to_ascii_uppercase();
    for (c, lon, lat) in COUNTRY_CENTROIDS {
        if *c == key {
            return Some((*lon, *lat));
        }
    }
    None
}

fn country_coords_by_name(name: &str) -> Option<(f64, f64)> {
    let key = name.trim().to_lowercase();
    for (n, lon, lat) in COUNTRY_NAME_CENTROIDS {
        if *n == key {
            return Some((*lon, *lat));
        }
    }
    // Fallback: match common names that differ from codes
    match key.as_str() {
        "united states" | "united states of america" | "the united states of america" | "usa" => {
            country_coords("US")
        }
        "united kingdom" | "uk" | "great britain" | "england" => country_coords("GB"),
        "russia" | "russian federation" => country_coords("RU"),
        "south korea" | "korea, republic of" => country_coords("KR"),
        "north korea" => country_coords("KP"),
        "iran" => country_coords("IR"),
        "syria" => country_coords("SY"),
        "venezuela" => country_coords("VE"),
        "bolivia" => country_coords("BO"),
        "tanzania" => country_coords("TZ"),
        "moldova" => country_coords("MD"),
        "vatican" | "vatican city" => country_coords("VA"),
        _ => None,
    }
}

/// Centroids as `(code, lon, lat)` — lon first for Canvas.
const COUNTRY_CENTROIDS: &[(&str, f64, f64)] = &[
    ("AD", 1.52, 42.51),
    ("AE", 53.85, 23.42),
    ("AF", 67.71, 33.94),
    ("AL", 20.17, 41.15),
    ("AM", 45.04, 40.07),
    ("AO", 17.87, -11.20),
    ("AR", -63.62, -38.42),
    ("AT", 14.55, 47.52),
    ("AU", 133.77, -25.27),
    ("AZ", 47.58, 40.14),
    ("BA", 17.68, 43.92),
    ("BD", 90.41, 23.68),
    ("BE", 4.47, 50.50),
    ("BF", -1.56, 12.24),
    ("BG", 25.49, 42.73),
    ("BH", 50.56, 26.07),
    ("BI", 29.92, -3.37),
    ("BJ", 2.32, 9.31),
    ("BN", 114.73, 4.54),
    ("BO", -64.0, -16.29),
    ("BR", -51.93, -14.24),
    ("BS", -77.40, 25.03),
    ("BT", 90.43, 27.51),
    ("BW", 24.68, -22.33),
    ("BY", 27.95, 44.94),
    ("BZ", -88.49, 17.19),
    ("CA", -106.35, 56.13),
    ("CH", 8.23, 46.82),
    ("CL", -71.54, -35.68),
    ("CM", 12.35, 7.37),
    ("CN", 104.20, 35.86),
    ("CO", -74.30, 4.57),
    ("CR", -83.75, 9.75),
    ("CU", -77.78, 21.52),
    ("CY", 33.43, 35.13),
    ("CZ", 15.47, 49.82),
    ("DE", 10.45, 51.17),
    ("DK", 9.50, 56.26),
    ("DZ", 1.66, 28.03),
    ("EC", -78.18, -1.83),
    ("EE", 26.00, 58.60),
    ("EG", 30.80, 26.82),
    ("ES", -3.75, 40.46),
    ("FI", 25.75, 61.92),
    ("FR", 2.21, 46.23),
    ("GB", -3.44, 55.37),
    ("GE", 43.36, 42.32),
    ("GH", -1.02, 7.95),
    ("GR", 21.82, 39.07),
    ("GT", -90.23, 15.78),
    ("HK", 114.11, 22.40),
    ("HN", -86.24, 15.20),
    ("HR", 15.20, 45.10),
    ("HU", 19.50, 47.16),
    ("ID", 113.92, -0.79),
    ("IE", -8.24, 53.41),
    ("IL", 34.85, 31.05),
    ("IN", 78.96, 20.59),
    ("IQ", 43.68, 33.22),
    ("IR", 53.69, 32.43),
    ("IS", -19.62, 64.96),
    ("IT", 12.57, 41.87),
    ("JM", -77.30, 18.11),
    ("JO", 36.24, 30.59),
    ("JP", 138.25, 36.20),
    ("KE", 37.91, -0.02),
    ("KG", 74.77, 41.20),
    ("KH", 104.99, 12.56),
    ("KP", 127.51, 40.34),
    ("KR", 127.77, 35.91),
    ("KW", 47.48, 29.31),
    ("KZ", 66.92, 48.02),
    ("LB", 35.86, 33.85),
    ("LK", 80.77, 7.87),
    ("LT", 23.88, 55.17),
    ("LU", 6.13, 49.82),
    ("LV", 24.60, 56.88),
    ("LY", 17.23, 26.33),
    ("MA", -7.09, 31.79),
    ("MD", 28.37, 47.41),
    ("ME", 19.37, 42.71),
    ("MK", 21.75, 41.61),
    ("ML", -3.996, 17.57),
    ("MM", 95.96, 21.91),
    ("MN", 103.85, 46.86),
    ("MT", 14.38, 35.94),
    ("MX", -102.55, 23.63),
    ("MY", 101.98, 4.21),
    ("MZ", 35.53, -18.66),
    ("NA", 18.49, -22.96),
    ("NG", 8.68, 9.08),
    ("NI", -85.20, 12.86),
    ("NL", 5.29, 52.13),
    ("NO", 8.47, 60.47),
    ("NP", 84.12, 28.39),
    ("NZ", 174.89, -40.90),
    ("OM", 55.92, 21.51),
    ("PA", -80.78, 8.54),
    ("PE", -75.02, -9.19),
    ("PH", 121.77, 12.88),
    ("PK", 69.35, 30.37),
    ("PL", 19.15, 51.92),
    ("PT", -8.22, 39.40),
    ("PY", -58.44, -23.44),
    ("QA", 51.18, 25.35),
    ("RO", 24.97, 45.94),
    ("RS", 21.00, 44.02),
    ("RU", 105.32, 61.52),
    ("SA", 45.08, 23.89),
    ("SE", 18.64, 60.13),
    ("SG", 103.82, 1.35),
    ("SI", 14.99, 46.15),
    ("SK", 19.70, 48.67),
    ("SN", -14.45, 14.50),
    ("SV", -88.90, 13.79),
    ("SY", 38.00, 34.80),
    ("TH", 100.99, 15.87),
    ("TN", 9.54, 33.89),
    ("TR", 35.24, 38.96),
    ("TW", 120.96, 23.70),
    ("TZ", 34.89, -6.37),
    ("UA", 31.17, 48.38),
    ("UG", 32.29, 1.37),
    ("US", -98.58, 39.83),
    ("UY", -55.77, -32.52),
    ("UZ", 64.59, 41.38),
    ("VA", 12.45, 41.90),
    ("VE", -66.59, 6.42),
    ("VN", 108.28, 14.06),
    ("YE", 48.52, 15.55),
    ("ZA", 22.94, -30.56),
    ("ZM", 27.85, -13.13),
    ("ZW", 29.15, -19.02),
];

const COUNTRY_NAME_CENTROIDS: &[(&str, f64, f64)] = &[
    ("andorra", 1.52, 42.51),
    ("united arab emirates", 53.85, 23.42),
    ("afghanistan", 67.71, 33.94),
    ("albania", 20.17, 41.15),
    ("armenia", 45.04, 40.07),
    ("angola", 17.87, -11.20),
    ("argentina", -63.62, -38.42),
    ("austria", 14.55, 47.52),
    ("australia", 133.77, -25.27),
    ("azerbaijan", 47.58, 40.14),
    ("bosnia and herzegovina", 17.68, 43.92),
    ("bangladesh", 90.41, 23.68),
    ("belgium", 4.47, 50.50),
    ("burkina faso", -1.56, 12.24),
    ("bulgaria", 25.49, 42.73),
    ("bahrain", 50.56, 26.07),
    ("burundi", 29.92, -3.37),
    ("benin", 2.32, 9.31),
    ("brunei", 114.73, 4.54),
    ("bolivia", -64.0, -16.29),
    ("brazil", -51.93, -14.24),
    ("bahamas", -77.40, 25.03),
    ("bhutan", 90.43, 27.51),
    ("botswana", 24.68, -22.33),
    ("belarus", 27.95, 44.94),
    ("belize", -88.49, 17.19),
    ("canada", -106.35, 56.13),
    ("switzerland", 8.23, 46.82),
    ("chile", -71.54, -35.68),
    ("cameroon", 12.35, 7.37),
    ("china", 104.20, 35.86),
    ("colombia", -74.30, 4.57),
    ("costa rica", -83.75, 9.75),
    ("cuba", -77.78, 21.52),
    ("cyprus", 33.43, 35.13),
    ("czech republic", 15.47, 49.82),
    ("czechia", 15.47, 49.82),
    ("germany", 10.45, 51.17),
    ("denmark", 9.50, 56.26),
    ("algeria", 1.66, 28.03),
    ("ecuador", -78.18, -1.83),
    ("estonia", 26.00, 58.60),
    ("egypt", 30.80, 26.82),
    ("spain", -3.75, 40.46),
    ("finland", 25.75, 61.92),
    ("france", 2.21, 46.23),
    ("united kingdom", -3.44, 55.37),
    ("georgia", 43.36, 42.32),
    ("ghana", -1.02, 7.95),
    ("greece", 21.82, 39.07),
    ("guatemala", -90.23, 15.78),
    ("hong kong", 114.11, 22.40),
    ("honduras", -86.24, 15.20),
    ("croatia", 15.20, 45.10),
    ("hungary", 19.50, 47.16),
    ("indonesia", 113.92, -0.79),
    ("ireland", -8.24, 53.41),
    ("israel", 34.85, 31.05),
    ("india", 78.96, 20.59),
    ("iraq", 43.68, 33.22),
    ("iran", 53.69, 32.43),
    ("iceland", -19.62, 64.96),
    ("italy", 12.57, 41.87),
    ("jamaica", -77.30, 18.11),
    ("jordan", 36.24, 30.59),
    ("japan", 138.25, 36.20),
    ("kenya", 37.91, -0.02),
    ("kyrgyzstan", 74.77, 41.20),
    ("cambodia", 104.99, 12.56),
    ("north korea", 127.51, 40.34),
    ("south korea", 127.77, 35.91),
    ("korea", 127.77, 35.91),
    ("kuwait", 47.48, 29.31),
    ("kazakhstan", 66.92, 48.02),
    ("lebanon", 35.86, 33.85),
    ("sri lanka", 80.77, 7.87),
    ("lithuania", 23.88, 55.17),
    ("luxembourg", 6.13, 49.82),
    ("latvia", 24.60, 56.88),
    ("libya", 17.23, 26.33),
    ("morocco", -7.09, 31.79),
    ("moldova", 28.37, 47.41),
    ("montenegro", 19.37, 42.71),
    ("north macedonia", 21.75, 41.61),
    ("macedonia", 21.75, 41.61),
    ("mali", -3.996, 17.57),
    ("myanmar", 95.96, 21.91),
    ("mongolia", 103.85, 46.86),
    ("malta", 14.38, 35.94),
    ("mexico", -102.55, 23.63),
    ("malaysia", 101.98, 4.21),
    ("mozambique", 35.53, -18.66),
    ("namibia", 18.49, -22.96),
    ("nigeria", 8.68, 9.08),
    ("nicaragua", -85.20, 12.86),
    ("netherlands", 5.29, 52.13),
    ("norway", 8.47, 60.47),
    ("nepal", 84.12, 28.39),
    ("new zealand", 174.89, -40.90),
    ("oman", 55.92, 21.51),
    ("panama", -80.78, 8.54),
    ("peru", -75.02, -9.19),
    ("philippines", 121.77, 12.88),
    ("pakistan", 69.35, 30.37),
    ("poland", 19.15, 51.92),
    ("portugal", -8.22, 39.40),
    ("paraguay", -58.44, -23.44),
    ("qatar", 51.18, 25.35),
    ("romania", 24.97, 45.94),
    ("serbia", 21.00, 44.02),
    ("russia", 105.32, 61.52),
    ("saudi arabia", 45.08, 23.89),
    ("sweden", 18.64, 60.13),
    ("singapore", 103.82, 1.35),
    ("slovenia", 14.99, 46.15),
    ("slovakia", 19.70, 48.67),
    ("senegal", -14.45, 14.50),
    ("el salvador", -88.90, 13.79),
    ("syria", 38.00, 34.80),
    ("thailand", 100.99, 15.87),
    ("tunisia", 9.54, 33.89),
    ("turkey", 35.24, 38.96),
    ("taiwan", 120.96, 23.70),
    ("tanzania", 34.89, -6.37),
    ("ukraine", 31.17, 48.38),
    ("uganda", 32.29, 1.37),
    ("united states", -98.58, 39.83),
    ("uruguay", -55.77, -32.52),
    ("uzbekistan", 64.59, 41.38),
    ("vatican", 12.45, 41.90),
    ("venezuela", -66.59, 6.42),
    ("vietnam", 108.28, 14.06),
    ("yemen", 48.52, 15.55),
    ("south africa", 22.94, -30.56),
    ("zambia", 27.85, -13.13),
    ("zimbabwe", 29.15, -19.02),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_by_code() {
        assert_eq!(country_coords("IT"), Some((12.57, 41.87)));
        assert_eq!(country_coords("us"), Some((-98.58, 39.83)));
    }

    #[test]
    fn lookup_by_name() {
        let s = crate::radio::Station {
            id: "x".into(),
            name: "Test".into(),
            url_resolved: String::new(),
            url: String::new(),
            favicon: String::new(),
            homepage: String::new(),
            country: "Italy".into(),
            countrycode: String::new(),
            geo_lat: None,
            geo_long: None,
            state: String::new(),
            language: String::new(),
            codec: String::new(),
            bitrate: 0,
            tags: vec![],
            votes: 0,
            hls: false,
        };
        assert!(station_coords(&s).is_some());
    }

    #[test]
    fn prefers_geo_coords() {
        let s = crate::radio::Station {
            id: "x".into(),
            name: "Test".into(),
            url_resolved: String::new(),
            url: String::new(),
            favicon: String::new(),
            homepage: String::new(),
            country: "Italy".into(),
            countrycode: "IT".into(),
            geo_lat: Some(45.0),
            geo_long: Some(9.0),
            state: String::new(),
            language: String::new(),
            codec: String::new(),
            bitrate: 0,
            tags: vec![],
            votes: 0,
            hls: false,
        };
        assert_eq!(station_coords(&s), Some((9.0, 45.0)));
    }
}
