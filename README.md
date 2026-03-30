# chaindash

PlatON 节点终端仪表盘，使用 Rust + Ratatui + Crossterm 构建。

## 功能概览

- 实时订阅最新区块，展示区块高度、区块时间与交易数变化
- 展示多个节点的共识状态：`Block / Epoch / View / QC / Locked / Committed / Role`
- 可选拉取指定节点的 PlatON Explorer 详情：排名、产块、奖励比例、收益地址等
- Unix 平台下展示本机 CPU / 内存 / 磁盘 / 网络摘要
- Unix 平台下支持磁盘挂载点自动发现、手动指定挂载点与使用率告警
- 顶部状态栏展示连接成功、重试、接口异常、磁盘告警等运行状态
- 支持多个 WebSocket 端点，断线后自动重连并按顺序切换到可用端点
- 支持整数和分数刷新间隔，例如 `1`、`3/2`、`2/3`
- 适配窄终端的紧凑布局

## 环境要求

- Rust stable
- Cargo
- 可访问的 PlatON WebSocket RPC

可选能力对应的额外要求：

- **节点共识状态**：RPC 端点需要支持 debug 共识状态接口
- **节点详情**：需要可访问的 PlatON Explorer API

> `--url` 仅支持 `ws://` / `wss://`。

## 构建与检查

```bash
cargo build
cargo build --release
cargo check
cargo test
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## 快速开始

### 单节点

```bash
cargo run -- --url main@wss://openapi2.platon.network/rpc
```

### 多节点端点

```bash
cargo run -- \
  --url main@wss://openapi2.platon.network/rpc,backup@ws://127.0.0.1:6789
```

### 查看指定节点详情

```bash
cargo run -- \
  --url main@wss://openapi2.platon.network/rpc \
  --node-id <NODE_ID>
```

### 自定义刷新间隔

```bash
cargo run -- --url main@wss://openapi2.platon.network/rpc --interval 2
cargo run -- --url main@wss://openapi2.platon.network/rpc --interval 3/2
cargo run -- --url main@wss://openapi2.platon.network/rpc --interval 2/3
```

### 使用 Docker 镜像运行

```bash
./run.sh main@wss://openapi2.platon.network/rpc
```

## 参数说明

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `--url <NAME@WS_URL[,NAME@WS_URL...]>` | `test@ws://127.0.0.1:6789` | PlatON WebSocket 端点列表。`NAME` 会显示在 UI 中。 |
| `--interval <RATIO>` | `1` | UI 刷新间隔，支持正整数或正分数。 |
| `--debug` | `false` | 启用调试日志。 |
| `--disk-mount-points <M1,M2,...>` | `/,/opt` | Unix 下手动指定要监控的挂载点列表。 |
| `--disk-auto-discovery` | `false` | Unix 下自动发现挂载点，并与手动指定列表合并。 |
| `--disk-alert-threshold <PERCENT>` | `90.0` | Unix 下磁盘告警阈值，达到或超过该值会高亮并在状态栏提示。 |
| `--disk-refresh-interval <SECONDS>` | `2` | Unix 下系统与磁盘采集间隔。 |
| `--node-id <NODE_ID>` | - | 拉取节点详情时使用的节点 ID。 |
| `--explorer-api-url <URL>` | `https://scan.platon.network/browser-server` | PlatON Explorer API 基础地址。 |

## 界面布局

### Unix 平台

- **状态栏**：显示连接、重试、告警与错误信息
- **第一行**：系统摘要 / 磁盘详情
- **第二行**：区块时间图 / 区块交易数图
- **第三行**：节点状态 / 节点详情

### 非 Unix 平台

- **状态栏**：显示连接、重试、告警与错误信息
- **第一行**：区块时间图 / 区块交易数图
- **第二行**：节点状态 / 节点详情

在较窄终端下，系统摘要、磁盘详情和节点详情会自动切换为紧凑布局。

## 交互

- `q`：退出
- `Ctrl-C`：退出
- `Tab`：切换到下一个磁盘（Unix）
- `Shift-Tab` / `BackTab`：切换到上一个磁盘（Unix）

## 使用说明

### 1. 多端点与 failover

`--url` 支持逗号分隔的多个端点，例如：

```text
main@wss://rpc-a.example,backup@wss://rpc-b.example
```

程序会：

- 为每个端点采集节点状态
- 为区块订阅按顺序尝试连接可用端点
- 在连接中断后自动重试

### 2. 节点详情采集

启用 `--node-id` 后，程序会定期从 Explorer API 拉取：

- 节点名称
- 排名
- 产块数量与产块率
- 24 小时出块表现
- 奖励比例与系统奖励
- 收益地址与预计收益

如果未传入 `--node-id`，程序不会启动节点详情采集，右下角详情面板会保持 `Loading...`。

### 3. Unix 磁盘监控

Unix 平台下支持两种挂载点来源：

- `--disk-mount-points`：手动指定
- `--disk-auto-discovery`：自动发现

自动发现开启后，会将自动发现结果与手动指定列表合并后监控。

当任一磁盘使用率达到 `--disk-alert-threshold` 时：

- 系统摘要中的磁盘使用率会高亮
- 磁盘详情会标记告警项
- 状态栏会显示告警消息

## 日志

默认日志文件：

```text
./errors.log
```

使用 `--debug` 可输出更详细的调试日志。

## 注意事项

- `--url` 必须使用 `NAME@WS_URL` 格式
- 仅支持 `ws://` 和 `wss://` 端点
- 非 Unix 平台不会显示系统摘要和磁盘详情
- `--interval` 必须大于 `0`
- 节点状态采集、区块订阅与节点详情采集彼此独立；某一项失败时会通过状态栏和日志提示
