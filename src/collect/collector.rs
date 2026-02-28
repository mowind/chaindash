use std::collections::HashMap;
use std::sync::{
    atomic::{
        AtomicBool,
        Ordering,
    },
    Arc,
    Mutex,
};


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
use alloy::providers::{ext::DebugApi, Provider, ProviderBuilder, WsConnect};

use alloy::eips::BlockNumberOrTag;
use futures::StreamExt;

use super::types::NodeInfo;
use crate::{
    collect::{docker_stats::Stats, types},
    error::{ChaindashError, Result},
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

#[derive(Debug, Clone)]
pub struct NodeStats {
    pub cpu_percent: f64,
    pub mem: u64,
    pub mem_percent: f64,
    pub mem_limit: u64,
    pub network_rx: u64,
    pub network_tx: u64,
    pub blk_read: u64,
    pub blk_write: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NodeDetail {
    pub node_name: String,
    pub ranking: i32,
    pub block_qty: u64,
    pub block_rate: String,
    pub daily_block_rate: String,
    pub reward_per: f64, // percentage, e.g., 50.0
    pub reward_value: f64,
    pub reward_address: String,
    pub verifier_time: u64,
}

impl NodeDetail {
    pub fn rewards(&self) -> f64 {
        self.reward_value * (1.0 - self.reward_per / 100.0)
    }
}

impl Default for &NodeStats {
    fn default() -> Self {
        &NodeStats {
            cpu_percent: 0.0,
            mem: 0,
            mem_percent: 0.0,
            mem_limit: 0,
            network_rx: 0,
            network_tx: 0,
            blk_read: 0,
            blk_write: 0,
        }
    }
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
    stats: HashMap<String, NodeStats>,
    node_detail: Option<NodeDetail>,
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

#[derive(Debug)]
pub struct Collector {
    data: SharedData,
    urls: Vec<(String, String)>,
    enable_docker_stats: bool,
    docker_port: u16,
    disk_mount_points: Vec<String>,
    disk_auto_discovery: bool,
    disk_alert_threshold: f32,
    disk_refresh_interval: u64,
    node_id: Option<String>,
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
            stats: HashMap::new(),
            node_detail: None,
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
        self.states.values().cloned().collect()
    }

    pub fn stats(&self) -> HashMap<String, NodeStats> {
        self.stats.clone()
    }

    pub fn node_detail(&self) -> Option<NodeDetail> {
        self.node_detail.clone()
    }

    pub fn update_node_detail(
        &mut self,
        detail: Option<NodeDetail>,
    ) {
        self.node_detail = detail;
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
        self.system_stats.current_disk_index = new_index;
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
                let v: Vec<&str> = url.split('@').collect();
                if v.len() < 2 {
                    return Err(format!("invalid url format: {url}").into());
                }
                Ok((v[0].into(), v[1].into()))
            })
            .collect::<Result<Vec<_>>>()?;
        let enable_docker_stats = opts.enable_docker_stats;
        let docker_port = opts.docker_port;
        let disk_mount_points = opts.disk_mount_points.clone();
        let disk_auto_discovery = opts.disk_auto_discovery;
        let disk_alert_threshold = opts.disk_alert_threshold;
        let disk_refresh_interval = opts.disk_refresh_interval;
        let node_id = opts.node_id.clone();
        let explorer_api_url = opts.explorer_api_url.clone();

        Ok(Collector {
            data,
            urls,
            enable_docker_stats,
            docker_port,
            disk_mount_points,
            disk_auto_discovery,
            disk_alert_threshold,
            disk_refresh_interval,
            node_id,
            explorer_api_url,
            stop_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Signal all spawned tasks to stop gracefully
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    pub(crate) async fn run(&self) -> Result<()> {
        let ws = WsConnect::new(self.urls[0].1.as_str());
        let provider = ProviderBuilder::new().connect_ws(ws).await?;
        let sub = provider.subscribe_blocks().await?;
        let mut sub = sub.into_stream();



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

            debug!("enable_docker_stats: {}", self.enable_docker_stats);
            if self.enable_docker_stats {
                debug!("enable_docker_stats: {}", self.enable_docker_stats);
                let host = url.1.clone();
                let host = host.replace("ws://", "").replace("wss://", "");
                let ip_port: Vec<&str> = host.as_str().split(':').collect();
                let host = format!("http://{}:{}", ip_port[0], self.docker_port);
                let data = self.data.clone();
                let name = url.0.clone();
                let stop_flag = self.stop_flag.clone();
                tokio::spawn(async move {
                    if let Err(e) = collect_node_stats(name.clone(), host, data, stop_flag).await {
                        warn!("collect_node_stats failed for {}: {}", name, e);
                    }
                });
            }
        }

        // 启动节点详情监控
        if let Some(node_id) = &self.node_id {
            debug!("start collect node detail: {}", node_id);
            let node_id = node_id.clone();
            let explorer_api_url = self.explorer_api_url.clone();
            let data = self.data.clone();
            let stop_flag = self.stop_flag.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    collect_node_detail(node_id, data, explorer_api_url, stop_flag).await
                {
                    warn!("collect_node_detail failed: {}", e);
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
            tokio::select! {
                Some(head) = sub.next() => {
                let number = BlockNumberOrTag::Number(head.number);
                let block = provider.get_block_by_number(number).full().await?;
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
                        let interval = data.cur_block_time - data.prev_block_time;
                        data.cur_interval = interval;
                        if interval > data.max_interval {
                            data.max_interval = interval
                        }
                        data.intervals.push(interval);
                    }
                }
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
    let ws = WsConnect::new(url.as_str());
    let provider = ProviderBuilder::new().connect_ws(ws).await?;
    let host = url.replace("ws://", "").replace("wss://", "");
    let mut interval = time::interval(Duration::from_secs(1));

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        tokio::select! {
            _ = interval.tick() => {
                let status = provider.debug_consensus_status().await?;
                let cur_number = provider.get_block_number().await?;
                let epoch = status.state.view.as_ref().map(|v| v.epoch).unwrap_or(0);
                let view = status.state.view.as_ref().and_then(|v| v.view_number).unwrap_or(0);
                let committed = status.state.highest_commit_block.as_ref().map(|b| b.number).unwrap_or(0);
                let locked = status.state.highest_lock_block.as_ref().map(|b| b.number).unwrap_or(0);
                let qc = status.state.highest_qc_block.as_ref().map(|b| b.number).unwrap_or(0);
                let validator = status.validator;


                let node = ConsensusState{
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
        }
    }
    Ok(())
}

async fn collect_node_stats(
    name: String,
    host: String,
    data: SharedData,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    debug!("name: {}, host: {}", name, host);

    let client = reqwest::Client::new();
    let url = format!("{host}/containers/{name}/stats");
    debug!("url: {:?}", url);

    let resp = client
        .get(&url)
        .send()
        .await?;

    debug!("status: {:?}", resp.status());

    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        let chunk = chunk_result.map_err(|e: reqwest::Error| ChaindashError::Http(e.to_string()))?;
        buf.extend_from_slice(&chunk);
        let stats: Stats = match serde_json::from_slice(buf.as_ref()) {
            Err(_) => continue,
            Ok(stats) => stats,
        };
        debug!("stats: {:#?}", stats);
        let _ = std::mem::take(&mut buf);

        update_node_stats(name.as_str(), data.clone(), &stats);
    }
    Ok(())
}

fn update_node_stats(
    name: &str,
    data: SharedData,
    stats: &Stats,
) {
    let (mem, mem_usage) = calc_mem_usage(stats);

    let (rx, tx) = get_network_rx_tx(stats);
    let (blk_read, blk_write) = get_blk(stats);

    let node_stats = NodeStats {
        cpu_percent: calc_cpu_usage(stats),
        mem,
        mem_percent: mem_usage,
        mem_limit: stats.memory_stats.limit,
        network_rx: rx,
        network_tx: tx,
        blk_read,
        blk_write,
    };

    let mut data = data.lock().expect("mutex poisoned - recovering");
    data.stats.insert(name.to_string(), node_stats);
}

fn calc_cpu_usage(stats: &Stats) -> f64 {
    let cpu_usage = &stats.cpu_stats.cpu_usage;
    let precpu_usage = &stats.precpu_stats.cpu_usage;
    let cpu_delta = cpu_usage.total_usage - precpu_usage.total_usage;
    let precpu_system_cpu_usage = stats.precpu_stats.system_cpu_usage.unwrap_or(0);
    let system_cpu_delta = stats.cpu_stats.system_cpu_usage.unwrap_or(0) - precpu_system_cpu_usage;
    let num_cpus = cpu_usage.percpu_usage.as_ref().map(|v| v.len()).unwrap_or(1);

    if system_cpu_delta == 0 {
        return 0.0;
    }

    (cpu_delta as f64 / system_cpu_delta as f64) * num_cpus as f64 * 100.0
}

fn calc_mem_usage(stats: &Stats) -> (u64, f64) {
    let memory_stat = &stats.memory_stats;
    let cache = *memory_stat.stats.get("cache").unwrap_or(&0);
    let used_memory = memory_stat.usage.saturating_sub(cache);
    let available_memory = memory_stat.limit;
    if available_memory == 0 {
        (used_memory, 0.0)
    } else {
        (used_memory, (used_memory as f64 / available_memory as f64) * 100.0)
    }
}

fn get_network_rx_tx(stats: &Stats) -> (u64, u64) {
    match &stats.networks {
        Some(networks) => {
            let mut rx: u64 = 0;
            let mut tx: u64 = 0;
            networks.iter().for_each(|(_, net)| {
                rx += net.rx_bytes;
                tx += net.tx_bytes;
            });

            (rx, tx)
        },
        None => (0, 0),
    }
}

fn get_blk(stats: &Stats) -> (u64, u64) {
    match &stats.blkio_stats {
        Some(blk) => {
            let mut read: u64 = 0;
            let mut write: u64 = 0;
            blk.io_service_bytes_recursive.iter().for_each(|entry| {
                if entry.op == "Read" {
                    read += entry.value;
                } else if entry.op == "Write" {
                    write += entry.value;
                }
            });

            (read, write)
        },
        None => (0, 0),
    }
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
    let mut system = System::new_all();
    let mut interval = time::interval(Duration::from_secs(disk_refresh_interval));

    let mut prev_network_rx: u64 = 0;
    let mut prev_network_tx: u64 = 0;

    // 自动发现相关状态
    let mut last_discovery_time = std::time::Instant::now();
    let discovery_interval = Duration::from_secs(5); // 5秒检测间隔
    let mut discovered_mount_points: Vec<String> = Vec::new();
    let auto_discovery_enabled = disk_auto_discovery; // 如果启用了自动发现

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        tokio::select! {
            _ = interval.tick() => {
                // 刷新系统信息
                system.refresh_all();

                // 获取CPU使用率
                let cpu_usage = system.global_cpu_info().cpu_usage();

                // 获取内存使用情况
                let memory_used = system.used_memory();
                let memory_total = system.total_memory();
                let memory_usage_percent = if memory_total > 0 {
                    (memory_used as f32 / memory_total as f32) * 100.0
                } else {
                    0.0
                };

                // 获取网络使用情况
                let networks = sysinfo::Networks::new_with_refreshed_list();
                let mut network_rx: u64 = 0;
                let mut network_tx: u64 = 0;

                for (_, network) in &networks {
                    network_rx += network.total_received();
                    network_tx += network.total_transmitted();
                }

                // 计算网络速率（字节/秒）
                let network_rx_rate = network_rx.saturating_sub(prev_network_rx);
                let network_tx_rate = network_tx.saturating_sub(prev_network_tx);

                prev_network_rx = network_rx;
                prev_network_tx = network_tx;

                // 检查是否需要执行自动发现
                if auto_discovery_enabled && last_discovery_time.elapsed() >= discovery_interval {
                    // 使用 spawn_blocking 包装阻塞 I/O 操作，避免阻塞异步执行器
                    match tokio::task::spawn_blocking(discover_mount_points).await {
                        Ok(Ok(mount_points)) => {
                            discovered_mount_points = mount_points.iter()
                                .map(|mp| mp.mount_point.clone())
                                .collect();
                            debug!("自动发现 {} 个挂载点: {:?}", discovered_mount_points.len(), discovered_mount_points);
                            last_discovery_time = std::time::Instant::now();
                        }
                        Ok(Err(e)) => {
                            warn!("自动发现挂载点失败: {}", e);
                        }
                        Err(e) => {
                            warn!("spawn_blocking 任务失败: {}", e);
                        }
                    }
                }


                // 调试：打印当前状态
                debug!("disk_mount_points: {:?}", disk_mount_points);
                debug!("auto_discovery_enabled: {}", auto_discovery_enabled);
                debug!("discovered_mount_points: {:?}", discovered_mount_points);

                // 确定要监控的挂载点列表：自动发现的 + 用户指定的
                let mount_points_to_monitor = if auto_discovery_enabled {
                    // 合并自动发现的和用户指定的挂载点（去重）
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

                // 获取磁盘使用情况
                let disks = Disks::new_with_refreshed_list();
                let mut disk_used: u64 = 0;
                let mut disk_total: u64 = 0;
                let mut disk_details = Vec::new();
                let mut has_disk_alerts = false;

                for disk in disks.list() {
                    let mount_point = disk.mount_point().to_string_lossy();

                    // 使用自动发现或命令行参数过滤
                    if mount_points_to_monitor.contains(&mount_point.to_string()) {
                        let total = disk.total_space();
                        let available = disk.available_space();
                        let used = total.saturating_sub(available);
                        let usage_percent = if total > 0 {
                            (used as f32 / total as f32) * 100.0
                        } else {
                            0.0
                        };

                        // 获取文件系统类型和设备名称
                        let filesystem = disk.file_system().to_string_lossy().to_string();
                        let device = disk.name().to_string_lossy().to_string();

                        // 检查是否为网络文件系统
                        let is_network = is_network_filesystem(&filesystem);

                        // 检查告警状态
                        let is_alert = usage_percent >= disk_alert_threshold;
                        if is_alert {
                            has_disk_alerts = true;
                        }

                        disk_details.push(DiskDetail {
                            mount_point: mount_point.to_string(),
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
                }

                let disk_usage_percent = if disk_total > 0 {
                    (disk_used as f32 / disk_total as f32) * 100.0
                } else {
                    0.0
                };

                // 更新系统统计，保留当前的磁盘索引
                let mut data = data.lock().expect("mutex poisoned - recovering");
                let current_index = data.system_stats.current_disk_index;
                data.system_stats = SystemStats {
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
                debug!("collect system stats: {:?}", &data.system_stats);
            }
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

async fn collect_node_detail(
    node_id: String,
    data: SharedData,
    explorer_api_url: String,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    use tokio::time::{
        self,
        timeout,
        Duration,
    };

    const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

    let client = reqwest::Client::new();
    let url = format!("{explorer_api_url}/staking/stakingDetails");
    let mut interval = time::interval(Duration::from_secs(10)); // 每10秒更新一次
    let ranking_url = format!("{explorer_api_url}/staking/aliveStakingList");

    // 立即获取一次，不等待第一次tick
    let _ =
        timeout(REQUEST_TIMEOUT, fetch_node_detail(&client, &url, &node_id, data.clone())).await;
    let _ =
        timeout(REQUEST_TIMEOUT, fetch_node_ranking(&client, &ranking_url, &node_id, data.clone()))
            .await;

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        interval.tick().await;
        let _ = timeout(REQUEST_TIMEOUT, fetch_node_detail(&client, &url, &node_id, data.clone()))
            .await;
        let _ = timeout(
            REQUEST_TIMEOUT,
            fetch_node_ranking(&client, &ranking_url, &node_id, data.clone()),
        )
        .await;
    }
    Ok(())
}

async fn fetch_node_ranking(client: &reqwest::Client, url: &str, node_id: &str, data: SharedData) {
    let body = serde_json::json!({
        "pageNo": 1,
        "pageSize": 300,
        "key": "",
        "queryStatus": "all",
    });

    debug!("fetch node ranking: {}", url);

    match client
        .post(url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            debug!("Reponse: {}", resp.status());
            if !resp.status().is_success() {
                warn!("Node detail API returned error status: {}", resp.status());
                return;
            }
            let body_bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn!("Failed to read response body: {}", e);
                    return;
                },
            };
            let node_list_resp: types::NodeListResponse = match serde_json::from_slice(&body_bytes) {
                Ok(node_list_resp) => node_list_resp,
                Err(e) => {
                    warn!("Failed to parse response JSON: {}", e);
                    return;
                },
            };
            debug!("Node list response: {:?}", node_list_resp);

            // 解析响应
            if node_list_resp.code == 0 {
                if let Some(data_obj) = node_list_resp.data {
                    let ranking = parse_node_ranking(&data_obj, node_id);
                    let mut data = data.lock().expect("mutex poisoned - recovering");
                    if let Some(old_detail) = data.node_detail() {
                        let mut new_detail = old_detail;
                        new_detail.ranking = ranking;
                        data.update_node_detail(Some(new_detail));
                    } else {
                        let detail = NodeDetail {
                            ranking,
                            ..Default::default()
                        };
                        data.update_node_detail(Some(detail));
                    }
                } else {
                    warn!("Node detail response missing data field");
                    let mut data = data.lock().expect("mutex poisoned - recovering");
                    data.update_node_detail(None);
                }
            } else {
                warn!(
                    "Node detail API returned error code: {}, err_msg: {}",
                    node_list_resp.code, node_list_resp.err_msg
                );
                let mut data = data.lock().expect("mutex poisoned - recovering");
                data.update_node_detail(None);
            }
        },
        Err(e) => {
            warn!("Failed to fetch node detail: {}", e);
            let mut data = data.lock().expect("mutex poisoned - recovering");
            data.update_node_detail(None);
        },
    }
}

async fn fetch_node_detail(client: &reqwest::Client, url: &str, node_id: &str, data: SharedData) {
    let body = serde_json::json!({
        "nodeId": node_id
    });

    debug!("fetch node detail: {}", url);

    match client
        .post(url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            debug!("Reponse: {}", resp.status());
            if !resp.status().is_success() {
                warn!("Node detail API returned error status: {}", resp.status());
                return;
            }
            let body_bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn!("Failed to read response body: {}", e);
                    return;
                },
            };
            let node_detail_resp: types::NodeDetailRespose = match serde_json::from_slice(&body_bytes) {
                Ok(node_detail_resp) => node_detail_resp,
                Err(e) => {
                    warn!("Failed to parse response JSON: {}", e);
                    return;
                },
            };
            debug!("Node detail response: {:?}", node_detail_resp);

            if node_detail_resp.code == 0 {
                if let Some(detail) = node_detail_resp.data {
                    let mut node_detail = parse_node_detail(&detail);
                    let mut data = data.lock().expect("mutex poisoned - recovering");
                    if let Some(old_detail) = data.node_detail() {
                        node_detail.ranking = old_detail.ranking;
                    }
                    data.update_node_detail(Some(node_detail));
                } else {
                    warn!("Node detail response missing data field");
                    let mut data = data.lock().expect("mutex poisoned - recovering");
                    data.update_node_detail(None);
                }
            } else {
                warn!(
                    "Node detail API returned error code: {}, err_msg: {}",
                    node_detail_resp.code, node_detail_resp.err_msg
                );
                let mut data = data.lock().expect("mutex poisoned - recovering");
                data.update_node_detail(None);
            }
        },
        Err(e) => {
            warn!("Failed to fetch node detail: {}", e);
            let mut data = data.lock().expect("mutex poisoned - recovering");
            data.update_node_detail(None);
        },
    }
}

fn parse_node_detail(node_detail: &types::NodeDetail) -> NodeDetail {
    let node_name = node_detail.node_name.clone();
    let block_qty = node_detail.block_qty as u64;
    let expect_block_qty = node_detail.expect_block_qty;
    let mut block_rate = String::new();
    if block_qty > 0 && expect_block_qty > 0 {
        let rate = (block_qty as f64) / (expect_block_qty as f64);
        block_rate = format!("{:.2}%", rate * 100.0);
    }
    let daily_block_rate = node_detail.gen_blocks_rate.clone();
    let reward_per = node_detail.reward_per.parse::<f64>().ok().unwrap_or(0.0);
    let reward_value = node_detail.reward_value.parse::<f64>().ok().unwrap_or(0.0);
    let reward_address = node_detail.denefit_addr.clone();
    let verifier_time = node_detail.verifier_time as u64;

    NodeDetail {
        node_name,
        ranking: 0,
        block_qty,
        block_rate,
        daily_block_rate,
        reward_per,
        reward_value,
        reward_address,
        verifier_time,
    }
}

fn parse_node_ranking(
    data: &[NodeInfo],
    node_id: &str,
) -> i32 {
    match data.iter().find(|n| n.node_id == node_id) {
        Some(node) => node.ranking as i32,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::collect::docker_stats::{
        BlkioStats,
        CPUStats,
        CPUUsage,
        MemoryStats,
        NetworkStats,
        PidsStats,
        Stats,
        StorageStats,
        ThrottlingData,
    };

    /// Helper to create a minimal Stats struct for testing
    fn create_test_stats(
        cpu_total: u64,
        precpu_total: u64,
        system_cpu: u64,
        pre_system_cpu: u64,
        percpu_len: usize,
        mem_usage: u64,
        mem_limit: u64,
        cache: u64,
    ) -> Stats {
        Stats {
            name: None,
            id: None,
            read: String::new(),
            preread: String::new(),
            pids_stats: None,
            blkio_stats: None,
            num_procs: None,
            storage_stats: None,
            cpu_stats: CPUStats {
                cpu_usage: CPUUsage {
                    total_usage: cpu_total,
                    percpu_usage: Some(vec![0; percpu_len]),
                    usage_in_kernelmode: 0,
                    usage_in_usermode: 0,
                },
                system_cpu_usage: Some(system_cpu),
                online_cups: None,
                throttling_data: None,
            },
            precpu_stats: CPUStats {
                cpu_usage: CPUUsage {
                    total_usage: precpu_total,
                    percpu_usage: Some(vec![0; percpu_len]),
                    usage_in_kernelmode: 0,
                    usage_in_usermode: 0,
                },
                system_cpu_usage: Some(pre_system_cpu),
                online_cups: None,
                throttling_data: None,
            },
            memory_stats: MemoryStats {
                usage: mem_usage,
                max_usage: mem_usage,
                stats: {
                    let mut m = HashMap::new();
                    m.insert("cache".to_string(), cache);
                    m
                },
                failcnt: None,
                limit: mem_limit,
                commit: None,
                commit_peak_bytes: None,
                privated_working_set: None,
            },
            networks: None,
        }
    }

    // ========================================
    // calc_cpu_usage tests
    // ========================================

    #[test]
    fn test_calc_cpu_usage_normal() {
        // cpu_delta = 100, system_cpu_delta = 1000, num_cpus = 2
        // expected = (100/1000) * 2 * 100.0 = 20.0
        let stats = create_test_stats(
            1100,  // cpu_total
            1000,  // precpu_total -> cpu_delta = 100
            11000, // system_cpu
            10000, // pre_system_cpu -> system_cpu_delta = 1000
            2,     // percpu_len (2 CPUs)
            0, 0, 0,
        );
        let result = calc_cpu_usage(&stats);
        assert!((result - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_calc_cpu_usage_single_cpu() {
        // cpu_delta = 500, system_cpu_delta = 5000, num_cpus = 1
        // expected = (500/5000) * 1 * 100.0 = 10.0
        let stats = create_test_stats(
            1500,  // cpu_total
            1000,  // precpu_total -> cpu_delta = 500
            15000, // system_cpu
            10000, // pre_system_cpu -> system_cpu_delta = 5000
            1,     // percpu_len (1 CPU)
            0, 0, 0,
        );
        let result = calc_cpu_usage(&stats);
        assert!((result - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_calc_cpu_usage_zero_cpu_delta() {
        // When cpu_delta is 0, result should be 0
        // cpu_delta = 0, system_cpu_delta = 1000, num_cpus = 2
        // expected = (0/1000) * 2 * 100.0 = 0.0
        let stats = create_test_stats(
            1000,  // cpu_total
            1000,  // precpu_total -> cpu_delta = 0
            11000, // system_cpu
            10000, // pre_system_cpu -> system_cpu_delta = 1000
            2,     // percpu_len (2 CPUs)
            0, 0, 0,
        );
        let result = calc_cpu_usage(&stats);
        assert!((result - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_calc_cpu_usage_four_cpus() {
        // cpu_delta = 1000, system_cpu_delta = 2000, num_cpus = 4
        // expected = (1000/2000) * 4 * 100.0 = 200.0
        let stats = create_test_stats(
            2000,  // cpu_total
            1000,  // precpu_total -> cpu_delta = 1000
            12000, // system_cpu
            10000, // pre_system_cpu -> system_cpu_delta = 2000
            4,     // percpu_len (4 CPUs)
            0, 0, 0,
        );
        let result = calc_cpu_usage(&stats);
        assert!((result - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_calc_cpu_usage_precpu_system_none() {
        // When precpu_stats.system_cpu_usage is None, it defaults to 0
        // This tests the .unwrap_or(0) behavior
        let mut stats = create_test_stats(
            1100,  // cpu_total
            1000,  // precpu_total
            11000, // system_cpu
            10000, // pre_system_cpu
            2, 0, 0, 0,
        );
        stats.precpu_stats.system_cpu_usage = None;
        // pre_system_cpu = 0, system_cpu_delta = 11000 - 0 = 11000
        // cpu_delta = 100
        // expected = (100/11000) * 2 * 100.0 ≈ 1.818
        let result = calc_cpu_usage(&stats);
        assert!((result - 1.8181818).abs() < 0.001);
    }

    #[test]
    fn test_calc_cpu_usage_zero_system_delta() {
        // When system_cpu_delta is 0, return 0.0 to avoid division by zero
        // cpu_delta = 100, system_cpu_delta = 0, num_cpus = 2
        // expected = 0.0 (division avoided)
        let stats = create_test_stats(
            1100,  // cpu_total
            1000,  // precpu_total -> cpu_delta = 100
            10000, // system_cpu
            10000, // pre_system_cpu -> system_cpu_delta = 0
            2,     // percpu_len (2 CPUs)
            0, 0, 0,
        );
        let result = calc_cpu_usage(&stats);
        assert!((result - 0.0).abs() < 0.001);
    }

    // ========================================
    // calc_mem_usage tests
    // ========================================

    #[test]
    fn test_calc_mem_usage_normal() {
        // usage = 1024, cache = 256, limit = 4096
        // used_memory = 1024 - 256 = 768
        // mem_percent = (768 / 4096) * 100.0 = 18.75
        let stats = create_test_stats(0, 0, 0, 0, 1, 1024, 4096, 256);
        let (used, percent) = calc_mem_usage(&stats);
        assert_eq!(used, 768);
        assert!((percent - 18.75).abs() < 0.001);
    }

    #[test]
    fn test_calc_mem_usage_zero_cache() {
        // usage = 1024, cache = 0, limit = 4096
        // used_memory = 1024 - 0 = 1024
        // mem_percent = (1024 / 4096) * 100.0 = 25.0
        let stats = create_test_stats(0, 0, 0, 0, 1, 1024, 4096, 0);
        let (used, percent) = calc_mem_usage(&stats);
        assert_eq!(used, 1024);
        assert!((percent - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_calc_mem_usage_full_usage() {
        // usage = 4096, cache = 0, limit = 4096
        // used_memory = 4096 - 0 = 4096
        // mem_percent = (4096 / 4096) * 100.0 = 100.0
        let stats = create_test_stats(0, 0, 0, 0, 1, 4096, 4096, 0);
        let (used, percent) = calc_mem_usage(&stats);
        assert_eq!(used, 4096);
        assert!((percent - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_calc_mem_usage_cache_exceeds_usage_returns_zero() {
        // usage = 256, cache = 512, limit = 4096
        // After fix: saturating_sub returns 0 (no panic)
        // used_memory = 256.saturating_sub(512) = 0
        let stats = create_test_stats(0, 0, 0, 0, 1, 256, 4096, 512);
        let (used, percent) = calc_mem_usage(&stats);
        assert_eq!(used, 0);
        assert!((percent - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_calc_mem_usage_zero_usage() {
        // usage = 0, cache = 0, limit = 4096
        // used_memory = 0
        // mem_percent = 0.0
        let stats = create_test_stats(0, 0, 0, 0, 1, 0, 4096, 0);
        let (used, percent) = calc_mem_usage(&stats);
        assert_eq!(used, 0);
        assert!((percent - 0.0).abs() < 0.001);
    }

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
