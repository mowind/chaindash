# chaindash

PlatON 节点终端仪表盘，使用 Rust + Ratatui + Crossterm 构建。

## 功能概览

- 实时订阅最新区块，展示区块高度、区块时间与交易数变化
- 展示多个节点的共识状态：`Block / Epoch / View / QC / Locked / Committed / Role`
- 可选拉取一个或多个节点的 PlatON Explorer 详情：排名、产块、奖励比例、收益地址等
- Unix 平台下展示本机 CPU / 内存 / 磁盘 / 网络摘要
- Unix 平台下支持磁盘挂载点自动发现、手动指定挂载点与使用率告警
- 顶部状态栏展示连接成功、重试、接口异常、磁盘告警等运行状态
- 可选通过 Telegram Bot 推送节点连接失败 / 恢复、节点排名变化、每日节点快照通知，并支持静默时间段、静默期摘要与限流防刷屏
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
- **Telegram 通知**：需要可用的 Telegram Bot Token 和一个或多个 Chat ID

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

### 查看多个节点详情

```bash
cargo run -- \
  --url main@wss://openapi2.platon.network/rpc \
  --node-id <NODE_ID_A>,<NODE_ID_B>
```

### 自定义刷新间隔

```bash
cargo run -- --url main@wss://openapi2.platon.network/rpc --interval 2
cargo run -- --url main@wss://openapi2.platon.network/rpc --interval 3/2
cargo run -- --url main@wss://openapi2.platon.network/rpc --interval 2/3
```

### 启用 Telegram 通知

```bash
cargo run -- \
  --url main@wss://openapi2.platon.network/rpc \
  --node-id <NODE_ID> \
  --telegram-bot-token <BOT_TOKEN> \
  --telegram-chat-id <CHAT_ID_A>,<CHAT_ID_B> \
  --telegram-notify-events connection,ranking-changed \
  --telegram-quiet-hours 23:00-08:00 \
  --telegram-rate-limit-seconds 120 \
  --telegram-template-ranking-changed "{icon} {node} {previous}->{current} ({delta_text})" \
  --telegram-template-daily-summary "{title}（{date}）\n🧾 节点数：{count}\n{details}"
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
| `--node-id <NODE_ID[,NODE_ID...]>` | - | 拉取节点详情时使用的节点 ID 列表，支持逗号分隔多个节点。 |
| `--explorer-api-url <URL>` | `https://scan.platon.network/browser-server` | PlatON Explorer API 基础地址。 |
| `--telegram-bot-token <TOKEN>` | - | Telegram Bot Token。与 `--telegram-chat-id` 一起使用时启用通知。 |
| `--telegram-chat-id <CHAT_ID[,CHAT_ID...]>` | - | Telegram Chat ID 列表，支持逗号分隔多个接收方。 |
| `--telegram-notify-events <EVENT[,EVENT...]>` | 全部事件 | Telegram 通知事件过滤。支持：`all`、`connection`、`connection-failed`、`connection-recovered`、`ranking`、`ranking-changed`、`daily`、`daily-summary`。 |
| `--telegram-quiet-hours <HH:MM-HH:MM>` | - | Telegram 通知静默时间段，使用本地时间，例如 `23:00-08:00`。 |
| `--telegram-rate-limit-seconds <SECONDS>` | `0` | 同一事件键的最小通知间隔，`0` 表示不限制。 |
| `--telegram-template-connection-failed <TEMPLATE>` | 默认模板 | 连接失败通知模板。支持占位符：`{prefix}`、`{node}`、`{reason}`。 |
| `--telegram-template-connection-recovered <TEMPLATE>` | 默认模板 | 连接恢复通知模板。支持占位符：`{prefix}`、`{node}`。 |
| `--telegram-template-ranking-changed <TEMPLATE>` | 默认模板 | 排名变化通知模板。支持占位符：`{prefix}`、`{icon}`、`{node}`、`{previous}`、`{current}`、`{delta}`、`{delta_text}`、`{direction}`。 |
| `--telegram-template-quiet-summary <TEMPLATE>` | 默认模板 | 静默期摘要模板。支持占位符：`{prefix}`、`{count}`、`{details}`。可用 `\n` 表示换行。 |
| `--telegram-template-daily-summary <TEMPLATE>` | 默认模板 | 每日节点快照模板。支持占位符：`{prefix}`、`{title}`、`{date}`、`{count}`、`{details}`。可用 `\n` 表示换行。 |
| `--telegram-api-url <URL>` | `https://api.telegram.org` | Telegram Bot API 基础地址。 |

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

启用 `--node-id` 后，程序会定期从 Explorer API 拉取一个或多个节点的：

- 节点名称
- 排名
- 产块数量与产块率
- 24 小时出块表现
- 奖励比例与系统奖励
- 收益地址与预计收益

如果未传入 `--node-id`，程序不会启动节点详情采集，右下角详情面板会保持 `Loading...`。当只配置一个节点时，面板会展示详细卡片；配置多个节点时，会切换为汇总表格。

### 3. Telegram 通知

同时配置 `--telegram-bot-token` 和 `--telegram-chat-id` 后，会启用 Telegram 推送，当前支持：

- 节点连接失败通知
- 节点连接恢复通知
- `--node-id` 对应节点的排名变化通知
- 每日 0 点按本地时间精确调度推送当前节点累计出块数量、累计系统奖励，以及基于前一日快照计算的当天出块数和当天系统奖励；每月 1 号的日报会额外统计上一自然月总出块数量和总系统奖励

支持使用 `--telegram-notify-events` 过滤通知事件，例如：

- `--telegram-notify-events connection`：仅发送连接失败 / 恢复通知
- `--telegram-notify-events connection-failed`：仅发送连接失败通知
- `--telegram-notify-events ranking-changed`：仅发送排名变化通知
- `--telegram-notify-events daily-summary`：仅发送每日节点快照通知

`--telegram-chat-id` 支持配置多个 chat id，程序会向每个接收方分别推送同一条通知。

`--telegram-quiet-hours` 可配置本地时间静默窗口；落在该时间段内的通知会被缓存。静默结束后的下一次通知机会，会先发送一条静默期摘要。

> `daily-summary` 为保证每日推送，会忽略静默时间段设置。

`--telegram-rate-limit-seconds` 可限制相同事件键的发送频率，例如同一节点的排名变化、同一节点的连接失败 / 恢复通知，避免短时间内频繁刷屏。

支持通过模板参数自定义通知文案，例如：

- `--telegram-template-connection-failed "🚨 节点连接异常\\n🔹 节点：{node}\\n📝 原因：{reason}"`
- `--telegram-template-ranking-changed "{icon} 节点排名变动\\n🔹 节点：{node}\\n📍 排名：{previous} → {current}（{delta_text}）"`
- `--telegram-template-quiet-summary "🌙 静默期摘要\\n🧾 共 {count} 条\\n{details}"`
- `--telegram-template-daily-summary "{title}（{date}）\\n🧾 节点数：{count}\\n{details}"`

每日节点快照会在本地时间 00:00 精确调度发送，使用当时缓存中的最新节点详情数据。程序会持久化最近的每日节点快照，并在次日对比前一日快照，计算当天出块数与当天系统奖励；如果缺少前一日快照，则对应字段显示为 `-`。当日报日期为每月 1 号时，还会额外对比上一个自然月首日快照，统计上一自然月总出块数量和总系统奖励；如果缺少该月首日快照，则对应月度字段显示为 `-`。

其中：

- `{prefix}` 可用于自定义前缀；默认模板已不再使用该占位符
- `{title}` 为日报标题，默认会根据日期自动渲染为 `📊 每日节点快照` 或 `📅 月度节点简报`
- `{delta}` 是纯数字变化量，例如 `2`
- `{delta_text}` 带正负号，例如 `+2` / `-3`
- `{direction}` 为 `up` / `down`
- `{date}` 为每日快照日期，例如 `2026-04-14`
- `{count}` 为本次快照包含的节点数量，例如 `2`
- `{details}` 为逐节点详情列表，例如：
  ```text
  🔹 验证节点A
    🧱 累计出块：123
    💰 累计系统奖励：45.6
    📅 当天出块：12
    🎁 当天系统奖励：5.6
    🗓️ 上月总出块：300
    🏆 上月总系统奖励：30
  ```

> 排名变化通知和每日节点快照都依赖节点详情采集，因此需要同时配置 `--node-id`。

### 4. Unix 磁盘监控

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
- Telegram 通知仅在同时配置 `--telegram-bot-token` 和至少一个 `--telegram-chat-id` 时启用
