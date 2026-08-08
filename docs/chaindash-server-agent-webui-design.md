# ChainDash Server / Agent / WebUI 设计方案

## 1. 文档状态

- 状态：提案
- 范围：将当前单进程 TUI 架构演进为 `Server + Agent + WebUI`
- 目标：先定义边界、数据协议、迁移路线和最小可行版本，不在本文中实现代码
- 依据：当前 chaindash 代码、`CONTEXT.md` 和现有 Peer 地理面板架构

## 2. 摘要

ChainDash 当前是一个运行在单台机器上的 Rust TUI 程序。它在同一个进程中完成：

1. 连接一个或多个被监控节点的 PlatON WebSocket RPC；
2. 采集区块、交易、共识状态、`admin_peers` 和节点详情；
3. 采集当前运行机器的 CPU、内存、网络、磁盘信息；
4. 维护 Peer Snapshot 和 Location Cache；
5. 运行 Telegram 通知；
6. 使用 ratatui/crossterm 绘制界面。

目标架构将这些职责拆分为三个部分：

- **Agent**：部署在被监控节点所在机器，负责本地采集和上报；
- **Server**：集中接收、存储、计算、告警和鉴权；
- **WebUI**：通过 Server 的 HTTP API 和实时推送展示监控数据。

推荐保留 TUI 作为过渡期和可选客户端，而不是一次性删除。最终数据流如下：

```text
┌────────────────────┐
│  被监控节点所在机器 │
│                    │
│  PlatON Node       │
│  CPU / Memory / IO │
│        ▲           │
│        │ local RPC │
│        ▼           │
│      Agent         │
└────────┬───────────┘
         │ HTTPS/WSS outbound
         ▼
┌────────────────────┐
│      Server        │
│                    │
│ ingest / storage   │
│ auth / alert       │
│ geo enrichment     │
│ HTTP API / WS      │
└────────┬───────────┘
         │ HTTP + WebSocket
         ▼
┌────────────────────┐
│      WebUI         │
└────────────────────┘
```

## 3. 为什么需要 Agent

单纯的 `Server + WebUI` 可以直接连接远程 PlatON WebSocket，但无法完整替代当前 TUI 的能力：

- Server 无法自然获得远程机器的 CPU、内存、磁盘和进程状态；
- Server 需要保存所有 PlatON RPC 凭据，安全面更大；
- Server 直接连接每个被监控节点时，网络拓扑和防火墙配置更复杂；
- Peer 和系统采集任务会集中在 Server，单个 Server 故障会影响所有采集。

Agent 的定位不是远程控制端，也不是 RPC 代理，而是一个最小权限的本地采集器：

- 读取本地系统指标；
- 连接本地或明确配置的 PlatON RPC；
- 通过出站连接向 Server 上报数据；
- 不监听公网端口；
- 不执行 Server 下发的任意命令。

如果用户只需要监控一个由 Server 可直接访问的 PlatON RPC，也可以提供无 Agent 的 Direct Collector 模式作为兼容能力，但它不应成为主要架构。

## 4. 目标和非目标

### 4.1 目标

1. 支持一个 Server 管理多个 Agent 和多个被监控节点。
2. 将当前 TUI 的核心监控能力迁移到 WebUI。
3. 支持 Agent 断线重连、重复上报和 Server 重启。
4. 让 Agent 只需要出站访问 Server。
5. 集中维护当前状态、指标历史、Peer Snapshot、Location Cache 和告警。
6. 为未来多用户、权限和多实例 Server 保留清晰的 seam。
7. 在迁移过程中保留现有 TUI，避免一次性重写采集逻辑。

### 4.2 非目标

第一阶段不包含：

- 远程执行 shell 命令；
- 远程启动、停止或重启 PlatON 进程；
- 将 Server 做成通用 RPC 代理；
- Kubernetes Operator；
- 事件溯源架构；
- 一开始就支持多租户和复杂 RBAC；
- 一开始就部署多个 Server 实例；
- 一开始就引入消息队列或时序数据库。

这些功能可能有价值，但会显著扩大信任边界和运维复杂度，应在基础监控链路稳定后单独设计。

## 5. 当前实现盘点

### 5.1 当前入口和运行模型

`src/main.rs` 当前负责：

- 解析 CLI 参数；
- 初始化 SQLite PeerGeoStore；
- 初始化 TUI terminal；
- 创建 `collect::Collector`；
- 启动 Tokio 采集任务；
- 通过 crossbeam channel 处理 UI 刷新、Geo store 更新和终端事件；
- 停止 Collector 并清理 terminal。

这说明当前进程同时承担了采集器、应用状态容器、持久化访问者和 UI 运行时四种职责。

### 5.2 当前 Collector 能力

`src/collect/collector.rs` 中的 Collector 当前组合了：

- 被监控节点状态采集；
- WebSocket block subscription；
- Peer Geo 采集；
- Explorer 节点详情采集；
- Unix 系统指标采集；
- Telegram 通知。

这些任务需要按职责拆开，不能把 `SharedData` 直接搬到 Server 和 Agent 之间使用。跨进程通信必须基于稳定的、可版本化的快照协议。

### 5.3 当前 Geo 设计可复用部分

当前 `PeerGeoStore` 是一个合适的内部 seam：

- SQLite 连接由专用 worker 线程拥有；
- 写入和读取通过 trait 访问；
- Peer Snapshot 和 Location Cache 有明确边界；
- UI 通过 Geo View Snapshot 读取，不在绘制过程中执行 SQL；
- 成功写入后通过 wake channel 通知 TUI 刷新。

目标架构中应保留这些语义，但将它移动为 Server 内部能力：

```text
Agent:
    admin_peers -> Peer Snapshot 上报

Server:
    current_peers -> Location Cache -> Geo View Snapshot

WebUI:
    读取 Geo View Snapshot 并绘制地图
```

## 6. 目标架构

## 6.1 Core

Core 是 Server、Agent 和 TUI 共享的 Rust crate，包含领域模型和传输 DTO，但不包含网络、数据库或 UI 代码。

建议名称：`chaindash-core`。

职责：

- `NodeSnapshot`、`SystemSnapshot`、`PeerSnapshot` 等领域结构；
- Agent 上报协议 DTO；
- 序列化和反序列化；
- 协议版本校验；
- 数值范围和字段大小校验；
- 采集时间、序列号和状态枚举；
- 与展示无关的派生计算。

Core 不应包含：

- `Arc<Mutex<Data>>`；
- ratatui Widget；
- SQLite connection；
- HTTP client/server；
- Telegram client；
- 浏览器专用 JSON 结构。

### 6.2 Agent

建议名称：`chaindash-agent`。

Agent 内部模块：

```text
agent/
├── config.rs
├── rpc_collector.rs
├── system_collector.rs
├── peer_collector.rs
├── snapshot.rs
├── reporter.rs
├── spool.rs
└── lifecycle.rs
```

职责：

- 读取 Agent 配置；
- 连接被监控节点；
- 采集本机系统信息；
- 创建 `AgentSnapshot`；
- 进行本地校验和限流；
- 发送心跳和快照；
- 处理重试、退避和 Server 不可用；
- 报告采集错误和连接状态；
- 优雅关闭。

Agent 不负责：

- 生成 Geo View Snapshot；
- 调用 ipinfo.io；
- 发送 Telegram 通知；
- 管理 WebUI 用户；
- 直接写 Server 数据库。

### 6.3 Server

建议名称：`chaindash-server`。

Server 内部模块：

```text
server/
├── config.rs
├── auth.rs
├── ingest.rs
├── api.rs
├── realtime.rs
├── storage.rs
├── geo.rs
├── alerts.rs
└── lifecycle.rs
```

职责：

- Agent 注册和鉴权；
- 接收快照和心跳；
- 验证 Agent 身份、协议版本、序列号和数据大小；
- 更新当前状态；
- 写入指标历史；
- 维护 Peer Snapshot、Location Cache 和 Geo View Snapshot；
- 进行 Explorer 和 IP 地理 enrichment；
- 评估告警规则；
- 发送 Telegram 通知；
- 对 WebUI 提供 REST API 和 WebSocket；
- 提供健康检查和运行状态。

### 6.4 WebUI

WebUI 是 Server 的客户端，不直接连接 Agent，也不直接访问数据库。

建议使用：

- TypeScript；
- Vite；
- React 或 Vue 二选一；
- Apache ECharts 或其他成熟图表库；
- WebSocket 接收实时状态变化。

第一阶段可以由 Server 提供编译后的静态资源，开发阶段允许 Vite dev server 独立运行。

## 7. 进程和打包形态

第一阶段建议使用同一个 Rust workspace，并提供三个子命令：

```bash
chaindash tui
chaindash agent
chaindash server
```

这样可以复用当前仓库的类型和测试，并降低部署门槛。

目标目录：

```text
.
├── Cargo.toml                 # workspace
├── crates/
│   ├── chaindash-core/
│   ├── chaindash-agent/
│   ├── chaindash-server/
│   └── chaindash-tui/
├── web/
│   ├── package.json
│   ├── src/
│   └── dist/
├── docs/
└── migrations/
```

迁移初期可以保留现有根 crate 和 `src/`，先将 Core 模型提取出来；不要求第一步就完成目录重排。

## 8. Agent 配置

当前的：

```text
--url name@ws://...
```

适合 TUI 兼容模式，不适合作为长期 Agent 配置格式。Agent 建议使用 TOML 配置文件，并允许环境变量覆盖敏感字段：

```toml
server_url = "https://monitor.example.com"
agent_id = "node-a-01"
agent_token_file = "/etc/chaindash/agent-token"

[monitored_node]
name = "mainnet-validator-1"
rpc_url = "ws://127.0.0.1:6789"

[collect]
node_interval_seconds = 5
system_interval_seconds = 5
peer_interval_seconds = 60

[system]
disk_mount_points = ["/", "/opt"]
auto_discovery = false
alert_threshold_percent = 90.0
```

敏感信息要求：

- Token 不出现在普通日志；
- 配置文件权限限制为 owner-only；
- RPC 凭据只保存在 Agent；
- WebUI 不显示 RPC 凭据；
- Server 只能看到 Agent 标识和上报数据。

## 9. Agent 与 Server 协议

## 9.1 传输选择

第一阶段采用 HTTPS JSON 上报：

- 实现简单；
- 便于调试；
- Agent 可以自然地重试；
- 不要求 Server 维护大量长连接；
- 适合低频监控快照。

Server 到 WebUI 使用 WebSocket 或 Server-Sent Events 推送。

Agent 到 Server 的长连接 WSS 可以作为后续优化，不作为第一阶段依赖。

### 9.2 协议版本

所有上报都包含：

```json
{
  "protocol_version": 1,
  "agent_id": "node-a-01",
  "sequence": 1842,
  "collected_at": "2026-01-01T12:00:00Z",
  "payload": {}
}
```

字段语义：

- `protocol_version`：协议不兼容变更时递增；
- `agent_id`：Server 注册的 Agent 标识；
- `sequence`：Agent 单调递增的上报序列号；
- `collected_at`：Agent 采集时间，使用 UTC；
- `payload`：快照或心跳内容。

Server 应按 `(agent_id, sequence)` 做幂等处理：

- 重复序列号不重复写入；
- 较旧序列号不能覆盖更新状态；
- 序列号回退只在 Agent 被重新注册或显式 reset 时允许。

### 9.3 AgentSnapshot

建议的第一版协议模型：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub protocol_version: u32,
    pub agent_id: String,
    pub sequence: u64,
    pub collected_at: DateTime<Utc>,
    pub monitored_node: MonitoredNodeSnapshot,
    pub system: Option<SystemSnapshot>,
    pub peers: Option<PeerSnapshot>,
    pub collector_status: Vec<CollectorStatus>,
}
```

被监控节点状态：

```rust
pub struct MonitoredNodeSnapshot {
    pub name: String,
    pub connected: bool,
    pub last_error: Option<String>,
    pub consensus: Option<ConsensusSnapshot>,
    pub chain: Option<ChainSnapshot>,
}
```

系统状态：

```rust
pub struct SystemSnapshot {
    pub cpu_usage_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub disks: Vec<DiskSnapshot>,
}
```

Peer 状态：

```rust
pub struct PeerSnapshot {
    pub observed_at: DateTime<Utc>,
    pub ips: Vec<String>,
}
```

`current_disk_index`、状态栏消息过期时间、TUI 选中的磁盘等是展示状态，不应进入 Agent 协议。

### 9.4 心跳

心跳不携带完整指标，只用于在线状态和版本管理：

```json
{
  "protocol_version": 1,
  "agent_id": "node-a-01",
  "agent_version": "0.3.0",
  "sequence": 1843,
  "sent_at": "2026-01-01T12:00:05Z",
  "status": "ok"
}
```

建议：

- 心跳周期：15 秒；
- Offline 判定：连续 3 个心跳周期未收到；
- Server 记录 `last_seen_at`；
- 在线状态由 Server 根据接收时间计算，不信任 Agent 自己声明的在线状态。

### 9.5 上报响应

Server 应返回明确的结果：

```json
{
  "accepted": true,
  "server_time": "2026-01-01T12:00:05Z",
  "next_upload_after_seconds": 5,
  "min_protocol_version": 1,
  "max_protocol_version": 1
}
```

错误响应至少区分：

- `401`：Token 无效；
- `403`：Agent 被禁用；
- `409`：序列号冲突或协议状态错误；
- `413`：上报数据过大；
- `429`：限流；
- `5xx`：Server 暂时不可用，Agent 应退避重试。

## 10. Server HTTP API

API 前缀统一为 `/api/v1`。

### 10.1 Agent 接口

```text
POST /api/v1/agents/register
POST /api/v1/agents/{agent_id}/heartbeat
POST /api/v1/agents/{agent_id}/snapshots
GET  /api/v1/agents/{agent_id}/config
```

第一阶段不建议让 Agent 自动公开注册。推荐流程：

1. 管理员在 WebUI 创建一次性 enrollment token；
2. Agent 使用 enrollment token 注册；
3. Server 返回长期 Agent token；
4. enrollment token 立即失效；
5. Agent 后续只使用自己的长期 token。

### 10.2 WebUI 查询接口

```text
GET /api/v1/overview
GET /api/v1/agents
GET /api/v1/agents/{agent_id}
GET /api/v1/agents/{agent_id}/current
GET /api/v1/agents/{agent_id}/metrics
GET /api/v1/agents/{agent_id}/peers
GET /api/v1/agents/{agent_id}/geo
GET /api/v1/agents/{agent_id}/alerts
```

历史指标查询应限制时间范围和采样粒度，避免一次查询返回无限数据：

```text
GET /api/v1/agents/{agent_id}/metrics
    ?from=...
    &to=...
    &resolution=minute
```

### 10.3 实时接口

```text
GET /api/v1/events
```

WebSocket 消息类型示例：

```json
{
  "type": "agent_state_changed",
  "agent_id": "node-a-01",
  "state": "offline",
  "at": "2026-01-01T12:01:00Z"
}
```

实时接口只推送状态变化或经过聚合的更新，不直接把数据库变更日志暴露给 WebUI。

## 11. 数据持久化

## 11.1 存储策略

第一阶段继续使用 SQLite，Server 单实例部署：

- 复用当前 SQLite migration 思路；
- 使用 WAL；
- 通过 `MonitorStore` seam 隔离数据库实现；
- 不允许 HTTP handler 直接执行任意 SQL；
- 写入、读取和 Geo enrichment 通过明确的存储接口完成。

未来需要多 Server 实例或更大规模时，再增加 PostgreSQL adapter，不应让 WebUI 或 Agent 感知存储类型。

### 11.2 建议表

```text
agents
- id
- display_name
- token_hash
- status
- agent_version
- protocol_version
- created_at
- last_seen_at
- disabled_at

monitored_nodes
- id
- agent_id
- name
- node_id
- rpc_metadata
- created_at

current_node_snapshots
- monitored_node_id
- sequence
- observed_at
- connected
- payload_json

current_system_snapshots
- agent_id
- sequence
- observed_at
- payload_json

metric_samples
- id
- agent_id
- monitored_node_id
- metric_name
- value
- observed_at

current_peers
- agent_id
- ip
- observed_at

location_cache
- ip
- country
- loc
- lookup_status
- error
- fetched_at
- expires_at

alert_events
- id
- agent_id
- rule_key
- state
- message
- observed_at
- resolved_at

schema_migrations
- version
- applied_at
```

第一版可以将部分复杂快照存为 JSON，常用列表和图表字段使用结构化列。不要在第一版为每个字段设计大量表和 join；先以查询需求为准验证模型。

### 11.3 当前状态和历史状态

两种数据需要分开：

- **Current state**：WebUI 总览和节点详情使用，始终只保留最新值；
- **Metric samples**：图表和历史趋势使用，按 retention policy 清理。

Peer Snapshot 不等同于 Peer 历史：

- `current_peers` 保存最近一次成功合并的 Peer Snapshot；
- 是否保存 Peer 历史需要单独配置，第一阶段默认不保存完整历史；
- `location_cache` 是按 IP 的 enrichment 缓存，不是 Peer Snapshot。

## 12. Geo 处理

Geo 处理迁移到 Server：

1. Agent 每 60 秒查询 `admin_peers`；
2. Agent 上报合并前或已去重的 IP 列表；
3. Server 验证 IP 格式、数量和大小；
4. Server 更新当前 Peer Snapshot；
5. Server 对新 IP 或过期 IP 查询 ipinfo.io；
6. Server 写入 Location Cache；
7. Server 组装 Geo View Snapshot；
8. WebUI 读取 Geo View Snapshot。

与当前实现保持一致的语义：

- 某次采集失败时保留上一次成功的 Peer Snapshot；
- enrichment 失败时保留以前的成功位置；
- 只绘制有合法坐标的 Located Peer；
- UI 不执行 SQL；
- UI 不直接调用 ipinfo.io；
- 不对不可 enrichment 的内网地址进行公开地理查询。

隐私选项：

- Server 默认只向普通 WebUI 用户返回国家和地图点；
- 精确 IP 展示应由权限控制；
- 可以配置是否保存原始 Peer IP；
- 内网 IP、保留地址和无效地址不进入公网 Geo enrichment。

## 13. 告警和通知

Telegram 通知从 Agent 迁移到 Server，原因是：

- Server 可以统一处理多个 Agent；
- 告警状态需要持久化；
- 同一事件可以避免重复通知；
- WebUI 可以显示告警历史和恢复状态。

告警计算输入包括：

- Agent 离线；
- 被监控节点 RPC 断开；
- 区块高度长时间不变化；
- 交易或共识状态异常；
- 磁盘使用率超过阈值；
- 节点排名变化；
- Peer 数量异常。

告警模块应通过 `AlertSink` seam 发送通知，第一版只实现 Telegram adapter，后续可加入 Webhook、邮件等 adapter。

Agent 仍然需要上报采集错误，但不直接执行通知策略。

## 14. 安全设计

### 14.1 Agent 鉴权

- 每个 Agent 独立身份和 Token；
- Server 保存 Token hash，不保存明文；
- enrollment token 一次性使用并设置过期时间；
- Agent 可以被禁用、删除和重新注册；
- 所有上报 API 使用 TLS；
- Agent Token 不能访问 WebUI 管理 API。

### 14.2 输入校验

Server 必须校验：

- Agent ID 是否与认证身份一致；
- JSON body 最大大小；
- 字符串长度；
- 磁盘数量和 Peer 数量上限；
- 数值是否有限且在合理范围；
- 时间戳是否过旧或超前；
- sequence 是否重复或回退；
- IP 是否是合法地址。

### 14.3 权限边界

第一阶段可以只有一个管理员角色，但数据模型应保留未来扩展空间：

```text
User -> Role -> Agent / Monitored Node scope
```

Agent 只允许：

- 写入自己的心跳；
- 写入自己的快照；
- 读取必要的配置响应。

Agent 不允许：

- 查询其他 Agent；
- 读取用户信息；
- 修改告警规则；
- 执行远程命令。

## 15. 可观测性和故障处理

### 15.1 Agent 断线

Agent 使用指数退避，并设置上限：

```text
1s -> 2s -> 4s -> 8s -> ... -> 60s
```

重新连接后：

- 发送最新当前快照；
- Server 根据 sequence 幂等处理；
- Server 更新 `last_seen_at`；
- 生成恢复事件；
- 不要求第一版补发所有历史样本。

### 15.2 Server 重启

- Agent 暂时进入重试状态；
- Server 从 SQLite 恢复当前状态和告警状态；
- Server 重新计算 Agent 在线状态；
- Agent 重新上报最新快照后恢复展示。

### 15.3 Collector 局部失败

不同采集器需要独立状态：

```text
node_rpc: ok / degraded / failed
system: ok / degraded / failed
peers: ok / degraded / failed
```

不能因为 Peer enrichment 失败就丢弃节点状态，也不能因为磁盘采集失败就让 Agent 整体退出。

### 15.4 日志和健康检查

Server 提供：

```text
GET /health/live
GET /health/ready
GET /metrics
```

第一阶段日志仍可使用 `log` crate；后续可以迁移到结构化日志。敏感 Token、RPC URL 中的凭据和 Peer 原始数据不得写入普通日志。

## 16. TUI 兼容策略

TUI 分为两个阶段处理：

### 过渡阶段

继续支持当前运行方式：

```bash
chaindash tui --url mainnet@ws://127.0.0.1:6789
```

TUI 继续使用本地 Collector 和本地 Geo store，但 Collector 的核心输出逐步改为 Core snapshot。

### 后续阶段

TUI 可以成为 Server 的另一个客户端：

```text
TUI -> Server API/WebSocket -> Server state
```

这样 TUI 不再直接连接 PlatON RPC，也不再拥有自己的 Geo 数据库。是否完成这一步取决于 TUI 是否继续维护，不影响 WebUI 主路线。

## 17. 迁移路线

### Phase 0：协议和模型设计

交付：

- Core 领域模型；
- AgentSnapshot 和 Heartbeat DTO；
- 协议版本规则；
- 字段验证规则；
- 序列号和幂等语义；
- 现有 TUI 数据到新模型的映射表。

验收：

- 模型可以独立序列化/反序列化；
- 对异常数值、超长字段和非法时间有测试；
- 不引入 Server 或 WebUI 运行时。

### Phase 1：抽取 Core，保持 TUI 行为不变

交付：

- 将 `ConsensusState`、节点链状态、系统指标、Peer Snapshot 等提取到 Core；
- 用快照或事件替代跨模块直接修改 `SharedData`；
- TUI 通过 adapter 将 Core 数据转换为 Widget 所需状态；
- 保留现有 TUI 命令和测试。

验收：

- 现有 `cargo +nightly fmt -- --check`、`cargo test` 和 `cargo check` 通过；
- TUI 的现有功能没有明显行为变化；
- Core 不依赖 ratatui、crossterm 或 SQLite。

### Phase 2：Agent 本地采集

交付：

- `chaindash agent` 子命令；
- 本地 RPC 采集；
- Unix 系统指标采集；
- Peer 快照采集；
- Agent 配置文件；
- 本地日志和退避重试。

第一阶段 Agent 可以把快照写入本地文件或发送到开发 Server，以便先验证采集链路。

### Phase 3：Server 接收和当前状态

交付：

- `chaindash server` 子命令；
- Agent 注册和 Token 鉴权；
- 心跳和快照 ingest；
- SQLite Server storage adapter；
- Agent/节点总览 API；
- Agent 在线状态。

验收：

- 一个 Agent 可以稳定注册并上报；
- Server 重启后可以恢复；
- 重复上报不会产生重复当前状态；
- Agent 不需要入站端口。

### Phase 4：WebUI 只读版本

交付：

- Agent 总览；
- 被监控节点详情；
- 系统资源；
- 区块和交易状态；
- Peer 地图；
- WebSocket 实时更新。

这一阶段不实现 WebUI 修改配置和告警规则，先验证展示闭环。

### Phase 5：集中 Geo、告警和历史指标

交付：

- Server 侧 Location Cache；
- Geo View Snapshot API；
- Telegram 通知迁移；
- 历史指标和 retention；
- 告警状态、恢复和历史页面。

### Phase 6：生产化

交付：

- Token 轮换；
- 更完整的用户和权限；
- 数据库备份；
- PostgreSQL adapter 评估；
- Agent 自动升级策略；
- 限流、审计和更完善的指标。

## 18. 推荐的代码 seam

为了避免 Server、Agent 和 TUI 互相了解实现细节，建议保留以下 seam：

### `SnapshotCollector`

Agent 的采集器 adapter，实现不同来源的采集：

```rust
trait SnapshotCollector {
    async fn collect(&self) -> Result<CollectorOutput>;
}
```

具体 adapter：

- PlatON RPC collector；
- system collector；
- peer collector。

### `AgentTransport`

隔离上报方式：

```rust
trait AgentTransport {
    async fn send_heartbeat(&self, heartbeat: Heartbeat) -> Result<ServerAck>;
    async fn send_snapshot(&self, snapshot: AgentSnapshot) -> Result<ServerAck>;
}
```

第一版实现为 HTTPS JSON adapter，未来可以增加 WSS adapter。

### `MonitorStore`

Server 的存储接口：

```rust
trait MonitorStore {
    fn register_agent(&self, input: RegisterAgent) -> Result<AgentRecord>;
    fn apply_snapshot(&self, snapshot: AgentSnapshot) -> Result<ApplyResult>;
    fn current_overview(&self) -> Result<OverviewSnapshot>;
}
```

第一版实现为 SQLite adapter，未来可以增加 PostgreSQL adapter。

### `GeoEnricher`

```rust
trait GeoEnricher {
    async fn enrich(&self, ip: &str) -> Result<LocationEntry>;
}
```

第一版实现为 ipinfo adapter，并在 Server 侧使用 Location Cache。

### `AlertSink`

```rust
trait AlertSink {
    async fn send(&self, event: AlertEvent) -> Result<()>;
}
```

第一版实现为 Telegram adapter。

这些 seam 的目标不是预先制造大量抽象，而是把真实会变化的部分隔离：传输、存储、Geo provider 和通知渠道都至少有明确的替换需求。

## 19. 最小可行版本（MVP）

MVP 只实现以下功能：

1. 一个 Server；
2. 多个 Agent；
3. Agent 注册和 Token 鉴权；
4. Agent 上报：
   - 被监控节点连接状态；
   - 当前区块；
   - 交易数量；
   - 共识状态；
   - CPU、内存、磁盘；
   - Peer IP 快照；
5. Server SQLite 当前状态存储；
6. REST 查询 API；
7. WebUI 节点总览和详情；
8. Server 侧 Peer Geo enrichment；
9. Server WebSocket 实时推送；
10. TUI 暂时继续可用。

MVP 暂不实现：

- WebUI 在线修改 Agent 配置；
- 远程控制；
- 复杂用户权限；
- 完整指标历史；
- Agent 历史数据补发；
- 多 Server 高可用。

## 20. 验收标准

### 功能

- Server 能管理至少两个 Agent；
- 每个 Agent 可以关联一个或多个被监控节点；
- Agent 断开后 Server 能在预期时间内标记 Offline；
- Agent 恢复后状态能自动恢复；
- WebUI 能查看节点、系统和 Peer 数据；
- Peer 地图不因单次 Geo 查询失败而清空已有位置。

### 数据一致性

- 重复上报不会重复覆盖或产生重复历史记录；
- 旧 sequence 不能覆盖新快照；
- Server 重启不会丢失 Agent 注册和当前状态；
- Collector 局部失败不会导致其他采集器停止。

### 安全

- Agent 不监听入站公网端口；
- 无有效 Token 不能上报；
- Agent 不能读取其他 Agent 数据；
- 上报 body、数组长度、IP 和时间戳均有校验；
- 日志不泄露 Token 和 RPC 凭据。

### 兼容性

- 迁移期间当前 TUI 仍能启动；
- Core 协议有明确版本号；
- Agent 和 Server 版本不兼容时返回可诊断错误；
- 使用项目要求的 nightly rustfmt，避免 unstable rustfmt 配置造成噪音 diff。

## 21. 未决问题

以下问题在实现前需要单独确认，但不阻塞总体架构：

1. WebUI 使用 React 还是 Vue；
2. Agent 一个进程是否允许关联多个被监控节点；
3. Server 是否需要支持无 Agent 的 Direct Collector；
4. Peer 原始 IP 在 WebUI 中的默认可见范围；
5. 指标历史默认保留 7 天、30 天还是不保存；
6. 是否需要在第一版保存完整 Peer Snapshot 历史；
7. Agent 配置采用 TOML、YAML 还是环境变量为主；
8. 是否把 TUI 最终改造成 Server API 客户端；
9. 第一阶段 Server 是否只支持单管理员；
10. 是否需要 Docker Compose 作为默认部署方式。

## 22. 结论

`Server + Agent + WebUI` 是 chaindash 的可行且推荐的演进方向。

关键原则是：

```text
采集在 Agent
存储、聚合、Geo 和告警在 Server
展示在 WebUI
TUI 在迁移期保留
```

实现时不应直接把 TUI 的 `SharedData`、Widget 或 SQLite 逻辑搬进 Server，而应先建立 Core 快照模型和版本化协议。当前 Collector 可以作为 Agent 采集实现的基础，当前 `PeerGeoStore` 可以作为 Server Geo 存储实现的基础。

最稳妥的第一步是先完成 Phase 0 和 Phase 1：定义协议并抽取 Core，同时保持现有 TUI 可运行。这样可以在不牺牲当前功能的情况下，逐步增加 Agent、Server 和 WebUI。
