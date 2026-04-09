use std::sync::{
    atomic::{
        AtomicBool,
        Ordering,
    },
    Arc,
};

use alloy::{
    eips::BlockNumberOrTag,
    providers::{
        Provider,
        ProviderBuilder,
        WsConnect,
    },
};
use futures::StreamExt;
use tokio::time::{
    self,
    Duration,
};

use super::data::{
    record_status_message,
    warn_with_status,
    SharedData,
    StatusLevel,
};
use crate::{
    error::Result,
    sync::lock_or_panic,
};

pub(crate) fn is_websocket_endpoint(url: &str) -> bool {
    url.starts_with("ws://") || url.starts_with("wss://")
}

pub(crate) async fn run_block_subscription_loop(
    urls: &[(String, String)],
    data: &SharedData,
    stop_flag: &Arc<AtomicBool>,
    retry_delay: Duration,
) -> Result<()> {
    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        let mut connection = None;
        for (name, url) in urls {
            let ws = WsConnect::new(url.as_str());
            let provider = match ProviderBuilder::new().connect_ws(ws).await {
                Ok(provider) => provider,
                Err(err) => {
                    warn_with_status(
                        data,
                        format!(
                            "Failed to connect block subscription for {} at {}: {}",
                            name, url, err
                        ),
                    );
                    continue;
                },
            };

            let sub = match provider.subscribe_blocks().await {
                Ok(sub) => sub,
                Err(err) => {
                    warn_with_status(
                        data,
                        format!("Failed to subscribe to blocks for {} at {}: {}", name, url, err),
                    );
                    continue;
                },
            };

            record_status_message(
                data,
                StatusLevel::Info,
                format!("Block subscription connected via {}", name),
            );
            connection = Some((name.clone(), provider, sub.into_stream()));
            break;
        }

        let Some((endpoint_name, provider, mut sub)) = connection else {
            time::sleep(retry_delay).await;
            continue;
        };

        let mut reconnect_required = false;
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            tokio::select! {
                maybe_head = sub.next() => {
                    let Some(head) = maybe_head else {
                        warn_with_status(
                            data,
                            format!(
                                "Block subscription stream ended for {}. Reconnecting soon",
                                endpoint_name
                            ),
                        );
                        reconnect_required = true;
                        break;
                    };

                    let number = BlockNumberOrTag::Number(head.number);
                    let block = match provider.get_block_by_number(number).full().await {
                        Ok(block) => block,
                        Err(err) => {
                            warn_with_status(
                                data,
                                format!(
                                    "Failed to fetch block {} via {}: {}. Reconnecting soon",
                                    head.number, endpoint_name, err
                                ),
                            );
                            reconnect_required = true;
                            break;
                        },
                    };
                    let txs = block.map(|b| b.transactions.len() as u64).unwrap_or(0);

                    let mut data = lock_or_panic(data);
                    data.record_block_sample(head.number, head.timestamp, txs);
                }
                _ = time::sleep(retry_delay) => {
                    if stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }
        }

        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        if reconnect_required {
            time::sleep(retry_delay).await;
        }
    }

    Ok(())
}
