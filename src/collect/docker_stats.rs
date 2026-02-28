//! Docker container stats types for parsing Docker API responses.
//!
//! These types are used internally to parse the Docker stats API JSON response.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// `NetworkStats` aggregates the network stats of one container
#[derive(Serialize, Debug, Deserialize)]
pub(crate) struct NetworkStats {
    // Bytes received. Windows and Linux.
    pub(crate) rx_bytes: u64,
    // Packets received. Windows and Linux.
    pub(crate) rx_packets: Option<u64>,
    // Received errors. Not used on Windows.
    pub(crate) rx_errors: u64,
    // Incoming packets dropped. Windows and Linux.
    pub(crate) rx_dropped: u64,
    // Bytes sent. Windows and Linux.
    pub(crate) tx_bytes: u64,
    // Packets sent. Windows and Linux.
    pub(crate) tx_packets: Option<u64>,
    // Sent errors. Not used on Windows.
    pub(crate) tx_errors: u64,
    // Outgoing packets dropped. Windows and Linux.
    pub(crate) tx_dropped: u64,
    // Endpoint ID. Not used on Linux.
    pub(crate) endpoint_id: Option<String>,
    // Instance ID. Not used on Linux.
    pub(crate) instance_id: Option<String>,
}

/// `PidsStats` contains the stats of a container's pids
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct PidsStats {
    pub(crate) current: Option<u64>,
    pub(crate) limit: Option<u64>,
}

/// `BlkioStatEntry` is one small entity to store a piece of Blkio stats.
/// Not used on Windows.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct BlkioStatEntry {
    pub(crate) major: u64,
    pub(crate) minor: u64,
    pub(crate) op: String,
    pub(crate) value: u64,
}

/// `BlkioStats` stores All IO service stats for data read and write.
/// This is a Linux specific structure as the differences between expressing
/// block I/O on Windows and Linux are sufficiently significant to make little
/// sense attempting to morph into a combined structure.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct BlkioStats {
    // number of bytes transferred to and from the block device.
    pub(crate) io_service_bytes_recursive: Vec<BlkioStatEntry>,
    pub(crate) io_serviced_recursive: Vec<BlkioStatEntry>,
    pub(crate) io_queue_recursive: Vec<BlkioStatEntry>,
    pub(crate) io_wait_time_recursive: Vec<BlkioStatEntry>,
    pub(crate) io_merged_recursive: Vec<BlkioStatEntry>,
    pub(crate) io_time_recursive: Vec<BlkioStatEntry>,
    pub(crate) sectors_recursive: Vec<BlkioStatEntry>,
}

/// `StorageStats` is the disk I/O stats for read/write on Windows.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct StorageStats {
    pub(crate) read_count_normalized: Option<u64>,
    pub(crate) read_size_bytes: Option<u64>,
    pub(crate) write_count_normalized: Option<u64>,
    pub(crate) write_size_bytes: Option<u64>,
}

/// `CPUUsage` stores **All CPU** stats aggregated since container inception.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct CPUUsage {
    // Total CPU time consumed.
    // Units: nanoseconds (Linux)
    // Units: 100's of nanoseconds (Windows)
    pub(crate) total_usage: u64,

    // Total CPU time consumed per core (Linux). Not used on Windows.
    // Units: nanoseconds.
    pub(crate) percpu_usage: Option<Vec<u64>>,

    // Time spent by tasks of the cgroup in kernel mode (Linux).
    // Time spent by all container processes in kernel mod (Windows).
    // Units: nanoseconds (Linux).
    // Units: 100's of nanoseconds (Windows). Not populated for Hyper-V containers.
    pub(crate) usage_in_kernelmode: u64,

    // Time spent by tasks of the cgroup in user mode (Linux).
    // Time spent by all container processes in user mode (Windows).
    // Units: nanoseconds (Linux).
    // Units: 100's of nanoseconds (Windows). Not populated for Hyper-V Containers
    pub(crate) usage_in_usermode: u64,
}

/// `ThrottlingData` stores CPU throttling stats of one running container.
/// Not used on Windows.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ThrottlingData {
    // Number of periods with throttling active.
    pub(crate) periods: u64,
    pub(crate) throttled_periods: u64,
    pub(crate) throtted_time: Option<u64>,
}

/// `CPUStats` aggregated and wraps all CPU related info of container.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct CPUStats {
    // CPU Usages. Linux and Windows.
    pub(crate) cpu_usage: CPUUsage,

    // System Usage. Linux only.
    pub(crate) system_cpu_usage: Option<u64>,

    // Online CPUs. Linux only.
    pub(crate) online_cups: Option<u32>,

    // Throttling Data. Linux only.
    pub(crate) throttling_data: Option<ThrottlingData>,
}

/// `MemoryStats` aggregates all memory stats since container inception on Linux.
/// Windows returns stats for commit and private working set only.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct MemoryStats {
    // current res_counter usage of memory.
    pub(crate) usage: u64,
    // maximum usage ever recorded.
    pub(crate) max_usage: u64,
    // all the stats exported via memory.stat.
    pub(crate) stats: HashMap<String, u64>,
    // number of times memory usage hits limits.
    pub(crate) failcnt: Option<u64>,
    pub(crate) limit: u64,

    // committed bytes
    pub(crate) commit: Option<u64>,
    // peak committed bytes
    #[serde(rename = "commitpeakbytes")]
    pub(crate) commit_peak_bytes: Option<u64>,
    // private working set
    #[serde(rename = "privatedworkingset")]
    pub(crate) privated_working_set: Option<u64>,
}

/// `Stats` is Ultimate struct aggregating all types of states of one container.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Stats {
    pub(crate) name: Option<String>,
    pub(crate) id: Option<String>,

    // Common stats
    pub(crate) read: String,
    pub(crate) preread: String,

    // Linux specific stats, not populated on Windows
    pub(crate) pids_stats: Option<PidsStats>,
    pub(crate) blkio_stats: Option<BlkioStats>,

    // Windows specific stats, not populated on Linux.
    pub(crate) num_procs: Option<u32>,
    pub(crate) storage_stats: Option<StorageStats>,

    // Shared stats
    pub(crate) cpu_stats: CPUStats,
    pub(crate) precpu_stats: CPUStats,
    pub(crate) memory_stats: MemoryStats,

    pub(crate) networks: Option<HashMap<String, NetworkStats>>,
}
