# sysinfo crate 磁盘监控功能研究报告

## 【品味评分】
🟡 凑合 - 现有代码能用，但有改进空间

## 接口规范

### 核心结构体

#### `sysinfo::Disks`
```rust
pub struct Disks { /* private fields */ }
```
磁盘列表的容器，提供磁盘信息的集合操作。

**构造方法：**
- `new() -> Disks` - 创建空磁盘列表
- `new_with_refreshed_list() -> Disks` - 创建并立即刷新所有磁盘信息
- `new_with_refreshed_list_specifics(refreshes: DiskRefreshKind) -> Disks` - 创建并刷新指定类型的磁盘信息

**方法：**
- `list() -> &[Disk]` - 获取磁盘列表的不可变引用
- `list_mut() -> &mut [Disk]` - 获取磁盘列表的可变引用
- `refresh(remove_not_listed_disks: bool)` - 刷新所有磁盘信息
- `refresh_specifics(remove_not_listed_disks: bool, refreshes: DiskRefreshKind)` - 刷新指定类型的磁盘信息

#### `sysinfo::Disk`
```rust
pub struct Disk { /* private fields */ }
```
单个磁盘的信息容器。

**方法：**
- `kind() -> DiskKind` - 磁盘类型（HDD/SSD等）
- `name() -> &OsStr` - 磁盘名称
- `file_system() -> &OsStr` - 文件系统类型（EXT4/NTFS等）
- `mount_point() -> &Path` - 挂载点路径
- `total_space() -> u64` - 总空间（字节）
- `available_space() -> u64` - 可用空间（字节）
- `is_removable() -> bool` - 是否可移动
- `is_read_only() -> bool` - 是否只读
- `refresh() -> bool` - 刷新磁盘信息
- `refresh_specifics(refreshes: DiskRefreshKind) -> bool` - 刷新指定信息
- `usage() -> DiskUsage` - 磁盘读写统计

#### `sysinfo::DiskRefreshKind`
```rust
pub struct DiskRefreshKind { /* private fields */ }
```
控制刷新哪些磁盘信息的枚举。

**工厂方法：**
- `everything() -> Self` - 刷新所有信息
- `new() -> Self` - 创建空的刷新类型
- `list()` - 刷新磁盘列表

## 基础使用

### 安装
在 `Cargo.toml` 中添加：
```toml
[dependencies]
sysinfo = "0.37"
```

### 最简单的使用示例
```rust
use sysinfo::Disks;

fn main() {
    // 获取所有磁盘信息
    let disks = Disks::new_with_refreshed_list();

    for disk in disks.list() {
        println!("磁盘名称: {:?}", disk.name());
        println!("挂载点: {:?}", disk.mount_point());
        println!("文件系统: {:?}", disk.file_system());
        println!("总空间: {} GB", disk.total_space() / 1_000_000_000);
        println!("可用空间: {} GB", disk.available_space() / 1_000_000_000);
        println!("类型: {:?}", disk.kind());
        println!("是否可移动: {}", disk.is_removable());
        println!("是否只读: {}", disk.is_read_only());
        println!("---");
    }
}
```

### 根据挂载点过滤磁盘
```rust
use sysinfo::Disks;
use std::path::Path;

fn get_disk_by_mount_point(mount_point: &str) -> Option<&Disk> {
    let disks = Disks::new_with_refreshed_list();

    disks.list().iter().find(|disk| {
        disk.mount_point() == Path::new(mount_point)
    })
}

fn filter_disks_by_mount_points(mount_points: &[&str]) -> Vec<&Disk> {
    let disks = Disks::new_with_refreshed_list();
    let mount_paths: Vec<_> = mount_points.iter()
        .map(|mp| Path::new(mp))
        .collect();

    disks.list().iter()
        .filter(|disk| mount_paths.contains(&disk.mount_point()))
        .collect()
}
```

## 进阶技巧

### 性能优化
```rust
use sysinfo::{Disks, DiskRefreshKind};

// 只刷新磁盘列表，不刷新使用统计（性能更好）
let mut disks = Disks::new();
disks.refresh_specifics(false, DiskRefreshKind::new().list());

// 或者只创建时刷新列表
let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::new().list());
```

### 监控磁盘使用率变化
```rust
use sysinfo::Disks;
use std::time::{Duration, Instant};
use std::thread;

fn monitor_disk_usage(mount_point: &str, interval_secs: u64) {
    let mut last_available = 0u64;
    let interval = Duration::from_secs(interval_secs);

    loop {
        let disks = Disks::new_with_refreshed_list();

        if let Some(disk) = disks.list().iter()
            .find(|d| d.mount_point().to_string_lossy() == mount_point)
        {
            let available = disk.available_space();
            let total = disk.total_space();
            let used = total.saturating_sub(available);
            let usage_percent = (used as f64 / total as f64) * 100.0;

            println!("{} 使用率: {:.1}% (可用: {} GB, 总共: {} GB)",
                mount_point, usage_percent,
                available / 1_000_000_000,
                total / 1_000_000_000);

            // 检测空间变化
            if last_available > 0 {
                let change = available as i64 - last_available as i64;
                if change < 0 {
                    println!("警告: 磁盘空间减少了 {} MB", (-change) / 1_000_000);
                }
            }

            last_available = available;
        }

        thread::sleep(interval);
    }
}
```

## 巧妙用法

### 1. 智能磁盘选择器
```rust
use sysinfo::Disks;

/// 选择最适合的磁盘（最大可用空间）
fn select_best_disk() -> Option<&Disk> {
    let disks = Disks::new_with_refreshed_list();

    disks.list().iter()
        .filter(|disk| {
            // 排除特殊文件系统
            let fs = disk.file_system().to_string_lossy();
            !fs.contains("tmpfs") &&
            !fs.contains("proc") &&
            !fs.contains("sysfs") &&
            !disk.is_removable()  // 排除移动设备
        })
        .max_by_key(|disk| disk.available_space())
}

/// 根据使用率选择磁盘
fn select_disk_by_usage_threshold(max_usage_percent: f64) -> Vec<&Disk> {
    let disks = Disks::new_with_refreshed_list();

    disks.list().iter()
        .filter(|disk| {
            let total = disk.total_space();
            let available = disk.available_space();
            let used = total.saturating_sub(available);
            let usage_percent = (used as f64 / total as f64) * 100.0;

            usage_percent > max_usage_percent
        })
        .collect()
}
```

### 2. 跨平台路径处理
```rust
use sysinfo::Disks;
use std::path::Path;

fn normalize_mount_point(path: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        path.to_string_lossy().to_lowercase()
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.to_string_lossy().to_string()
    }
}

fn find_disk_cross_platform(mount_point: &str) -> Option<&Disk> {
    let disks = Disks::new_with_refreshed_list();
    let normalized_target = normalize_mount_point(Path::new(mount_point));

    disks.list().iter().find(|disk| {
        normalize_mount_point(disk.mount_point()) == normalized_target
    })
}
```

## 注意事项

### 1. 平台差异
- **Linux**: 默认排除 tmpfs 挂载点，需要启用 `linux-tmpfs` 功能
- **Linux**: 默认排除网络设备，需要启用 `linux-netdevs` 功能才能显示 CIFS/NFS
- **Windows**: 驱动器字母（C:, D: 等）作为挂载点
- **macOS**: 类似 Unix 的挂载点路径

### 2. 性能考虑
- `new_with_refreshed_list()` 会刷新所有信息，性能开销较大
- 频繁刷新时使用 `refresh_specifics()` 只更新需要的信息
- 保持 `Disks` 实例重用，避免重复创建

### 3. 常见错误
```rust
// ❌ 错误：每次循环都创建新实例
for _ in 0..10 {
    let disks = Disks::new_with_refreshed_list(); // 性能差
    // ...
}

// ✅ 正确：重用实例
let mut disks = Disks::new();
for _ in 0..10 {
    disks.refresh(false); // 只刷新，不重新扫描
    // ...
}
```

### 4. 版本兼容性
- sysinfo 0.30（当前项目使用）与 0.37（最新）API 基本兼容
- 建议升级到最新版本以获得更好的性能和功能

## 真实代码片段分析

### 现有代码（chaindash/src/collect/collector.rs）
```rust
// 第784-799行
let disks = Disks::new_with_refreshed_list();
let mut disk_used: u64 = 0;
let mut disk_total: u64 = 0;
let mut disk_available: u64 = 0;

for disk in disks.list() {
    // 只统计opt分区
    let mount_point = disk.mount_point().to_string_lossy();
    if mount_point == "/opt" {
        disk_total += disk.total_space();
        disk_available += disk.available_space();
    } else if mount_point == "/"{
        disk_total += disk.total_space();
        disk_available += disk.available_space();
    }
}
```

**【品味评分】**
🟡 凑合

**【改进建议】**
1. **消除重复条件**：两个 if 分支做同样的事情
2. **使用集合过滤**：更清晰的过滤逻辑
3. **添加错误处理**：除零保护

**【改进后的代码】**
```rust
let disks = Disks::new_with_refreshed_list();
let target_mount_points = ["/opt", "/"];

let (disk_total, disk_available): (u64, u64) = disks.list()
    .iter()
    .filter(|disk| {
        let mount_point = disk.mount_point().to_string_lossy();
        target_mount_points.contains(&mount_point.as_ref())
    })
    .fold((0, 0), |(total, available), disk| {
        (total + disk.total_space(), available + disk.available_space())
    });

let disk_used = disk_total.saturating_sub(disk_available);
let disk_usage_percent = if disk_total > 0 {
    (disk_used as f32 / disk_total as f32) * 100.0
} else {
    0.0
};
```

### 更优雅的解决方案
```rust
use sysinfo::Disks;

#[derive(Debug)]
struct DiskStats {
    mount_point: String,
    total_gb: f64,
    available_gb: f64,
    used_gb: f64,
    usage_percent: f32,
    is_removable: bool,
    filesystem: String,
}

impl DiskStats {
    fn from_disk(disk: &Disk) -> Self {
        let total = disk.total_space() as f64;
        let available = disk.available_space() as f64;
        let used = total - available;
        let usage_percent = if total > 0.0 {
            (used / total * 100.0) as f32
        } else {
            0.0
        };

        Self {
            mount_point: disk.mount_point().to_string_lossy().to_string(),
            total_gb: total / 1_000_000_000.0,
            available_gb: available / 1_000_000_000.0,
            used_gb: used / 1_000_000_000.0,
            usage_percent,
            is_removable: disk.is_removable(),
            filesystem: disk.file_system().to_string_lossy().to_string(),
        }
    }
}

fn get_disk_stats(filter_mount_points: Option<&[&str]>) -> Vec<DiskStats> {
    let disks = Disks::new_with_refreshed_list();

    disks.list().iter()
        .filter(|disk| {
            filter_mount_points.map_or(true, |points| {
                let mount_point = disk.mount_point().to_string_lossy();
                points.contains(&mount_point.as_ref())
            })
        })
        .map(DiskStats::from_disk)
        .collect()
}
```

## 引用来源

1. **官方文档**：
   - https://docs.rs/sysinfo/latest/sysinfo/ - 主文档
   - https://docs.rs/sysinfo/latest/sysinfo/struct.Disk.html - Disk 结构体
   - https://docs.rs/sysinfo/latest/sysinfo/struct.Disks.html - Disks 结构体

2. **GitHub 仓库**：
   - https://github.com/GuillaumeGomez/sysinfo - 源代码和示例
   - https://github.com/GuillaumeGomez/sysinfo/blob/master/examples/simple.rs - 示例代码

3. **项目现有代码**：
   - `/home/wangjw/repos/rust/chaindash/src/collect/collector.rs` - 第784-799行

## 总结建议

### 立即改进项
1. **消除重复代码**：合并 `/opt` 和 `/` 的相同处理逻辑
2. **使用函数式编程**：用 `filter` + `fold` 替代手动循环
3. **添加防御性编程**：除零保护和溢出保护

### 长期优化项
1. **升级 sysinfo**：从 0.30 升级到 0.37 以获得更好性能
2. **缓存 Disks 实例**：避免重复创建，提高性能
3. **添加监控告警**：磁盘空间低于阈值时告警

### Linus 的实用主义建议
"不要过度设计。你的代码已经能工作，这是最重要的。先解决实际问题，再优化代码结构。磁盘监控的核心是准确获取数据并正确显示，代码简洁性次之。"

记住：**"好代码没有特殊情况"**。你的代码中 `/opt` 和 `/` 的处理逻辑相同，应该合并。**"如果实现需要超过3层缩进，重新设计它"**。你的循环逻辑可以更扁平化。