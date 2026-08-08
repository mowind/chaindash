use std::net::{
    IpAddr,
    SocketAddr,
};

use serde_json::Value;

use crate::error::{
    ChaindashError,
    Result,
};

/// Extract the literal IP address from an `admin_peers` `network.remoteAddress` string.
///
/// Accepts `1.2.3.4:30303`, `[2001:db8::1]:30303` and bare addresses. Returns `None`
/// for hostnames and anything that is not a literal IPv4/IPv6 address.
pub(crate) fn ip_from_remote_address(remote_address: &str) -> Option<IpAddr> {
    if let Ok(socket_addr) = remote_address.parse::<SocketAddr>() {
        return Some(socket_addr.ip());
    }

    if let Ok(ip) = remote_address.parse::<IpAddr>() {
        return Some(ip);
    }

    let host = if let Some(rest) = remote_address.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else if let Some((host, _port)) = remote_address.rsplit_once(':') {
        host
    } else {
        remote_address
    };

    host.parse::<IpAddr>().ok()
}

/// Whether a literal IP is suitable for external geolocation enrichment.
///
/// Hostnames are rejected by [`ip_from_remote_address`]. Private, loopback,
/// unspecified, link-local, multicast and broadcast addresses are never sent
/// to an external service.
pub(crate) fn is_enrichable(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !v4.is_unspecified()
                && !v4.is_loopback()
                && !v4.is_private()
                && !v4.is_link_local()
                && !v4.is_multicast()
                && !v4.is_broadcast()
        },
        IpAddr::V6(v6) => {
            !v6.is_unspecified()
                && !v6.is_loopback()
                && !v6.is_unique_local()
                && !v6.is_unicast_link_local()
                && !v6.is_multicast()
        },
    }
}

/// Extract the literal IPs from a raw `admin_peers` response array.
///
/// The IP is read from the peer's `network.remoteAddress` only; the `enode`
/// string is never parsed. Hostnames are rejected, but every literal IPv4/IPv6
/// stays in the snapshot; non-enrichable addresses are filtered later, when
/// enrichment is decided. Duplicates are preserved here and removed during the
/// merge step.
pub(crate) fn parse_peer_ips(peers: &[Value]) -> Vec<IpAddr> {
    peers
        .iter()
        .filter_map(|peer| {
            peer.get("network")
                .and_then(|network| network.get("remoteAddress"))
                .and_then(Value::as_str)
                .and_then(ip_from_remote_address)
        })
        .collect()
}

/// Merge per-node `admin_peers` results into a sorted, IP-deduplicated snapshot.
///
/// Returns `Err` when any Monitored Node failed, so the caller retains the last
/// successful Peer Snapshot instead of replacing it with partial data.
pub(crate) fn merge_node_results(results: &[Result<Vec<IpAddr>>]) -> Result<Vec<String>> {
    let mut ips: Vec<String> = Vec::new();
    for result in results {
        let node_ips = result
            .as_ref()
            .map_err(|err| ChaindashError::Other(format!("admin_peers failed: {err}")))?;
        ips.extend(node_ips.iter().map(|ip| ip.to_string()));
    }
    ips.sort();
    ips.dedup();
    Ok(ips)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    fn peers_fixture() -> &'static str {
        r#"[
            {
                "id": "1111",
                "name": "platON/v1.1.0/linux-amd64/go1.17.2",
                "caps": ["eth/63", "eth/64"],
                "network": {
                    "localAddress": "192.168.0.5:16789",
                    "remoteAddress": "39.99.168.168:16789",
                    "inbound": true,
                    "trusted": false,
                    "static": false
                },
                "protocols": {"eth": {"version": 64}}
            },
            {
                "id": "2222",
                "name": "platON/v1.1.0/linux-amd64/go1.17.2",
                "network": {
                    "remoteAddress": "[2001:db8:85a3::8a2e:370:7334]:30303"
                }
            },
            {
                "id": "3333",
                "name": "platON/v1.1.0/linux-amd64/go1.17.2",
                "network": {
                    "remoteAddress": "10.0.0.1:30303"
                }
            },
            {
                "id": "4444",
                "name": "platON/v1.1.0/linux-amd64/go1.17.2",
                "network": {
                    "remoteAddress": "127.0.0.1:30303"
                }
            },
            {
                "id": "5555",
                "name": "platON/v1.1.0/linux-amd64/go1.17.2",
                "network": {
                    "remoteAddress": "0.0.0.0:30303"
                }
            },
            {
                "id": "6666",
                "name": "platON/v1.1.0/linux-amd64/go1.17.2",
                "network": {
                    "remoteAddress": "peer.example.com:30303"
                }
            },
            {
                "id": "7777",
                "name": "platON/v1.1.0/linux-amd64/go1.17.2",
                "network": {
                    "remoteAddress": "39.99.168.168:16789"
                }
            },
            {
                "id": "8888",
                "name": "platON/v1.1.0/linux-amd64/go1.17.2",
                "network": {
                    "remoteAddress": "203.0.113.9:30303"
                }
            }
        ]"#
    }

    #[test]
    fn test_ip_from_remote_address_parses_ipv4_with_port() {
        assert_eq!(
            ip_from_remote_address("39.99.168.168:16789"),
            Some(IpAddr::V4(Ipv4Addr::new(39, 99, 168, 168)))
        );
    }

    #[test]
    fn test_ip_from_remote_address_parses_ipv6_with_port() {
        assert_eq!(
            ip_from_remote_address("[2001:db8:85a3::8a2e:370:7334]:30303"),
            Some("2001:db8:85a3::8a2e:370:7334".parse().unwrap())
        );
    }

    #[test]
    fn test_ip_from_remote_address_parses_bare_address() {
        assert_eq!(
            ip_from_remote_address("203.0.113.9"),
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9)))
        );
        assert_eq!(ip_from_remote_address("2001:db8::1"), Some("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn test_ip_from_remote_address_rejects_hostname() {
        assert_eq!(ip_from_remote_address("peer.example.com:30303"), None);
        assert_eq!(ip_from_remote_address("peer.example.com"), None);
    }

    #[test]
    fn test_is_enrichable_rejects_private_loopback_unspecified() {
        assert!(!is_enrichable("10.0.0.1".parse().unwrap()));
        assert!(!is_enrichable("192.168.1.1".parse().unwrap()));
        assert!(!is_enrichable("172.16.0.1".parse().unwrap()));
        assert!(!is_enrichable("127.0.0.1".parse().unwrap()));
        assert!(!is_enrichable("0.0.0.0".parse().unwrap()));
        assert!(!is_enrichable("::1".parse().unwrap()));
        assert!(!is_enrichable("::".parse().unwrap()));
        assert!(!is_enrichable("fe80::1".parse().unwrap()));
        assert!(!is_enrichable("224.0.0.1".parse().unwrap()));
        assert!(!is_enrichable("255.255.255.255".parse().unwrap()));
    }

    #[test]
    fn test_is_enrichable_accepts_public_addresses() {
        assert!(is_enrichable("39.99.168.168".parse().unwrap()));
        assert!(is_enrichable("203.0.113.9".parse().unwrap()));
        assert!(is_enrichable("2600:1f18::1".parse().unwrap()));
    }

    #[test]
    fn test_parse_peer_ips_extracts_all_literal_ipv4_and_ipv6() {
        let fixture: Vec<Value> = serde_json::from_str(peers_fixture()).unwrap();
        let ips = parse_peer_ips(&fixture);
        let expected: Vec<IpAddr> = vec![
            "39.99.168.168".parse().unwrap(),
            "2001:db8:85a3::8a2e:370:7334".parse().unwrap(),
            "10.0.0.1".parse().unwrap(),
            "127.0.0.1".parse().unwrap(),
            "0.0.0.0".parse().unwrap(),
            "39.99.168.168".parse().unwrap(),
            "203.0.113.9".parse().unwrap(),
        ];

        // Hostnames are rejected; private/loopback/unspecified literal IPs stay
        // in the Peer Snapshot and are only excluded from external enrichment.
        // Duplicates are preserved here and deduplicated during the merge step.
        assert_eq!(ips, expected);
    }

    #[test]
    fn test_parse_peer_ips_ignores_missing_network_field() {
        let fixture: Vec<Value> = serde_json::from_str(
            r#"[{"id": "1", "name": "n", "network": {"remoteAddress": "8.8.8.8:1"}},
               {"id": "2", "name": "n"}]"#,
        )
        .unwrap();

        let expected: Vec<IpAddr> = vec!["8.8.8.8".parse().unwrap()];
        assert_eq!(parse_peer_ips(&fixture), expected);
    }

    #[test]
    fn test_merge_node_results_deduplicates_and_sorts() {
        let results = vec![
            Ok(vec!["203.0.113.9".parse().unwrap(), "39.99.168.168".parse().unwrap()]),
            Ok(vec!["39.99.168.168".parse().unwrap(), "203.0.113.10".parse().unwrap()]),
        ];

        assert_eq!(
            merge_node_results(&results).unwrap(),
            vec![
                "203.0.113.10".to_string(),
                "203.0.113.9".to_string(),
                "39.99.168.168".to_string(),
            ]
        );
    }

    #[test]
    fn test_merge_node_results_fails_on_any_node_error() {
        let results = vec![
            Ok(vec!["203.0.113.9".parse().unwrap()]),
            Err(ChaindashError::Rpc("connection reset".to_string())),
        ];

        assert!(merge_node_results(&results).is_err());
    }

    #[test]
    fn test_merge_node_results_accepts_empty_results() {
        assert_eq!(merge_node_results(&[]).unwrap(), Vec::<String>::new());
    }
}
