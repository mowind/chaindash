# 磁盘监控功能设计文档

## 概述
基于现有Chaindash架构，扩展磁盘监控功能，实现详细视图、阈值告警和自动发现功能。现有代码已支持多挂载点监控，本设计在此基础上增加UI交互和告警功能。

## 架构

### 现有架构分析
```
App (main.rs)
├── SharedData (Arc<Mutex<Data>>)
├── Collector (异步数据收集)
│   ├── 区块链数据
│   ├── 节点状态
│   ├── 系统统计 (包含磁盘)
│   └── 磁盘详情 (DiskDetail[])
└── Widgets (UI组件)
    ├── SystemWidget (系统监控)
    ├── NodeWidget
    ├── TxsWidget
    └── 其他组件
```

### 扩展设计
```
现有架构 + 磁盘监控扩展
├── DiskWidget (新组件)
│   ├── 详细视图 (Tab切换)
│   ├── 告警显示 (高亮)
│   └── 自动发现状态
├── 增强的Collector
│   ├── 自动挂载点发现
│   └── 告警阈值检查
└── 增强的SystemStats
    ├── 告警状态字段
    └── 当前选中挂载点
```

## 组件和接口

### 1. 增强的DiskDetail结构体
```rust
// src/collect/collector.rs
#[derive(Debug, Clone)]
pub struct DiskDetail {
    pub mount_point: String,      // 挂载点路径
    pub filesystem: String,       // 文件系统类型
    pub total: u64,               // 总容量(bytes)
    pub used: u64,                // 已使用(bytes)
    pub available: u64,           // 可用空间(bytes)
    pub usage_percent: f32,       // 使用率百分比
    pub device: String,           // 设备名称
    pub is_alert: bool,           // 告警状态(使用率>90%)
    pub is_network: bool,         // 是否为网络挂载点
    pub last_updated: Instant,    // 最后更新时间
}
```

### 2. 增强的SystemStats结构体
```rust
// src/collect/collector.rs
#[derive(Debug, Clone)]
pub struct SystemStats {
    // 现有字段...
    pub cpu_usage: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub memory_usage_percent: f32,
    pub network_rx: u64,
    pub network_tx: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub disk_usage_percent: f32,

    // 新增字段
    pub disk_details: Vec<DiskDetail>,    // 磁盘详情列表
    pub current_disk_index: usize,        // 当前选中的磁盘索引
    pub alert_threshold: f32,             // 告警阈值(默认90%)
    pub has_disk_alerts: bool,            // 是否有磁盘告警
}
```

### 3. 新的DiskWidget组件
```rust
// src/widgets/disk.rs
pub struct DiskWidget {
    title: String,
    selected_index: usize,
}

impl UpdatableWidget for DiskWidget {
    fn update(&mut self, data: &Data) {
        // 更新磁盘数据显示
    }

    fn get_update_interval(&self) -> Duration {
        Duration::from_secs(1)
    }
}

impl Widget for DiskWidget {
    fn render(&mut self, area: Rect, buf: &mut Buffer, app: &App) {
        // 渲染磁盘详细视图
        // 包括：Tab切换指示器、当前磁盘详情、告警状态
    }
}
```

### 4. 增强的Collector
```rust
// src/collect/collector.rs
impl Collector {
    // 现有方法...

    // 新增：自动发现挂载点
    fn discover_mount_points(&self) -> Vec<String> {
        // 读取/proc/mounts或使用sysinfo crate
        // 排除特殊文件系统(proc, sysfs, tmpfs等)
    }

    // 新增：检查磁盘告警
    fn check_disk_alerts(&self, disk_details: &mut Vec<DiskDetail>, threshold: f32) -> bool {
        let mut has_alert = false;
        for disk in disk_details.iter_mut() {
            disk.is_alert = disk.usage_percent >= threshold;
            if disk.is_alert {
                has_alert = true;
            }
        }
        has_alert
    }
}
```

## 数据模型

### 数据流
```
1. Collector定时任务 (每2秒)
   ├── 读取/proc/mounts → 发现新挂载点
   ├── 调用statvfs获取磁盘信息
   ├── 计算使用率百分比
   └── 检查告警阈值 → 设置is_alert标志

2. DiskWidget渲染 (每1秒)
   ├── 从SharedData读取disk_details
   ├── 根据current_disk_index显示对应磁盘
   ├── 高亮显示告警状态
   └── 显示Tab切换指示器

3. 用户交互
   ├── Tab键 → current_disk_index++
   ├── Shift+Tab → current_disk_index--
   └── 循环切换所有磁盘
```

### 状态管理
```rust
// App状态中的磁盘相关状态
struct App {
    // 现有字段...
    shared_data: Arc<Mutex<Data>>,
    widgets: Vec<Box<dyn UpdatableWidget>>,

    // 新增：磁盘UI状态
    disk_widget_visible: bool,      // 是否显示DiskWidget
    disk_auto_discovery: bool,      // 是否启用自动发现
}
```

## UI设计

### 布局方案
```
方案A：集成到SystemWidget中
┌─────────────────────────────────────┐
│ System Monitor                      │
├─────────────────────────────────────┤
│ CPU: 12%  Mem: 4.2G/16G (26%)      │
│ Disk: 45G/100G (45%) [2 alerts]    │
│ Network: ↓1.2M/s ↑0.8M/s           │
├─────────────────────────────────────┤
│ [Tab: / (45%)] [ /home (92%!) ]    │
│ Mount: /dev/sda1 on /              │
│ FS: ext4  Total: 100G  Used: 45G   │
│ Available: 55G  Usage: 45%         │
└─────────────────────────────────────┘

方案B：独立的DiskWidget
┌─────────────────────────────────────┐
│ Disk Monitor (Tab to switch)        │
├─────────────────────────────────────┤
│ [1/3] /dev/sda1 on / (45%)         │
│ [2/3] /dev/sdb1 on /home (92%!)    │
│ [3/3] nfs-server:/data (65%)       │
├─────────────────────────────────────┤
│ Mount Point: /                      │
│ Filesystem: ext4                    │
│ Device: /dev/sda1                   │
│ Total: 100.0 GB                     │
│ Used: 45.2 GB (45%)                 │
│ Available: 54.8 GB                  │
└─────────────────────────────────────┘
```

**推荐方案A**：保持界面简洁，集成到现有SystemWidget中。

### 颜色方案
- 正常状态：白色/默认颜色
- 告警状态(>90%)：红色背景或红色文字
- 网络挂载点：黄色文字
- 当前选中：高亮/反色显示

## 错误处理

### 可能出现的错误
1. **挂载点不可访问**：网络挂载点断开、权限不足
2. **statvfs调用失败**：文件系统错误、设备移除
3. **/proc/mounts读取失败**：权限问题

### 处理策略
```rust
enum DiskError {
    AccessDenied(String),      // 权限不足
    MountPointGone(String),    // 挂载点消失
    NetworkTimeout(String),    // 网络挂载点超时
    FilesystemError(String),   // 文件系统错误
}

impl DiskDetail {
    fn from_mount_point(mount_point: &str) -> Result<Self, DiskError> {
        // 尝试获取磁盘信息
        // 失败时返回具体错误类型
    }
}
```

### UI错误显示
- 不可访问的挂载点显示为"Unavailable"
- 网络超时显示为"Network Timeout"
- 错误信息在详细视图中显示

## 测试策略

### 单元测试
1. **DiskDetail结构测试**
   - 测试使用率计算是否正确
   - 测试告警阈值检查
   - 测试数据格式转换(字节→人类可读)

2. **Collector磁盘功能测试**
   - 测试自动发现逻辑
   - 测试错误处理
   - 测试网络挂载点处理

3. **DiskWidget渲染测试**
   - 测试Tab切换逻辑
   - 测试告警显示
   - 测试布局计算

### 集成测试
1. **端到端数据流测试**
   - Collector → SharedData → DiskWidget
   - 验证数据一致性和实时性

2. **用户交互测试**
   - Tab键切换功能
   - 告警状态更新
   - 自动发现功能

### 测试工具
- 使用mock文件系统进行测试
- 模拟/proc/mounts内容
- 模拟statvfs返回值

## 性能考虑

### 资源占用目标
- 内存增加：< 1MB
- CPU增加：< 0.5%
- 更新频率：2秒/次（收集），1秒/次（UI）

### 优化措施
1. **懒加载**：只在需要时读取/proc/mounts
2. **增量更新**：只更新变化的磁盘信息
3. **缓存**：缓存文件系统类型和设备名称
4. **批处理**：批量读取多个挂载点信息

## 兼容性

### 支持的系统
- Linux (主要目标)
- 其他Unix-like系统 (BSD, macOS)

### 文件系统支持
- ext2/3/4
- XFS
- Btrfs
- NTFS (通过fuse)
- NFS/SMB (网络文件系统)

### 排除的特殊文件系统
- proc
- sysfs
- tmpfs
- devtmpfs
- cgroup
- overlay

## 配置选项

### 命令行参数扩展
```rust
// 现有参数
--disk-mount-points: 指定监控的挂载点

// 新增参数
--disk-alert-threshold: 告警阈值(默认90)
--disk-auto-discovery: 启用自动发现(默认true)
--disk-refresh-interval: 刷新间隔(默认2秒)
```

### 运行时配置
- 告警阈值可通过UI临时调整
- 可临时禁用自动发现
- 可手动添加/移除监控的挂载点

## 实施优先级

### Phase 1: 核心功能
1. 增强DiskDetail和SystemStats结构
2. 实现Tab切换显示
3. 实现90%阈值告警

### Phase 2: 自动发现
1. 实现自动挂载点发现
2. 排除特殊文件系统
3. 网络挂载点标记

### Phase 3: 用户体验
1. 优化UI布局和颜色
2. 添加键盘快捷键帮助
3. 错误状态友好显示

### Phase 4: 高级功能
1. 可配置告警阈值
2. 磁盘IO监控
3. 历史趋势图表