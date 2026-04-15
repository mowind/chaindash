use std::{
    env,
    fs,
    io::ErrorKind,
    path::PathBuf,
    time::{
        SystemTime,
        UNIX_EPOCH,
    },
};

use chrono::{
    Datelike,
    NaiveDate,
};
use log::warn;
use serde::{
    Deserialize,
    Serialize,
};

use super::data::NodeDetail;
use crate::error::Result;

const DAILY_NODE_SNAPSHOT_FILE_NAME: &str = "daily-node-snapshots.json";
const DAILY_NODE_SNAPSHOT_HISTORY_LIMIT: usize = 32;
const DEFAULT_STATE_DIR_NAME: &str = "chaindash";
const DEFAULT_STATE_FALLBACK_DIR_NAME: &str = ".chaindash";

#[derive(Debug, Clone)]
pub(crate) struct DailyNodeSummaryDetail {
    pub node_id: String,
    pub node_name: String,
    pub ranking: i32,
    pub block_qty: u64,
    pub reward_value: f64,
    pub daily_block_qty: Option<u64>,
    pub daily_reward_value: Option<f64>,
    pub show_monthly_totals: bool,
    pub monthly_block_qty: Option<u64>,
    pub monthly_reward_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDailyNodeSnapshot {
    date: String,
    node_details: Vec<StoredDailyNodeSnapshotDetail>,
}

impl StoredDailyNodeSnapshot {
    fn from_node_details(
        date: NaiveDate,
        node_details: &[NodeDetail],
    ) -> Self {
        Self {
            date: date.to_string(),
            node_details: node_details
                .iter()
                .map(|detail| StoredDailyNodeSnapshotDetail {
                    node_id: detail.node_id.clone(),
                    node_name: detail.node_name.clone(),
                    block_qty: detail.block_qty,
                    reward_value: detail.reward_value,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDailyNodeSnapshotDetail {
    node_id: String,
    node_name: String,
    block_qty: u64,
    reward_value: f64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoredDailyNodeSnapshotStore {
    snapshots: Vec<StoredDailyNodeSnapshot>,
}

impl StoredDailyNodeSnapshotStore {
    fn snapshot_for(
        &self,
        date: NaiveDate,
    ) -> Option<StoredDailyNodeSnapshot> {
        let date = date.to_string();
        self.snapshots.iter().find(|snapshot| snapshot.date == date).cloned()
    }

    fn upsert(
        &mut self,
        snapshot: StoredDailyNodeSnapshot,
    ) {
        self.snapshots.retain(|existing| existing.date != snapshot.date);
        self.snapshots.push(snapshot);
        self.snapshots.sort_by(|left, right| left.date.cmp(&right.date));
    }

    fn prune(&mut self) {
        if self.snapshots.len() <= DAILY_NODE_SNAPSHOT_HISTORY_LIMIT {
            return;
        }

        let excess = self.snapshots.len() - DAILY_NODE_SNAPSHOT_HISTORY_LIMIT;
        self.snapshots.drain(0..excess);
    }
}

#[derive(Debug, Clone)]
pub(super) struct DailyNodeSnapshotStore {
    path: PathBuf,
}

impl Default for DailyNodeSnapshotStore {
    fn default() -> Self {
        Self::new(default_daily_node_snapshot_path())
    }
}

impl DailyNodeSnapshotStore {
    pub(super) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(super) fn daily_summary_details(
        &self,
        date: NaiveDate,
        node_details: &[NodeDetail],
    ) -> Vec<DailyNodeSummaryDetail> {
        let store = self.read_store();
        let previous_snapshot =
            date.pred_opt().and_then(|previous_date| store.snapshot_for(previous_date));
        let show_monthly_totals = date.day() == 1;
        let monthly_snapshot = previous_month_start_snapshot_date(date)
            .and_then(|monthly_date| store.snapshot_for(monthly_date));

        build_daily_summary_details(
            node_details,
            previous_snapshot.as_ref(),
            monthly_snapshot.as_ref(),
            show_monthly_totals,
        )
    }

    pub(super) fn save_snapshot(
        &self,
        date: NaiveDate,
        node_details: &[NodeDetail],
    ) -> Result<()> {
        if node_details.is_empty() {
            return Ok(());
        }

        let mut store = self.read_store();
        store.upsert(StoredDailyNodeSnapshot::from_node_details(date, node_details));
        store.prune();
        self.write_store(&store)
    }

    fn read_store(&self) -> StoredDailyNodeSnapshotStore {
        match fs::read_to_string(&self.path) {
            Ok(content) if content.trim().is_empty() => StoredDailyNodeSnapshotStore::default(),
            Ok(content) => match serde_json::from_str(&content) {
                Ok(store) => store,
                Err(err) => {
                    warn!(
                        "Failed to parse daily node snapshot store at {}: {}",
                        self.path.display(),
                        err
                    );
                    StoredDailyNodeSnapshotStore::default()
                },
            },
            Err(err) if err.kind() == ErrorKind::NotFound => {
                StoredDailyNodeSnapshotStore::default()
            },
            Err(err) => {
                warn!(
                    "Failed to read daily node snapshot store at {}: {}",
                    self.path.display(),
                    err
                );
                StoredDailyNodeSnapshotStore::default()
            },
        }
    }

    fn write_store(
        &self,
        store: &StoredDailyNodeSnapshotStore,
    ) -> Result<()> {
        let Some(parent) = self.path.parent() else {
            return Err("daily node snapshot path has no parent directory".into());
        };

        fs::create_dir_all(parent)?;

        let temp_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let temp_path =
            parent.join(format!("{}.{}.tmp", DAILY_NODE_SNAPSHOT_FILE_NAME, temp_suffix));
        let content = serde_json::to_vec_pretty(store)?;

        fs::write(&temp_path, content)?;

        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        fs::rename(&temp_path, &self.path)?;

        Ok(())
    }
}

fn build_daily_summary_details(
    node_details: &[NodeDetail],
    previous_snapshot: Option<&StoredDailyNodeSnapshot>,
    monthly_snapshot: Option<&StoredDailyNodeSnapshot>,
    show_monthly_totals: bool,
) -> Vec<DailyNodeSummaryDetail> {
    node_details
        .iter()
        .map(|detail| {
            let previous = find_snapshot_detail(previous_snapshot, detail);
            let monthly = find_snapshot_detail(monthly_snapshot, detail);

            DailyNodeSummaryDetail {
                node_id: detail.node_id.clone(),
                node_name: detail.node_name.clone(),
                ranking: detail.ranking,
                block_qty: detail.block_qty,
                reward_value: detail.reward_value,
                daily_block_qty: previous
                    .map(|snapshot| detail.block_qty.saturating_sub(snapshot.block_qty)),
                daily_reward_value: previous.map(|snapshot| {
                    non_negative_reward_delta(detail.reward_value, snapshot.reward_value)
                }),
                show_monthly_totals,
                monthly_block_qty: monthly
                    .map(|snapshot| detail.block_qty.saturating_sub(snapshot.block_qty)),
                monthly_reward_value: monthly.map(|snapshot| {
                    non_negative_reward_delta(detail.reward_value, snapshot.reward_value)
                }),
            }
        })
        .collect()
}

fn previous_month_start_snapshot_date(date: NaiveDate) -> Option<NaiveDate> {
    if date.day() != 1 {
        return None;
    }

    let previous_day = date.pred_opt()?;

    NaiveDate::from_ymd_opt(previous_day.year(), previous_day.month(), 1)
}

fn find_snapshot_detail<'a>(
    snapshot: Option<&'a StoredDailyNodeSnapshot>,
    detail: &NodeDetail,
) -> Option<&'a StoredDailyNodeSnapshotDetail> {
    let snapshot = snapshot?;

    if !detail.node_id.is_empty() {
        if let Some(found) = snapshot
            .node_details
            .iter()
            .find(|candidate| !candidate.node_id.is_empty() && candidate.node_id == detail.node_id)
        {
            return Some(found);
        }
    }

    if !detail.node_name.is_empty() {
        if let Some(found) = snapshot.node_details.iter().find(|candidate| {
            !candidate.node_name.is_empty() && candidate.node_name == detail.node_name
        }) {
            return Some(found);
        }
    }

    None
}

fn non_negative_reward_delta(
    current: f64,
    previous: f64,
) -> f64 {
    if current > previous {
        current - previous
    } else {
        0.0
    }
}

fn default_daily_node_snapshot_path() -> PathBuf {
    if let Some(path) = env::var_os("CHAINDASH_STATE_DIR") {
        return PathBuf::from(path).join(DAILY_NODE_SNAPSHOT_FILE_NAME);
    }

    if let Some(path) = env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path)
            .join(DEFAULT_STATE_DIR_NAME)
            .join(DAILY_NODE_SNAPSHOT_FILE_NAME);
    }

    if let Some(path) = env::var_os("HOME") {
        return PathBuf::from(path)
            .join(".local")
            .join("state")
            .join(DEFAULT_STATE_DIR_NAME)
            .join(DAILY_NODE_SNAPSHOT_FILE_NAME);
    }

    PathBuf::from(DEFAULT_STATE_FALLBACK_DIR_NAME).join(DAILY_NODE_SNAPSHOT_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{
            SystemTime,
            UNIX_EPOCH,
        },
    };

    use chrono::NaiveDate;

    use super::*;

    fn sample_node_detail(
        node_id: &str,
        node_name: &str,
        ranking: i32,
        block_qty: u64,
        reward_value: f64,
    ) -> NodeDetail {
        NodeDetail {
            node_id: node_id.to_string(),
            node_name: node_name.to_string(),
            ranking,
            block_qty,
            block_rate: "75.00%".to_string(),
            daily_block_rate: "1/day".to_string(),
            reward_per: 10.0,
            reward_value,
            reward_address: "addr".to_string(),
            verifier_time: 0,
            last_updated_at: None,
        }
    }

    fn temp_snapshot_store_path() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);

        std::env::temp_dir()
            .join(format!("chaindash-daily-snapshot-test-{suffix}"))
            .join(DAILY_NODE_SNAPSHOT_FILE_NAME)
    }

    #[test]
    fn test_daily_summary_details_use_previous_snapshot_delta() {
        let path = temp_snapshot_store_path();
        let store = DailyNodeSnapshotStore::new(path.clone());
        let previous_date = NaiveDate::from_ymd_opt(2026, 4, 14).expect("date should be valid");
        let current_date = previous_date.succ_opt().expect("date should advance");

        store
            .save_snapshot(previous_date, &[sample_node_detail("node-a", "node-a", 1, 100, 10.0)])
            .expect("snapshot should save");

        let details = store.daily_summary_details(
            current_date,
            &[sample_node_detail("node-a", "node-a", 1, 135, 18.5)],
        );

        assert_eq!(details.len(), 1);
        assert_eq!(details[0].daily_block_qty, Some(35));
        assert_eq!(details[0].daily_reward_value, Some(8.5));
        assert!(!details[0].show_monthly_totals);
        assert_eq!(details[0].monthly_block_qty, None);
        assert_eq!(details[0].monthly_reward_value, None);

        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn test_daily_summary_details_leave_daily_values_empty_without_previous_snapshot() {
        let store = DailyNodeSnapshotStore::new(temp_snapshot_store_path());
        let current_date = NaiveDate::from_ymd_opt(2026, 4, 15).expect("date should be valid");

        let details = store.daily_summary_details(
            current_date,
            &[sample_node_detail("node-a", "node-a", 1, 135, 18.5)],
        );

        assert_eq!(details.len(), 1);
        assert_eq!(details[0].daily_block_qty, None);
        assert_eq!(details[0].daily_reward_value, None);
        assert!(!details[0].show_monthly_totals);
        assert_eq!(details[0].monthly_block_qty, None);
        assert_eq!(details[0].monthly_reward_value, None);
    }

    #[test]
    fn test_save_snapshot_round_trips_previous_day_data() {
        let path = temp_snapshot_store_path();
        let store = DailyNodeSnapshotStore::new(path.clone());
        let snapshot_date = NaiveDate::from_ymd_opt(2026, 4, 14).expect("date should be valid");

        store
            .save_snapshot(snapshot_date, &[sample_node_detail("node-a", "node-a", 1, 100, 10.0)])
            .expect("snapshot should save");

        let details = store.daily_summary_details(
            snapshot_date.succ_opt().expect("date should advance"),
            &[sample_node_detail("node-a", "node-a", 1, 120, 15.0)],
        );

        assert_eq!(details[0].daily_block_qty, Some(20));
        assert_eq!(details[0].daily_reward_value, Some(5.0));
        assert!(!details[0].show_monthly_totals);
        assert_eq!(details[0].monthly_block_qty, None);
        assert_eq!(details[0].monthly_reward_value, None);

        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn test_daily_summary_details_include_previous_month_totals_on_first_day() {
        let path = temp_snapshot_store_path();
        let store = DailyNodeSnapshotStore::new(path.clone());
        let month_start = NaiveDate::from_ymd_opt(2026, 4, 1).expect("date should be valid");
        let previous_day = NaiveDate::from_ymd_opt(2026, 4, 30).expect("date should be valid");
        let report_date = NaiveDate::from_ymd_opt(2026, 5, 1).expect("date should be valid");

        store
            .save_snapshot(month_start, &[sample_node_detail("node-a", "node-a", 1, 100, 10.0)])
            .expect("snapshot should save");
        store
            .save_snapshot(previous_day, &[sample_node_detail("node-a", "node-a", 1, 145, 16.5)])
            .expect("snapshot should save");

        let details = store.daily_summary_details(
            report_date,
            &[sample_node_detail("node-a", "node-a", 1, 150, 18.0)],
        );

        assert_eq!(details.len(), 1);
        assert_eq!(details[0].daily_block_qty, Some(5));
        assert_eq!(details[0].daily_reward_value, Some(1.5));
        assert!(details[0].show_monthly_totals);
        assert_eq!(details[0].monthly_block_qty, Some(50));
        assert_eq!(details[0].monthly_reward_value, Some(8.0));

        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}
