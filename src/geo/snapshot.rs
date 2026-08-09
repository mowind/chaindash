use std::collections::HashSet;

/// A peer whose cached location contains enough valid data to be plotted on
/// the Peer Map. The UI never displays the exact coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct LocatedPeer {
    pub ip: String,
    pub country: String,
    pub lat: f64,
    pub lng: f64,
}

/// Immutable read-only view assembled from the current Peer Snapshot and the
/// Location Cache, used for rendering the Peer Map.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GeoViewSnapshot {
    /// Total number of peers in the current Peer Snapshot.
    pub total_peers: usize,
    /// Number of peers with a valid plottable `loc`.
    pub located_peers: usize,
    /// Number of unique countries among peers with a valid country field.
    pub unique_countries: usize,
    /// Owned data for every plottable peer, sorted by IP.
    pub peers: Vec<LocatedPeer>,
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
/// A peer with a valid country but no usable `loc` contributes to the country
/// count without being plotted; a peer with a valid `loc` is always plottable.
pub(crate) fn assemble_snapshot(rows: &[(String, String, String)]) -> GeoViewSnapshot {
    let mut countries = HashSet::new();
    let mut peers = Vec::new();
    for (ip, country, loc) in rows {
        if !country.is_empty() {
            countries.insert(country.clone());
        }
        if let Some((lat, lng)) = parse_loc(loc) {
            peers.push(LocatedPeer {
                ip: ip.clone(),
                country: country.clone(),
                lat,
                lng,
            });
        }
    }
    peers.sort_by(|left, right| left.ip.cmp(&right.ip));

    GeoViewSnapshot {
        total_peers: rows.len(),
        located_peers: peers.len(),
        unique_countries: countries.len(),
        peers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_assemble_snapshot_counts_countries_without_loc() {
        let rows = vec![
            ("1.1.1.1".to_string(), "CN".to_string(), "39.9042,116.4074".to_string()),
            ("2.2.2.2".to_string(), "US".to_string(), "".to_string()),
            ("3.3.3.3".to_string(), "".to_string(), "".to_string()),
        ];

        let snapshot = assemble_snapshot(&rows);

        assert_eq!(snapshot.total_peers, 3);
        assert_eq!(snapshot.located_peers, 1);
        assert_eq!(snapshot.unique_countries, 2);
        assert_eq!(snapshot.peers.len(), 1);
        assert_eq!(snapshot.peers[0].ip, "1.1.1.1");
        assert_eq!(snapshot.peers[0].country, "CN");
    }

    #[test]
    fn test_assemble_snapshot_does_not_count_duplicate_countries() {
        let rows = vec![
            ("1.1.1.1".to_string(), "CN".to_string(), "".to_string()),
            ("2.2.2.2".to_string(), "CN".to_string(), "39.9042,116.4074".to_string()),
        ];

        let snapshot = assemble_snapshot(&rows);

        assert_eq!(snapshot.unique_countries, 1);
        assert_eq!(snapshot.located_peers, 1);
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
