use std::sync::{
    atomic::{
        AtomicBool,
        Ordering,
    },
    Arc,
};

use alloy::providers::{
    ext::DebugApi,
    Provider,
    ProviderBuilder,
    WsConnect,
};
use tokio::time::{
    self,
    Duration,
};

use super::data::{
    warn_with_status,
    ConsensusState,
    SharedData,
};
use crate::{
    error::Result,
    notify::TelegramNotifier,
    sync::lock_or_panic,
};

fn websocket_host(url: &str) -> String {
    url.trim_start_matches("ws://").trim_start_matches("wss://").to_string()
}

pub(crate) async fn collect_node_state(
    name: String,
    url: String,
    data: SharedData,
    notifier: Option<Arc<TelegramNotifier>>,
    stop_flag: Arc<AtomicBool>,
    retry_delay: Duration,
) -> Result<()> {
    let host = websocket_host(&url);

    while !stop_flag.load(Ordering::Relaxed) {
        let ws = WsConnect::new(url.as_str());
        let provider = match ProviderBuilder::new().connect_ws(ws).await {
            Ok(provider) => provider,
            Err(err) => {
                if let Some(notifier) = notifier.as_ref() {
                    notifier
                        .notify_node_connection_failed(
                            &name,
                            &url,
                            &format!("建立 WebSocket 连接失败: {err}"),
                        )
                        .await;
                }
                warn_with_status(
                    &data,
                    format!(
                        "Failed to connect node state collector for {} at {}: {}",
                        name, url, err
                    ),
                );
                time::sleep(retry_delay).await;
                continue;
            },
        };
        let mut interval = time::interval(Duration::from_secs(1));

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                return Ok(());
            }

            interval.tick().await;

            let status = match provider.debug_consensus_status().await {
                Ok(status) => status,
                Err(err) => {
                    if let Some(notifier) = notifier.as_ref() {
                        notifier
                            .notify_node_connection_failed(
                                &name,
                                &url,
                                &format!("debug_consensus_status 调用失败: {err}"),
                            )
                            .await;
                    }
                    warn_with_status(
                        &data,
                        format!("Node state RPC failed for {}: {}. Reconnecting soon", name, err),
                    );
                    break;
                },
            };
            let cur_number = match provider.get_block_number().await {
                Ok(cur_number) => cur_number,
                Err(err) => {
                    if let Some(notifier) = notifier.as_ref() {
                        notifier
                            .notify_node_connection_failed(
                                &name,
                                &url,
                                &format!("eth_blockNumber 调用失败: {err}"),
                            )
                            .await;
                    }
                    warn_with_status(
                        &data,
                        format!(
                            "Node block number RPC failed for {}: {}. Reconnecting soon",
                            name, err
                        ),
                    );
                    break;
                },
            };
            let epoch = status.state.view.as_ref().map(|v| v.epoch).unwrap_or(0);
            let view = status.state.view.as_ref().and_then(|v| v.view_number).unwrap_or(0);
            let committed =
                status.state.highest_commit_block.as_ref().map(|b| b.number).unwrap_or(0);
            let locked = status.state.highest_lock_block.as_ref().map(|b| b.number).unwrap_or(0);
            let qc = status.state.highest_qc_block.as_ref().map(|b| b.number).unwrap_or(0);
            let validator = status.validator;

            let node = ConsensusState {
                name: name.clone(),
                host: host.clone(),
                current_number: cur_number,
                epoch,
                view,
                committed,
                locked,
                qc,
                validator,
            };

            {
                let mut data = lock_or_panic(&data);
                data.update_consensus_state(name.clone(), node);
            }

            if let Some(notifier) = notifier.as_ref() {
                notifier.notify_node_connection_recovered(&name, &url).await;
            }
        }

        time::sleep(retry_delay).await;
    }

    Ok(())
}
