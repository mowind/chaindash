use std::{
    path::Path,
    sync::{
        Arc,
        Mutex,
    },
    thread,
    time::{
        SystemTime,
        UNIX_EPOCH,
    },
};

use crossbeam_channel::{
    bounded,
    unbounded,
    Receiver,
    Sender,
};
use rusqlite::{
    params,
    Connection,
};

use super::snapshot::{
    assemble_snapshot,
    GeoViewSnapshot,
};
use crate::{
    error::{
        ChaindashError,
        Result,
    },
    sync::lock_or_panic,
};

pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

const LOCATION_CACHE_TTL_SECONDS: i64 = 24 * 60 * 60;

/// Versioned, idempotent migrations applied when the worker starts.
const MIGRATIONS: &[&str] = &[
    // v1: current Peer Snapshot and per-IP Location Cache.
    "
    CREATE TABLE IF NOT EXISTS current_peers (
        ip TEXT PRIMARY KEY,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS location_cache (
        ip TEXT PRIMARY KEY,
        country TEXT NOT NULL DEFAULT '',
        loc TEXT NOT NULL DEFAULT '',
        refreshed_at INTEGER NOT NULL DEFAULT 0,
        last_error TEXT NOT NULL DEFAULT ''
    );
    ",
];

/// Wall-clock source in unix seconds. Injected so cache expiry can be tested
/// deterministically without sleeping.
pub(crate) fn default_clock() -> Clock {
    Arc::new(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
    })
}

/// A single Location Cache write: either a successful enrichment (country
/// and/or loc) or a failure that must preserve any previous success.
#[derive(Debug, Clone)]
pub struct LocationEntry {
    pub ip: String,
    pub country: Option<String>,
    /// Raw `lat,lng` string from IPinfo.
    pub loc: Option<String>,
    pub error: Option<String>,
}

impl LocationEntry {
    pub(crate) fn success(
        ip: String,
        country: Option<String>,
        loc: Option<String>,
    ) -> Self {
        LocationEntry {
            ip,
            country,
            loc,
            error: None,
        }
    }

    pub(crate) fn failed(
        ip: String,
        error: String,
    ) -> Self {
        LocationEntry {
            ip,
            country: None,
            loc: None,
            error: Some(error),
        }
    }
}

/// Highest-level boundary for the peer geography feature.
///
/// The production implementation is a channel-backed SQLite worker that owns
/// the connection on a dedicated thread and serializes all database operations.
/// The collector writes through this seam; the UI reads owned Geo View
/// Snapshots through it and never executes SQL during drawing.
pub trait PeerGeoStore: Send + Sync + std::fmt::Debug {
    /// Replace the current Peer Snapshot transactionally.
    ///
    /// Returns the IPs that need enrichment: new IPs, and cached IPs whose
    /// last successful refresh is older than 24 hours.
    fn replace_peer_snapshot(
        &self,
        ips: Vec<String>,
    ) -> Result<Vec<String>>;

    /// Persist enrichment results. A failed entry preserves the previous
    /// successful Location Cache row for the same IP.
    fn update_location_cache(
        &self,
        entries: Vec<LocationEntry>,
    ) -> Result<()>;

    /// Request an owned Geo View Snapshot built from the current Peer Snapshot
    /// joined to the Location Cache, including the Peer Country Distribution.
    fn geo_view_snapshot(&self) -> Result<GeoViewSnapshot>;

    /// Wake channel that fires after every successful write commit.
    fn updates(&self) -> Receiver<()>;

    /// Stop the worker, letting queued transactions finish.
    fn shutdown(&self);
}

// ============================================================================
// SQLite worker implementation
// ============================================================================

enum StoreCommand {
    ReplaceSnapshot(Vec<String>, Sender<Result<Vec<String>>>),
    UpdateLocations(Vec<LocationEntry>, Sender<Result<()>>),
    Snapshot(Sender<Result<GeoViewSnapshot>>),
    Shutdown,
}

/// The production `PeerGeoStore`: a channel-backed SQLite worker that owns the
/// connection on a dedicated thread and serializes all database operations.
#[derive(Debug)]
pub struct SqlitePeerGeoStore {
    tx: Sender<StoreCommand>,
    updates: Receiver<()>,
    /// Keeps the wake channel open while this handle lives, so the UI never
    /// sees a disconnected receiver even if the worker exits unexpectedly.
    _wake: Sender<()>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
}

impl SqlitePeerGeoStore {
    /// Open (or create) the database at `db_path`, run migrations, and start
    /// the worker thread. Fails when the database cannot open or migrate.
    pub fn open(db_path: &str) -> Result<Self> {
        Self::open_with_clock(db_path, default_clock())
    }

    #[cfg(test)]
    pub(crate) fn open_in_memory_with_clock(clock: Clock) -> Result<Self> {
        Self::open_with_clock(":memory:", clock)
    }

    fn open_with_clock(
        db_path: &str,
        clock: Clock,
    ) -> Result<Self> {
        let (tx, rx) = unbounded();
        let (wake, updates) = bounded(1);
        let (ready_tx, ready_rx) = bounded(1);

        let db_path = db_path.to_string();
        let wake_clone = wake.clone();
        let thread = thread::Builder::new()
            .name("peer-geo-store".to_string())
            .spawn(move || {
                let init = (|| -> Result<Connection> {
                    let mut conn = if db_path == ":memory:" {
                        Connection::open_in_memory()
                    } else {
                        Connection::open(Path::new(&db_path))
                    }?;
                    run_migrations(&mut conn)?;
                    Ok(conn)
                })();
                match init {
                    Ok(conn) => {
                        let _ = ready_tx.send(Ok(()));
                        worker_loop(rx, wake_clone, conn, clock);
                    },
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                    },
                }
            })
            .map_err(|err| {
                ChaindashError::Other(format!("failed to spawn peer geo store worker: {err}"))
            })?;

        let init = ready_rx
            .recv()
            .map_err(|_| ChaindashError::Other("peer geo store worker init failed".to_string()))?;
        init?;

        Ok(SqlitePeerGeoStore {
            tx,
            updates,
            _wake: wake,
            thread: Mutex::new(Some(thread)),
        })
    }
}

fn worker_loop(
    rx: Receiver<StoreCommand>,
    wake: Sender<()>,
    mut conn: Connection,
    clock: Clock,
) {
    while let Ok(command) = rx.recv() {
        let now = clock();
        match command {
            StoreCommand::ReplaceSnapshot(ips, reply) => {
                let result = replace_peer_snapshot_tx(&mut conn, &ips, now);
                if result.is_ok() {
                    let _ = wake.try_send(());
                }
                let _ = reply.send(result);
            },
            StoreCommand::UpdateLocations(entries, reply) => {
                let result = update_location_cache_tx(&mut conn, &entries, now);
                if result.is_ok() {
                    let _ = wake.try_send(());
                }
                let _ = reply.send(result);
            },
            StoreCommand::Snapshot(reply) => {
                let _ = reply.send(build_geo_view_snapshot(&mut conn));
            },
            StoreCommand::Shutdown => break,
        }
    }
}

impl PeerGeoStore for SqlitePeerGeoStore {
    fn replace_peer_snapshot(
        &self,
        ips: Vec<String>,
    ) -> Result<Vec<String>> {
        let (reply_tx, reply_rx) = bounded(1);
        self.tx.send(StoreCommand::ReplaceSnapshot(ips, reply_tx)).map_err(store_unavailable)?;
        reply_rx.recv().map_err(store_unavailable)?
    }

    fn update_location_cache(
        &self,
        entries: Vec<LocationEntry>,
    ) -> Result<()> {
        let (reply_tx, reply_rx) = bounded(1);
        self.tx
            .send(StoreCommand::UpdateLocations(entries, reply_tx))
            .map_err(store_unavailable)?;
        reply_rx.recv().map_err(store_unavailable)?
    }

    fn geo_view_snapshot(&self) -> Result<GeoViewSnapshot> {
        let (reply_tx, reply_rx) = bounded(1);
        self.tx.send(StoreCommand::Snapshot(reply_tx)).map_err(store_unavailable)?;
        reply_rx.recv().map_err(store_unavailable)?
    }

    fn updates(&self) -> Receiver<()> {
        self.updates.clone()
    }

    fn shutdown(&self) {
        let _ = self.tx.send(StoreCommand::Shutdown);
        if let Some(thread) = lock_or_panic(&self.thread).take() {
            let _ = thread.join();
        }
    }
}

impl Drop for SqlitePeerGeoStore {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn store_unavailable(err: impl std::fmt::Display) -> ChaindashError {
    ChaindashError::Other(format!("peer geo store worker is unavailable: {err}"))
}

/// Store handle used when the database cannot be opened or migrated.
///
/// The TUI keeps running with an empty geography panel; every store operation
/// reports the original failure so the status bar can surface it.
#[derive(Debug)]
pub struct NullPeerGeoStore {
    error: String,
    updates: Receiver<()>,
}

impl NullPeerGeoStore {
    /// Build a store that reports `error` for every operation.
    pub fn new(error: String) -> Self {
        let (_tx, updates) = bounded(1);
        NullPeerGeoStore { error, updates }
    }
}

impl PeerGeoStore for NullPeerGeoStore {
    fn replace_peer_snapshot(
        &self,
        _ips: Vec<String>,
    ) -> Result<Vec<String>> {
        Err(ChaindashError::Other(self.error.clone()))
    }

    fn update_location_cache(
        &self,
        _entries: Vec<LocationEntry>,
    ) -> Result<()> {
        Err(ChaindashError::Other(self.error.clone()))
    }

    fn geo_view_snapshot(&self) -> Result<GeoViewSnapshot> {
        Err(ChaindashError::Other(self.error.clone()))
    }

    fn updates(&self) -> Receiver<()> {
        self.updates.clone()
    }

    fn shutdown(&self) {}
}

// ============================================================================
// Database operations (also exercised directly by tests)
// ============================================================================

/// Apply versioned migrations idempotently.
pub(crate) fn run_migrations(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );",
    )?;

    for (index, migration_sql) in MIGRATIONS.iter().enumerate() {
        let version = index as i64 + 1;
        let already_applied: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            params![version],
            |row| row.get(0),
        )?;
        if already_applied {
            continue;
        }

        let now = default_clock()();
        let tx = conn.transaction()?;
        tx.execute_batch(migration_sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, now],
        )?;
        tx.commit()?;
    }

    Ok(())
}

/// Replace the current Peer Snapshot in one transaction.
///
/// Returns the IPs needing enrichment: not in the Location Cache yet, or with
/// a last successful refresh older than `LOCATION_CACHE_TTL_SECONDS`.
pub(crate) fn replace_peer_snapshot_tx(
    conn: &mut Connection,
    ips: &[String],
    now: i64,
) -> Result<Vec<String>> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM current_peers", [])?;
    let mut unique_ips = ips.to_vec();
    unique_ips.sort();
    unique_ips.dedup();
    {
        let mut stmt = tx.prepare("INSERT INTO current_peers (ip, updated_at) VALUES (?1, ?2)")?;
        for ip in &unique_ips {
            stmt.execute(params![ip, now])?;
        }
    }
    let needs_enrichment: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT c.ip
             FROM current_peers c
             LEFT JOIN location_cache l ON l.ip = c.ip
             WHERE l.ip IS NULL OR l.refreshed_at < ?1
             ORDER BY c.ip",
        )?;
        let rows = stmt
            .query_map(params![now - LOCATION_CACHE_TTL_SECONDS], |row| row.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    tx.commit()?;
    Ok(needs_enrichment)
}

/// Persist enrichment results in one transaction.
///
/// A failed entry updates only `last_error`, preserving any previous
/// successful country/loc/refresh time.
pub(crate) fn update_location_cache_tx(
    conn: &mut Connection,
    entries: &[LocationEntry],
    now: i64,
) -> Result<()> {
    let tx = conn.transaction()?;
    for entry in entries {
        if let Some(error) = &entry.error {
            tx.execute(
                "INSERT INTO location_cache (ip, country, loc, refreshed_at, last_error)
                 VALUES (?1, '', '', 0, ?2)
                 ON CONFLICT(ip) DO UPDATE SET last_error = excluded.last_error",
                params![entry.ip, error],
            )?;
        } else {
            let country = entry.country.as_deref().unwrap_or("");
            let loc = entry.loc.as_deref().unwrap_or("");
            tx.execute(
                "INSERT INTO location_cache (ip, country, loc, refreshed_at, last_error)
                 VALUES (?1, ?2, ?3, ?4, '')
                 ON CONFLICT(ip) DO UPDATE SET
                     country = excluded.country,
                     loc = excluded.loc,
                     refreshed_at = excluded.refreshed_at,
                     last_error = ''",
                params![entry.ip, country, loc, now],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Build the Geo View Snapshot by joining the Peer Snapshot to the Location
/// Cache. Read failures propagate to the caller so the UI can report them.
pub(crate) fn build_geo_view_snapshot(conn: &mut Connection) -> Result<GeoViewSnapshot> {
    let mut stmt = conn.prepare(
        "SELECT p.ip, COALESCE(l.country, '')
         FROM current_peers p
         LEFT JOIN location_cache l ON l.ip = p.ip
         ORDER BY p.ip",
    )?;
    let rows =
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let rows: std::result::Result<Vec<_>, _> = rows.collect();
    Ok(assemble_snapshot(&rows?))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{
                AtomicI64,
                Ordering,
            },
            Arc,
        },
        time::{
            Duration,
            Instant,
        },
    };

    use rusqlite::Connection;

    use super::*;

    fn test_clock() -> Clock {
        Arc::new(|| 1_700_000_000)
    }

    fn in_memory_conn() -> Connection {
        Connection::open_in_memory().expect("in-memory connection should open")
    }

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepare should work");
        let names = stmt.query_map([], |row| row.get::<_, String>(0)).expect("query should work");
        names.collect::<std::result::Result<Vec<_>, _>>().expect("rows should read")
    }

    #[test]
    fn test_run_migrations_creates_tables() {
        let mut conn = in_memory_conn();

        run_migrations(&mut conn).expect("migration should succeed");

        let names = table_names(&conn);
        assert!(names.contains(&"current_peers".to_string()));
        assert!(names.contains(&"location_cache".to_string()));
        assert!(names.contains(&"schema_migrations".to_string()));
    }

    #[test]
    fn test_run_migrations_is_idempotent() {
        let mut conn = in_memory_conn();

        run_migrations(&mut conn).expect("first migration should succeed");
        run_migrations(&mut conn).expect("second migration should succeed");

        let version: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .expect("query should work");
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn test_replace_snapshot_replaces_peers_transactionally() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migration should succeed");

        replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string(), "2.2.2.2".to_string()], 100)
            .expect("first replace should succeed");
        replace_peer_snapshot_tx(&mut conn, &["3.3.3.3".to_string()], 200)
            .expect("second replace should succeed");

        let snapshot = build_geo_view_snapshot(&mut conn).expect("snapshot should build");
        assert_eq!(snapshot.total_peers, 1);
        assert_eq!(snapshot.unique_countries, 0);
        assert_eq!(snapshot.unknown_country_count, 1);
    }

    #[test]
    fn test_replace_snapshot_deduplicates_ips_at_store_boundary() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migration should succeed");

        replace_peer_snapshot_tx(
            &mut conn,
            &["2.2.2.2".to_string(), "1.1.1.1".to_string(), "2.2.2.2".to_string()],
            100,
        )
        .expect("duplicate IPs should not make the snapshot write fail");

        let snapshot = build_geo_view_snapshot(&mut conn).expect("snapshot should build");
        assert_eq!(snapshot.total_peers, 2);
    }

    #[test]
    fn test_replace_snapshot_returns_ips_needing_enrichment() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migration should succeed");

        let now = 1_700_000_000;
        let needs = replace_peer_snapshot_tx(
            &mut conn,
            &["1.1.1.1".to_string(), "2.2.2.2".to_string()],
            now,
        )
        .expect("replace should succeed");
        assert_eq!(needs, vec!["1.1.1.1".to_string(), "2.2.2.2".to_string()]);

        update_location_cache_tx(
            &mut conn,
            &[LocationEntry::success(
                "1.1.1.1".to_string(),
                Some("CN".to_string()),
                Some("39.9042,116.4074".to_string()),
            )],
            now,
        )
        .expect("update should succeed");

        let needs = replace_peer_snapshot_tx(
            &mut conn,
            &["1.1.1.1".to_string(), "2.2.2.2".to_string()],
            now,
        )
        .expect("replace should succeed");
        assert_eq!(needs, vec!["2.2.2.2".to_string()]);
    }

    #[test]
    fn test_replace_snapshot_refreshes_stale_cache_after_24h() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migration should succeed");

        let now = 1_700_000_000;
        replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now)
            .expect("replace should succeed");
        update_location_cache_tx(
            &mut conn,
            &[LocationEntry::success(
                "1.1.1.1".to_string(),
                Some("CN".to_string()),
                Some("39.9042,116.4074".to_string()),
            )],
            now,
        )
        .expect("update should succeed");

        let fresh = replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now + 60)
            .expect("replace should succeed");
        assert!(fresh.is_empty(), "cache younger than 24h must not need refresh");

        let stale =
            replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now + 24 * 60 * 60 + 1)
                .expect("replace should succeed");
        assert_eq!(stale, vec!["1.1.1.1".to_string()]);
    }

    #[test]
    fn test_unknown_country_cache_entry_keeps_refresh_cadence() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migration should succeed");

        let now = 1_700_000_000;
        replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now)
            .expect("replace should succeed");
        update_location_cache_tx(
            &mut conn,
            &[LocationEntry::success("1.1.1.1".to_string(), None, None)],
            now,
        )
        .expect("unknown country update should succeed");

        let fresh = replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now + 60)
            .expect("replace should succeed");
        assert!(fresh.is_empty(), "unknown country cache should honor the 24h TTL");

        let stale =
            replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now + 24 * 60 * 60 + 1)
                .expect("replace should succeed");
        assert_eq!(stale, vec!["1.1.1.1".to_string()]);
    }

    #[test]
    fn test_disconnected_peer_keeps_location_cache() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migration should succeed");

        let now = 1_700_000_000;
        replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now)
            .expect("replace should succeed");
        update_location_cache_tx(
            &mut conn,
            &[LocationEntry::success(
                "1.1.1.1".to_string(),
                Some("CN".to_string()),
                Some("39.9042,116.4074".to_string()),
            )],
            now,
        )
        .expect("update should succeed");

        replace_peer_snapshot_tx(&mut conn, &["2.2.2.2".to_string()], now + 60)
            .expect("replace should succeed");

        let cached: i64 = conn
            .query_row("SELECT COUNT(*) FROM location_cache WHERE ip = '1.1.1.1'", [], |row| {
                row.get(0)
            })
            .expect("query should work");
        assert_eq!(cached, 1);
    }

    #[test]
    fn test_failed_enrichment_preserves_previous_success() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migration should succeed");

        let now = 1_700_000_000;
        replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now)
            .expect("replace should succeed");
        update_location_cache_tx(
            &mut conn,
            &[LocationEntry::success(
                "1.1.1.1".to_string(),
                Some("CN".to_string()),
                Some("39.9042,116.4074".to_string()),
            )],
            now,
        )
        .expect("update should succeed");

        update_location_cache_tx(
            &mut conn,
            &[LocationEntry::failed("1.1.1.1".to_string(), "timeout".to_string())],
            now + 60,
        )
        .expect("failed update should succeed");

        let snapshot = build_geo_view_snapshot(&mut conn).expect("snapshot should build");
        assert_eq!(snapshot.total_peers, 1);
        assert_eq!(snapshot.country_counts.len(), 1);
        assert_eq!(snapshot.country_counts[0].country_code, "CN");
        assert_eq!(snapshot.country_counts[0].peer_count, 1);

        let (country, loc, refreshed_at, last_error): (String, String, i64, String) = conn
            .query_row(
                "SELECT country, loc, refreshed_at, last_error FROM location_cache WHERE ip = \
                 '1.1.1.1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("query should work");
        assert_eq!(country, "CN");
        assert_eq!(loc, "39.9042,116.4074");
        assert_eq!(refreshed_at, now);
        assert_eq!(last_error, "timeout");
    }

    #[test]
    fn test_successful_enrichment_moves_peer_from_unknown_to_known_country() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migration should succeed");

        let now = 1_700_000_000;
        replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now)
            .expect("replace should succeed");
        update_location_cache_tx(
            &mut conn,
            &[LocationEntry::success("1.1.1.1".to_string(), None, None)],
            now,
        )
        .expect("unknown enrichment should succeed");

        let unknown_snapshot = build_geo_view_snapshot(&mut conn).expect("snapshot should build");
        assert_eq!(unknown_snapshot.unknown_country_count, 1);
        assert!(unknown_snapshot.country_counts.is_empty());

        update_location_cache_tx(
            &mut conn,
            &[LocationEntry::success("1.1.1.1".to_string(), Some(" us ".to_string()), None)],
            now + 60,
        )
        .expect("known enrichment should succeed");

        let known_snapshot = build_geo_view_snapshot(&mut conn).expect("snapshot should build");
        assert_eq!(known_snapshot.unknown_country_count, 0);
        assert_eq!(known_snapshot.country_counts.len(), 1);
        assert_eq!(known_snapshot.country_counts[0].country_code, "US");
        assert_eq!(known_snapshot.country_counts[0].peer_count, 1);
    }

    #[test]
    fn test_error_only_entry_is_unknown_country() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migration should succeed");

        let now = 1_700_000_000;
        replace_peer_snapshot_tx(&mut conn, &["1.1.1.1".to_string()], now)
            .expect("replace should succeed");
        update_location_cache_tx(
            &mut conn,
            &[LocationEntry::failed("1.1.1.1".to_string(), "timeout".to_string())],
            now,
        )
        .expect("failed update should succeed");

        let snapshot = build_geo_view_snapshot(&mut conn).expect("snapshot should build");
        assert_eq!(snapshot.total_peers, 1);
        assert_eq!(snapshot.unique_countries, 0);
        assert_eq!(snapshot.unknown_country_count, 1);
    }

    #[test]
    fn test_geo_view_snapshot_aggregates_full_peer_snapshot() {
        let store =
            SqlitePeerGeoStore::open_in_memory_with_clock(test_clock()).expect("store should open");

        store
            .replace_peer_snapshot(vec![
                "1.1.1.1".to_string(),
                "2.2.2.2".to_string(),
                "3.3.3.3".to_string(),
                "4.4.4.4".to_string(),
                "1.1.1.1".to_string(),
            ])
            .expect("snapshot replacement should succeed");
        store
            .update_location_cache(vec![
                LocationEntry::success("1.1.1.1".to_string(), Some(" cn ".to_string()), None),
                LocationEntry::success("2.2.2.2".to_string(), Some("US".to_string()), None),
                LocationEntry::success("3.3.3.3".to_string(), Some("invalid".to_string()), None),
                LocationEntry::success(
                    "4.4.4.4".to_string(),
                    Some("US".to_string()),
                    Some("39.9042,116.4074".to_string()),
                ),
            ])
            .expect("location cache update should succeed");

        let snapshot = store.geo_view_snapshot().expect("snapshot should succeed");

        assert_eq!(snapshot.total_peers, 4);
        assert_eq!(snapshot.unknown_country_count, 1);
        assert_eq!(
            snapshot
                .country_counts
                .iter()
                .map(|country| (country.country_code.as_str(), country.peer_count))
                .collect::<Vec<_>>(),
            vec![("US", 2), ("CN", 1)]
        );
        assert_eq!(
            snapshot.country_counts.iter().map(|country| country.peer_count).sum::<usize>()
                + snapshot.unknown_country_count,
            snapshot.total_peers
        );

        store.shutdown();
    }

    #[test]
    fn test_worker_round_trip_through_store_handle() {
        let store =
            SqlitePeerGeoStore::open_in_memory_with_clock(test_clock()).expect("store should open");

        let needs = store
            .replace_peer_snapshot(vec!["1.1.1.1".to_string()])
            .expect("replace should succeed");
        assert_eq!(needs, vec!["1.1.1.1".to_string()]);

        store
            .update_location_cache(vec![LocationEntry::success(
                "1.1.1.1".to_string(),
                Some("CN".to_string()),
                Some("39.9042,116.4074".to_string()),
            )])
            .expect("update should succeed");

        let snapshot = store.geo_view_snapshot().expect("snapshot should succeed");
        assert_eq!(snapshot.total_peers, 1);
        assert_eq!(snapshot.unique_countries, 1);
        assert_eq!(snapshot.unknown_country_count, 0);
        assert_eq!(snapshot.country_counts[0].country_code, "CN");
        assert_eq!(snapshot.country_counts[0].peer_count, 1);

        store.shutdown();
    }

    #[test]
    fn test_worker_wakes_ui_after_successful_write() {
        let store =
            SqlitePeerGeoStore::open_in_memory_with_clock(test_clock()).expect("store should open");
        let updates = store.updates();

        store.replace_peer_snapshot(vec!["1.1.1.1".to_string()]).expect("replace should succeed");
        updates.recv_timeout(Duration::from_millis(100)).expect("write should wake the ui");

        store.shutdown();
    }

    #[test]
    fn test_worker_shutdown_joins_promptly() {
        let store =
            SqlitePeerGeoStore::open_in_memory_with_clock(test_clock()).expect("store should open");

        let started = Instant::now();
        store.shutdown();

        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn test_store_reopens_existing_database() {
        let dir = std::env::temp_dir().join(format!("chaindash-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir should create");
        let path = dir.join("peers-reopen.db");
        let _ = std::fs::remove_file(&path);
        let path_str = path.to_str().expect("path should be utf8").to_string();

        let store = SqlitePeerGeoStore::open_with_clock(&path_str, test_clock())
            .expect("first open should succeed");
        store.replace_peer_snapshot(vec!["1.1.1.1".to_string()]).expect("replace should succeed");
        store
            .update_location_cache(vec![LocationEntry::success(
                "1.1.1.1".to_string(),
                Some("CN".to_string()),
                Some("39.9042,116.4074".to_string()),
            )])
            .expect("update should succeed");
        store.shutdown();

        let reopened = SqlitePeerGeoStore::open_with_clock(&path_str, test_clock())
            .expect("reopen should succeed");
        let snapshot = reopened.geo_view_snapshot().expect("snapshot should load");
        assert_eq!(snapshot.total_peers, 1);
        assert_eq!(snapshot.country_counts[0].country_code, "CN");
        reopened.shutdown();

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_null_store_reports_open_failure() {
        let store = NullPeerGeoStore::new("cannot open db".to_string());

        assert!(store.geo_view_snapshot().is_err());
        assert!(store.replace_peer_snapshot(vec![]).is_err());
        assert!(store.update_location_cache(vec![]).is_err());
        store.shutdown();
    }

    #[test]
    fn test_clock_determinism() {
        let clock = test_clock();
        assert_eq!(clock(), 1_700_000_000);
        assert_eq!(clock(), 1_700_000_000);

        let counter = Arc::new(AtomicI64::new(0));
        let counter_clone = Arc::clone(&counter);
        let clock: Clock = Arc::new(move || counter_clone.fetch_add(1, Ordering::SeqCst));
        assert_eq!(clock(), 0);
        assert_eq!(clock(), 1);
    }
}
