#[warn(dead_code)]
use std::collections::HashMap;
use std::{
    fmt::format,
    sync::{
        Arc,
        Mutex,
    },
};

use hyper::{
    body::{
        Buf,
        HttpBody as _,
    },
    client::HttpConnector,
    Client,
};
use hyper_tls::HttpsConnector;
use log::{
    debug,
    warn,
};
use serde::{
    Deserialize,
    Serialize,
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
use web3::{
    futures::StreamExt,
    transports::WebSocket,
    types::BlockId,
};

use crate::Opts;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Deserialize, Serialize, Debug)]
struct Container {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Names")]
    names: Vec<String>,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "ImageID")]
    image_id: String,
    #[serde(rename = "Command")]
    command: String,
    #[serde(rename = "Created")]
    created: u64,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Status")]
    status: String,
    //#[serde(rename = "Ports")]
    //ports: Vec<>,
    #[serde(rename = "Labels")]
    labels: HashMap<String, String>,
    #[serde(rename = "SizeRw")]
    size_rw: Option<u64>,
    #[serde(rename = "SizeRootFs")]
    size_root_fs: Option<u64>,
    #[serde(rename = "HostConfig")]
    host_config: HashMap<String, String>,
}

type ContainerList = Vec<Container>;

/// `NetworkStats` aggregates the network stats of one container
#[derive(Serialize, Debug, Deserialize)]
struct NetworkStats {
    // Bytes received. Windows and Linux.
    rx_bytes: u64,
    // Packets received. Windows and Linux.
    rx_packets: Option<u64>,
    // Received errors. Not used on Windows.
    rx_errors: u64,
    // Incoming packets dropped. Windows and Linux.
    rx_dropped: u64,
    // Bytes sent. Windows and Linux.
    tx_bytes: u64,
    // Packets sent. Windows and Linux.
    tx_packets: Option<u64>,
    // Sent errors. Not used on Windows.
    tx_errors: u64,
    // Outgoing packets dropped. Windows and Linux.
    tx_dropped: u64,
    // Endpoint ID. Not used on Linux.
    endpoint_id: Option<String>,
    // Instance ID. Not used on Linux.
    instance_id: Option<String>,
}

/// `PidsStats` contains the stats of a container's pids
#[derive(Serialize, Deserialize, Debug)]
struct PidsStats {
    current: Option<u64>,
    limit: Option<u64>,
}

/// `BlkioStatEntry` is one small entity to store a piece of Blkio stats.
/// Not used on Windows.
#[derive(Serialize, Deserialize, Debug)]
struct BlkioStatEntry {
    major: u64,
    minor: u64,
    op: String,
    value: u64,
}

/// `BlkioStats` stores All IO service stats for data read and write.
/// This is a Linux speicfic structure as the differences between expressing
/// block I/O on Windows and Linux are sufficiently significant to make little
/// sense attempting to morph into a combined structure.
#[derive(Serialize, Deserialize, Debug)]
struct BlkioStats {
    // number of bytes transferred to and from the block device.
    io_service_bytes_recursive: Vec<BlkioStatEntry>,
    io_serviced_recursive: Vec<BlkioStatEntry>,
    io_queue_recursive: Vec<BlkioStatEntry>,
    io_wait_time_recursive: Vec<BlkioStatEntry>,
    io_merged_recursive: Vec<BlkioStatEntry>,
    io_time_recursive: Vec<BlkioStatEntry>,
    sectors_recursive: Vec<BlkioStatEntry>,
}

/// `StorageStats` is the disk I/O stats for read/write on Windows.
#[derive(Serialize, Deserialize, Debug)]
struct StorageStats {
    read_count_normalized: Option<u64>,
    read_size_bytes: Option<u64>,
    write_count_normalized: Option<u64>,
    write_size_bytes: Option<u64>,
}

/// `CPUUsage` stores **All CPU** stats aggregated since container inception.
#[derive(Serialize, Deserialize, Debug)]
struct CPUUsage {
    // Total CPU time consumed.
    // Units: nanoseconds (Linux)
    // Units: 100's of nanoseconds (Windows)
    total_usage: u64,

    // Total CPU time consumed per core (Linux). Not used on Windows.
    // Units: nanoseconds.
    percpu_usage: Option<Vec<u64>>,

    // Time spent by tasks of the cgroup in kernel mode (Linux).
    // Time spent by all container processes in kernel mod (Windows).
    // Units: nanoseconds (Linux).
    // Units: 100's of nanoseconds (Windows). Not populated for Hyper-V containers.
    usage_in_kernelmode: u64,

    // Time spent by tasks of the cgroup in user mode (Linux).
    // Time spent by all container processes in user mode (Windows).
    // Units: nanoseconds (Linux).
    // Units: 100's of nanoseconds (Windows). Not populated for Hyper-V Containers
    usage_in_usermode: u64,
}

/// `ThrottlingData` stores CPU throttling stats of one running container.
/// Not used on Windows.
#[derive(Serialize, Deserialize, Debug)]
struct ThrottlingData {
    // Number of periods with throttling active.
    periods: u64,
    throttled_periods: u64,
    throtted_time: Option<u64>,
}

/// `CPUStats` aggregated and wraps all CPU related info of container.
#[derive(Serialize, Deserialize, Debug)]
struct CPUStats {
    // CPU Usages. Linux and Windows.
    cpu_usage: CPUUsage,

    // System Usage. Linux only.
    system_cpu_usage: Option<u64>,

    // Online CPUs. Linux only.
    online_cups: Option<u32>,

    // Throttling Data. Linux only.
    throttling_data: Option<ThrottlingData>,
}

/// `MemoryStats` aggregates all memory stats since container inception on Linux.
/// Windows returns stats for commit and private working set only.
#[derive(Serialize, Deserialize, Debug)]
struct MemoryStats {
    // current res_counter usage of memory.
    usage: u64,
    // maximum usage ever recorded.
    max_usage: u64,
    // all the stats exported via memory.stat.
    stats: HashMap<String, u64>,
    // number of times memory usage hits limits.
    failcnt: Option<u64>,
    limit: u64,

    // committed bytes
    commit: Option<u64>,
    // peak committed bytes
    #[serde(rename = "commitpeakbytes")]
    commit_peak_bytes: Option<u64>,
    // private working set
    #[serde(rename = "privatedworkingset")]
    privated_working_set: Option<u64>,
}

/// `Stats` is Ultimate struct aggregating all types of states of one container.
#[derive(Serialize, Deserialize, Debug)]
struct Stats {
    name: Option<String>,
    id: Option<String>,

    // Common stats
    read: String,
    preread: String,

    // Linux specific stats, not populated on Windows
    pids_stats: Option<PidsStats>,
    blkio_stats: Option<BlkioStats>,

    // Windwos specific stats, not populated on Linux.
    num_procs: Option<u32>,
    storage_stats: Option<StorageStats>,

    // Shared stats
    cpu_stats: CPUStats,
    precpu_stats: CPUStats,
    memory_stats: MemoryStats,

    networks: Option<HashMap<String, NetworkStats>>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct NodeDetail {
    pub node_name: String,
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
}

pub async fn run(collector: Collector) -> Result<()> {
    tokio::select! {
        res = collector.run() => {
            res
        }
    }
}

impl Default for ConsensusState {
    fn default() -> Self {
        ConsensusState {
            name: String::from(""),
            host: String::default(),
            current_number: 0,
            epoch: 0,
            view: 0,
            committed: 0,
            locked: 0,
            qc: 0,
            validator: false,
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
        let txns = self.txns.clone();
        self.txns.clear();
        txns
    }

    pub fn intervals_and_clear(&mut self) -> Vec<u64> {
        let intervals = self.intervals.clone();
        self.intervals.clear();
        intervals
    }

    pub fn cur_interval(&self) -> u64 {
        self.cur_interval
    }

    pub fn max_interval(&self) -> u64 {
        self.max_interval
    }

    pub fn states(&self) -> Vec<ConsensusState> {
        let states: Vec<ConsensusState> = self.states.iter().map(|(_, val)| val.clone()).collect();
        states
    }

    pub fn stats(&self) -> HashMap<String, NodeStats> {
        let stats = self.stats.clone();
        stats
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
    ) -> Self {
        let urls: Vec<&str> = opts.url.as_str().split(",").collect();
        let urls: Vec<(String, String)> = urls
            .into_iter()
            .map(|url| {
                let v: Vec<&str> = url.split("@").collect();
                if v.len() < 2 {
                    panic!("invalid url");
                }
                (v[0].into(), v[1].into())
            })
            .collect();
        let enable_docker_stats = opts.enable_docker_stats;
        let docker_port = opts.docker_port;
        let disk_mount_points = opts.disk_mount_points.clone();
        let disk_auto_discovery = opts.disk_auto_discovery;
        let disk_alert_threshold = opts.disk_alert_threshold;
        let disk_refresh_interval = opts.disk_refresh_interval;
        let node_id = opts.node_id.clone();

        Collector {
            data,
            urls,
            enable_docker_stats,
            docker_port,
            disk_mount_points,
            disk_auto_discovery,
            disk_alert_threshold,
            disk_refresh_interval,
            node_id,
        }
    }

    pub(crate) async fn run(&self) -> Result<()> {
        let ws = WebSocket::new(self.urls[0].1.as_str()).await?;
        let web3 = web3::Web3::new(ws.clone());
        let mut sub = web3.platon_subscribe().subscribe_new_heads().await?;

        let urls = self.urls.clone();
        let _: Vec<_> = urls
            .into_iter()
            .map(|url| {
                let name = url.0.clone();
                tokio::spawn(collect_node_state(name.clone(), url.1.clone(), self.data.clone()));

                debug!("enable_docker_stats: {}", self.enable_docker_stats);
                if self.enable_docker_stats {
                    debug!("enable_docker_stats: {}", self.enable_docker_stats);
                    let host = url.1.clone();
                    let host = host.replace("ws://", "");
                    let ip_port: Vec<&str> = host.as_str().split(":").collect();
                    let host = format!("http://{}:{}", ip_port[0], self.docker_port);
                    tokio::spawn(collect_node_stats(name.clone(), host, self.data.clone()));
                }
            })
            .collect();

        // 启动节点详情监控
        if let Some(node_id) = &self.node_id {
            debug!("start collect node detail: {}", node_id);
            tokio::spawn(collect_node_detail(node_id.clone(), self.data.clone()));
        }

        // 启动本机系统监控
        #[cfg(target_family = "unix")]
        tokio::spawn(collect_system_stats(
            self.data.clone(),
            self.disk_mount_points.clone(),
            self.disk_auto_discovery,
            self.disk_alert_threshold,
            self.disk_refresh_interval,
        ));

        loop {
            tokio::select! {
                Some(head) = (&mut sub).next() => {
                    let head = head.unwrap();
                    let number = head.number.unwrap();
                    let number = BlockId::from(number);
                    let txs = web3.platon().block_transaction_count(number).await?;
                    let txs = txs.unwrap().as_u64();

                    let mut data = self.data.lock().unwrap();
                    data.cur_block_number = head.number.unwrap().as_u64();
                    if data.cur_block_time > 0 {
                        data.prev_block_time = data.cur_block_time;
                    }
                    data.cur_block_time = head.timestamp.as_u64();
                    data.cur_txs = txs;

                    if txs > data.max_txs {
                        data.max_txs = txs;
                        data.max_txs_block_number = head.number.unwrap().as_u64();
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
    }
}

async fn collect_node_state(
    name: String,
    url: String,
    data: SharedData,
) -> Result<()> {
    let ws = WebSocket::new(url.as_str()).await?;
    let web3 = web3::Web3::new(ws.clone());
    let debug = web3.debug();
    let platon = web3.platon();
    let host = url.replace("ws://", "");

    let mut interval = time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let state = debug.consensus_status().await?;
                let cur_number = platon.block_number().await?;
                let node = ConsensusState{
                    name: name.clone(),
                    host: host.clone(),
                    current_number: cur_number.as_u64(),
                    epoch: state.state.view.epoch,
                    view: state.state.view.view,
                    committed: state.state.committed.number,
                    locked: state.state.locked.number,
                    qc: state.state.qc.number,
                    validator: state.validator,
                };

                let mut data = data.lock().unwrap();
                data.states.insert(name.clone(), node);
            }
        }
    }
}

async fn get_container_id(
    host: String,
    name: String,
) -> Result<String> {
    let client = Client::new();
    let uri = format!("{}/containers/json", host).parse()?;
    let resp = client.get(uri).await?;
    let body = hyper::body::to_bytes(resp.into_body()).await?;

    let container_list: ContainerList = serde_json::from_slice(body.as_ref()).unwrap();

    let v: Vec<String> = container_list
        .into_iter()
        .filter(|c| {
            let cc: Vec<_> = c
                .names
                .iter()
                .filter(|cname| {
                    if cname.contains(name.as_str()) {
                        true
                    } else {
                        false
                    }
                })
                .collect();
            cc.len() > 0
        })
        .map(|c| c.id.clone())
        .collect();
    if v.len() > 0 {
        Ok(v[0].clone())
    } else {
        Err("not found".into())
    }
}

async fn collect_node_stats(
    name: String,
    host: String,
    data: SharedData,
) -> Result<()> {
    debug!("name: {}, host: {}", name, host);
    //let id = get_container_id(host.clone(), name.clone()).await?;

    let client = Client::new();
    let uri = format!("{}/containers/{}/stats", host, name).parse()?;
    debug!("uri: {:?}", uri);

    let mut resp = client.get(uri).await?;
    debug!("status: {:?}", resp.status());
    debug!("headers: {:#?}", resp.headers());

    let mut bufs: Vec<u8> = Vec::new();

    loop {
        tokio::select! {
            Some(chunk) = resp.body_mut().data() => {
                let chunk = chunk?;
                if chunk.has_remaining() {
                    bufs.append(&mut chunk.to_vec().clone());
                    let stats: Stats = match serde_json::from_slice(bufs.as_ref()) {
                        Err(_) => continue,
                        Ok(stats) => stats,
                    };
                    debug!("stats: {:#?}", stats);
                    //bufs.clear();
                    let _ = std::mem::replace(&mut bufs, Default::default());

                    update_node_stats(name.as_str(), data.clone(), &stats);
                }
            }
        }
    }
}

fn update_node_stats(
    name: &str,
    data: SharedData,
    stats: &Stats,
) {
    let (mem, mem_usage) = calc_mem_usage(&stats);

    let (rx, tx) = get_network_rx_tx(&stats);
    let (blk_read, blk_write) = get_blk(&stats);

    let node_stats = NodeStats {
        cpu_percent: calc_cpu_usage(&stats),
        mem,
        mem_percent: mem_usage,
        mem_limit: stats.memory_stats.limit,
        network_rx: rx,
        network_tx: tx,
        blk_read,
        blk_write,
    };

    let mut data = data.lock().unwrap();
    data.stats.insert(name.to_string(), node_stats);
}

fn calc_cpu_usage(stats: &Stats) -> f64 {
    let cpu_usage = &stats.cpu_stats.cpu_usage;
    let precpu_usage = &stats.precpu_stats.cpu_usage;
    let cpu_delta = cpu_usage.total_usage - precpu_usage.total_usage;
    let precpu_system_cpu_usage = stats.precpu_stats.system_cpu_usage.unwrap_or(0);
    let system_cpu_delta = stats.cpu_stats.system_cpu_usage.unwrap() - precpu_system_cpu_usage;
    let num_cpus = cpu_usage.percpu_usage.clone().unwrap().len();

    (cpu_delta as f64 / system_cpu_delta as f64) * num_cpus as f64 * 100.0
}

fn calc_mem_usage(stats: &Stats) -> (u64, f64) {
    let memory_stat = &stats.memory_stats;
    let cache = memory_stat.stats.get("cache").unwrap();
    let used_memory = memory_stat.usage - cache;
    let avaliable_memory = memory_stat.limit;
    (used_memory, (used_memory as f64 / avaliable_memory as f64) * 100.0)
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
        None => return (0, 0),
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
                    match discover_mount_points() {
                        Ok(mount_points) => {
                            discovered_mount_points = mount_points.iter()
                                .map(|mp| mp.mount_point.clone())
                                .collect();
                            debug!("自动发现 {} 个挂载点: {:?}", discovered_mount_points.len(), discovered_mount_points);
                            last_discovery_time = std::time::Instant::now();
                        }
                        Err(e) => {
                            warn!("自动发现挂载点失败: {}", e);
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
                let mut data = data.lock().unwrap();
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
    device: String,
    mount_point: String,
    filesystem: String,
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
            return discover_mount_points_fallback();
        },
    };

    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.len() >= 3 {
            let device = parts[0].to_string();
            let mount_point = parts[1].to_string();
            let filesystem = parts[2].to_string();

            // 跳过特殊文件系统
            if !is_special_filesystem(&filesystem) {
                mount_points.push(MountPointInfo {
                    device,
                    mount_point,
                    filesystem,
                });
            }
        }
    }

    Ok(mount_points)
}

/// 使用sysinfo作为后备的挂载点发现
fn discover_mount_points_fallback() -> Result<Vec<MountPointInfo>> {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();
    let mut mount_points = Vec::new();

    for disk in disks.list() {
        let device = disk.name().to_string_lossy().to_string();
        let mount_point = disk.mount_point().to_string_lossy().to_string();
        let filesystem = disk.file_system().to_string_lossy().to_string();

        // 跳过特殊文件系统
        if !is_special_filesystem(&filesystem) {
            mount_points.push(MountPointInfo {
                device,
                mount_point,
                filesystem,
            });
        }
    }

    Ok(mount_points)
}

async fn collect_node_detail(
    node_id: String,
    data: SharedData,
) -> Result<()> {
    use tokio::time::{
        self,
        Duration,
    };

    let https = HttpsConnector::new();
    let client = hyper::Client::builder().build(https);
    let url = "https://scan.platon.network/browser-server/staking/stakingDetails";
    let mut interval = time::interval(Duration::from_secs(60)); // 每60秒更新一次

    // 立即获取一次，不等待第一次tick
    fetch_node_detail(&client, url, &node_id, data.clone()).await;

    loop {
        interval.tick().await;
        fetch_node_detail(&client, url, &node_id, data.clone()).await;
    }
}

async fn fetch_node_detail(
    client: &hyper::Client<hyper_tls::HttpsConnector<HttpConnector>>,
    url: &str,
    node_id: &str,
    data: SharedData,
) {
    use hyper::{
        Body,
        Method,
        Request,
    };

    let body = serde_json::json!({
        "nodeId": node_id
    });
    let req = match Request::builder()
        .method(Method::POST)
        .uri(url)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
    {
        Ok(req) => req,
        Err(e) => {
            warn!("Failed to build request: {}", e);
            return;
        },
    };

    debug!("fetch node detail: {:?}", req);

    match client.request(req).await {
        Ok(resp) => {
            debug!("Reponse: {}", resp.status());
            if !resp.status().is_success() {
                warn!("Node detail API returned error status: {}", resp.status());
                return;
            }
            let body_bytes = match hyper::body::to_bytes(resp.into_body()).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn!("Failed to read response body: {}", e);
                    return;
                },
            };
            let json: serde_json::Value = match serde_json::from_slice(&body_bytes) {
                Ok(json) => json,
                Err(e) => {
                    warn!("Failed to parse response JSON: {}", e);
                    return;
                },
            };
            debug!("Body: {}", json);

            // 解析响应
            if let Some(code) = json.get("code").and_then(|c| c.as_i64()) {
                if code == 0 {
                    if let Some(data_obj) = json.get("data") {
                        match parse_node_detail(data_obj) {
                            Ok(detail) => {
                                let mut data = data.lock().unwrap();
                                data.update_node_detail(Some(detail));
                            },
                            Err(e) => {
                                warn!("Failed to parse node detail: {}", e);
                                let mut data = data.lock().unwrap();
                                data.update_node_detail(None);
                            },
                        }
                    } else {
                        warn!("Node detail response missing data field");
                        let mut data = data.lock().unwrap();
                        data.update_node_detail(None);
                    }
                } else {
                    warn!("Node detail API returned error code: {}", code);
                    let mut data = data.lock().unwrap();
                    data.update_node_detail(None);
                }
            } else {
                warn!("Node detail response missing code field");
                let mut data = data.lock().unwrap();
                data.update_node_detail(None);
            }
        },
        Err(e) => {
            warn!("Failed to fetch node detail: {}", e);
            let mut data = data.lock().unwrap();
            data.update_node_detail(None);
        },
    }
}

fn parse_node_detail(data: &serde_json::Value) -> Result<NodeDetail> {
    let node_name = data.get("nodeName").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let block_qty = data.get("blockQty").and_then(|v| v.as_u64()).unwrap_or(0);
    let expect_block_qty = data.get("expectBlockQty").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut block_rate = String::new();
    if block_qty > 0 && expect_block_qty > 0 {
        let rate = (block_qty as f64) / (expect_block_qty as f64);
        block_rate = format!("{:.2}%", rate * 100.0);
    }

    let daily_block_rate =
        data.get("genBlocksRate").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let reward_per = data
        .get("rewardPer")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let reward_value = data
        .get("rewardValue")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let reward_address = data.get("denefitAddr").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let verifier_time = data.get("verifierTime").and_then(|v| v.as_u64()).unwrap_or(0);

    Ok(NodeDetail {
        node_name,
        block_qty,
        block_rate,
        daily_block_rate,
        reward_per,
        reward_value,
        reward_address,
        verifier_time,
    })
}
