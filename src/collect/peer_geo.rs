use std::{
    collections::HashSet,
    net::IpAddr,
    sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        Arc,
        Mutex,
    },
};

use alloy::providers::{
    Provider,
    ProviderBuilder,
    WsConnect,
};
use serde_json::Value;
use tokio::{
    task::JoinSet,
    time::{
        self,
        Duration,
    },
};

use super::data::{
    warn_with_status,
    SharedData,
};
use crate::{
    error::Result,
    geo::{
        peers::{
            is_enrichable,
            merge_node_results,
            parse_peer_ips,
        },
        IpInfoClient,
        IpInfoEntry,
        LocationEntry,
        PeerGeoStore,
    },
    sync::lock_or_panic,
};

const PEER_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Poll `admin_peers` for every Monitored Node once per minute, replace the
/// Peer Snapshot through the store, and enrich new or stale IPs immediately.
///
/// A poll round with any node failure retains the last successful Peer
/// Snapshot. Enrichment failures are persisted as error states that preserve
/// previous successful locations. In-flight enrichments are drained before the
/// task returns so their cache writes land before the store shuts down.
pub(crate) async fn collect_peer_geo(
    urls: Vec<(String, String)>,
    store: Arc<dyn PeerGeoStore>,
    data: SharedData,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    let ipinfo = Arc::new(IpInfoClient::new());
    let pending: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut enrichment_tasks = JoinSet::new();
    let mut ticker = time::interval(Duration::from_secs(1));
    let poll_started = time::Instant::now();
    let mut next_poll = Duration::ZERO;

    while !stop_flag.load(Ordering::Relaxed) {
        // Poll stop_flag every second so shutdown is not delayed by the
        // one-minute poll interval.
        ticker.tick().await;
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        if !poll_due(poll_started.elapsed(), &mut next_poll) {
            continue;
        }

        let mut results = Vec::new();
        for (name, url) in &urls {
            match fetch_admin_peers_ips(url).await {
                Ok(ips) => results.push(Ok(ips)),
                Err(err) => {
                    warn_with_status(&data, format!("admin_peers failed for {name}: {err}"));
                    results.push(Err(err));
                },
            }
        }

        let Ok(ips) = merge_node_results(&results) else {
            continue;
        };

        match store.replace_peer_snapshot(ips) {
            Ok(needs_enrichment) => {
                for ip in needs_enrichment
                    .into_iter()
                    .filter(|ip| ip.parse::<IpAddr>().map(is_enrichable).unwrap_or(false))
                {
                    if !lock_or_panic(&pending).insert(ip.clone()) {
                        continue;
                    }

                    let store = Arc::clone(&store);
                    let data = data.clone();
                    let pending = Arc::clone(&pending);
                    let ipinfo = Arc::clone(&ipinfo);
                    enrichment_tasks.spawn(async move {
                        let entry = build_location_entry(&ip, ipinfo.lookup(&ip).await);
                        if let Err(err) = store.update_location_cache(vec![entry]) {
                            warn_with_status(
                                &data,
                                format!("location cache store write failed: {err}"),
                            );
                        }
                        lock_or_panic(&pending).remove(&ip);
                    });
                }
            },
            Err(err) => {
                warn_with_status(&data, format!("peer snapshot store write failed: {err}"));
            },
        }
    }

    // Let queued enrichments finish writing before the store shuts down.
    while enrichment_tasks.join_next().await.is_some() {}

    Ok(())
}

/// Return whether a poll is due and schedule the next one relative to the
/// same monotonic clock. Keeping this decision pure makes the one-minute
/// cadence testable without sleeping.
fn poll_due(
    elapsed: Duration,
    next_poll: &mut Duration,
) -> bool {
    if elapsed < *next_poll {
        return false;
    }

    *next_poll = elapsed + PEER_POLL_INTERVAL;
    true
}

/// Fetch and parse the `admin_peers` response of one Monitored Node.
async fn fetch_admin_peers_ips(url: &str) -> Result<Vec<IpAddr>> {
    let provider = ProviderBuilder::new().connect_ws(WsConnect::new(url)).await?;
    let peers: Vec<Value> = provider.client().request_noparams("admin_peers").await?;
    Ok(parse_peer_ips(&peers))
}

/// Map an IPinfo lookup result to a Location Cache entry.
///
/// A failed or empty lookup becomes an error entry; the store preserves any
/// previous successful location for the same IP.
fn build_location_entry(
    ip: &str,
    lookup: Result<Option<IpInfoEntry>>,
) -> LocationEntry {
    match lookup {
        Ok(Some(entry)) => LocationEntry::success(ip.to_string(), entry.country, entry.loc),
        Ok(None) => LocationEntry::failed(
            ip.to_string(),
            "ipinfo returned no usable location data".to_string(),
        ),
        Err(err) => LocationEntry::failed(ip.to_string(), err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ChaindashError;

    #[test]
    fn test_poll_interval_is_one_minute() {
        assert_eq!(PEER_POLL_INTERVAL, Duration::from_secs(60));
    }

    #[test]
    fn test_poll_scheduler_uses_one_minute_boundaries_without_sleeping() {
        let mut next_poll = Duration::ZERO;

        assert!(poll_due(Duration::ZERO, &mut next_poll));
        assert_eq!(next_poll, Duration::from_secs(60));
        assert!(!poll_due(Duration::from_secs(59), &mut next_poll));
        assert!(poll_due(Duration::from_secs(60), &mut next_poll));
        assert_eq!(next_poll, Duration::from_secs(120));
    }

    #[test]
    fn test_build_location_entry_from_success_fixture() {
        let lookup = parse_ipinfo_fixture(
            r#"{"ip": "39.99.168.168", "country": "CN", "loc": "39.9042,116.4074"}"#,
        );

        let entry = build_location_entry("39.99.168.168", lookup);

        assert_eq!(entry.ip, "39.99.168.168");
        assert_eq!(entry.country.as_deref(), Some("CN"));
        assert_eq!(entry.loc.as_deref(), Some("39.9042,116.4074"));
        assert_eq!(entry.error, None);
    }

    #[test]
    fn test_build_location_entry_from_empty_fixture() {
        let lookup = parse_ipinfo_fixture(r#"{"ip": "1.1.1.1"}"#);

        let entry = build_location_entry("1.1.1.1", lookup);

        assert_eq!(entry.country, None);
        assert_eq!(entry.loc, None);
        assert!(entry.error.is_some());
    }

    #[test]
    fn test_build_location_entry_from_failure() {
        let lookup = Err(ChaindashError::Http("timeout".to_string()));

        let entry = build_location_entry("1.1.1.1", lookup);

        assert!(entry.error.as_deref().unwrap().contains("timeout"));
    }

    fn parse_ipinfo_fixture(body: &str) -> Result<Option<IpInfoEntry>> {
        crate::geo::ipinfo::parse_ipinfo_response(body)
    }
}
