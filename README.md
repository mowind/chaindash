# chaindash

PlatON 节点终端仪表盘，使用 Rust + Ratatui 构建。

## 功能

- 实时订阅区块高度、区块时间、交易数
- 展示节点共识状态
- 展示指定节点的 Explorer 详情信息
- Unix 平台下展示本机 CPU / 内存 / 磁盘 / 网络摘要
- 支持多个节点端点 failover
- 支持分数刷新间隔，如 `1/2`、`3/2`
- 适配窄终端的紧凑布局

## 环境要求

- Rust stable
- Cargo
- 可访问的 PlatON WebSocket RPC

> `--url` 仅支持 `ws://` / `wss://`。

## 构建

```bash
cargo build
cargo build --release
cargo check
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## 测试

```bash
cargo test
cargo test -- --nocapture
```

## 运行

### 单节点

```bash
cargo run -- --url main@wss://openapi2.platon.network/rpc
```

### 多节点

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
cargo run -- --url main@wss://openapi2.platon.network/rpc --interval 1/2
```

## 常用参数

```text
--url <NAME@WS_URL[,NAME@WS_URL...]>
    PlatON WebSocket 端点列表

--interval <RATIO>
    UI 刷新间隔，支持整数和分数，必须大于 0

--node-id <NODE_ID>
    拉取 Explorer 节点详情时使用的节点 ID

--explorer-api-url <URL>
    PlatON Explorer API 地址

--disk-mount-points <MOUNTS>
    Unix 下监控的挂载点列表，逗号分隔

--disk-auto-discovery
    Unix 下启用自动发现挂载点

--debug
    启用调试日志
```

## 交互

- `q`：退出
- `Ctrl-C`：退出
- `Tab`：切换磁盘
- `Shift-Tab` / `BackTab`：切换到上一个磁盘

## 布局说明

- 顶部：系统摘要 / 磁盘摘要（Unix）
- 中上：区块时间图、区块交易数图
- 中部：节点状态表
- 底部：节点详情

在较窄终端下，部分组件会自动切换为紧凑布局。

## 日志

默认日志文件：

```text
./errors.log
```

使用 `--debug` 可输出更多调试信息。

## 说明

- Explorer 节点详情依赖 `--node-id`
- 未配置 `--node-id` 时，节点详情区域仅显示空状态
- 磁盘与系统信息只在 Unix 平台启用
