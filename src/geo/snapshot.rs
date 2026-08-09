use std::collections::{
    BTreeMap,
    HashSet,
};

/// A normalized Country Code and the number of Peers assigned to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountryCount {
    /// Normalized two-letter Country Code.
    pub country_code: String,
    /// Number of IP-deduplicated Peers assigned to the Country Code.
    pub peer_count: usize,
}

/// A peer whose cached location contains enough valid data to be plotted on
/// the legacy map view. The UI never displays the exact coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct LocatedPeer {
    pub ip: String,
    pub country: String,
    pub lat: f64,
    pub lng: f64,
}

/// Immutable read-only view assembled from the current Peer Snapshot and the
/// Location Cache, used for rendering the Peer Country Distribution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GeoViewSnapshot {
    /// Total number of IP-deduplicated Peers in the current Peer Snapshot.
    pub total_peers: usize,
    /// Number of Peers with a valid plottable `loc`.
    pub located_peers: usize,
    /// Number of known Country Codes in `country_counts`.
    pub unique_countries: usize,
    /// Ordered known Country Codes and their Peer counts.
    pub country_counts: Vec<CountryCount>,
    /// Number of Peers classified as Unknown Country.
    pub unknown_country_count: usize,
    /// Owned data for every plottable peer, sorted by IP.
    pub peers: Vec<LocatedPeer>,
}

/// Normalize a provider country value into the Country Code contract.
///
/// Only exactly two ASCII letters are accepted after trimming. The returned
/// Country Code is always uppercase.
pub(crate) fn normalize_country_code(country: &str) -> Option<String> {
    let country = country.trim();
    if country.len() != 2 || !country.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return None;
    }

    Some(country.to_ascii_uppercase())
}

/// Parse an IPinfo `loc` value ("lat,lng") into coordinates.
pub(crate) fn parse_loc(loc: &str) -> Option<(f64, f64)> {
    let (lat, lng) = loc.split_once(',')?;
    let lat = lat.trim().parse::<f64>().ok()?;
    let lng = lng.trim().parse::<f64>().ok()?;
    if !lat.is_finite() || !lng.is_finite() {
        return None;
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lng) {
        return None;
    }
    Some((lat, lng))
}

/// Assemble a Geo View Snapshot from `(ip, country, loc)` join rows.
///
/// Every IP is counted once. A peer with a valid Country Code but no usable
/// `loc` contributes to the country count without being plotted; a peer with a
/// valid `loc` is retained in the legacy map data.
pub(crate) fn assemble_snapshot(rows: &[(String, String, String)]) -> GeoViewSnapshot {
    let mut seen_ips = HashSet::new();
    let mut country_totals = BTreeMap::new();
    let mut unknown_country_count = 0;
    let mut peers = Vec::new();

    for (ip, country, loc) in rows {
        if !seen_ips.insert(ip.clone()) {
            continue;
        }

        let country_code = normalize_country_code(country);
        if let Some(country_code) = &country_code {
            *country_totals.entry(country_code.clone()).or_insert(0) += 1;
        } else {
            unknown_country_count += 1;
        }

        if let Some((lat, lng)) = parse_loc(loc) {
            peers.push(LocatedPeer {
                ip: ip.clone(),
                country: country_code.unwrap_or_default(),
                lat,
                lng,
            });
        }
    }

    peers.sort_by(|left, right| left.ip.cmp(&right.ip));

    let mut country_counts: Vec<CountryCount> = country_totals
        .into_iter()
        .map(|(country_code, peer_count)| CountryCount {
            country_code,
            peer_count,
        })
        .collect();
    country_counts.sort_by(|left, right| {
        right
            .peer_count
            .cmp(&left.peer_count)
            .then_with(|| left.country_code.cmp(&right.country_code))
    });

    GeoViewSnapshot {
        total_peers: seen_ips.len(),
        located_peers: peers.len(),
        unique_countries: country_counts.len(),
        country_counts,
        unknown_country_count,
        peers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_country_code_accepts_trimmed_ascii_letters() {
        assert_eq!(normalize_country_code(" cn "), Some("CN".to_string()));
        assert_eq!(normalize_country_code("Us"), Some("US".to_string()));
    }

    #[test]
    fn test_normalize_country_code_rejects_invalid_values() {
        for country in ["", "C", "USA", "12", "C1", "中", "CN!", "C N"] {
            assert_eq!(normalize_country_code(country), None, "country: {country:?}");
        }
    }

    #[test]
    fn test_parse_loc_accepts_valid_coordinates() {
        assert_eq!(parse_loc("39.9042,116.4074"), Some((39.9042, 116.4074)));
        assert_eq!(parse_loc(" -33.4940 , 143.2104 "), Some((-33.494, 143.2104)));
    }

    #[test]
    fn test_parse_loc_rejects_malformed_values() {
        assert_eq!(parse_loc(""), None);
        assert_eq!(parse_loc("abc,def"), None);
        assert_eq!(parse_loc("39.9042"), None);
        assert_eq!(parse_loc("91.0,0.0"), None);
        assert_eq!(parse_loc("0.0,181.0"), None);
        assert_eq!(parse_loc("NaN,0.0"), None);
    }

    #[test]
    fn test_assemble_snapshot_counts_countries_without_loc_and_unknown_peers() {
        let rows = vec![
            ("1.1.1.1".to_string(), "CN".to_string(), "39.9042,116.4074".to_string()),
            ("2.2.2.2".to_string(), " us ".to_string(), "".to_string()),
            ("3.3.3.3".to_string(), "not-a-code".to_string(), "".to_string()),
        ];

        let snapshot = assemble_snapshot(&rows);

        assert_eq!(snapshot.total_peers, 3);
        assert_eq!(snapshot.located_peers, 1);
        assert_eq!(snapshot.unique_countries, 2);
        assert_eq!(snapshot.unknown_country_count, 1);
        assert_eq!(snapshot.peers.len(), 1);
        assert_eq!(snapshot.peers[0].ip, "1.1.1.1");
        assert_eq!(snapshot.peers[0].country, "CN");
        assert_eq!(
            snapshot.country_counts,
            vec![
                CountryCount {
                    country_code: "CN".to_string(),
                    peer_count: 1,
                },
                CountryCount {
                    country_code: "US".to_string(),
                    peer_count: 1,
                },
            ]
        );
    }

    #[test]
    fn test_assemble_snapshot_sorts_country_counts_by_count_then_code() {
        let rows = vec![
            ("1.1.1.1".to_string(), "US".to_string(), "".to_string()),
            ("2.2.2.2".to_string(), "DE".to_string(), "".to_string()),
            ("3.3.3.3".to_string(), "us".to_string(), "".to_string()),
            ("4.4.4.4".to_string(), "CN".to_string(), "".to_string()),
            ("5.5.5.5".to_string(), "DE".to_string(), "".to_string()),
        ];

        let snapshot = assemble_snapshot(&rows);

        assert_eq!(
            snapshot
                .country_counts
                .iter()
                .map(|country| (country.country_code.as_str(), country.peer_count))
                .collect::<Vec<_>>(),
            vec![("DE", 2), ("US", 2), ("CN", 1)]
        );
    }

    #[test]
    fn test_assemble_snapshot_deduplicates_ips_before_counting() {
        let rows = vec![
            ("1.1.1.1".to_string(), "CN".to_string(), "".to_string()),
            ("1.1.1.1".to_string(), "US".to_string(), "1.0,2.0".to_string()),
            ("2.2.2.2".to_string(), "US".to_string(), "".to_string()),
        ];

        let snapshot = assemble_snapshot(&rows);

        assert_eq!(snapshot.total_peers, 2);
        assert_eq!(snapshot.unknown_country_count, 0);
        assert_eq!(
            snapshot
                .country_counts
                .iter()
                .map(|country| (country.country_code.as_str(), country.peer_count))
                .collect::<Vec<_>>(),
            vec![("CN", 1), ("US", 1)]
        );
        assert_eq!(snapshot.located_peers, 0);
    }

    #[test]
    fn test_assemble_snapshot_sorts_peers_by_ip() {
        let rows = vec![
            ("9.9.9.9".to_string(), "US".to_string(), "1.0,2.0".to_string()),
            ("1.1.1.1".to_string(), "CN".to_string(), "3.0,4.0".to_string()),
        ];

        let snapshot = assemble_snapshot(&rows);

        assert_eq!(
            snapshot.peers.iter().map(|peer| peer.ip.as_str()).collect::<Vec<_>>(),
            vec!["1.1.1.1", "9.9.9.9"]
        );
    }

    #[test]
    fn test_assemble_snapshot_empty_rows_produce_empty_snapshot() {
        let snapshot = assemble_snapshot(&[]);

        assert_eq!(snapshot, GeoViewSnapshot::default());
    }
}
