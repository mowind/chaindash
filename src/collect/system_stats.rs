use std::{
    sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        Arc,
        Mutex,
    },
    time::Instant,
};

use log::{
    debug,
    warn,
};
use sysinfo::{
    Disks,
    System,
};
use tokio::time::{
    self,
    Duration,
};

use super::data::{
    record_status_message,
    warn_with_status,
    DiskDetail,
    SharedData,
    StatusLevel,
    SystemStats,
};
use crate::{
    error::{
        ChaindashError,
        Result,
    },
    sync::lock_or_panic,
};

#[derive(Debug, Clone, Copy)]
struct NetworkSample {
    rx_total: u64,
    tx_total: u64,
    collected_at: Instant,
}

fn compute_network_rates(
    previous: Option<NetworkSample>,
    rx_total: u64,
    tx_total: u64,
    collected_at: Instant,
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

#[derive(Debug, Clone)]
struct MountPointInfo {
    mount_point: String,
}

pub(crate) async fn collect_system_stats(
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

    let mut last_discovery_time = Instant::now();
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
        collected_at: Instant,
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
                            last_discovery_time = Instant::now();
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
                        network_rx_total = network_rx_total.saturating_add(network.total_received());
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
                            last_updated: Instant::now(),
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
                        collected_at: Instant::now(),
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
                    let mut data_guard = lock_or_panic(&data);
                    data_guard.replace_system_stats(SystemStats {
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
                        current_disk_index: 0,
                        alert_threshold: disk_alert_threshold,
                        has_disk_alerts,
                        auto_discovery_enabled,
                    })
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

fn is_network_filesystem(filesystem: &str) -> bool {
    let fs_lower = filesystem.to_lowercase();
    fs_lower.contains("nfs") || fs_lower.contains("smb") || fs_lower.contains("cifs")
}

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

fn discover_mount_points() -> Result<Vec<MountPointInfo>> {
    use std::{
        fs::File,
        io::{
            BufRead,
            BufReader,
        },
    };

    let mut mount_points = Vec::new();

    let file = match File::open("/proc/mounts") {
        Ok(f) => f,
        Err(e) => {
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

            if !is_special_filesystem(&filesystem) {
                mount_points.push(MountPointInfo { mount_point });
            }
        }
    }

    Ok(mount_points)
}

fn discover_mount_points_fallback() -> Vec<MountPointInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut mount_points = Vec::new();

    for disk in disks.list() {
        let mount_point = disk.mount_point().to_string_lossy().to_string();
        let filesystem = disk.file_system().to_string_lossy().to_string();

        if !is_special_filesystem(&filesystem) {
            mount_points.push(MountPointInfo { mount_point });
        }
    }

    mount_points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_network_rates_first_sample_returns_zero() {
        let collected_at = Instant::now();
        let (_, rx_rate, tx_rate) = compute_network_rates(None, 2048, 4096, collected_at);

        assert_eq!(rx_rate, 0);
        assert_eq!(tx_rate, 0);
    }

    #[test]
    fn test_compute_network_rates_normalizes_by_elapsed_time() {
        let start = Instant::now();
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
