use std::{
    collections::HashMap,
    convert::TryFrom,
    sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        Arc,
        Mutex,
    },
    time::{
        Duration as StdDuration,
        Instant,
    },
};

use alloy::{
    eips::BlockNumberOrTag,
    providers::{
        ext::DebugApi,
        Provider,
        ProviderBuilder,
        WsConnect,
    },
};
use futures::StreamExt;
use log::{
    debug,
    warn,
};
#[cfg(target_family = "unix")]
use sysinfo::{
    Disks,
    System,
};
use tokio::time::{
    self,
    Duration,
};

use super::types::NodeInfo;
use crate::{
    collect::types,
    error::{
        ChaindashError,
        Result,
    },
    opts::Opts,
};

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
    pub reward_per: f64, // percentage, e.g., 50.0
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

#[derive(Debug)]
pub struct Data {
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

    states: HashMap<String, ConsensusState>,
    status_message: Option<StatusMessage>,
    node_details: HashMap<String, NodeDetail>,
    node_details_loaded: bool,
    #[cfg(target_family = "unix")]
    system_stats: SystemStats,
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
    pub last_updated: std::time::Instant,
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

pub type SharedData = Arc<Mutex<Data>>;

fn record_status_message(
    data: &SharedData,
    level: StatusLevel,
    text: impl Into<String>,
) {
    let mut data = data.lock().expect("mutex poisoned - recovering");
    data.set_status_message(level, text);
}

fn warn_with_status(
    data: &SharedData,
    message: impl Into<String>,
) {
    let message = message.into();
    warn!("{message}");
    record_status_message(data, StatusLevel::Warn, message);
}

fn summarize_node_detail_failures(node_ids: &[String]) -> String {
    if node_ids.is_empty() {
        return "Node details unavailable".to_string();
    }

    if node_ids.len() == 1 {
        return format!("Node detail unavailable for {}", node_ids[0]);
    }

    let preview: Vec<&str> =
        node_ids.iter().take(NODE_DETAIL_STATUS_PREVIEW_COUNT).map(String::as_str).collect();
    let preview = preview.join(", ");
    let remaining = node_ids.len().saturating_sub(NODE_DETAIL_STATUS_PREVIEW_COUNT);

    if remaining == 0 {
        format!("Node details unavailable for {} node(s): {}", node_ids.len(), preview)
    } else {
        format!(
            "Node details unavailable for {} node(s): {}, +{} more",
            node_ids.len(),
            preview,
            remaining
        )
    }
}

const COLLECTOR_RETRY_DELAY: Duration = Duration::from_secs(1);
const INFO_STATUS_TTL: StdDuration = StdDuration::from_secs(5);
const WARN_STATUS_TTL: StdDuration = StdDuration::from_secs(15);
const NODE_DETAIL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const NODE_DETAIL_STATUS_PREVIEW_COUNT: usize = 3;
const DEFAULT_NODE_DETAIL_KEY: &str = "__default__";

fn is_websocket_endpoint(url: &str) -> bool {
    url.starts_with("ws://") || url.starts_with("wss://")
}

fn websocket_host(url: &str) -> String {
    url.trim_start_matches("ws://").trim_start_matches("wss://").to_string()
}

#[derive(Debug, Clone, Copy)]
struct NetworkSample {
    rx_total: u64,
    tx_total: u64,
    collected_at: std::time::Instant,
}

fn compute_network_rates(
    previous: Option<NetworkSample>,
    rx_total: u64,
    tx_total: u64,
    collected_at: std::time::Instant,
) -> (NetworkSample, u64, u64) {
    let current = NetworkSample {
        rx_total,
        tx_total,
        collected_at,
    };

    let Some(previous) = previous else {
        return (current, 0, 0);
    };

    let elapsed = collected_at.saturating_duration_since(previous.collected_at);
    let elapsed_secs = elapsed.as_secs_f64();
    if elapsed_secs <= f64::EPSILON {
        return (current, 0, 0);
    }

    let network_rx_rate = (rx_total.saturating_sub(previous.rx_total) as f64 / elapsed_secs) as u64;
    let network_tx_rate = (tx_total.saturating_sub(previous.tx_total) as f64 / elapsed_secs) as u64;

    (current, network_rx_rate, network_tx_rate)
}

#[derive(Debug)]
pub struct Collector {
    data: SharedData,
    urls: Vec<(String, String)>,
    disk_mount_points: Vec<String>,
    disk_auto_discovery: bool,
    disk_alert_threshold: f32,
    disk_refresh_interval: u64,
    node_ids: Vec<String>,
    explorer_api_url: String,
    stop_flag: Arc<AtomicBool>,
}

pub async fn run(collector: Arc<Collector>) -> Result<()> {
    tokio::select! {
        res = collector.run() => {
            res
        }
    }
}

impl Default for Data {
    fn default() -> Data {
        Data {
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
            states: HashMap::new(),
            status_message: None,
            node_details: HashMap::new(),
            node_details_loaded: false,
            #[cfg(target_family = "unix")]
            system_stats: SystemStats::default(),
        }
    }
}

impl Data {
    pub fn new() -> SharedData {
        Arc::new(Mutex::new(Data::default()))
    }

    pub fn cur_block_number(&self) -> u64 {
        self.cur_block_number
    }

    pub fn cur_block_time(&self) -> u64 {
        self.cur_block_time
    }

    pub fn prev_block_time(&self) -> u64 {
        self.prev_block_time
    }

    pub fn cur_txs(&self) -> u64 {
        self.cur_txs
    }

    pub fn max_txs(&self) -> u64 {
        self.max_txs
    }

    pub fn max_txs_block_number(&self) -> u64 {
        self.max_txs_block_number
    }

    pub fn txns_and_clear(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.txns)
    }

    pub fn intervals_and_clear(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.intervals)
    }

    pub fn cur_interval(&self) -> u64 {
        self.cur_interval
    }

    pub fn max_interval(&self) -> u64 {
        self.max_interval
    }

    pub fn states(&self) -> Vec<ConsensusState> {
        let mut states: Vec<_> = self.states.values().cloned().collect();
        states.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.host.cmp(&right.host))
                .then_with(|| left.current_number.cmp(&right.current_number))
        });
        states
    }

    pub fn node_detail(&self) -> Option<NodeDetail> {
        self.node_details().into_iter().next()
    }

    pub fn node_details_loaded(&self) -> bool {
        self.node_details_loaded
    }

    pub fn node_details(&self) -> Vec<NodeDetail> {
        let mut node_details: Vec<_> = self.node_details.values().cloned().collect();
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

    pub fn update_node_detail(
        &mut self,
        detail: Option<NodeDetail>,
    ) {
        self.node_details.clear();

        let Some(mut detail) = detail else {
            return;
        };

        if detail.node_name.is_empty() && !detail.node_id.is_empty() {
            detail.node_name = detail.node_id.clone();
        }

        self.node_details.insert(Self::node_detail_key(&detail.node_id), detail);
        self.node_details_loaded = true;
    }

    pub fn merge_node_ranking(
        &mut self,
        ranking: Option<i32>,
    ) {
        let Some(ranking) = ranking else {
            return;
        };

        if self.node_details.len() == 1 {
            if let Some(detail) = self.node_details.values_mut().next() {
                detail.ranking = ranking;
                return;
            }
        }

        self.merge_node_ranking_for("", Some(ranking));
    }

    pub fn merge_node_ranking_for(
        &mut self,
        node_id: &str,
        ranking: Option<i32>,
    ) {
        let Some(ranking) = ranking else {
            return;
        };

        let key = Self::node_detail_key(node_id);
        if let Some(detail) = self.node_details.get_mut(&key) {
            detail.ranking = ranking;
            if detail.node_id.is_empty() && !node_id.is_empty() {
                detail.node_id = node_id.to_string();
            }
            if detail.node_name.is_empty() && !node_id.is_empty() {
                detail.node_name = node_id.to_string();
            }
        }
    }

    pub fn merge_node_detail(
        &mut self,
        detail: Option<NodeDetail>,
    ) {
        let Some(mut detail) = detail else {
            return;
        };

        let node_id = if detail.node_id.is_empty() && self.node_details.len() == 1 {
            self.node_details.keys().next().cloned().unwrap_or_default()
        } else {
            detail.node_id.clone()
        };

        if detail.node_id.is_empty() && node_id != DEFAULT_NODE_DETAIL_KEY {
            detail.node_id = node_id.clone();
        }

        self.merge_node_detail_for(&node_id, Some(detail));
    }

    pub fn merge_node_detail_for(
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
        if let Some(existing) = self.node_details.get(&key) {
            detail.ranking = existing.ranking;
        }
        if detail.node_name.is_empty() && !detail.node_id.is_empty() {
            detail.node_name = detail.node_id.clone();
        }

        self.node_details.insert(key, detail);
        self.node_details_loaded = true;
    }

    #[cfg(target_family = "unix")]
    pub fn system_stats(&self) -> SystemStats {
        self.system_stats.clone()
    }

    #[cfg(target_family = "unix")]
    pub fn update_disk_index(
        &mut self,
        new_index: usize,
    ) {
        self.system_stats.current_disk_index = if self.system_stats.disk_details.is_empty() {
            0
        } else {
            new_index.min(self.system_stats.disk_details.len().saturating_sub(1))
        };
    }

    pub fn status_message(&mut self) -> Option<StatusMessage> {
        if self
            .status_message
            .as_ref()
            .and_then(|message| message.expires_at)
            .is_some_and(|expires_at| Instant::now() >= expires_at)
        {
            self.status_message = None;
        }

        self.status_message.clone()
    }

    pub fn set_status_message(
        &mut self,
        level: StatusLevel,
        text: impl Into<String>,
    ) {
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
            return;
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
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    pub fn remove_node_detail(
        &mut self,
        node_id: &str,
    ) {
        self.node_details.remove(&Self::node_detail_key(node_id));
        self.node_details_loaded = true;
    }

    pub fn mark_node_details_loaded(&mut self) {
        self.node_details_loaded = true;
    }

    fn node_detail_key(node_id: &str) -> String {
        if node_id.is_empty() {
            DEFAULT_NODE_DETAIL_KEY.to_string()
        } else {
            node_id.to_string()
        }
    }
}

impl Collector {
    pub fn new(
        opts: &Opts,
        data: SharedData,
    ) -> Result<Self> {
        let urls: Vec<&str> = opts.url.as_str().split(',').collect();
        let urls: Vec<(String, String)> = urls
            .into_iter()
            .map(|url: &str| {
                let Some((name, endpoint)) = url.split_once('@') else {
                    return Err(format!("invalid url format: {url}").into());
                };
                if !is_websocket_endpoint(endpoint) {
                    return Err(ChaindashError::Other(format!(
                        "invalid websocket url for {name}: {endpoint}",
                    )));
                }
                Ok((name.into(), endpoint.into()))
            })
            .collect::<Result<Vec<_>>>()?;
        let disk_mount_points = opts.disk_mount_points.clone();
        let disk_auto_discovery = opts.disk_auto_discovery;
        let disk_alert_threshold = opts.disk_alert_threshold;
        let disk_refresh_interval = opts.disk_refresh_interval;
        let mut node_ids = Vec::new();
        for node_id in &opts.node_id {
            let node_id = node_id.trim();
            if !node_id.is_empty() && !node_ids.iter().any(|existing| existing == node_id) {
                node_ids.push(node_id.to_string());
            }
        }
        let explorer_api_url = opts.explorer_api_url.clone();

        Ok(Collector {
            data,
            urls,
            disk_mount_points,
            disk_auto_discovery,
            disk_alert_threshold,
            disk_refresh_interval,
            node_ids,
            explorer_api_url,
            stop_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Signal all spawned tasks to stop gracefully
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    pub(crate) async fn run(&self) -> Result<()> {
        let urls = self.urls.clone();
        for url in urls {
            let name = url.0.clone();
            let url_str = url.1.clone();
            let data = self.data.clone();
            let stop_flag = self.stop_flag.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    collect_node_state(name.clone(), url_str.clone(), data, stop_flag).await
                {
                    warn!("collect_node_state failed for {}: {}", name, e);
                }
            });
        }

        // 启动节点详情监控
        if !self.node_ids.is_empty() {
            debug!("start collect node detail: {:?}", self.node_ids);
            let node_ids = self.node_ids.clone();
            let explorer_api_url = self.explorer_api_url.clone();
            let data = self.data.clone();
            let stop_flag = self.stop_flag.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    collect_node_details(node_ids, data, explorer_api_url, stop_flag).await
                {
                    warn!("collect_node_details failed: {}", e);
                }
            });
        }

        // 启动本机系统监控
        #[cfg(target_family = "unix")]
        {
            let data = self.data.clone();
            let disk_mount_points = self.disk_mount_points.clone();
            let disk_auto_discovery = self.disk_auto_discovery;
            let disk_alert_threshold = self.disk_alert_threshold;
            let disk_refresh_interval = self.disk_refresh_interval;
            let stop_flag = self.stop_flag.clone();
            tokio::spawn(async move {
                if let Err(e) = collect_system_stats(
                    data,
                    disk_mount_points,
                    disk_auto_discovery,
                    disk_alert_threshold,
                    disk_refresh_interval,
                    stop_flag,
                )
                .await
                {
                    warn!("collect_system_stats failed: {}", e);
                }
            });
        }

        loop {
            if self.stop_flag.load(Ordering::Relaxed) {
                break;
            }

            let mut connection = None;
            for (name, url) in &self.urls {
                let ws = WsConnect::new(url.as_str());
                let provider = match ProviderBuilder::new().connect_ws(ws).await {
                    Ok(provider) => provider,
                    Err(err) => {
                        warn_with_status(
                            &self.data,
                            format!(
                                "Failed to connect block subscription for {} at {}: {}",
                                name, url, err
                            ),
                        );
                        continue;
                    },
                };

                let sub = match provider.subscribe_blocks().await {
                    Ok(sub) => sub,
                    Err(err) => {
                        warn_with_status(
                            &self.data,
                            format!(
                                "Failed to subscribe to blocks for {} at {}: {}",
                                name, url, err
                            ),
                        );
                        continue;
                    },
                };

                record_status_message(
                    &self.data,
                    StatusLevel::Info,
                    format!("Block subscription connected via {}", name),
                );
                connection = Some((name.clone(), provider, sub.into_stream()));
                break;
            }

            let Some((endpoint_name, provider, mut sub)) = connection else {
                time::sleep(COLLECTOR_RETRY_DELAY).await;
                continue;
            };

            let mut reconnect_required = false;
            loop {
                if self.stop_flag.load(Ordering::Relaxed) {
                    break;
                }

                tokio::select! {
                    maybe_head = sub.next() => {
                        let Some(head) = maybe_head else {
                            warn_with_status(
                                &self.data,
                                format!(
                                    "Block subscription stream ended for {}. Reconnecting soon",
                                    endpoint_name
                                ),
                            );
                            reconnect_required = true;
                            break;
                        };

                        let number = BlockNumberOrTag::Number(head.number);
                        let block = match provider.get_block_by_number(number).full().await {
                            Ok(block) => block,
                            Err(err) => {
                                warn_with_status(
                                    &self.data,
                                    format!(
                                        "Failed to fetch block {} via {}: {}. Reconnecting soon",
                                        head.number, endpoint_name, err
                                    ),
                                );
                                reconnect_required = true;
                                break;
                            },
                        };
                        let txs = block.map(|b| b.transactions.len() as u64).unwrap_or(0);

                        let mut data = self.data.lock().expect("mutex poisoned - recovering");
                        data.cur_block_number = head.number;
                        if data.cur_block_time > 0 {
                            data.prev_block_time = data.cur_block_time;
                        }
                        data.cur_block_time = head.timestamp;
                        data.cur_txs = txs;

                        if txs > data.max_txs {
                            data.max_txs = txs;
                            data.max_txs_block_number = head.number;
                        }
                        data.txns.push(txs);
                        if data.prev_block_time > 0 {
                            let interval_ms = data
                                .cur_block_time
                                .saturating_sub(data.prev_block_time)
                                .saturating_mul(1000);
                            data.cur_interval = interval_ms;
                            if interval_ms > data.max_interval {
                                data.max_interval = interval_ms
                            }
                            data.intervals.push(interval_ms);
                        }
                    }
                    _ = time::sleep(COLLECTOR_RETRY_DELAY) => {
                        if self.stop_flag.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }

            if self.stop_flag.load(Ordering::Relaxed) {
                break;
            }

            if reconnect_required {
                time::sleep(COLLECTOR_RETRY_DELAY).await;
            }
        }
        Ok(())
    }
}

async fn collect_node_state(
    name: String,
    url: String,
    data: SharedData,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    let host = websocket_host(&url);

    while !stop_flag.load(Ordering::Relaxed) {
        let ws = WsConnect::new(url.as_str());
        let provider = match ProviderBuilder::new().connect_ws(ws).await {
            Ok(provider) => provider,
            Err(err) => {
                warn_with_status(
                    &data,
                    format!(
                        "Failed to connect node state collector for {} at {}: {}",
                        name, url, err
                    ),
                );
                time::sleep(COLLECTOR_RETRY_DELAY).await;
                continue;
            },
        };
        let mut interval = time::interval(Duration::from_secs(1));

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                return Ok(());
            }

            interval.tick().await;

            let status = match provider.debug_consensus_status().await {
                Ok(status) => status,
                Err(err) => {
                    warn_with_status(
                        &data,
                        format!("Node state RPC failed for {}: {}. Reconnecting soon", name, err),
                    );
                    break;
                },
            };
            let cur_number = match provider.get_block_number().await {
                Ok(cur_number) => cur_number,
                Err(err) => {
                    warn_with_status(
                        &data,
                        format!(
                            "Node block number RPC failed for {}: {}. Reconnecting soon",
                            name, err
                        ),
                    );
                    break;
                },
            };
            let epoch = status.state.view.as_ref().map(|v| v.epoch).unwrap_or(0);
            let view = status.state.view.as_ref().and_then(|v| v.view_number).unwrap_or(0);
            let committed =
                status.state.highest_commit_block.as_ref().map(|b| b.number).unwrap_or(0);
            let locked = status.state.highest_lock_block.as_ref().map(|b| b.number).unwrap_or(0);
            let qc = status.state.highest_qc_block.as_ref().map(|b| b.number).unwrap_or(0);
            let validator = status.validator;

            let node = ConsensusState {
                name: name.clone(),
                host: host.clone(),
                current_number: cur_number,
                epoch,
                view,
                committed,
                locked,
                qc,
                validator,
            };

            let mut data = data.lock().expect("mutex poisoned - recovering");
            data.states.insert(name.clone(), node);
        }

        time::sleep(COLLECTOR_RETRY_DELAY).await;
    }

    Ok(())
}

#[cfg(target_family = "unix")]
async fn collect_system_stats(
    data: SharedData,
    disk_mount_points: Vec<String>,
    disk_auto_discovery: bool,
    disk_alert_threshold: f32,
    disk_refresh_interval: u64,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    let system = Arc::new(Mutex::new(System::new_all()));
    let mut interval = time::interval(Duration::from_secs(disk_refresh_interval));

    let mut previous_network_sample: Option<NetworkSample> = None;

    let mut last_discovery_time = std::time::Instant::now();
    let discovery_interval = Duration::from_secs(5);
    let mut discovered_mount_points: Vec<String> = Vec::new();
    let auto_discovery_enabled = disk_auto_discovery;

    #[derive(Debug)]
    struct SystemSnapshot {
        cpu_usage: f32,
        memory_used: u64,
        memory_total: u64,
        memory_usage_percent: f32,
        network_rx_total: u64,
        network_tx_total: u64,
        collected_at: std::time::Instant,
        disk_used: u64,
        disk_total: u64,
        disk_usage_percent: f32,
        disk_details: Vec<DiskDetail>,
        has_disk_alerts: bool,
    }

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        tokio::select! {
            _ = interval.tick() => {
                if auto_discovery_enabled && last_discovery_time.elapsed() >= discovery_interval {
                    match tokio::task::spawn_blocking(discover_mount_points).await {
                        Ok(Ok(mount_points)) => {
                            discovered_mount_points = mount_points
                                .iter()
                                .map(|mp| mp.mount_point.clone())
                                .collect();
                            debug!(
                                "自动发现 {} 个挂载点: {:?}",
                                discovered_mount_points.len(),
                                discovered_mount_points
                            );
                            last_discovery_time = std::time::Instant::now();
                        }
                        Ok(Err(e)) => {
                            warn_with_status(&data, format!("自动发现挂载点失败: {}", e));
                        }
                        Err(e) => {
                            warn_with_status(&data, format!("spawn_blocking 任务失败: {}", e));
                        }
                    }
                }

                debug!("disk_mount_points: {:?}", disk_mount_points);
                debug!("auto_discovery_enabled: {}", auto_discovery_enabled);
                debug!("discovered_mount_points: {:?}", discovered_mount_points);

                let mount_points_to_monitor = if auto_discovery_enabled {
                    let mut all_points = discovered_mount_points.clone();
                    for point in &disk_mount_points {
                        if !all_points.contains(point) {
                            all_points.push(point.clone());
                        }
                    }
                    debug!("合并后的挂载点列表: {:?}", all_points);
                    all_points
                } else {
                    debug!("使用用户指定的挂载点列表: {:?}", disk_mount_points);
                    disk_mount_points.clone()
                };

                debug!("最终监控的挂载点: {:?}", mount_points_to_monitor);

                let mount_points_clone = mount_points_to_monitor.clone();
                let system_clone = Arc::clone(&system);
                let snapshot_task_data = data.clone();
                let snapshot = tokio::task::spawn_blocking(move || {
                    let mut system = system_clone.lock().expect("system mutex poisoned");
                    system.refresh_all();

                    let cpu_usage = system.global_cpu_info().cpu_usage();
                    let memory_used = system.used_memory();
                    let memory_total = system.total_memory();
                    let memory_usage_percent = if memory_total > 0 {
                        (memory_used as f32 / memory_total as f32) * 100.0
                    } else {
                        0.0
                    };

                    drop(system);

                    let networks = sysinfo::Networks::new_with_refreshed_list();
                    let mut network_rx_total: u64 = 0;
                    let mut network_tx_total: u64 = 0;
                    for (_, network) in &networks {
                        network_rx_total =
                            network_rx_total.saturating_add(network.total_received());
                        network_tx_total =
                            network_tx_total.saturating_add(network.total_transmitted());
                    }

                    let disks = Disks::new_with_refreshed_list();
                    let mut disk_used: u64 = 0;
                    let mut disk_total: u64 = 0;
                    let mut disk_details = Vec::new();
                    let mut has_disk_alerts = false;

                    for disk in disks.list() {
                        let mount_point = disk.mount_point().to_string_lossy().to_string();
                        if !mount_points_clone.contains(&mount_point) {
                            continue;
                        }

                        let total = disk.total_space();
                        let available = disk.available_space();
                        let used = total.saturating_sub(available);
                        let usage_percent = if total > 0 {
                            (used as f32 / total as f32) * 100.0
                        } else {
                            0.0
                        };

                        let filesystem = disk.file_system().to_string_lossy().to_string();
                        let device = disk.name().to_string_lossy().to_string();
                        let is_network = is_network_filesystem(&filesystem);
                        let is_alert = usage_percent >= disk_alert_threshold;
                        if is_alert {
                            has_disk_alerts = true;
                        }

                        disk_details.push(DiskDetail {
                            mount_point,
                            filesystem,
                            total,
                            used,
                            available,
                            usage_percent,
                            device,
                            is_alert,
                            is_network,
                            last_updated: std::time::Instant::now(),
                        });

                        disk_total = disk_total.saturating_add(total);
                        disk_used = disk_used.saturating_add(used);
                    }

                    let disk_usage_percent = if disk_total > 0 {
                        (disk_used as f32 / disk_total as f32) * 100.0
                    } else {
                        0.0
                    };

                    SystemSnapshot {
                        cpu_usage,
                        memory_used,
                        memory_total,
                        memory_usage_percent,
                        network_rx_total,
                        network_tx_total,
                        collected_at: std::time::Instant::now(),
                        disk_used,
                        disk_total,
                        disk_usage_percent,
                        disk_details,
                        has_disk_alerts,
                    }
                })
                .await
                .map_err(|err| {
                    let message = format!("system stats task join error: {}", err);
                    record_status_message(&snapshot_task_data, StatusLevel::Error, message.clone());
                    ChaindashError::Other(message)
                })?;

                let (network_sample, network_rx_rate, network_tx_rate) = compute_network_rates(
                    previous_network_sample,
                    snapshot.network_rx_total,
                    snapshot.network_tx_total,
                    snapshot.collected_at,
                );
                previous_network_sample = Some(network_sample);

                let SystemSnapshot {
                    cpu_usage,
                    memory_used,
                    memory_total,
                    memory_usage_percent,
                    disk_used,
                    disk_total,
                    disk_usage_percent,
                    disk_details,
                    has_disk_alerts,
                    ..
                } = snapshot;

                let alert_disk_count = disk_details.iter().filter(|disk| disk.is_alert).count();

                let previous_alert = {
                    let mut data_guard = data.lock().expect("mutex poisoned - recovering");
                    let previous_alert = data_guard.system_stats.has_disk_alerts;
                    let current_index = if disk_details.is_empty() {
                        0
                    } else {
                        data_guard
                            .system_stats
                            .current_disk_index
                            .min(disk_details.len().saturating_sub(1))
                    };
                    data_guard.system_stats = SystemStats {
                        cpu_usage,
                        memory_used,
                        memory_total,
                        memory_usage_percent,
                        network_rx: network_rx_rate,
                        network_tx: network_tx_rate,
                        disk_used,
                        disk_total,
                        disk_usage_percent,
                        disk_details,
                        current_disk_index: current_index,
                        alert_threshold: disk_alert_threshold,
                        has_disk_alerts,
                        auto_discovery_enabled,
                    };
                    debug!("collect system stats: {:?}", &data_guard.system_stats);
                    previous_alert
                };

                if has_disk_alerts && !previous_alert {
                    warn_with_status(
                        &data,
                        format!(
                            "{} disk(s) exceed {:.0}% usage threshold",
                            alert_disk_count,
                            disk_alert_threshold,
                        ),
                    );
                } else if !has_disk_alerts && previous_alert {
                    record_status_message(
                        &data,
                        StatusLevel::Info,
                        "Disk usage returned below alert threshold",
                    );
                }
            }
            else => break,
        }
    }
    Ok(())
}

/// 检查是否为网络文件系统
fn is_network_filesystem(filesystem: &str) -> bool {
    let fs_lower = filesystem.to_lowercase();
    fs_lower.contains("nfs") || fs_lower.contains("smb") || fs_lower.contains("cifs")
}

/// 检查是否为特殊文件系统（不应该被监控）
fn is_special_filesystem(filesystem: &str) -> bool {
    let fs_lower = filesystem.to_lowercase();
    fs_lower == "proc"
        || fs_lower == "sysfs"
        || fs_lower == "tmpfs"
        || fs_lower == "devtmpfs"
        || fs_lower == "cgroup"
        || fs_lower == "cgroup2"
        || fs_lower == "overlay"
        || fs_lower == "devpts"
        || fs_lower == "mqueue"
        || fs_lower == "hugetlbfs"
        || fs_lower == "securityfs"
        || fs_lower == "pstore"
        || fs_lower == "debugfs"
        || fs_lower == "tracefs"
        || fs_lower == "fusectl"
        || fs_lower == "configfs"
        || fs_lower == "binfmt_misc"
        || fs_lower == "autofs"
        || fs_lower == "rpc_pipefs"
        || fs_lower == "efivarfs"
        || fs_lower == "bpf"
        || fs_lower.contains("fuse")
        || fs_lower.starts_with("cgroup")
}

/// 自动发现挂载点信息
#[derive(Debug, Clone)]
struct MountPointInfo {
    mount_point: String,
}

/// 读取/proc/mounts并返回非特殊文件系统的挂载点
fn discover_mount_points() -> Result<Vec<MountPointInfo>> {
    use std::{
        fs::File,
        io::{
            BufRead,
            BufReader,
        },
    };

    let mut mount_points = Vec::new();

    // 尝试读取/proc/mounts
    let file = match File::open("/proc/mounts") {
        Ok(f) => f,
        Err(e) => {
            // 如果/proc/mounts不可用，尝试使用sysinfo作为后备
            warn!("无法读取/proc/mounts: {}, 使用sysinfo作为后备", e);
            return Ok(discover_mount_points_fallback());
        },
    };

    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.len() >= 3 {
            let mount_point = parts[1].to_string();
            let filesystem = parts[2].to_string();

            // 跳过特殊文件系统
            if !is_special_filesystem(&filesystem) {
                mount_points.push(MountPointInfo { mount_point });
            }
        }
    }

    Ok(mount_points)
}

/// 使用sysinfo作为后备的挂载点发现
fn discover_mount_points_fallback() -> Vec<MountPointInfo> {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();
    let mut mount_points = Vec::new();

    for disk in disks.list() {
        let mount_point = disk.mount_point().to_string_lossy().to_string();
        let filesystem = disk.file_system().to_string_lossy().to_string();

        // 跳过特殊文件系统
        if !is_special_filesystem(&filesystem) {
            mount_points.push(MountPointInfo { mount_point });
        }
    }

    mount_points
}

async fn collect_node_details(
    node_ids: Vec<String>,
    data: SharedData,
    explorer_api_url: String,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    use tokio::time::{
        self,
        Duration,
    };

    let client = reqwest::Client::new();
    let detail_url = format!("{explorer_api_url}/staking/stakingDetails");
    let ranking_url = format!("{explorer_api_url}/staking/aliveStakingList");
    let mut interval = time::interval(Duration::from_secs(10));

    fetch_all_node_details(&client, &detail_url, &node_ids, data.clone()).await;
    fetch_node_rankings(&client, &ranking_url, &node_ids, data.clone()).await;

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        interval.tick().await;
        fetch_all_node_details(&client, &detail_url, &node_ids, data.clone()).await;
        fetch_node_rankings(&client, &ranking_url, &node_ids, data.clone()).await;
    }

    Ok(())
}

async fn fetch_all_node_details(
    client: &reqwest::Client,
    url: &str,
    node_ids: &[String],
    data: SharedData,
) {
    let requests = node_ids.iter().map(|node_id| {
        let data = data.clone();
        async move {
            match tokio::time::timeout(
                NODE_DETAIL_REQUEST_TIMEOUT,
                fetch_node_detail(client, url, node_id, data.clone()),
            )
            .await
            {
                Ok(Ok(())) => None,
                Ok(Err(message)) => {
                    warn!("{message}");
                    let mut data = data.lock().expect("mutex poisoned - recovering");
                    data.remove_node_detail(node_id);
                    Some(node_id.clone())
                },
                Err(err) => {
                    warn!(
                        "Node detail request timed out after {:?} for {}: {}",
                        NODE_DETAIL_REQUEST_TIMEOUT, node_id, err
                    );
                    let mut data = data.lock().expect("mutex poisoned - recovering");
                    data.remove_node_detail(node_id);
                    Some(node_id.clone())
                },
            }
        }
    });

    let failures: Vec<String> =
        futures::future::join_all(requests).await.into_iter().flatten().collect();

    {
        let mut data = data.lock().expect("mutex poisoned - recovering");
        data.mark_node_details_loaded();
    }

    if !failures.is_empty() {
        record_status_message(&data, StatusLevel::Warn, summarize_node_detail_failures(&failures));
    }
}

async fn fetch_node_rankings(
    client: &reqwest::Client,
    url: &str,
    node_ids: &[String],
    data: SharedData,
) {
    let body = serde_json::json!({
        "pageNo": 1,
        "pageSize": 300,
        "key": "",
        "queryStatus": "all",
    });

    debug!("fetch node ranking: {}", url);

    match client.post(url).header("content-type", "application/json").json(&body).send().await {
        Ok(resp) => {
            debug!("Reponse: {}", resp.status());
            if !resp.status().is_success() {
                warn_with_status(
                    &data,
                    format!("Node ranking API returned error status: {}", resp.status()),
                );
                return;
            }
            let body_bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn_with_status(&data, format!("Failed to read response body: {}", e));
                    return;
                },
            };
            let node_list_resp: types::NodeListResponse = match serde_json::from_slice(&body_bytes)
            {
                Ok(node_list_resp) => node_list_resp,
                Err(e) => {
                    warn_with_status(&data, format!("Failed to parse response JSON: {}", e));
                    return;
                },
            };
            debug!("Node list response: {:?}", node_list_resp);

            if node_list_resp.code == 0 {
                if let Some(data_obj) = node_list_resp.data {
                    let mut data = data.lock().expect("mutex poisoned - recovering");
                    for node_id in node_ids {
                        let ranking = parse_node_ranking(&data_obj, node_id);
                        data.merge_node_ranking_for(node_id, ranking);
                    }
                } else {
                    warn_with_status(&data, "Node ranking response missing data field");
                }
            } else {
                warn_with_status(
                    &data,
                    format!(
                        "Node ranking API returned error code: {}, err_msg: {}",
                        node_list_resp.code, node_list_resp.err_msg
                    ),
                );
            }
        },
        Err(e) => {
            warn_with_status(&data, format!("Failed to fetch node ranking: {}", e));
        },
    }
}

async fn fetch_node_detail(
    client: &reqwest::Client,
    url: &str,
    node_id: &str,
    data: SharedData,
) -> std::result::Result<(), String> {
    let body = serde_json::json!({
        "nodeId": node_id
    });

    debug!("fetch node detail: {}", url);

    match client.post(url).header("content-type", "application/json").json(&body).send().await {
        Ok(resp) => {
            debug!("Reponse: {}", resp.status());
            if !resp.status().is_success() {
                return Err(format!(
                    "Node detail API returned error status for {}: {}",
                    node_id,
                    resp.status()
                ));
            }
            let body_bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Err(format!(
                        "Failed to read node detail response body for {}: {}",
                        node_id, e
                    ));
                },
            };
            let node_detail_resp: types::NodeDetailResponse =
                match serde_json::from_slice(&body_bytes) {
                    Ok(node_detail_resp) => node_detail_resp,
                    Err(e) => {
                        return Err(format!(
                            "Failed to parse node detail JSON for {}: {}",
                            node_id, e
                        ));
                    },
                };
            debug!("Node detail response: {:?}", node_detail_resp);

            if node_detail_resp.code == 0 {
                if let Some(detail) = node_detail_resp.data {
                    let node_detail = parse_node_detail(node_id, &detail);
                    let mut data = data.lock().expect("mutex poisoned - recovering");
                    data.merge_node_detail_for(node_id, Some(node_detail));
                } else {
                    return Err(format!("Node detail response missing data field for {}", node_id));
                }
            } else {
                return Err(format!(
                    "Node detail API returned error code for {}: {}, err_msg: {}",
                    node_id, node_detail_resp.code, node_detail_resp.err_msg
                ));
            }
        },
        Err(e) => {
            return Err(format!("Failed to fetch node detail for {}: {}", node_id, e));
        },
    }

    Ok(())
}

fn parse_node_detail(
    node_id: &str,
    node_detail: &types::NodeDetail,
) -> NodeDetail {
    let node_name = node_detail.node_name.clone();
    let block_qty = u64::try_from(node_detail.block_qty).unwrap_or(0);
    let expect_block_qty = u64::try_from(node_detail.expect_block_qty).unwrap_or(0);
    let block_rate = if expect_block_qty > 0 {
        let rate = block_qty as f64 / expect_block_qty as f64;
        format!("{:.2}%", rate * 100.0)
    } else {
        "0.00%".to_string()
    };
    let daily_block_rate = node_detail.gen_blocks_rate.clone();
    let reward_per = node_detail.reward_per.parse::<f64>().ok().unwrap_or(0.0);
    let reward_value = node_detail.reward_value.parse::<f64>().ok().unwrap_or(0.0);
    let reward_address = node_detail.benefit_addr.clone();
    let verifier_time = u64::try_from(node_detail.verifier_time).unwrap_or(0);

    NodeDetail {
        node_id: node_id.to_string(),
        node_name,
        ranking: 0,
        block_qty,
        block_rate,
        daily_block_rate,
        reward_per,
        reward_value,
        reward_address,
        verifier_time,
        last_updated_at: Some(Instant::now()),
    }
}

fn parse_node_ranking(
    data: &[NodeInfo],
    node_id: &str,
) -> Option<i32> {
    data.iter()
        .find(|node| node.node_id == node_id)
        .and_then(|node| i32::try_from(node.ranking).ok())
}

/// Test-only methods for Data struct
#[cfg(test)]
#[cfg(target_family = "unix")]
impl Data {
    /// Set disk details for testing disk navigation
    pub fn set_disk_details_for_test(
        &mut self,
        details: Vec<DiskDetail>,
    ) {
        self.system_stats.disk_details = details;
    }

    /// Get current disk index for testing
    pub fn current_disk_index_for_test(&self) -> usize {
        self.system_stats.current_disk_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_new_invalid_url_no_at_sign() {
        // Test URL without @ sign returns error
        use clap::Parser;

        use crate::Opts;

        let opts = Opts::parse_from(["test", "--url", "invalid_url"]);
        let data: SharedData = Arc::new(Mutex::new(Data::default()));

        let result = Collector::new(&opts, data);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid url format"));
    }

    #[test]
    fn test_collector_new_valid_url() {
        // Test valid URL format succeeds
        use clap::Parser;

        use crate::Opts;

        let opts = Opts::parse_from(["test", "--url", "test@ws://127.0.0.1:6789"]);
        let data: SharedData = Arc::new(Mutex::new(Data::default()));

        let result = Collector::new(&opts, data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_collector_new_rejects_non_websocket_url() {
        use clap::Parser;

        use crate::Opts;

        let opts = Opts::parse_from(["test", "--url", "test@http://127.0.0.1:6789"]);
        let data: SharedData = Arc::new(Mutex::new(Data::default()));

        let result = Collector::new(&opts, data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid websocket url"));
    }

    #[test]
    fn test_collector_new_rejects_invalid_endpoint_in_list() {
        use clap::Parser;

        use crate::Opts;

        let opts = Opts::parse_from([
            "test",
            "--url",
            "main@ws://127.0.0.1:6789,backup@http://127.0.0.1:6790",
        ]);
        let data: SharedData = Arc::new(Mutex::new(Data::default()));

        let result = Collector::new(&opts, data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid websocket url for backup"));
    }

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
        data.states.insert(
            "node-b".to_string(),
            ConsensusState {
                name: "node-b".to_string(),
                host: "host-b".to_string(),
                current_number: 2,
                ..Default::default()
            },
        );
        data.states.insert(
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
    fn test_status_message_hides_expired_entries() {
        let mut data = Data {
            status_message: Some(StatusMessage {
                level: StatusLevel::Info,
                text: "expired".to_string(),
                expires_at: Some(Instant::now() - StdDuration::from_secs(1)),
            }),
            ..Default::default()
        };

        assert!(data.status_message().is_none());
        assert!(data.status_message.is_none());
    }

    #[test]
    fn test_set_status_message_applies_ttl_by_level() {
        let mut data = Data::default();

        data.set_status_message(StatusLevel::Info, "info");
        assert!(data.status_message.as_ref().and_then(|message| message.expires_at).is_some());

        data.set_status_message(StatusLevel::Warn, "warn");
        assert!(data.status_message.as_ref().and_then(|message| message.expires_at).is_some());

        data.set_status_message(StatusLevel::Error, "error");
        assert!(data.status_message.as_ref().is_some_and(|message| message.expires_at.is_none()));
    }

    #[test]
    fn test_set_status_message_deduplicates_same_active_message() {
        let mut data = Data::default();

        data.set_status_message(StatusLevel::Warn, "warn");
        let first = data.status_message.clone().expect("status should exist");

        data.set_status_message(StatusLevel::Warn, "warn");
        let second = data.status_message.clone().expect("status should exist");

        assert_eq!(first.level, second.level);
        assert_eq!(first.text, second.text);
        assert_eq!(first.expires_at, second.expires_at);
    }

    #[test]
    fn test_set_status_message_recreates_same_message_after_expiry() {
        let mut data = Data {
            status_message: Some(StatusMessage {
                level: StatusLevel::Warn,
                text: "warn".to_string(),
                expires_at: Some(Instant::now() - StdDuration::from_secs(1)),
            }),
            ..Default::default()
        };

        data.set_status_message(StatusLevel::Warn, "warn");

        assert!(data.status_message.as_ref().is_some_and(|message| {
            message.text == "warn"
                && message.level == StatusLevel::Warn
                && message.expires_at.is_some_and(|expires_at| expires_at > Instant::now())
        }));
    }

    #[test]
    fn test_parse_node_ranking_missing_returns_none() {
        let ranking = parse_node_ranking(&[], "missing-node");

        assert_eq!(ranking, None);
    }

    #[test]
    fn test_summarize_node_detail_failures_for_single_node() {
        let summary = summarize_node_detail_failures(&["node-a".to_string()]);

        assert_eq!(summary, "Node detail unavailable for node-a");
    }

    #[test]
    fn test_summarize_node_detail_failures_truncates_long_lists() {
        let summary = summarize_node_detail_failures(&[
            "node-a".to_string(),
            "node-b".to_string(),
            "node-c".to_string(),
            "node-d".to_string(),
        ]);

        assert_eq!(
            summary,
            "Node details unavailable for 4 node(s): node-a, node-b, node-c, +1 more"
        );
    }

    #[test]
    fn test_parse_node_detail_clamps_negative_values() {
        let parsed = parse_node_detail(
            "node-a-id",
            &types::NodeDetail {
                node_name: "node-a".to_string(),
                total_value: "0".to_string(),
                delegate_value: "0".to_string(),
                delegate_qty: 0,
                block_qty: -1,
                expect_block_qty: -10,
                gen_blocks_rate: "1/day".to_string(),
                reward_per: "10".to_string(),
                reward_value: "20".to_string(),
                benefit_addr: "addr".to_string(),
                verifier_time: -5,
            },
        );

        assert_eq!(parsed.block_qty, 0);
        assert_eq!(parsed.block_rate, "0.00%");
        assert_eq!(parsed.verifier_time, 0);
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
                last_updated: std::time::Instant::now(),
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
                last_updated: std::time::Instant::now(),
            },
        ]);

        data.update_disk_index(10);

        assert_eq!(data.current_disk_index_for_test(), 1);
    }

    #[test]
    fn test_compute_network_rates_first_sample_returns_zero() {
        let collected_at = std::time::Instant::now();
        let (_, rx_rate, tx_rate) = compute_network_rates(None, 2048, 4096, collected_at);

        assert_eq!(rx_rate, 0);
        assert_eq!(tx_rate, 0);
    }

    #[test]
    fn test_compute_network_rates_normalizes_by_elapsed_time() {
        let start = std::time::Instant::now();
        let previous = Some(NetworkSample {
            rx_total: 1_000_000,
            tx_total: 2_000_000,
            collected_at: start,
        });
        let end = start + std::time::Duration::from_secs(2);

        let (_, rx_rate, tx_rate) = compute_network_rates(previous, 5_000_000, 8_000_000, end);

        assert_eq!(rx_rate, 2_000_000);
        assert_eq!(tx_rate, 3_000_000);
    }
}
