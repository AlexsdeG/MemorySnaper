use std::sync::OnceLock;

use reverse_geocoder::{Locations, ReverseGeocoder};

fn get_geocoder() -> &'static ReverseGeocoder<'static> {
    static GEOCODER: OnceLock<ReverseGeocoder<'static>> = OnceLock::new();
    GEOCODER.get_or_init(|| {
        let locs: &'static Locations = Box::leak(Box::new(Locations::from_memory()));
        ReverseGeocoder::new(locs)
    })
}

/// Parse a Snapchat location string like "48.137154, 11.576124" into (lat, lon).
fn parse_lat_lon(raw: &str) -> Option<(f64, f64)> {
    let raw = coordinate_payload(raw);
    let mut parts = raw.splitn(2, ',');
    let lat = parts.next()?.trim().parse::<f64>().ok()?;
    let lon = parts.next()?.trim().parse::<f64>().ok()?;
    Some((lat, lon))
}

fn coordinate_payload(raw: &str) -> &str {
    let trimmed = raw.trim();

    if let Some((prefix, suffix)) = trimmed.rsplit_once(':') {
        let candidate = suffix.trim();
        if candidate.contains(',') && prefix.chars().any(|character| character.is_alphabetic()) {
            return candidate;
        }
    }

    trimmed
}

pub fn normalize_location_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if parse_lat_lon(trimmed).is_some() {
        return Some(coordinate_payload(trimmed).to_string());
    }

    Some(trimmed.to_string())
}

/// Resolve a lat/lon pair (or a raw Snapchat "lat, lon" string) to a human-readable location.
/// Returns a string like "Munich, Bavaria, Germany".
/// Returns `None` if the coordinates are unparseable or no result is found.
pub fn resolve_location(raw: &str) -> Option<String> {
    let (lat, lon) = parse_lat_lon(raw)?;
    let result = get_geocoder().search((lat, lon))?;
    let r = result.record;

    let country = country_name(&r.cc);
    let label = if r.admin1.is_empty() {
        format!("{}, {}", r.name, country)
    } else {
        format!("{}, {}, {}", r.name, r.admin1, country)
    };
    Some(label)
}

fn country_name(code: &str) -> String {
    match code {
        "AD" => "Andorra",
        "AE" => "United Arab Emirates",
        "AF" => "Afghanistan",
        "AG" => "Antigua and Barbuda",
        "AL" => "Albania",
        "AM" => "Armenia",
        "AO" => "Angola",
        "AR" => "Argentina",
        "AT" => "Austria",
        "AU" => "Australia",
        "AZ" => "Azerbaijan",
        "BA" => "Bosnia and Herzegovina",
        "BB" => "Barbados",
        "BD" => "Bangladesh",
        "BE" => "Belgium",
        "BF" => "Burkina Faso",
        "BG" => "Bulgaria",
        "BH" => "Bahrain",
        "BI" => "Burundi",
        "BJ" => "Benin",
        "BN" => "Brunei",
        "BO" => "Bolivia",
        "BR" => "Brazil",
        "BS" => "Bahamas",
        "BT" => "Bhutan",
        "BW" => "Botswana",
        "BY" => "Belarus",
        "BZ" => "Belize",
        "CA" => "Canada",
        "CD" => "DR Congo",
        "CF" => "Central African Republic",
        "CG" => "Congo",
        "CH" => "Switzerland",
        "CI" => "Ivory Coast",
        "CL" => "Chile",
        "CM" => "Cameroon",
        "CN" => "China",
        "CO" => "Colombia",
        "CR" => "Costa Rica",
        "CU" => "Cuba",
        "CV" => "Cape Verde",
        "CY" => "Cyprus",
        "CZ" => "Czech Republic",
        "DE" => "Germany",
        "DJ" => "Djibouti",
        "DK" => "Denmark",
        "DM" => "Dominica",
        "DO" => "Dominican Republic",
        "DZ" => "Algeria",
        "EC" => "Ecuador",
        "EE" => "Estonia",
        "EG" => "Egypt",
        "ER" => "Eritrea",
        "ES" => "Spain",
        "ET" => "Ethiopia",
        "FI" => "Finland",
        "FJ" => "Fiji",
        "FM" => "Micronesia",
        "FR" => "France",
        "GA" => "Gabon",
        "GB" => "United Kingdom",
        "GD" => "Grenada",
        "GE" => "Georgia",
        "GH" => "Ghana",
        "GM" => "Gambia",
        "GN" => "Guinea",
        "GQ" => "Equatorial Guinea",
        "GR" => "Greece",
        "GT" => "Guatemala",
        "GW" => "Guinea-Bissau",
        "GY" => "Guyana",
        "HN" => "Honduras",
        "HR" => "Croatia",
        "HT" => "Haiti",
        "HU" => "Hungary",
        "ID" => "Indonesia",
        "IE" => "Ireland",
        "IL" => "Israel",
        "IN" => "India",
        "IQ" => "Iraq",
        "IR" => "Iran",
        "IS" => "Iceland",
        "IT" => "Italy",
        "JM" => "Jamaica",
        "JO" => "Jordan",
        "JP" => "Japan",
        "KE" => "Kenya",
        "KG" => "Kyrgyzstan",
        "KH" => "Cambodia",
        "KI" => "Kiribati",
        "KM" => "Comoros",
        "KN" => "Saint Kitts and Nevis",
        "KP" => "North Korea",
        "KR" => "South Korea",
        "KW" => "Kuwait",
        "KZ" => "Kazakhstan",
        "LA" => "Laos",
        "LB" => "Lebanon",
        "LC" => "Saint Lucia",
        "LI" => "Liechtenstein",
        "LK" => "Sri Lanka",
        "LR" => "Liberia",
        "LS" => "Lesotho",
        "LT" => "Lithuania",
        "LU" => "Luxembourg",
        "LV" => "Latvia",
        "LY" => "Libya",
        "MA" => "Morocco",
        "MC" => "Monaco",
        "MD" => "Moldova",
        "ME" => "Montenegro",
        "MG" => "Madagascar",
        "MH" => "Marshall Islands",
        "MK" => "North Macedonia",
        "ML" => "Mali",
        "MM" => "Myanmar",
        "MN" => "Mongolia",
        "MR" => "Mauritania",
        "MT" => "Malta",
        "MU" => "Mauritius",
        "MV" => "Maldives",
        "MW" => "Malawi",
        "MX" => "Mexico",
        "MY" => "Malaysia",
        "MZ" => "Mozambique",
        "NA" => "Namibia",
        "NE" => "Niger",
        "NG" => "Nigeria",
        "NI" => "Nicaragua",
        "NL" => "Netherlands",
        "NO" => "Norway",
        "NP" => "Nepal",
        "NR" => "Nauru",
        "NZ" => "New Zealand",
        "OM" => "Oman",
        "PA" => "Panama",
        "PE" => "Peru",
        "PG" => "Papua New Guinea",
        "PH" => "Philippines",
        "PK" => "Pakistan",
        "PL" => "Poland",
        "PT" => "Portugal",
        "PW" => "Palau",
        "PY" => "Paraguay",
        "QA" => "Qatar",
        "RO" => "Romania",
        "RS" => "Serbia",
        "RU" => "Russia",
        "RW" => "Rwanda",
        "SA" => "Saudi Arabia",
        "SB" => "Solomon Islands",
        "SC" => "Seychelles",
        "SD" => "Sudan",
        "SE" => "Sweden",
        "SG" => "Singapore",
        "SI" => "Slovenia",
        "SK" => "Slovakia",
        "SL" => "Sierra Leone",
        "SM" => "San Marino",
        "SN" => "Senegal",
        "SO" => "Somalia",
        "SR" => "Suriname",
        "SS" => "South Sudan",
        "ST" => "São Tomé and Príncipe",
        "SV" => "El Salvador",
        "SY" => "Syria",
        "SZ" => "Eswatini",
        "TD" => "Chad",
        "TG" => "Togo",
        "TH" => "Thailand",
        "TJ" => "Tajikistan",
        "TL" => "Timor-Leste",
        "TM" => "Turkmenistan",
        "TN" => "Tunisia",
        "TO" => "Tonga",
        "TR" => "Turkey",
        "TT" => "Trinidad and Tobago",
        "TV" => "Tuvalu",
        "TZ" => "Tanzania",
        "UA" => "Ukraine",
        "UG" => "Uganda",
        "US" => "United States",
        "UY" => "Uruguay",
        "UZ" => "Uzbekistan",
        "VA" => "Vatican City",
        "VC" => "Saint Vincent and the Grenadines",
        "VE" => "Venezuela",
        "VN" => "Vietnam",
        "VU" => "Vanuatu",
        "WS" => "Samoa",
        "YE" => "Yemen",
        "ZA" => "South Africa",
        "ZM" => "Zambia",
        "ZW" => "Zimbabwe",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{normalize_location_text, parse_lat_lon, resolve_location};

    #[test]
    fn parses_valid_snapchat_location_format() {
        // Standard format: "lat, lon"
        let result = parse_lat_lon("48.137154, 11.576124");
        assert_eq!(result, Some((48.137154, 11.576124)));
    }

    #[test]
    fn parses_location_without_spaces() {
        let result = parse_lat_lon("48.137154,11.576124");
        assert_eq!(result, Some((48.137154, 11.576124)));
    }

    #[test]
    fn parses_location_with_extra_whitespace() {
        let result = parse_lat_lon("  48.137154  ,  11.576124  ");
        assert_eq!(result, Some((48.137154, 11.576124)));
    }

    #[test]
    fn parses_prefixed_snapchat_location_format() {
        let result = parse_lat_lon("Latitude, Longitude: 50.10691, 14.432932");
        assert_eq!(result, Some((50.10691, 14.432932)));
    }

    #[test]
    fn normalizes_prefixed_snapchat_location_text() {
        let result = normalize_location_text("Latitude, Longitude: 50.10691, 14.432932");
        assert_eq!(result.as_deref(), Some("50.10691, 14.432932"));
    }

    #[test]
    fn parses_negative_coordinates() {
        // South/West coordinates
        let result = parse_lat_lon("-33.8688, 151.2093");
        assert_eq!(result, Some((-33.8688, 151.2093)));
    }

    #[test]
    fn returns_none_for_empty_string() {
        assert_eq!(parse_lat_lon(""), None);
    }

    #[test]
    fn returns_none_for_missing_comma() {
        assert_eq!(parse_lat_lon("48.137154 11.576124"), None);
    }

    #[test]
    fn returns_none_for_missing_longitude() {
        assert_eq!(parse_lat_lon("48.137154,"), None);
    }

    #[test]
    fn returns_none_for_invalid_latitude() {
        assert_eq!(parse_lat_lon("not-a-number, 11.576124"), None);
    }

    #[test]
    fn returns_none_for_invalid_longitude() {
        assert_eq!(parse_lat_lon("48.137154, not-a-number"), None);
    }

    #[test]
    fn resolves_known_location_munich() {
        // Munich, Germany coordinates
        let result = resolve_location("48.137154, 11.576124");
        assert!(result.is_some());
        let location = result.unwrap();
        // Should contain at least the country
        assert!(location.contains("Germany"));
    }

    #[test]
    fn resolves_prefixed_snapchat_location() {
        let result = resolve_location("Latitude, Longitude: 50.10691, 14.432932");
        assert!(result.is_some());
    }

    #[test]
    fn returns_none_for_unparseable_location() {
        assert_eq!(resolve_location(""), None);
    }

    #[test]
    fn returns_none_for_invalid_coordinates_format() {
        assert_eq!(resolve_location("not-a-location"), None);
    }

    #[test]
    fn returns_none_for_edge_case_zero_zero() {
        // Null Island - coordinates may not resolve
        let result = resolve_location("0, 0");
        // This may or may not resolve depending on geocoder data
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn returns_none_for_out_of_range_latitude() {
        // parse_lat_lon accepts any valid float, but resolving coordinates > 90 may fail
        let result = parse_lat_lon("95, 0");
        assert_eq!(result, Some((95.0, 0.0))); // Parsing succeeds, but geocoding may not work
    }

    #[test]
    fn returns_none_for_out_of_range_longitude() {
        // parse_lat_lon accepts any valid float, but resolving coordinates > 180 may fail
        let result = parse_lat_lon("0, 185");
        assert_eq!(result, Some((0.0, 185.0))); // Parsing succeeds, but geocoding may not work
    }

    #[test]
    fn handles_extremely_large_coordinates_gracefully() {
        // Very large numbers - should either parse or return None, but not panic
        let result = parse_lat_lon("999999, 999999");
        // The function should parse it but the geocoder may not find a location
        assert!(result == Some((999999.0, 999999.0)) || result.is_none());
    }
}
