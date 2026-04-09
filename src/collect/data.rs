use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
    time::{
        Duration as StdDuration,
        Instant,
    },
};

use crossbeam_channel::Sender;
use log::warn;

use crate::sync::lock_or_panic;

#[derive(Debug, Clone, Default)]
pub struct ConsensusState {
    pub name: String,
    pub host: String,
    pub current_number: u64,
    pub epoch: u64,
    pub view: u64,
    pub committed: u64,
    pub locked: u64,
    pub qc: u64,
    pub validator: bool,
}

#[derive(Debug, Clone, Default)]
pub struct NodeDetail {
    pub node_id: String,
    pub node_name: String,
    pub ranking: i32,
    pub block_qty: u64,
    pub block_rate: String,
    pub daily_block_rate: String,
    pub reward_per: f64,
    pub reward_value: f64,
    pub reward_address: String,
    pub verifier_time: u64,
    pub last_updated_at: Option<Instant>,
}

impl NodeDetail {
    pub fn rewards(&self) -> f64 {
        self.reward_value * (1.0 - self.reward_per / 100.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub level: StatusLevel,
    pub text: String,
    expires_at: Option<Instant>,
}

#[cfg(target_family = "unix")]
#[derive(Debug, Clone)]
pub struct SystemStats {
    pub cpu_usage: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub memory_usage_percent: f32,
    pub network_rx: u64,
    pub network_tx: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub disk_usage_percent: f32,
    pub disk_details: Vec<DiskDetail>,
    pub current_disk_index: usize,
    pub alert_threshold: f32,
    pub has_disk_alerts: bool,
    pub auto_discovery_enabled: bool,
}

#[cfg(target_family = "unix")]
#[derive(Debug, Clone)]
pub struct DiskDetail {
    pub mount_point: String,
    pub filesystem: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub usage_percent: f32,
    pub device: String,
    pub is_alert: bool,
    pub is_network: bool,
    pub last_updated: Instant,
}

#[cfg(target_family = "unix")]
impl Default for SystemStats {
    fn default() -> Self {
        SystemStats {
            cpu_usage: 0.0,
            memory_used: 0,
            memory_total: 0,
            memory_usage_percent: 0.0,
            network_rx: 0,
            network_tx: 0,
            disk_used: 0,
            disk_total: 0,
            disk_usage_percent: 0.0,
            disk_details: Vec::new(),
            current_disk_index: 0,
            alert_threshold: 90.0,
            has_disk_alerts: false,
            auto_discovery_enabled: true,
        }
    }
}

#[derive(Debug)]
struct ChainStats {
    cur_block_number: u64,
    cur_block_time: u64,
    prev_block_time: u64,
    cur_txs: u64,
    max_txs: u64,
    max_txs_block_number: u64,
    txns: Vec<u64>,
    intervals: Vec<u64>,
    cur_interval: u64,
    max_interval: u64,
}

impl Default for ChainStats {
    fn default() -> Self {
        Self {
            cur_block_number: 0,
            cur_block_time: 0,
            prev_block_time: 0,
            cur_txs: 0,
            max_txs: 0,
            max_txs_block_number: 0,
            txns: vec![0],
            intervals: vec![0],
            cur_interval: 0,
            max_interval: 0,
        }
    }
}

impl ChainStats {
    fn record_block_sample(
        &mut self,
        block_number: u64,
        block_timestamp: u64,
        txs: u64,
    ) {
        self.cur_block_number = block_number;
        if self.cur_block_time > 0 {
            self.prev_block_time = self.cur_block_time;
        }
        self.cur_block_time = block_timestamp;
        self.cur_txs = txs;

        if txs > self.max_txs {
            self.max_txs = txs;
            self.max_txs_block_number = block_number;
        }
        self.txns.push(txs);
        if self.prev_block_time > 0 {
            let interval_ms =
                self.cur_block_time.saturating_sub(self.prev_block_time).saturating_mul(1000);
            self.cur_interval = interval_ms;
            if interval_ms > self.max_interval {
                self.max_interval = interval_ms;
            }
            self.intervals.push(interval_ms);
        }
    }
}

#[derive(Debug, Default)]
struct NodeStateStore {
    states: HashMap<String, ConsensusState>,
}

impl NodeStateStore {
    fn sorted(&self) -> Vec<ConsensusState> {
        let mut states: Vec<_> = self.states.values().cloned().collect();
        states.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.host.cmp(&right.host))
                .then_with(|| left.current_number.cmp(&right.current_number))
        });
        states
    }

    fn update(
        &mut self,
        name: String,
        state: ConsensusState,
    ) {
        self.states.insert(name, state);
    }
}

const DEFAULT_NODE_DETAIL_KEY: &str = "__default__";

#[derive(Debug, Default)]
struct NodeDetailStore {
    details: HashMap<String, NodeDetail>,
    loaded: bool,
}

impl NodeDetailStore {
    fn sorted(&self) -> Vec<NodeDetail> {
        let mut node_details: Vec<_> = self.details.values().cloned().collect();
        node_details.sort_by(|left, right| {
            let left_unranked = left.ranking <= 0;
            let right_unranked = right.ranking <= 0;
            let left_ranking = if left_unranked {
                i32::MAX
            } else {
                left.ranking
            };
            let right_ranking = if right_unranked {
                i32::MAX
            } else {
                right.ranking
            };

            left_unranked
                .cmp(&right_unranked)
                .then_with(|| left_ranking.cmp(&right_ranking))
                .then_with(|| left.node_name.cmp(&right.node_name))
                .then_with(|| left.node_id.cmp(&right.node_id))
        });
        node_details
    }

    fn first(&self) -> Option<NodeDetail> {
        self.sorted().into_iter().next()
    }

    fn update_node_detail(
        &mut self,
        detail: Option<NodeDetail>,
    ) {
        self.details.clear();

        let Some(mut detail) = detail else {
            return;
        };

        if detail.node_name.is_empty() && !detail.node_id.is_empty() {
            detail.node_name = detail.node_id.clone();
        }

        self.details.insert(Self::node_detail_key(&detail.node_id), detail);
        self.loaded = true;
    }

    fn merge_node_ranking(
        &mut self,
        ranking: Option<i32>,
    ) {
        let Some(ranking) = ranking else {
            return;
        };

        if self.details.len() == 1 {
            if let Some(detail) = self.details.values_mut().next() {
                detail.ranking = ranking;
                return;
            }
        }

        self.merge_node_ranking_for("", Some(ranking));
    }

    fn merge_node_ranking_for(
        &mut self,
        node_id: &str,
        ranking: Option<i32>,
    ) {
        let Some(ranking) = ranking else {
            return;
        };

        let key = Self::node_detail_key(node_id);
        if let Some(detail) = self.details.get_mut(&key) {
            detail.ranking = ranking;
            if detail.node_id.is_empty() && !node_id.is_empty() {
                detail.node_id = node_id.to_string();
            }
            if detail.node_name.is_empty() && !node_id.is_empty() {
                detail.node_name = node_id.to_string();
            }
        }
    }

    fn merge_node_detail(
        &mut self,
        detail: Option<NodeDetail>,
    ) {
        let Some(mut detail) = detail else {
            return;
        };

        let node_id = if detail.node_id.is_empty() && self.details.len() == 1 {
            self.details.keys().next().cloned().unwrap_or_default()
        } else {
            detail.node_id.clone()
        };

        if detail.node_id.is_empty() && node_id != DEFAULT_NODE_DETAIL_KEY {
            detail.node_id = node_id.clone();
        }

        self.merge_node_detail_for(&node_id, Some(detail));
    }

    fn merge_node_detail_for(
        &mut self,
        node_id: &str,
        detail: Option<NodeDetail>,
    ) {
        let Some(mut detail) = detail else {
            return;
        };

        let key = Self::node_detail_key(node_id);
        if detail.node_id.is_empty() {
            detail.node_id = node_id.to_string();
        }
        if let Some(existing) = self.details.get(&key) {
            detail.ranking = existing.ranking;
        }
        if detail.node_name.is_empty() && !detail.node_id.is_empty() {
            detail.node_name = detail.node_id.clone();
        }

        self.details.insert(key, detail);
        self.loaded = true;
    }

    fn remove(
        &mut self,
        node_id: &str,
    ) {
        self.details.remove(&Self::node_detail_key(node_id));
        self.loaded = true;
    }

    fn mark_loaded(&mut self) {
        self.loaded = true;
    }

    fn node_detail_key(node_id: &str) -> String {
        if node_id.is_empty() {
            DEFAULT_NODE_DETAIL_KEY.to_string()
        } else {
            node_id.to_string()
        }
    }
}

const INFO_STATUS_TTL: StdDuration = StdDuration::from_secs(5);
const WARN_STATUS_TTL: StdDuration = StdDuration::from_secs(15);

#[derive(Debug, Default)]
struct UiStatusStore {
    status_message: Option<StatusMessage>,
}

impl UiStatusStore {
    fn current(&self) -> Option<StatusMessage> {
        self.status_message.clone()
    }

    fn expire_if_needed(&mut self) {
        if self
            .status_message
            .as_ref()
            .and_then(|message| message.expires_at)
            .is_some_and(|expires_at| Instant::now() >= expires_at)
        {
            self.status_message = None;
        }
    }

    fn set(
        &mut self,
        level: StatusLevel,
        text: impl Into<String>,
    ) -> bool {
        let text = text.into();
        let now = Instant::now();

        if self
            .status_message
            .as_ref()
            .filter(|message| {
                message.level == level
                    && message.text == text
                    && message.expires_at.is_none_or(|expires_at| now < expires_at)
            })
            .is_some()
        {
            return false;
        }

        let expires_at = match level {
            StatusLevel::Info => Some(now + INFO_STATUS_TTL),
            StatusLevel::Warn => Some(now + WARN_STATUS_TTL),
            StatusLevel::Error => None,
        };

        self.status_message = Some(StatusMessage {
            level,
            text,
            expires_at,
        });

        true
    }

    fn clear(&mut self) -> bool {
        let had_status = self.status_message.is_some();
        self.status_message = None;
        had_status
    }
}

#[cfg(target_family = "unix")]
#[derive(Debug, Default)]
struct SystemState {
    stats: SystemStats,
}

#[cfg(target_family = "unix")]
impl SystemState {
    fn replace_stats(
        &mut self,
        mut system_stats: SystemStats,
    ) -> bool {
        let previous_alert = self.stats.has_disk_alerts;
        system_stats.current_disk_index = if system_stats.disk_details.is_empty() {
            0
        } else {
            self.stats.current_disk_index.min(system_stats.disk_details.len().saturating_sub(1))
        };
        self.stats = system_stats;
        previous_alert
    }

    fn stats(&self) -> SystemStats {
        self.stats.clone()
    }

    fn update_disk_index(
        &mut self,
        new_index: usize,
    ) {
        self.stats.current_disk_index = if self.stats.disk_details.is_empty() {
            0
        } else {
            new_index.min(self.stats.disk_details.len().saturating_sub(1))
        };
    }

    #[cfg(test)]
    fn set_disk_details_for_test(
        &mut self,
        details: Vec<DiskDetail>,
    ) {
        self.stats.disk_details = details;
    }

    #[cfg(test)]
    fn current_disk_index_for_test(&self) -> usize {
        self.stats.current_disk_index
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiDirtyState {
    pub chain: bool,
    pub node_state: bool,
    pub node_details: bool,
    pub status: bool,
    #[cfg(target_family = "unix")]
    pub system: bool,
}

impl UiDirtyState {
    fn any(self) -> bool {
        self.chain || self.node_state || self.node_details || self.status || {
            #[cfg(target_family = "unix")]
            {
                self.system
            }
            #[cfg(not(target_family = "unix"))]
            {
                false
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct Data {
    chain: ChainStats,
    node_state: NodeStateStore,
    node_details: NodeDetailStore,
    status: UiStatusStore,
    ui_dirty: UiDirtyState,
    ui_waker: Option<Sender<()>>,
    #[cfg(target_family = "unix")]
    system: SystemState,
}

pub type SharedData = Arc<Mutex<Data>>;

pub(crate) fn record_status_message(
    data: &SharedData,
    level: StatusLevel,
    text: impl Into<String>,
) {
    let mut data = lock_or_panic(data);
    data.set_status_message(level, text);
}

pub(crate) fn warn_with_status(
    data: &SharedData,
    message: impl Into<String>,
) {
    let message = message.into();
    warn!("{message}");
    record_status_message(data, StatusLevel::Warn, message);
}

impl Data {
    pub fn new() -> SharedData {
        Arc::new(Mutex::new(Data::default()))
    }

    fn notify_ui_if_needed(&self) {
        if !self.ui_dirty.any() {
            return;
        }

        if let Some(sender) = &self.ui_waker {
            let _ = sender.try_send(());
        }
    }

    fn mark_chain_dirty(&mut self) {
        self.ui_dirty.chain = true;
        self.notify_ui_if_needed();
    }

    fn mark_node_state_dirty(&mut self) {
        self.ui_dirty.node_state = true;
        self.notify_ui_if_needed();
    }

    fn mark_node_details_dirty(&mut self) {
        self.ui_dirty.node_details = true;
        self.notify_ui_if_needed();
    }

    fn mark_status_dirty(&mut self) {
        self.ui_dirty.status = true;
        self.notify_ui_if_needed();
    }

    #[cfg(target_family = "unix")]
    fn mark_system_dirty(&mut self) {
        self.ui_dirty.system = true;
        self.notify_ui_if_needed();
    }

    pub(crate) fn set_ui_waker(
        &mut self,
        sender: Sender<()>,
    ) {
        self.ui_waker = Some(sender);
    }

    pub(crate) fn take_ui_dirty(&mut self) -> UiDirtyState {
        std::mem::take(&mut self.ui_dirty)
    }

    pub fn cur_block_number(&self) -> u64 {
        self.chain.cur_block_number
    }

    pub fn cur_block_time(&self) -> u64 {
        self.chain.cur_block_time
    }

    pub fn prev_block_time(&self) -> u64 {
        self.chain.prev_block_time
    }

    pub fn cur_txs(&self) -> u64 {
        self.chain.cur_txs
    }

    pub fn max_txs(&self) -> u64 {
        self.chain.max_txs
    }

    pub fn max_txs_block_number(&self) -> u64 {
        self.chain.max_txs_block_number
    }

    pub fn txns_and_clear(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.chain.txns)
    }

    pub fn intervals_and_clear(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.chain.intervals)
    }

    pub fn cur_interval(&self) -> u64 {
        self.chain.cur_interval
    }

    pub fn max_interval(&self) -> u64 {
        self.chain.max_interval
    }

    pub fn states(&self) -> Vec<ConsensusState> {
        self.node_state.sorted()
    }

    pub fn node_detail(&self) -> Option<NodeDetail> {
        self.node_details.first()
    }

    pub fn node_details_loaded(&self) -> bool {
        self.node_details.loaded
    }

    pub fn node_details(&self) -> Vec<NodeDetail> {
        self.node_details.sorted()
    }

    pub fn update_node_detail(
        &mut self,
        detail: Option<NodeDetail>,
    ) {
        self.node_details.update_node_detail(detail);
        self.mark_node_details_dirty();
    }

    pub fn merge_node_ranking(
        &mut self,
        ranking: Option<i32>,
    ) {
        self.node_details.merge_node_ranking(ranking);
        self.mark_node_details_dirty();
    }

    pub fn merge_node_ranking_for(
        &mut self,
        node_id: &str,
        ranking: Option<i32>,
    ) {
        self.node_details.merge_node_ranking_for(node_id, ranking);
        self.mark_node_details_dirty();
    }

    pub fn merge_node_detail(
        &mut self,
        detail: Option<NodeDetail>,
    ) {
        self.node_details.merge_node_detail(detail);
        self.mark_node_details_dirty();
    }

    pub fn merge_node_detail_for(
        &mut self,
        node_id: &str,
        detail: Option<NodeDetail>,
    ) {
        self.node_details.merge_node_detail_for(node_id, detail);
        self.mark_node_details_dirty();
    }

    pub(crate) fn record_block_sample(
        &mut self,
        block_number: u64,
        block_timestamp: u64,
        txs: u64,
    ) {
        self.chain.record_block_sample(block_number, block_timestamp, txs);
        self.mark_chain_dirty();
    }

    pub(crate) fn update_consensus_state(
        &mut self,
        name: String,
        state: ConsensusState,
    ) {
        self.node_state.update(name, state);
        self.mark_node_state_dirty();
    }

    #[cfg(target_family = "unix")]
    pub(crate) fn replace_system_stats(
        &mut self,
        system_stats: SystemStats,
    ) -> bool {
        let previous_alert = self.system.replace_stats(system_stats);
        self.mark_system_dirty();
        previous_alert
    }

    #[cfg(target_family = "unix")]
    pub fn system_stats(&self) -> SystemStats {
        self.system.stats()
    }

    #[cfg(target_family = "unix")]
    pub fn update_disk_index(
        &mut self,
        new_index: usize,
    ) {
        self.system.update_disk_index(new_index);
    }

    pub fn status_message(&self) -> Option<StatusMessage> {
        self.status.current()
    }

    pub fn expire_status_message_if_needed(&mut self) {
        self.status.expire_if_needed();
    }

    pub fn set_status_message(
        &mut self,
        level: StatusLevel,
        text: impl Into<String>,
    ) {
        if self.status.set(level, text) {
            self.mark_status_dirty();
        }
    }

    pub fn clear_status_message(&mut self) {
        if self.status.clear() {
            self.mark_status_dirty();
        }
    }

    pub fn remove_node_detail(
        &mut self,
        node_id: &str,
    ) {
        self.node_details.remove(node_id);
        self.mark_node_details_dirty();
    }

    pub fn mark_node_details_loaded(&mut self) {
        self.node_details.mark_loaded();
        self.mark_node_details_dirty();
    }
}

#[cfg(test)]
#[cfg(target_family = "unix")]
impl Data {
    pub fn set_disk_details_for_test(
        &mut self,
        details: Vec<DiskDetail>,
    ) {
        self.system.set_disk_details_for_test(details);
    }

    pub fn current_disk_index_for_test(&self) -> usize {
        self.system.current_disk_index_for_test()
    }
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::bounded;

    use super::*;

    #[test]
    fn test_merge_node_ranking_preserves_existing_detail_fields() {
        let mut data = Data::default();
        data.update_node_detail(Some(NodeDetail {
            node_id: "node-a-id".to_string(),
            node_name: "node-a".to_string(),
            ranking: 1,
            block_qty: 12,
            block_rate: "75.00%".to_string(),
            daily_block_rate: "1/day".to_string(),
            reward_per: 10.0,
            reward_value: 20.0,
            reward_address: "addr".to_string(),
            verifier_time: 30,
            last_updated_at: None,
        }));

        data.merge_node_ranking(Some(9));

        let detail = data.node_detail().expect("node detail should exist");
        assert_eq!(detail.ranking, 9);
        assert_eq!(detail.node_name, "node-a");
        assert_eq!(detail.block_qty, 12);
    }

    #[test]
    fn test_merge_node_ranking_for_missing_detail_does_not_create_placeholder() {
        let mut data = Data::default();

        data.merge_node_ranking_for("missing-node-id", Some(9));

        assert!(data.node_detail().is_none());
        assert!(data.node_details().is_empty());
    }

    #[test]
    fn test_merge_node_detail_preserves_existing_ranking() {
        let mut data = Data::default();
        data.update_node_detail(Some(NodeDetail {
            node_id: "node-a-id".to_string(),
            node_name: "old-node".to_string(),
            ranking: 7,
            block_qty: 12,
            block_rate: "75.00%".to_string(),
            daily_block_rate: "1/day".to_string(),
            reward_per: 10.0,
            reward_value: 20.0,
            reward_address: "old-addr".to_string(),
            verifier_time: 30,
            last_updated_at: None,
        }));

        data.merge_node_detail(Some(NodeDetail {
            node_id: "node-a-id".to_string(),
            node_name: "new-node".to_string(),
            ranking: 0,
            block_qty: 24,
            block_rate: "80.00%".to_string(),
            daily_block_rate: "2/day".to_string(),
            reward_per: 5.0,
            reward_value: 40.0,
            reward_address: "new-addr".to_string(),
            verifier_time: 60,
            last_updated_at: None,
        }));

        let detail = data.node_detail().expect("node detail should exist");
        assert_eq!(detail.ranking, 7);
        assert_eq!(detail.node_name, "new-node");
        assert_eq!(detail.block_qty, 24);
    }

    #[test]
    fn test_merge_node_ranking_none_preserves_existing_detail() {
        let mut data = Data::default();
        let existing = NodeDetail {
            node_id: "node-a-id".to_string(),
            node_name: "node-a".to_string(),
            ranking: 3,
            block_qty: 12,
            block_rate: "75.00%".to_string(),
            daily_block_rate: "1/day".to_string(),
            reward_per: 10.0,
            reward_value: 20.0,
            reward_address: "addr".to_string(),
            verifier_time: 30,
            last_updated_at: None,
        };
        data.update_node_detail(Some(existing.clone()));

        data.merge_node_ranking(None);

        assert_eq!(data.node_detail().expect("node detail should exist").ranking, existing.ranking);
        assert_eq!(
            data.node_detail().expect("node detail should exist").node_name,
            existing.node_name
        );
    }

    #[test]
    fn test_merge_node_detail_none_preserves_existing_ranking() {
        let mut data = Data::default();
        let existing = NodeDetail {
            node_id: "node-a-id".to_string(),
            node_name: "node-a".to_string(),
            ranking: 3,
            block_qty: 12,
            block_rate: "75.00%".to_string(),
            daily_block_rate: "1/day".to_string(),
            reward_per: 10.0,
            reward_value: 20.0,
            reward_address: "addr".to_string(),
            verifier_time: 30,
            last_updated_at: None,
        };
        data.update_node_detail(Some(existing.clone()));

        data.merge_node_detail(None);

        assert_eq!(data.node_detail().expect("node detail should exist").ranking, existing.ranking);
        assert_eq!(
            data.node_detail().expect("node detail should exist").node_name,
            existing.node_name
        );
    }

    #[test]
    fn test_remove_node_detail_removes_only_target() {
        let mut data = Data::default();
        data.merge_node_detail_for(
            "node-a-id",
            Some(NodeDetail {
                node_id: "node-a-id".to_string(),
                node_name: "node-a".to_string(),
                ranking: 1,
                block_qty: 12,
                block_rate: "75.00%".to_string(),
                daily_block_rate: "1/day".to_string(),
                reward_per: 10.0,
                reward_value: 20.0,
                reward_address: "addr-a".to_string(),
                verifier_time: 30,
                last_updated_at: None,
            }),
        );
        data.merge_node_detail_for(
            "node-b-id",
            Some(NodeDetail {
                node_id: "node-b-id".to_string(),
                node_name: "node-b".to_string(),
                ranking: 2,
                block_qty: 24,
                block_rate: "80.00%".to_string(),
                daily_block_rate: "2/day".to_string(),
                reward_per: 5.0,
                reward_value: 40.0,
                reward_address: "addr-b".to_string(),
                verifier_time: 60,
                last_updated_at: None,
            }),
        );

        data.remove_node_detail("node-a-id");

        let details = data.node_details();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].node_id, "node-b-id");
    }

    #[test]
    fn test_states_returns_stably_sorted_results() {
        let mut data = Data::default();
        data.node_state.states.insert(
            "node-b".to_string(),
            ConsensusState {
                name: "node-b".to_string(),
                host: "host-b".to_string(),
                current_number: 2,
                ..Default::default()
            },
        );
        data.node_state.states.insert(
            "node-a".to_string(),
            ConsensusState {
                name: "node-a".to_string(),
                host: "host-a".to_string(),
                current_number: 1,
                ..Default::default()
            },
        );

        let names: Vec<String> = data.states().into_iter().map(|state| state.name).collect();

        assert_eq!(names, vec!["node-a".to_string(), "node-b".to_string()]);
    }

    #[test]
    fn test_expire_status_message_if_needed_clears_expired_entries() {
        let mut data = Data::default();
        data.status.status_message = Some(StatusMessage {
            level: StatusLevel::Info,
            text: "expired".to_string(),
            expires_at: Some(Instant::now() - StdDuration::from_secs(1)),
        });

        assert!(data.status_message().is_some());
        data.expire_status_message_if_needed();
        assert!(data.status_message().is_none());
        assert!(data.status.status_message.is_none());
    }

    #[test]
    fn test_set_status_message_applies_ttl_by_level() {
        let mut data = Data::default();

        data.set_status_message(StatusLevel::Info, "info");
        assert!(data
            .status
            .status_message
            .as_ref()
            .and_then(|message| message.expires_at)
            .is_some());

        data.set_status_message(StatusLevel::Warn, "warn");
        assert!(data
            .status
            .status_message
            .as_ref()
            .and_then(|message| message.expires_at)
            .is_some());

        data.set_status_message(StatusLevel::Error, "error");
        assert!(data
            .status
            .status_message
            .as_ref()
            .is_some_and(|message| message.expires_at.is_none()));
    }

    #[test]
    fn test_set_status_message_deduplicates_same_active_message() {
        let mut data = Data::default();

        data.set_status_message(StatusLevel::Warn, "warn");
        let first = data.status.status_message.clone().expect("status should exist");

        data.set_status_message(StatusLevel::Warn, "warn");
        let second = data.status.status_message.clone().expect("status should exist");

        assert_eq!(first.level, second.level);
        assert_eq!(first.text, second.text);
        assert_eq!(first.expires_at, second.expires_at);
    }

    #[test]
    fn test_set_status_message_recreates_same_message_after_expiry() {
        let mut data = Data::default();
        data.status.status_message = Some(StatusMessage {
            level: StatusLevel::Warn,
            text: "warn".to_string(),
            expires_at: Some(Instant::now() - StdDuration::from_secs(1)),
        });

        data.set_status_message(StatusLevel::Warn, "warn");

        assert!(data.status.status_message.as_ref().is_some_and(|message| {
            message.text == "warn"
                && message.level == StatusLevel::Warn
                && message.expires_at.is_some_and(|expires_at| expires_at > Instant::now())
        }));
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_update_disk_index_clamps_to_last_available_disk() {
        let mut data = Data::default();
        data.set_disk_details_for_test(vec![
            DiskDetail {
                mount_point: "/".to_string(),
                filesystem: "ext4".to_string(),
                total: 100,
                used: 50,
                available: 50,
                usage_percent: 50.0,
                device: "/dev/sda1".to_string(),
                is_alert: false,
                is_network: false,
                last_updated: Instant::now(),
            },
            DiskDetail {
                mount_point: "/data".to_string(),
                filesystem: "ext4".to_string(),
                total: 200,
                used: 100,
                available: 100,
                usage_percent: 50.0,
                device: "/dev/sdb1".to_string(),
                is_alert: false,
                is_network: false,
                last_updated: Instant::now(),
            },
        ]);

        data.update_disk_index(10);

        assert_eq!(data.current_disk_index_for_test(), 1);
    }

    #[test]
    fn test_record_block_sample_marks_chain_dirty_and_notifies_ui() {
        let (sender, receiver) = bounded(1);
        let mut data = Data::default();
        data.set_ui_waker(sender);

        data.record_block_sample(10, 20, 30);

        receiver.try_recv().expect("ui should be notified");
        assert_eq!(
            data.take_ui_dirty(),
            UiDirtyState {
                chain: true,
                ..UiDirtyState::default()
            }
        );
    }

    #[test]
    fn test_duplicate_status_message_does_not_notify_ui_again() {
        let (sender, receiver) = bounded(1);
        let mut data = Data::default();
        data.set_ui_waker(sender);

        data.set_status_message(StatusLevel::Warn, "warn");
        receiver.try_recv().expect("first status should notify ui");
        assert_eq!(
            data.take_ui_dirty(),
            UiDirtyState {
                status: true,
                ..UiDirtyState::default()
            }
        );

        data.set_status_message(StatusLevel::Warn, "warn");

        assert!(receiver.try_recv().is_err());
        assert_eq!(data.take_ui_dirty(), UiDirtyState::default());
    }
}
