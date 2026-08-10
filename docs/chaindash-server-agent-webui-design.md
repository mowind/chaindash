# ChainDash Server / Agent / WebUI 设计方案

## 1. 文档状态

- 状态：设计方案，尚未开始 Server / Agent / WebUI 实现
- 修订日期：2026-08-10
- 代码基线：`e7eba3a3f3458fa23c3422061be87382baed40d5`
- 当前版本：`chaindash 0.2.0`
- 适用范围：未来的 Core、Agent、Server、WebUI，以及现有 TUI 的迁移兼容

本次修订以当前代码为准，重点修正以下旧假设：

1. 当前 `Data` 不是可直接上报的统一节点快照。
2. 一个 Agent 必须支持多个 Monitored Node，不能把该问题继续留作开放项。
3. 区块流、共识状态、Peer、Explorer 详情和系统指标具有不同的数据归属与刷新周期。
4. 协议必须显式表达组件新鲜度、失败、权威空值和最后成功值，不能只依赖顶层时间戳和 `Option<T>`。
5. Geo、Telegram、TUI redraw 和 Docker 构建已有必须保留或明确替换的行为约束。

---

## 2. 摘要

ChainDash 目前是一个单进程 Rust TUI：

```text
CLI 配置
   │
   ▼
Collector + SharedData + PeerGeoStore
   │
   ▼
Ratatui / Crossterm TUI
```

目标形态为：

```text
┌──────────────────────────────┐
│ Monitored Host / Node Host   │
│                              │
│  chaindash-agent             │
│  - RPC / Explorer 采集       │
│  - 主机系统指标              │
│  - Peer Snapshot             │
│  - 本地有界上报队列          │
└──────────────┬───────────────┘
               │ outbound HTTPS
               │ authenticated AgentReport
               ▼
┌──────────────────────────────┐
│ chaindash-server             │
│                              │
│  - Agent 接入和租约          │
│  - 当前状态和历史            │
│  - Geo 聚合和 Location Cache │
│  - 告警状态机和通知 Outbox   │
│  - REST + SSE                │
│  - WebUI 静态资源            │
└──────────────┬───────────────┘
               │ same-origin HTTP / SSE
               ▼
┌──────────────────────────────┐
│ Browser WebUI                │
│  - 总览 / 节点 / Agent       │
│  - Peer Country Distribution│
│  - 告警 / 历史趋势           │
└──────────────────────────────┘
```

核心决策：

- Agent 负责采集和上报，不对外开放 RPC 或管理端口。
- Server 负责持久化、聚合、Geo enrichment、告警、通知和 Web 查询。
- 一个 Agent 支持一个或多个 Monitored Node。
- 系统指标归属于 Agent 主机，不归属于单个 Monitored Node。
- 共识和 Peer 归属于 Monitored Node。
- 区块与交易指标归属于一个 Agent 的 Chain Observation，并记录实际来源端点。
- Explorer Validator Detail 使用稳定的 `validator_node_id`，可选关联到 Monitored Node，不再依赖列表位置隐式配对。
- Agent 发送完整报告，但每个组件具有独立的状态 revision、成功值 revision 和采集时间；Server 按组件合并。
- 采集失败不会用空数据覆盖最后成功值；成功采集到空集合则是权威空值。
- 现有 TUI 在迁移期间继续工作，并保留当前 tmux、状态栏和重绘约束。

---

## 3. 为什么需要 Agent

仅有中心化 Server 无法可靠覆盖以下部署：

- PlatON RPC 仅监听 `127.0.0.1`。
- RPC 位于私网、容器网络或防火墙后。
- 节点主机不允许开放额外入站端口。
- CPU、内存、磁盘、网络和挂载点只能在目标主机本地采集。
- 多个机房只能主动访问中心 Server，Server 无法反向连接。

因此采用反向上报：

```text
Agent --主动出站--> Server
```

而不是：

```text
Server --主动入站--> 每台节点主机
```

Agent 应保持较薄，但“薄”不表示无状态。为了应对断网、Server 重启和重试，Agent 至少需要：

- 稳定的 `agent_id`；
- 持久化、单调递增的 `boot_generation` 和每次启动唯一的 `boot_id`；
- 组件 `state_revision` / `value_revision`；
- 有界、可恢复的上报队列；
- Collector 运行状态；
- 最后成功采集值。

---

## 4. 目标和非目标

### 4.1 目标

1. 保留现有 TUI 能力和多端点监控能力。
2. 支持一个 Agent 采集多个 Monitored Node。
3. 支持多个 Agent 汇聚到一个 Server。
4. WebUI 展示当前状态、历史趋势、Peer 国家分布和告警。
5. Agent 只需要访问本机或私网 RPC，并向 Server 主动出站。
6. Server 重启后保留 Agent、节点、Geo、告警、通知和日报状态。
7. 采集、上报、存储、Geo、告警和展示具有清晰的数据归属。
8. Agent 重试不重复创建历史样本或通知 outbox 事件；外部通知通道按 at-least-once 语义处理。
9. 单个 Collector 失败不停止其他健康 Collector。
10. 单个 Monitored Node 的 Peer 采集失败不冻结其他节点的 Peer 状态。
11. Server 可在单机 SQLite 上运行，并保留未来迁移 PostgreSQL 的路径。
12. 现有 `PeerGeoStore` 的事务替换、失败保留和无 SQL 绘制语义得到保留。

### 4.2 非目标

第一阶段不实现：

- 远程执行任意命令；
- Server 直接管理 PlatON 进程；
- Server 向 Agent 下发任意 RPC URL 或 Shell 配置；
- 区块浏览器全量索引；
- 替代 Prometheus、Grafana 或通用日志平台；
- 多租户计费；
- Kubernetes Operator；
- 插件市场；
- WebUI 暴露原始 Peer IP；
- 将 Crossterm / Ratatui 的终端细节放入 Core 或网络协议。

---

## 5. 当前实现盘点

### 5.1 当前入口和运行模型

当前程序只有一个二进制入口：

```text
src/main.rs
```

启动顺序为：

1. 解析 `Opts`。
2. `setup_app` 创建 `SharedData`、Geo Store 和 Widgets。
3. Geo SQLite 打开并迁移；失败时退化为 `NullPeerGeoStore`，TUI 继续运行。
4. 初始化终端、日志、事件线程和 UI wake channel。
5. 先读取一次 `GeoViewSnapshot`。
6. 启动 `Collector` Tokio task。
7. 进入基于 Crossbeam channel 的事件驱动绘制循环。
8. 退出时停止 Collector、恢复终端、等待后台任务并关闭 Geo Store。

当前关键依赖：

- Tokio 1
- Alloy WebSocket Provider
- Ratatui 0.30
- Crossterm 0.29
- Reqwest 0.12
- Rusqlite 0.40（bundled SQLite）
- Sysinfo 0.30

当前 `SharedData` 为：

```text
Arc<Mutex<Data>>
```

它同时包含采集数据和 TUI 专属状态：

- Chain samples；
- Consensus states；
- Validator node details；
- Unix system stats；
- 状态栏 TTL；
- UI dirty bits；
- UI wake sender；
- 图表使用的 destructive `take/clear` 缓冲。

因此未来不能直接给 `Data` 增加 `Serialize` 并作为协议 DTO。

### 5.2 当前数据归属和 Collector 行为

| 数据 | 当前采集方式 | 当前归属 | 当前周期 / 触发 | 失败语义 |
| --- | --- | --- | --- | --- |
| Block / Tx | 从 `--url` 按顺序选择第一个可订阅端点 | 全局单一 Chain state | 区块事件驱动，1 秒重试 | 切换下一个端点，保留旧值 |
| Consensus | 每个 `name@url` 独立任务 | 每个 URL/name | 1 秒 | 断线重连，但旧 state 没有显式 stale 标记 |
| Peer IP | 每分钟依次调用所有 URL 的 `admin_peers` | 所有 URL 合并后的全局集合 | 60 秒；单 RPC 10 秒超时 | 任一 URL 失败则整轮不替换 |
| Explorer Detail | 对每个独立 `--node-id` 请求 Explorer | Validator node ID | 10 秒；单请求 30 秒超时 | 失败节点详情从当前内存集合移除 |
| Ranking | 请求 Explorer 列表再匹配 `--node-id` | Validator node ID | 10 秒；30 秒超时 | 保留已存在 ranking，记录状态消息 |
| System | 一个 Unix host collector | Agent 所在主机 | 默认 2 秒，可配置 | 任务错误会记录状态；非 Unix 不支持 |
| Telegram | Collector 内直接调用 | 当前进程 | 状态变化 / 本地午夜 | 状态只在内存，发送失败只记录日志 |
| Daily Snapshot | 本地 JSON 文件 | 当前进程 / 本机时区 | 本地时间 00:00 | 文件损坏时退化为空历史 |

特别需要注意：

- 多个 `--url` 在当前代码里同时被当作“多个共识监控对象”和“一个区块流的故障转移来源”。
- `--node-id` 与 `--url` 没有显式一一映射。
- Peer Snapshot 是跨全部 URL 的 Agent 级 union，而不是每个节点独立快照。
- `SystemStats.network_rx` / `network_tx` 是 bytes/s 速率，不是累计字节数。
- `NodeDetail.last_updated_at`、`DiskDetail.last_updated` 和状态 TTL 使用进程内 `Instant`，不能跨进程传输。
- System collector 采集的是整台主机，不是 PlatON 进程资源。

### 5.3 当前 Geo 语义

`PeerGeoStore` 是当前最成熟的可复用 seam：

```rust
trait PeerGeoStore {
    fn replace_peer_snapshot(&self, ips: Vec<String>) -> Result<Vec<String>>;
    fn update_location_cache(&self, entries: Vec<LocationEntry>) -> Result<()>;
    fn geo_view_snapshot(&self) -> Result<GeoViewSnapshot>;
    fn updates(&self) -> Receiver<()>;
    fn shutdown(&self);
}
```

当前必须保留的行为：

1. SQLite connection 由专用 worker thread 独占。
2. `current_peers` 使用事务执行完整替换。
3. IP 在快照内排序并去重。
4. 成功 Location Cache 的 TTL 为 24 小时。
5. enrichment 失败不覆盖旧的 country / loc / successful refresh time。
6. 每个 IP 在 `GeoViewSnapshot` 中只计数一次。
7. 非法或缺失国家码计入 Unknown Country。
8. Country Count 按 peer 数降序，再按国家码升序。
9. Widget 绘制时不执行 SQL 或外部 HTTP。
10. Geo read 失败时保留最后成功的 Widget snapshot，并每秒重试。
11. 成功写入只发送容量为 1 的 coalesced wake signal；消费者重新读取完整 snapshot。
12. 关闭时等待正在进行的 enrichment 完成。

当前实现的限制：

- `current_peers(ip PRIMARY KEY)` 没有 Agent / Monitored Node 归属。
- 任一 URL 失败会冻结所有 URL 的合并 Peer Snapshot。
- 首次 enrichment 失败的 IP 会在每分钟轮询中重复重试。
- enrichment task 没有显式并发上限。
- 当前 SQLite 没有启用 WAL；WAL 是未来 Server 的新要求，不是当前事实。

### 5.4 当前 Telegram 和日报语义

当前实现支持：

- 连接失败；
- 连接恢复；
- 排名变化；
- 每日节点快照；
- 多个 Chat ID；
- 事件过滤；
- 静默时间；
- 静默期摘要；
- 每事件 key 限流；
- 模板替换；
- 每月 1 日附加上月累计差值。

当前状态均在内存：

- 连接健康状态；
- 上次 ranking；
- 上次发送时间；
- 静默期摘要 bucket。

当前发送语义存在以下可靠性缺口：

- 状态在 HTTP 发送前已经推进；
- 发送失败只记录日志，没有 durable retry；
- quiet summary 在发送前从内存取出，发送失败可能丢失；
- 多 Chat ID 的部分成功没有独立 delivery 状态；
- 重启会丢失去重、限流和 quiet state。

这些行为不能直接搬入 Server，必须改为持久化 Alert State + Notification Outbox。

### 5.5 当前 TUI 约束

当前 TUI 已从固定 tick 全量更新演进为 dirty-state + wake 模式：

```text
Collector mutation
  -> Data dirty bit
  -> bounded(1) UI wake
  -> Widget update
  -> Ratatui draw
```

Geo 使用独立 wake channel，读取失败后每秒重试。

当前布局：

- 可选状态栏占 3 行；
- dashboard auxiliary row 固定 6 行；
- Unix auxiliary row：System Stats 50% / Disk Details 25% / Peer Countries 25%；
- Block Time 与 Block Transactions 并排；
- Node 与 Node Details 并排；
- 小于 3 行的面板不绘制。

状态栏：

- Info：5 秒过期；
- Warn：15 秒过期；
- Error：不自动过期。

状态栏出现或消失会让 dashboard 整体上下移动。当前必须使用：

```text
BeginSynchronizedUpdate
Terminal::resize(current fullscreen area)
full redraw
EndSynchronizedUpdate
```

禁止在该路径调用 `Terminal::clear()`，因为 Ratatui 0.30 会查询光标位置，在 tmux / SSH / Docker 链路可能超时。

Peer Countries 在 tmux / screen 或兼容模式下必须使用固定宽度国家码：

- 检测 `TMUX`；
- 检测 `TERM=tmux-*` / `TERM=screen-*`；
- 支持 `CHAINDASH_ASCII_COUNTRIES`；
- Docker 构建启用 `ascii-countries` Cargo feature；
- `run.sh` 显式传入兼容变量。

这些是 TUI adapter 约束，不属于 Core 或 Agent 协议。

---

## 6. 目标领域模型和数据归属

### 6.1 Agent

Agent 是部署在某台主机上的采集进程。

Agent 拥有：

- 稳定 `agent_id`；
- `display_name`；
- 版本；
- 主机系统指标；
- 一个或多个 Monitored Node 配置；
- 一个带稳定 `chain_id` 的 Chain Observation 配置；
- 零个或多个 Validator Observation 配置；
- 上报队列和 Collector 状态。

一个 Agent 对应一台采集主机。若同一主机运行多个 Agent，它们必须使用不同 `agent_id`，Server 应提示潜在重复采集。

### 6.2 Monitored Node

Monitored Node 是 Agent 直接访问的 PlatON RPC 目标。

每个 Monitored Node 必须有：

- 稳定 `monitored_node_id`；
- 显示名称；
- RPC WebSocket URL；
- 共识采集状态；
- 独立 Peer Snapshot 状态。

`display_name` 不能作为数据库主键。当前代码使用名称作为 `HashMap` key，未来必须避免重复名称覆盖。

### 6.3 Chain Observation

当前 Block / Tx 不是每个 Monitored Node 各自维护，而是从有序端点列表中选择一个可用来源。

目标模型保留该语义，但显式建模：

- 每个 Agent v1 配置一个稳定 `chain_id`，例如 `platon-mainnet`；
- 同一 Agent 的 Monitored Node 必须属于该 Chain Observation；
- Agent 配置一个或多个 `chain_source_node_ids`；
- Agent 按顺序故障转移；
- Chain Snapshot 必须记录 `chain_id` 和 `source_monitored_node_id`；
- Source 切换产生 Collector status / event；
- Chain freshness 使用接收区块的时间和本地 observed time，而不是仅使用区块 timestamp。

`chain_id` 不能只依赖自由文本显示名称。实现时应同时采集 RPC 可提供的 numeric chain/network identity，并在 Server 验证同一 `chain_id` 的来源一致性。v1 每个 Agent 只有一个 Chain Observation；多链 Agent 需要未来协议版本扩展。

### 6.4 Validator 与 Validator Observation

Explorer 的 `--node-id` 与 RPC URL 当前没有映射。目标模型区分：

- **Validator**：Server 全局实体，身份为 `(chain_id, validator_node_id)`；
- **Validator Observation**：某个 Agent 对该 Validator 的 Explorer 采集结果；
- **Monitored Node Link**：该 Agent 内可选的 `monitored_node_id` 关联。

Agent 上报的 Validator Observation 包含：

- `chain_id`；
- 稳定 `validator_node_id`，通常就是 Explorer node ID；
- 可选 `monitored_node_id`；
- Explorer detail；
- ranking；
- 最后成功值和采集错误。

Server 保存每个 Agent 的 source observation，再为全局 Validator 选择最新成功 observation 作为聚合 current view；时间相同则使用稳定 `agent_id` 排序。不同 Agent 对同一 Validator 报告冲突值时保留各 source，并显示 data-conflict 状态，不能静默覆盖。

排名告警和日报针对全局 `(chain_id, validator_node_id)` 计算，避免两个 Agent 观察同一 Validator 时重复通知。不得按数组位置将 `validator_node_id` 与 Monitored Node 配对。

### 6.5 Component Observation

每类独立采集结果使用统一 envelope：

```rust
struct ComponentObservation<T> {
    state_revision: u64,
    status: ObservationStatus,
    attempted_at: DateTime<Utc>,
    latest: Option<VersionedValue<T>>,
    error: Option<CollectorError>,
}

struct VersionedValue<T> {
    origin_boot_generation: u64,
    value_revision: u64,
    observed_at: DateTime<Utc>,
    value: T,
}

enum ObservationStatus {
    Starting,
    Ok,
    Error,
    Disabled,
    Unsupported,
}
```

`latest` 表示 Agent 当前保留的最后成功值，而不是“本次 attempt 的返回值”。Agent 将该 envelope 持久化到 state dir，因此重启后仍能携带它。`origin_boot_generation` 标记该值实际产生于哪个 boot，避免新 boot 重发 last-good value 时被误记成新样本。

不变量：

- `Ok`：`latest` 必须存在，`error` 必须为空。
- `Error`：`error` 必须存在；`latest` 可以存在并继续表示较早的最后成功值，但其 `value_revision` 不变。
- `Starting`：尚无成功或失败结论；`latest` 通常为空。
- `Disabled`：配置明确关闭，不产生故障告警。
- `Unsupported`：当前平台不支持，例如非 Unix 系统指标，不产生故障告警。
- `Ok + latest.value = PeerSnapshot { ips: [] }` 表示权威空集合，必须清空该节点旧 Peer Snapshot。
- 缺少整个组件不是错误，Server 按协议版本和配置判断；v1 Agent 应发送所有已知组件状态。

Collector 每完成一次采集 attempt 就递增当前 boot 内的 `state_revision`；每次成功产生新的权威值时递增当前 boot 内的 `value_revision`，并把当前 `boot_generation` 写入 `origin_boot_generation`。重复报告可以携带相同 revision。Server 分别合并 collector state 与 last-good value，不能因一次失败或一次 Agent 重启把旧成功值伪装成新样本。

---

## 7. 目标模块

### 7.1 Core

Core 是共享的深模块，隐藏采集状态组合、时间语义和快照不变量。

建议初始结构：

```text
crates/
├── chaindash-core/
│   ├── src/model/
│   ├── src/observation/
│   ├── src/collector/
│   ├── src/protocol/
│   └── src/time/
├── chaindash-agent/
├── chaindash-server/
└── chaindash-tui/
web/
```

Core 负责：

- Agent / Monitored Node / Chain / Validator / Validator Observation 类型；
- Component Observation 状态机和不变量；
- Chain、Consensus、System、Disk、Validator、Peer DTO；
- Peer IP 标准化、去重和可 enrichment 判断；
- UTC 时间模型；
- Collector Engine 的只读当前视图；
- 协议版本和序列化测试 fixture。

Core 不负责：

- SQLite；
- Reqwest Server transport；
- Axum handler；
- Telegram HTTP；
- Ratatui Widget；
- Crossterm terminal；
- 浏览器状态。

不要给每个小函数创建 trait。只在真实可替换 seam 上定义 interface：

- Agent transport：HTTPS adapter + in-memory test adapter；
- Server repository：SQLite adapter + in-memory integration adapter；
- Notification channel：Telegram adapter + mock adapter；
- Clock：system clock + deterministic test clock。

### 7.2 Agent

Agent 内部结构：

```text
Config Loader
     │
     ▼
Collector Supervisor
     │
     ├── Chain Collector
     ├── Consensus Collectors (per node)
     ├── Peer Collectors (per node)
     ├── Validator Explorer Collector
     └── System Collector (Unix)
     │
     ▼
Observation Store
     │
     ▼
Report Assembler
     │
     ▼
Durable Bounded Spool
     │
     ▼
HTTPS Transport
```

Agent 责任：

- 读取本地配置；
- 并行采集；
- 将 blocking system work 放入 `spawn_blocking`；
- 为每个 Collector 维护 `state_revision`、`value_revision` 和最后成功值；
- 组装不可变 AgentReport；
- 先持久化、后发送；
- 串行按顺序发送 backlog；
- 指数退避并加入 jitter；
- 对永久错误降频；
- 优雅停止并在截止时间内 flush；
- 暴露本地日志和可选 stdout health summary。

Collector Supervisor 必须改进当前行为：

- 捕获 task 返回错误和 panic；
- 将状态发布为 `Error`；
- 对可重试错误重启 task；
- 使用有上限的指数退避；
- 单个 task 永久退出时其他 task 继续运行；
- shutdown 使用统一 cancellation，而不是仅依赖各循环不同频率轮询 `AtomicBool`。

Agent 不负责：

- WebUI；
- 全局历史；
- IPinfo enrichment；
- Telegram；
- 跨 Agent 聚合；
- 告警去重和通知可靠性。

### 7.3 Server

Server 内部建议模块：

```text
HTTP Router
├── Agent Ingest
├── Web Query
├── SSE Invalidation
└── Health / Readiness

Domain Modules
├── Agent Registry
├── Report Ingestor
├── Current State Projector
├── History Writer
├── Geo Enricher
├── Alert Engine
├── Notification Outbox
└── Daily Summary Scheduler

Adapters
├── SQLite Repository
├── IPinfo Client
├── Telegram Channel
└── Static Web Assets
```

Server 责任：

- Agent token 验证和身份绑定；
- 报告幂等接收；
- 检测重复 Agent 实例；
- 分别按组件 `state_revision` 和 `value_revision` 合并 Collector 状态与最后成功值；
- 记录 collector error 和最后成功值；
- 追加历史和下采样；
- Peer Snapshot per-node 替换；
- Location Cache；
- Agent offline 判断；
- 告警状态机；
- durable notification outbox；
- Daily / Monthly summary；
- REST / SSE；
- WebUI 静态文件。

Server 的数据库迁移失败应使 readiness 失败并拒绝 ingest。不要复制当前 TUI 的 `NullPeerGeoStore` 宽松策略到 Server 主存储。

### 7.4 WebUI

第一阶段页面：

1. Overview
   - Agent 在线数；
   - Monitored Node 健康数；
   - 最新区块和 source；
   - 打开告警数；
   - Peer 国家摘要。
2. Agents
   - 在线 / 离线；
   - 版本；
   - last seen；
   - clock skew；
   - system summary；
   - collector statuses。
3. Monitored Nodes
   - RPC / consensus；
   - current block；
   - Peer country distribution；
   - last success / last error。
4. Validators
   - ranking；
   - block quantity / rate；
   - reward；
   - freshness；
   - 可选关联节点。
5. Alerts
   - open / recovered；
   - first seen / last seen；
   - delivery status。
6. History
   - Block interval；
   - transactions；
   - CPU / memory / disk / network rate；
   - rank；
   - peer country trends。

WebUI 不接触数据库，只调用 REST 并监听 SSE。

浏览器可以显示国旗 emoji，但必须同时显示国家码并提供文本 fallback；不能把“emoji 一定为两列”的 TUI 假设搬到 WebUI。

---

## 8. 进程和打包形态

### 8.1 推荐二进制

```text
chaindash-tui
chaindash-agent
chaindash-server
```

也可以在过渡期保留：

```text
chaindash tui
chaindash agent
chaindash server
```

长期推荐独立 crate / binary，原因是：

- Server 不应链接 Ratatui / Crossterm；
- Agent 不需要 Web 静态资源；
- TUI 不需要 Axum、Server repository 和 Web auth；
- 可独立发布、裁剪权限和构建镜像。

### 8.2 当前 Docker 限制

当前 Dockerfile：

- 只复制根 `Cargo.toml`、`Cargo.lock` 和 `src/`；
- 使用 Ubuntu 24.04 builder；
- stable Rust 编译；
- `scratch` artifact stage 只导出 `/chaindash`；
- 使用 `ascii-countries` feature。

它不能直接构建未来 workspace、migrations 或 `web/dist`。

目标打包需要：

- Agent image：仅 Agent binary、CA certificates、config / state volume；
- Server image：Server binary、CA certificates、Web assets、migrations、SQLite data volume；
- TUI artifact：继续支持 `ascii-countries` feature；
- WebUI：独立 Node build stage，再复制静态产物到 Server image；
- Server SQLite 默认挂载 `/var/lib/chaindash`；
- 配置和 token 使用只读 secret mount。

主机指标以原生 systemd Agent 最准确。容器 Agent 如需主机指标，必须明确 `/proc`、mount namespace、network namespace 和磁盘挂载可见性；不能默认把容器自身指标称为主机指标。

编译继续使用 stable Rust。仓库格式化使用：

```bash
cargo +nightly fmt -- --check
```

不要仅为 rustfmt 将生产 Docker toolchain 改为 nightly。

---

## 9. Agent 配置

不再继续扩展单个长 CLI。Agent 使用 YAML，CLI 只提供配置路径和一次性验证命令。

```yaml
agent:
  id: "node-host-a"
  display_name: "Singapore Validator Host"
  state_dir: "/var/lib/chaindash-agent"

server:
  base_url: "https://dashboard.example.com"
  token_file: "/etc/chaindash-agent/token"
  connect_timeout_seconds: 10
  request_timeout_seconds: 30

report:
  interval_seconds: 5
  spool_max_bytes: 67108864
  spool_max_age_hours: 72

chain:
  id: "platon-mainnet"
  source_node_ids:
    - "rpc-main"
    - "rpc-backup"

monitored_nodes:
  - id: "rpc-main"
    display_name: "Main Node"
    rpc_url: "ws://127.0.0.1:6789"
    collect_consensus: true
    collect_peers: true

  - id: "rpc-backup"
    display_name: "Backup Node"
    rpc_url: "ws://10.0.0.12:6789"
    collect_consensus: true
    collect_peers: true

validators:
  - node_id: "0xvalidator-node-id"
    monitored_node_id: "rpc-main"

explorer:
  base_url: "https://scan.platon.network/browser-server"

system:
  enabled: true
  disk_mount_points:
    - "/"
    - "/opt"
  disk_auto_discovery: true
  disk_alert_threshold: 90
  refresh_interval_seconds: 2
```

配置规则：

- `agent.id` 必须稳定、唯一、符合长度和字符限制。
- 可提供 `chaindash-agent init` 生成随机 ID；一旦注册不自动变化。
- `state_dir` 保存单实例锁、`boot_generation`、spool、每个组件的 last-good `VersionedValue` 和本地运行元数据，必须仅 Agent 用户可读写。
- state dir 丢失或从旧备份恢复时，Agent 必须进入 registration conflict，而不是自行重置 Server generation。
- `monitored_nodes[].id` 在 Agent 内唯一，并在 Server 上与 `agent_id` 组成唯一键。
- `display_name` 可重复，不参与身份判断。
- `chain.id` 必须稳定；同一 ID 的 Agent 应观察到一致的 RPC chain/network identity。
- `chain.source_node_ids` 必须引用已存在节点。
- `validators[].monitored_node_id` 可选，但引用时必须有效。
- URL 中如包含凭据，不在日志、状态或 WebUI 返回。
- token 文件建议权限 `0600`。
- 配置重载第一阶段不要求；使用 restart 应用修改。

现有 CLI 迁移规则：

- 每个 `name@url` 转换为一个 Monitored Node。
- 原列表顺序转换为 `chain.source_node_ids` 顺序。
- 每个 `--node-id` 转换为该 `chain.id` 下的 Validator Observation 配置。
- 因当前没有 URL 与 node ID 的映射，迁移工具不得按位置自动关联；需要用户显式确认。

---

## 10. Agent 与 Server 协议

### 10.1 传输

第一阶段使用 HTTPS JSON：

```text
POST /api/v1/agent-reports
Authorization: Bearer <agent-token>
Content-Type: application/json
Content-Encoding: gzip        # 可选
Idempotency-Key: <report_id>
```

选择理由：

- 易调试；
- 易测试；
- 反向代理兼容；
- Agent 只主动出站；
- 后续可保持领域模型不变，增加 Protobuf / gRPC adapter。

Server 必须限制：

- header 大小；
- 解压前后 body 大小；
- Monitored Node 数；
- Validator 数；
- 每节点 Peer 数；
- 字符串长度；
- 时间戳范围；
- 单 Agent 请求频率。

### 10.2 协议版本

```rust
const PROTOCOL_VERSION: u16 = 1;
```

每个请求包含：

```json
{
  "protocol_version": 1,
  "agent_version": "0.3.0"
}
```

兼容规则：

- Server 支持当前协议和至少一个可迁移的旧协议。
- 未知新增 JSON 字段默认忽略。
- 删除或改变字段语义必须提升协议版本。
- Server 返回 `426 Upgrade Required` 时包含最小和最大支持版本。
- 协议 DTO 使用 golden JSON fixture 和 round-trip test 固定行为。

### 10.3 AgentReport

概念结构：

```rust
struct AgentReport {
    protocol_version: u16,
    report_id: String,
    agent_id: String,
    agent_version: String,
    chain_id: String,
    boot_generation: u64,
    boot_id: String,
    report_sequence: u64,
    generated_at: DateTime<Utc>,

    system: ComponentObservation<SystemSnapshot>,
    chain: ComponentObservation<ChainSnapshot>,
    monitored_nodes: Vec<MonitoredNodeReport>,
    validators: Vec<ValidatorReport>,
    spool: SpoolStatus,
}

struct MonitoredNodeReport {
    monitored_node_id: String,
    display_name: String,
    consensus: ComponentObservation<ConsensusSnapshot>,
    peers: ComponentObservation<PeerSnapshot>,
}

struct ValidatorReport {
    validator_node_id: String,
    monitored_node_id: Option<String>,
    detail: ComponentObservation<ValidatorDetailSnapshot>,
}
```

System Snapshot 字段至少包括：

- CPU usage percent；
- memory used / total bytes；
- network RX / TX bytes per second；
- disk detail 列表；
- disk alert threshold；
- auto-discovery 状态。

Chain Snapshot 字段至少包括：

- source Monitored Node ID；
- block number；
- block timestamp；
- observed at；
- transaction count；
- current / max block interval；
- current / max transactions；
- 可选的 bounded recent samples。

Consensus Snapshot 字段至少包括：

- current block；
- epoch；
- view；
- committed；
- locked；
- QC；
- validator role。

Peer Snapshot：

```rust
struct PeerSnapshot {
    ips: Vec<IpAddr>,
}
```

Agent 只发送标准化后的 literal IPv4 / IPv6：

- 从 `network.remoteAddress` 读取；
- 不解析 `enode`；
- 拒绝 hostname；
- 排序和去重；
- private / loopback 等地址仍可属于快照，但 Server 不得发送给公共 Geo provider。

### 10.4 顺序、幂等和多实例

Agent 在 state dir 中持久化 `boot_generation`，每次成功取得单实例锁后原子递增；每次启动同时生成唯一 `boot_id`。每个 boot 的 `report_sequence` 从 1 开始。

规则：

1. Report 在发送前写入 durable spool。
2. HTTP 重试复用同一个 `report_id` 和 body。
3. 每个 Agent 同时只允许一个 in-flight report。
4. backlog 严格 oldest-first。
5. 重启后先发送旧 generation 的 backlog，再发送新 generation 的 report。
6. Server 以 `report_id` 去重。
7. 同一 `(boot_generation, boot_id)` 内拒绝小于已接受 sequence 的新请求，但重复 report 返回成功。
8. 更小的 `boot_generation` 在新 generation 生效后被拒绝。
9. 更大的 generation 只有在 `report_sequence = 1` 时才能成为 active boot。
10. 相同 generation 却出现不同 `boot_id` 表示复制 state dir 或重复进程，Server 返回冲突并产生 duplicate-agent 告警。
11. Agent 必须锁定 state dir，防止同一配置在本机启动两个实例；state dir 丢失或从旧备份恢复时，需要显式 reset / re-register，不能静默覆盖 Server generation。

组件合并：

- Collector 状态按 `(agent_id, component_key, boot_generation, state_revision)` 合并。
- 最后成功值按 `(agent_id, component_key, latest.origin_boot_generation, latest.value_revision)` 合并，不使用承载该值的 report boot。
- `Ok` 的 `latest` 是当前成功值。
- `Error` 更新 collector status 和错误时间；如果携带的 `latest` 比 Server 已存值更新，仍可补齐 last-good value，但历史身份使用其 origin boot，不能把它记为本次失败或重启时产生的新样本。
- 相同 `value_revision` 的重复 report 不重复写历史。
- history 使用 `latest.observed_at`，liveness 使用 Server `received_at`。

### 10.5 时间和时钟偏差

- 所有协议时间使用 UTC RFC 3339。
- Agent 内部可继续用 monotonic clock 计算间隔，但传输前转换为 wall-clock observation time。
- Server 使用 `received_at` 判断 Agent 是否在线，避免 Agent 时钟错误影响 liveness。
- Server 记录 `generated_at - server_time` 估算 clock skew。
- 超过阈值时标记 Agent clock warning；历史图可保留原 observed time，但必须显示偏差状态。
- 不接受远离 Server 当前时间超过安全上限的鉴权或注册时间戳。

### 10.6 上报响应

```json
{
  "accepted": true,
  "duplicate": false,
  "report_id": "01J...",
  "server_time": "2026-08-10T12:00:01Z",
  "active_boot_generation": 42,
  "active_boot_id": "01J...",
  "next_report_after_ms": 5000
}
```

常见状态：

- `200`：接受或幂等重复。
- `400`：schema / invariant 错误。
- `401`：token 无效。
- `403`：token 与 agent ID 不匹配。
- `409`：活跃 boot 冲突或过期 boot。
- `413`：payload 超限。
- `422`：引用不存在、状态和值组合非法。
- `426`：协议版本不兼容。
- `429`：限流。
- `503`：存储暂时不可用，Agent 应重试。

### 10.7 离线缓存

Spool 应使用本地 SQLite 或原子 append-only 文件，不能只放内存。

默认建议：

- 最大 64 MiB；
- 最大 72 小时；
- oldest-first；
- 成功确认后删除；
- corruption 隔离并记录；
- 队列溢出时保留最新完整状态，删除最旧中间报告，并在后续报告中携带 dropped report count 和时间范围。

因为每个 report 都是完整当前视图，删除中间报告不会阻止当前状态恢复，但会形成明确的历史缺口。Server 必须可展示该缺口。

---

## 11. Server HTTP 和实时接口

### 11.1 Agent 接口

```text
POST /api/v1/agent-reports
```

Agent token 由 Server 管理 CLI 或受保护的管理员接口预先创建。协议范围和 ingest limits 通过成功响应、`426` 或 `413` 返回，不额外开放 Agent 配置接口，也不远程下发 RPC 配置。

### 11.2 Web 查询接口

```text
GET /api/v1/overview
GET /api/v1/agents
GET /api/v1/agents/{agent_id}
GET /api/v1/agents/{agent_id}/history
GET /api/v1/nodes
GET /api/v1/nodes/{agent_id}/{monitored_node_id}
GET /api/v1/nodes/{agent_id}/{monitored_node_id}/history
GET /api/v1/validators
GET /api/v1/validators/{chain_id}/{validator_node_id}
GET /api/v1/validators/{chain_id}/{validator_node_id}/sources
GET /api/v1/peers/countries
GET /api/v1/alerts
GET /api/v1/alerts/{alert_id}
GET /api/v1/notification-deliveries
```

查询接口必须：

- 分页；
- 限制时间范围和点数；
- 返回 `observed_at`、`received_at`、freshness 和 collector status；
- 默认不返回 RPC credential、token、原始 Peer IP 或内部错误堆栈；
- 对大范围 history 使用下采样。

### 11.3 SSE

```text
GET /api/v1/events
```

SSE 发送失效通知，而不是数据库变更日志：

```text
event: resource-updated
data: {"resource":"agent","id":"node-host-a","version":42}
```

事件类型：

- `agent-updated`；
- `node-updated`；
- `validator-updated`；
- `peer-countries-updated`；
- `alert-opened`；
- `alert-recovered`；
- `notification-updated`。

客户端收到事件后重新调用 REST 获取权威快照。事件允许 coalesce；SSE 断线后客户端重新拉取，不依赖逐事件补放。

---

## 12. 数据持久化

### 12.1 SQLite 策略

第一阶段使用 SQLite：

- 单机部署；
- WAL 模式；
- foreign keys 开启；
- busy timeout；
- migration 由 Server 启动时单点执行；
- migration 失败 readiness=false；
- 定期 backup / integrity check；
- 写入通过 repository module 串行化或使用明确事务边界。

当前 TUI Geo SQLite 未启用 WAL，不能把“当前已验证 WAL”写入实现假设。

### 12.2 建议表

#### Agent Registry

```text
agents
- agent_id TEXT PRIMARY KEY
- display_name TEXT
- chain_id TEXT
- first_seen_at INTEGER
- last_seen_at INTEGER
- active_boot_generation INTEGER
- active_boot_id TEXT
- agent_version TEXT
- protocol_version INTEGER
- clock_skew_ms INTEGER
- status TEXT
- created_at INTEGER
- updated_at INTEGER
```

#### Agent Token

```text
agent_tokens
- token_id TEXT PRIMARY KEY
- agent_id TEXT
- token_hash BLOB
- created_at INTEGER
- expires_at INTEGER NULL
- revoked_at INTEGER NULL
```

只存 token hash，不存明文。

#### Report Receipt

```text
agent_reports
- report_id TEXT PRIMARY KEY
- agent_id TEXT
- boot_generation INTEGER
- boot_id TEXT
- report_sequence INTEGER
- generated_at INTEGER
- received_at INTEGER
- payload_size INTEGER
- ingest_status TEXT
- UNIQUE(agent_id, boot_generation, boot_id, report_sequence)
```

该表只用于幂等和审计，短期保留，例如 7 天，不必永久保存完整 JSON body。

#### Monitored Node

```text
monitored_nodes
- agent_id TEXT
- monitored_node_id TEXT
- chain_id TEXT
- display_name TEXT
- configured_at INTEGER
- last_seen_at INTEGER
- PRIMARY KEY (agent_id, monitored_node_id)
```

#### Collector Current Status

```text
collector_status
- agent_id TEXT
- component_key TEXT
- boot_generation INTEGER
- boot_id TEXT
- state_revision INTEGER
- latest_value_origin_boot_generation INTEGER NULL
- latest_value_revision INTEGER NULL
- status TEXT
- attempted_at INTEGER
- last_success_at INTEGER NULL
- last_error_code TEXT NULL
- last_error_message TEXT NULL
- updated_at INTEGER
- PRIMARY KEY (agent_id, component_key)
```

`component_key` 示例：

```text
system
chain
node:rpc-main:consensus
node:rpc-main:peers
validator:0xabc:detail
```

#### Typed Current / History

```text
system_current
system_samples

chain_current
chain_samples

consensus_current
consensus_samples

validator_source_current
validator_source_samples
validator_current
```

Validator identity 和 source 关系：

```text
validators
- chain_id TEXT
- validator_node_id TEXT
- created_at INTEGER
- updated_at INTEGER
- PRIMARY KEY (chain_id, validator_node_id)
```

```text
validator_sources
- chain_id TEXT
- validator_node_id TEXT
- agent_id TEXT
- monitored_node_id TEXT NULL
- last_seen_at INTEGER
- PRIMARY KEY (chain_id, validator_node_id, agent_id)
```

`validator_source_current` / samples 保存每个 Agent 的原始 observation；`validator_current` 是全局 Validator 的可重建 projection，并记录被选中的 `source_agent_id` 和 data-conflict 状态。

原则：

- current 表保存最后成功值；
- collector failure 不清空 current；
- history 只在 `value_revision` 变化时写入，单纯 Collector error / recovery 不复制 last-good sample；
- 相同 report 重试不重复写 history；
- 速率字段命名带 `_bytes_per_second`。

#### Peer Snapshot

```text
peer_snapshots
- snapshot_id INTEGER PRIMARY KEY
- agent_id TEXT
- monitored_node_id TEXT
- value_origin_boot_generation INTEGER
- value_revision INTEGER
- observed_at INTEGER
- received_at INTEGER
- peer_count INTEGER
- UNIQUE(agent_id, monitored_node_id, value_origin_boot_generation, value_revision)
```

```text
current_peers
- agent_id TEXT
- monitored_node_id TEXT
- ip TEXT
- observed_at INTEGER
- PRIMARY KEY (agent_id, monitored_node_id, ip)
```

`current_peers` 对单个 `(agent_id, monitored_node_id)` 执行事务完整替换。

#### Location Cache

```text
location_cache
- ip TEXT PRIMARY KEY
- country_code TEXT NULL
- latitude REAL NULL
- longitude REAL NULL
- last_attempt_at INTEGER NULL
- last_success_at INTEGER NULL
- next_retry_at INTEGER NULL
- status TEXT
- last_error TEXT NULL
```

#### Daily Validator Snapshot

```text
daily_validator_snapshots
- snapshot_date TEXT
- chain_id TEXT
- validator_node_id TEXT
- source_agent_id TEXT
- node_name TEXT
- block_qty INTEGER
- reward_value REAL
- created_at INTEGER
- PRIMARY KEY (snapshot_date, chain_id, validator_node_id)
```

它替代当前 Agent 本地 `daily-node-snapshots.json`，并由 Server 计算日差值和月差值。

#### Alert / Outbox

```text
alerts
- alert_id TEXT PRIMARY KEY
- rule_key TEXT
- subject_key TEXT
- incident_sequence INTEGER
- status TEXT
- severity TEXT
- first_seen_at INTEGER
- last_seen_at INTEGER
- recovered_at INTEGER NULL
- details_json TEXT
- UNIQUE(rule_key, subject_key, incident_sequence)
```

SQLite 另建 partial unique index，确保同一 `(rule_key, subject_key)` 最多只有一个 `Pending` / `Open` incident；已恢复 incident 允许保留多次历史。

```text
notification_outbox
- delivery_id TEXT PRIMARY KEY
- alert_id TEXT NULL
- event_key TEXT
- channel_id TEXT
- destination TEXT
- payload TEXT
- status TEXT
- not_before INTEGER
- claimed_at INTEGER NULL
- last_attempt_at INTEGER NULL
- attempt_count INTEGER
- manual_retry_count INTEGER
- next_attempt_at INTEGER NULL
- delivered_at INTEGER NULL
- dead_lettered_at INTEGER NULL
- last_error TEXT NULL
- UNIQUE(event_key, channel_id, destination)
```

```text
notification_channels
- channel_id TEXT PRIMARY KEY
- kind TEXT
- status TEXT
- last_success_at INTEGER NULL
- last_error_at INTEGER NULL
- last_error TEXT NULL
- updated_at INTEGER
```

Secret 仍存于 Server secret/config，不写入该状态表。

### 12.3 当前状态和历史

Server 写入顺序：

1. 验证身份、协议、大小和 schema。
2. 检查 `report_id` 幂等。
3. 注册 / 验证 active boot。
4. 在事务中写 receipt、collector status，并按 `(latest.origin_boot_generation, latest.value_revision)` 合并 last-good current values。
5. 对新的 `value_revision` 写 typed history；仅状态变化不复制历史样本。
6. 对成功 Peer Snapshot 执行 per-node 完整替换。
7. 提交事务。
8. 触发异步 Geo / Alert evaluation。
9. 发送 coalesced SSE invalidation。

默认保留建议：

- 原始高频 system / chain samples：7 天；
- 1 分钟下采样：90 天；
- 1 小时下采样：长期；
- Alert 和通知审计：180 天；
- report receipt：7 天；
- Peer raw current set：仅当前；
- Peer 国家历史 aggregate：90 天；
- daily validator snapshot：至少 400 天。

具体数字应可配置，但 schema 和查询必须从第一天支持 retention job。

---

## 13. Geo 处理

### 13.1 目标流程

```text
Agent per-node admin_peers
        │
        ▼
ComponentObservation<PeerSnapshot>
        │
        ▼
Server per-node transactional replacement
        │
        ├── canonical / deduplicated IP
        ├── Location Cache lookup
        ├── bounded Geo enrichment queue
        └── aggregate by node / agent / all agents
        │
        ▼
Peer Country Distribution
```

### 13.2 快照替换规则

- 每个 Monitored Node 独立采集和上报。
- 一个节点失败只保留该节点的最后成功快照。
- 其他节点的成功快照正常替换。
- `Ok + []` 清空该节点 Peer。
- `Error` 不替换 Peer。
- Server 的 Agent aggregate 和全局 aggregate 从各节点最后成功快照计算。
- 聚合时按 canonical IP 去重。

这有意改进当前“任一 URL 失败冻结全局快照”的语义。

### 13.3 Location Cache

- canonical IP 为全局 cache key；
- 成功 location 默认 24 小时后可刷新；
- refresh 失败保留旧成功值；
- `last_attempt_at` 与 `last_success_at` 分离；
- 首次失败使用指数退避，而不是每分钟无限重试；
- enrichment 有全局并发和 provider rate limit；
- Server shutdown 时停止领取新任务并等待有截止时间的 in-flight task；
- provider 返回空 country / loc 时记录明确状态。

### 13.4 IP 安全和隐私

Agent 可上报 private / loopback / link-local / multicast / unspecified literal IP，因为它们可能属于真实 Peer Snapshot；但 Server：

- 绝不把这些地址发送给公共 Geo provider；
- 将它们计入 Unknown / private category；
- 不通过默认 Web API 返回原始 IP；
- 日志中默认 mask IP；
- 只允许受限管理员查询原始 Peer，且该能力不属于 v1；
- 对 Agent 传入 IP 使用标准库重新 parse / canonicalize，不信任字符串格式。

### 13.5 WebUI 国家分布

返回：

```json
{
  "total_peers": 30,
  "unique_countries": 10,
  "unknown_country_count": 2,
  "countries": [
    {"country_code": "SG", "peer_count": 6},
    {"country_code": "FI", "peer_count": 4}
  ]
}
```

排序继续使用：peer count 降序、country code 升序。

---

## 14. 告警、通知和日报

### 14.1 Alert Engine

目标告警规则：

- Agent offline / recovered；
- Monitored Node RPC failed / recovered；
- block stream disconnected / recovered；
- block stalled / recovered；
- consensus height lag；
- committed / locked / QC 异常；
- disk threshold exceeded / recovered；
- ranking changed；
- peer count or country distribution 异常变化。

其中当前代码只实现了连接失败 / 恢复、ranking change 和 daily summary Telegram 通知。其他均为新功能，不能描述为已存在能力。

每条规则使用稳定：

```text
rule_key + subject_key
```

状态机：

```text
Healthy -> Pending -> Open -> Recovered
```

支持：

- `for` 持续时间；
- hysteresis；
- cooldown；
- severity；
- recovery 通知；
- maintenance / silence；
- last evaluated `value_revision`。

Agent offline 使用 Server `received_at` 和允许 miss 次数判断，不使用 Agent `generated_at`。

### 14.2 Notification Outbox

通知产生和发送分离：

```text
Alert transition
  -> durable outbox row
  -> delivery worker
  -> Telegram adapter
```

规则：

- 先提交 alert + outbox，再执行网络发送；
- 失败按 destination 独立重试；
- 同一 event / channel / destination 使用幂等 key，保证不重复创建 outbox row；
- 只有 adapter 成功确认后才写 `delivered_at`；
- 多 Chat ID 部分成功分别记录；
- Server 重启后继续处理未完成 outbox；
- 达到最大重试后转为 `DeadLetter` 并写 `dead_lettered_at`；
- 人工重试执行 `DeadLetter -> Pending`，清空 terminal timestamp、增加 `manual_retry_count`，但保留原 delivery ID 和审计；
- channel credential 无效时将 channel 标为 `Attention` / `Disabled` 并暂停领取对应 delivery。

Delivery 状态至少包括：

```text
Pending -> Delivering -> Delivered
                 └----> RetryScheduled -> Delivering
                 └----> DeadLetter -> Pending  # manual retry
```

Telegram 不支持使用本系统 `delivery_id` 做服务端去重。如果 Telegram 已接收消息，而 ChainDash 在写 `delivered_at` 前崩溃，重试可能再次发送。因此外部 delivery 明确为 **at-least-once**；系统保证 outbox 和审计不重复创建，但不承诺第三方最终只显示一次。

### 14.3 静默时间和限流

新的产品语义：

- Alert / event 始终持久化，不因 quiet hours 或 rate limit 丢失。
- quiet hours 只影响 delivery 的 `not_before`。
- 静默期可按 channel / destination 聚合摘要。
- quiet summary 在成功发送前保持 durable。
- Daily summary 可配置是否绕过 quiet hours；默认沿用当前行为：绕过。
- Daily summary 不自动 flush 与其无关的 quiet alert summary。
- Rate limit 应用于 delivery policy，不阻止 alert state 变化和历史记录。
- Rate limit 时间在 outbox 成功入队或成功送达后推进，不能在外部发送前无条件推进。

### 14.4 Daily / Monthly Summary

日报调度移到 Server：

- 使用 Server 配置的明确 IANA timezone；
- 默认每天本地 00:00；
- 每个全局 `(chain_id, validator_node_id)` 保存累计 block qty 和 reward value，并记录选中的 source Agent；
- 与前一日快照计算日差值；
- 每月 1 日与上月首日快照计算上月差值；
- 缺少基准显示 unknown，不伪造 0；
- reward 差值不允许负数；
- 可手动补发指定日期；
- 同一日期 / destination 只创建一个 outbox event；外部发送仍遵循 at-least-once。

Agent 不再持有 Telegram token，也不再写 `daily-node-snapshots.json`。

---

## 15. 安全设计

### 15.1 Agent 鉴权

- 每个 Agent 独立随机 token；
- token 仅创建时显示一次；
- Server 只存 hash；
- token 与 `agent_id` 绑定；
- 支持 overlap rotation；
- 支持 revoke；
- 所有生产流量使用 TLS；
- 反向代理转发时只信任显式配置的 proxy IP。

未来可增加 mTLS，但不作为 v1 前置条件。

### 15.2 WebUI 鉴权

v1 默认：

- Server 绑定 `127.0.0.1`；
- WebUI 和 API same-origin；
- 对外部署通过反向代理提供 TLS 和身份认证；
- 非 loopback listen 必须显式开启，并要求配置受信任的 auth proxy 或内置管理 token 模式；
- CORS 默认关闭；
- CSP、`X-Content-Type-Options`、frame 限制和安全 cookie 明确配置。

细粒度 RBAC 可后续增加，但不能默认把公网 WebUI 视为无认证安全。

### 15.3 输入校验

Server 必须校验：

- agent / node / validator ID 长度和字符集；
- token 绑定；
- URL 或 secret 不出现在 payload；
- component status / value invariant；
- boot generation、report sequence、state revision 和 value revision 单调性；
- 时间戳范围；
- 数值非 NaN / 非无限；
- percentage 合法范围；
- IP 可被标准库 parse；
- peer 数和数组上限；
- gzip 解压比例；
- report body 大小；
- history 查询范围和 page size。

### 15.4 权限

Agent：

- 普通用户运行；
- 不需要 root；
- 只读配置和 token；
- state dir 仅 Agent 用户可写；
- 仅访问配置中的 RPC、Explorer 和 Server。

Server：

- 普通用户运行；
- DB 目录仅 Server 用户可写；
- Telegram / Geo provider secret 不进入 WebUI；
- 静态资源只读；
- backup 单独授权。

---

## 16. 可观测性和故障处理

### 16.1 Agent offline

建议默认：

```text
report interval = 5s
offline after = 20s
```

Server 使用 `last_seen_at`：

- 0-20 秒：online；
- 超过 20 秒：offline；
- 可选中间状态 delayed；
- 恢复后关闭 offline alert。

阈值必须相对于 report interval 配置，不能硬编码为一个与 interval 无关的固定数。

### 16.2 Collector failure

每个 Collector 状态单独展示：

- Starting；
- Ok；
- Error；
- Disabled；
- Unsupported。

Error 包含结构化 code 和经过脱敏的 message。Server / WebUI 同时显示：

- 当前 Collector 状态；
- last attempt；
- last success；
- last error；
- 当前展示值的 age。

不能把旧成功值显示成刚刚采集到的健康值。

### 16.3 Server failure

- Agent 将 report 持久化到 spool；
- 连接失败指数退避；
- 401 / 403 不无限高频重试，进入 auth error；
- 400 / 422 将报告隔离为 invalid，不阻塞后续报告，并记录摘要；
- 409 duplicate boot 进入配置错误状态；
- 429 / 503 遵循 `Retry-After`；
- Server 恢复后 oldest-first 补传。

### 16.4 Geo failure

- DB write failure：不发布新 snapshot；
- per-node collection failure：保留该节点旧 snapshot；
- Geo provider failure：保留旧成功 location；
- enrichment queue backlog：展示 queue depth；
- cache read failure：Web 查询返回 degraded metadata，不清空最后 aggregate。

### 16.5 Notification failure

- 每 destination 独立 attempt count；
- 指数退避；
- 记录 HTTP status 和脱敏原因；
- token 无效进入 channel disabled / attention 状态；
- outbox backlog 和 oldest age 暴露在 health / metrics。

### 16.6 Health endpoints

```text
GET /health/live
GET /health/ready
GET /metrics        # 可选 Prometheus
```

Readiness 至少检查：

- migrations 完成；
- DB 可写；
- ingest router 已启动；
- fatal background module 未退出。

外部 Geo / Telegram 不可用不应让 Server readiness 失败，但应显示 degraded。

### 16.7 日志

Agent 和 Server 使用结构化字段：

- agent_id；
- boot_id；
- report_id；
- monitored_node_id；
- validator_node_id；
- component；
- state_revision；
- value_revision；
- error_code。

禁止记录：

- bearer token；
- Telegram token；
- URL credential；
- 完整 Peer IP（默认）；
- 未截断的第三方响应 body。

---

## 17. TUI 兼容策略

### 17.1 过渡阶段

TUI 继续本地采集，但改为读取 Core Observation Store：

```text
Collectors -> Core Observation Store
                         ├── TUI adapter
                         └── Agent report adapter
```

不要保留两套 RPC / Explorer / Peer 采集实现。

### 17.2 必须保留的回归约束

- 多 `name@url` 共识状态；
- 区块订阅按端点顺序 failover；
- Node Details 单节点卡片和多节点汇总；
- Unix system / disk；
- Peer Country Distribution；
- Block Time 与 Block Transactions 并排；
- auxiliary row 6 行；
- panel 最低 3 行；
- Info 5 秒 / Warn 15 秒 / Error persistent；
- dirty-state wake 和 Geo coalesced wake；
- Geo read failure 保留最后 snapshot；
- status visibility change 使用 synchronized update + current-area resize；
- 不调用 `Terminal::clear()`；
- tmux / screen / Docker 使用 ASCII country code；
- 普通终端可显示 flag + country code。

### 17.3 后续阶段

远程 TUI 可以作为 Server REST / SSE 的另一个 client adapter，但不是 Server / WebUI MVP 的前置条件。

如果实现远程 TUI：

- 不复用 Agent token；
- 使用只读用户凭据；
- 与 WebUI 使用同一 query DTO；
- 不读取 Server SQLite；
- 保留现有终端兼容策略。

---

## 18. 迁移路线

### Phase 0：锁定模型和协议

目标：先消除当前数据归属歧义。

工作：

- 明确 Agent、Monitored Node、Chain Observation、Validator 和 Validator Observation；
- 决定稳定 ID 格式；
- 固定 AgentReport v1；
- 固定 Component Observation invariant；
- 定义 per-node Peer success / empty / error 语义；
- 定义 report order、boot conflict 和幂等；
- 添加 golden JSON fixtures；
- 将不可逆决策记录为 ADR。

退出条件：

- 不再存在 singular `monitored_node` DTO；
- 不再依赖数组位置关联 URL 和 Explorer node ID；
- 所有组件都能表达 last success 和 current error；
- Agent / node / validator / chain 数据归属无歧义。

### Phase 1：抽取 Core，保持 TUI 行为

工作：

- 创建 `chaindash-core`；
- 将 DTO 与 `Instant` / UI state 分离；
- 抽取 Collector Engine；
- 将 peer collection 改为 per-node observation；
- 引入 Collector status、`state_revision` 和 `value_revision`；
- TUI 改读 Observation Store；
- 保留现有 Geo Store adapter 直到 Server Geo 可替代；
- 增加上述 TUI regression tests。

验证：

```bash
cargo +nightly fmt -- --check
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
```

### Phase 2：Agent

工作：

- YAML config；
- stable agent ID；
- boot ID / report sequence；
- report assembler；
- durable spool；
- HTTPS transport；
- Collector Supervisor；
- local config validation / dry-run；
- native systemd unit；
- Agent Docker image。

验收：

- 断网重试；
- Server 重启补传；
- report 幂等；
- spool overflow 可见；
- 单 Collector panic 不停止其他 Collector；
- 多节点采集可区分。

### Phase 3：Server ingest 和存储

工作：

- HTTP ingest；
- token registry；
- active boot lease；
- SQLite migrations / WAL / backup；
- current projector；
- history writer；
- per-node Peer replacement；
- query REST；
- SSE invalidation。

验收：

- duplicate report 不重复历史；
- Error 不覆盖 last good；
- Ok empty 清空 Peer；
- 一个节点失败不影响其他节点；
- Server restart 保留状态；
- migration failure readiness=false。

### Phase 4：Geo、Alert、Notification、日报

工作：

- Server Geo enrichment queue；
- Location Cache；
- country aggregate；
- Alert Engine；
- Telegram adapter；
- Notification Outbox；
- quiet / rate limit policy；
- daily / monthly snapshot scheduler；
- 从本地 JSON 导入历史的可选工具。

验收：

- Geo refresh failure 保留旧成功 location；
- private IP 不发往公网 provider；
- notification retry 跨重启；
- 多 destination 部分成功正确；
- daily summary outbox event 幂等，外部 delivery at-least-once；
- quiet summary 不因发送失败丢失。

### Phase 5：WebUI

工作：

- Overview；
- Agent / Node / Validator details；
- Peer Country Distribution；
- Alerts；
- History charts；
- SSE reconnect；
- loading / empty / stale / error 状态；
- responsive layout；
- country code fallback；
- same-origin security headers。

### Phase 6：生产化

工作：

- separate images；
- systemd units；
- reverse proxy examples；
- token rotation；
- retention jobs；
- backup / restore；
- load and chaos tests；
- optional PostgreSQL repository；
- optional remote TUI。

---

## 19. 测试策略

### 19.1 Core

- Component Observation invariant；
- UTC time conversion；
- boot generation / report sequence / state revision / value origin + revision rules；
- ID validation；
- peer parse / canonicalize / deduplicate；
- non-enrichable IP classification；
- country aggregation；
- golden JSON / backward compatibility。

### 19.2 Agent

使用 fake adapters：

- fake RPC；
- fake Explorer；
- fake clock；
- in-memory Agent transport；
- temporary spool。

场景：

- 多 Monitored Node；
- chain source failover；
- 一个 consensus collector 失败；
- 一个 peer collector 失败；
- successful empty peers；
- system unsupported；
- Server 401 / 409 / 429 / 503；
- restart 后 backlog；
- duplicate report body；
- spool corruption / overflow；
- collector panic / restart / shutdown deadline。

### 19.3 Server

- migration from empty and previous schema；
- token hash / revoke / rotate；
- report idempotency；
- active boot generation / duplicate process conflict；
- component state merge 与跨 boot last-good provenance；
- current / history transaction；
- per-node peer replacement；
- Location Cache failure preservation；
- retention / downsampling；
- offline detection；
- alert transition；
- outbox retry；
- multi-destination partial delivery；
- provider 已接收但 ack 丢失时的 at-least-once duplicate 场景；
- global Validator multi-source selection / conflict；
- daily / monthly summary；
- API redaction；
- payload limits。

### 19.4 WebUI

- loading；
- authoritative empty；
- stale last-good；
- current collector error；
- Agent offline；
- SSE reconnect；
- REST refetch；
- small screen；
- country flag fallback；
- no raw Peer IP exposure；
- accessible status labels and keyboard navigation。

### 19.5 TUI

除现有测试外，保持：

- status bar visibility reflow；
- no cursor-position query backend；
- synchronized update；
- tmux ASCII mode；
- Docker `ascii-countries` build；
- country border cells；
- short terminal panel suppression；
- Geo read failure retention；
- block charts side-by-side。

### 19.6 端到端和故障注入

```text
fake node -> Agent -> Server -> SQLite -> REST/SSE -> browser test
```

故障注入：

- RPC reset；
- Explorer timeout；
- Server restart；
- SQLite busy / disk full；
- Geo 429；
- Telegram 500；
- Agent clock skew；
- duplicate Agent process；
- out-of-order / duplicate report；
- network partition；
- spool overflow。

---

## 20. MVP 验收标准

MVP 完成时必须满足：

1. 一个 Server 接收至少两个 Agent。
2. 一个 Agent 监控至少两个 Monitored Node。
3. Chain source failover 可见且不与节点身份混淆。
4. Consensus、Peer、System、Validator Detail 具有独立 freshness 和 error。
5. Agent 断线后 WebUI 显示 offline。
6. Collector failure 不清空最后成功值，也不伪装为新鲜。
7. successful empty Peer 能清空单节点 Peer。
8. 单节点 Peer 失败不影响其他节点。
9. Agent 重试和 Server 重启不产生重复 history。
10. Server 重启后告警、通知 outbox、Geo cache 和日报历史仍存在。
11. WebUI 可查看 Overview、Agent、Node、Validator、Peer Countries、Alerts 和基础历史。
12. Web API 默认不暴露 token、URL credential 和 raw Peer IP。
13. Telegram 发送失败可重试，多 destination 独立记录，并明确展示 at-least-once 可能重复的 delivery 语义。
14. Daily summary 跨重启不重复创建 outbox event。
15. 现有 TUI 回归测试通过，包括 tmux 国家码模式和状态栏 redraw。
16. Server migration failure 会阻止 readiness，而不是静默退化为空存储。
17. Agent / Server 可独立打包并以普通用户运行。
18. 两个 Agent 观察同一 `(chain_id, validator_node_id)` 时保留 source observations、冲突可见，并且 ranking / daily notification 不重复创建。

---

## 21. 已解决决策和剩余开放项

### 21.1 已解决

1. **一个 Agent 是否支持多个 Monitored Node？**
   - 是。当前 CLI 已支持多个 URL，目标不得回退为单节点。
2. **是否直接序列化当前 `Data`？**
   - 否。它包含不同归属、`Instant` 和 TUI 专属状态。
3. **Peer 按 Agent 还是按节点存储？**
   - 按 Monitored Node 存储，Server 再聚合。
4. **系统指标归属？**
   - Agent 主机级。
5. **区块流归属？**
   - Agent 的 Chain Observation，记录实际 source node。
6. **Explorer node ID 如何建模和关联 RPC？**
   - 全局 Validator 身份为 `(chain_id, validator_node_id)`；每个 Agent 保留独立 source observation，并通过显式可选 `monitored_node_id` 关联，不按位置推断。
7. **Geo enrichment 在哪里？**
   - Server。Agent 只收集标准化 Peer IP。
8. **Telegram 在哪里？**
   - Server，以 durable Alert State + Outbox 实现。
9. **WebUI 如何实时更新？**
   - REST 权威快照 + SSE 失效通知。
10. **TUI 是否继续保留？**
    - 保留，并迁移到共享 Core，不作为 WebUI 前置依赖。

### 21.2 实现前仍需确认

1. WebUI 前端技术栈：React + TypeScript + Vite，或 Rust/WASM。
2. 默认 history retention 数值是否采用本文建议。
3. Agent raw Peer IP 集中上报是否符合部署方隐私政策；若不允许，需要切换为 Agent-side country aggregate 模式，并接受 cache 重复。
4. Server 对外认证采用哪种反向代理集成：OIDC、Basic Auth 或内置 session。
5. PostgreSQL adapter 的触发条件：Agent 数、写入速率、HA 或多 Server 实例。

v1 已确定每个 Agent 只有一个带稳定 `chain_id` 的 Chain Observation；多链 Agent 留给未来协议版本。这些开放项不影响 Phase 0 的身份、组件状态和幂等模型，但第 3 项必须在开始 Server Geo schema 前确认。

---

## 22. 推荐下一步

按以下顺序推进：

1. 为身份模型、Component Observation 和 AgentReport v1 建立 ADR。
2. 在 `chaindash-core` 中实现协议类型、invariant 和 golden fixtures。
3. 将当前 Collector 输出改为独立 Component Observation，同时保持 TUI 读取路径。
4. 将 Peer collection 从全 Agent all-or-nothing 改为 per-node replace-on-success。
5. 完成 TUI regression 后，再实现 Agent spool 和 Server ingest。
6. Server current / history / Geo 正确后，再迁移 Telegram 和日报。
7. 最后实现 WebUI，避免前端建立在尚未稳定的数据语义上。

核心原则是：

> 先固定身份、数据归属、新鲜度和失败语义，再引入网络与 WebUI。否则只会把当前单进程中的隐式耦合搬到跨进程协议里。
