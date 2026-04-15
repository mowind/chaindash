use std::{
    convert::TryFrom,
    sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        Arc,
    },
    time::Instant,
};

use chrono::{
    Local,
    LocalResult,
    NaiveDate,
    TimeZone,
};
use log::{
    debug,
    warn,
};
use tokio::time::{
    self,
    Duration,
    Instant as TokioInstant,
};

use super::{
    daily_snapshot::DailyNodeSnapshotStore,
    data::{
        record_status_message,
        warn_with_status,
        NodeDetail,
        SharedData,
        StatusLevel,
    },
    types::{
        self,
        NodeInfo,
    },
};
use crate::{
    error::Result,
    notify::TelegramNotifier,
    sync::lock_or_panic,
};

const NODE_DETAIL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const NODE_RANKING_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const NODE_DETAIL_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const NODE_DETAIL_STATUS_PREVIEW_COUNT: usize = 3;
const DAILY_SUMMARY_STOP_POLL_INTERVAL: Duration = Duration::from_secs(1);

struct DailySummarySchedule {
    date: NaiveDate,
    deadline: TokioInstant,
}

fn next_local_midnight_after(now: &chrono::DateTime<Local>) -> chrono::DateTime<Local> {
    let next_date = now.date_naive().succ_opt().unwrap_or(now.date_naive());
    let mut candidate = next_date.and_hms_opt(0, 0, 0).expect("midnight should be valid");

    loop {
        match Local.from_local_datetime(&candidate) {
            LocalResult::Single(datetime) => return datetime,
            LocalResult::Ambiguous(first, _) => return first,
            LocalResult::None => {
                candidate += chrono::Duration::minutes(1);
            },
        }
    }
}

fn next_daily_summary_schedule_from(
    now: &chrono::DateTime<Local>,
    now_instant: TokioInstant,
) -> DailySummarySchedule {
    let next_midnight = next_local_midnight_after(now);
    let deadline_offset = next_midnight
        .signed_duration_since(*now)
        .to_std()
        .unwrap_or_else(|_| Duration::from_secs(1));
    let deadline_offset = if deadline_offset.is_zero() {
        Duration::from_secs(1)
    } else {
        deadline_offset
    };

    DailySummarySchedule {
        date: next_midnight.date_naive(),
        deadline: now_instant + deadline_offset,
    }
}

fn next_daily_summary_schedule() -> DailySummarySchedule {
    let now = Local::now();

    next_daily_summary_schedule_from(&now, TokioInstant::now())
}

fn summarize_node_detail_failures(node_ids: &[String]) -> String {
    if node_ids.is_empty() {
        return "Node details unavailable".to_string();
    }

    if node_ids.len() == 1 {
        return format!("Node detail unavailable for {}", node_ids[0]);
    }

    let preview: Vec<&str> =
        node_ids.iter().take(NODE_DETAIL_STATUS_PREVIEW_COUNT).map(String::as_str).collect();
    let preview = preview.join(", ");
    let remaining = node_ids.len().saturating_sub(NODE_DETAIL_STATUS_PREVIEW_COUNT);

    if remaining == 0 {
        format!("Node details unavailable for {} node(s): {}", node_ids.len(), preview)
    } else {
        format!(
            "Node details unavailable for {} node(s): {}, +{} more",
            node_ids.len(),
            preview,
            remaining
        )
    }
}

pub(crate) async fn collect_node_details(
    node_ids: Vec<String>,
    data: SharedData,
    explorer_api_url: String,
    notifier: Option<Arc<TelegramNotifier>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let detail_url = format!("{explorer_api_url}/staking/stakingDetails");
    let ranking_url = format!("{explorer_api_url}/staking/aliveStakingList");

    if let Some(daily_summary_notifier) = notifier.clone() {
        tokio::join!(
            run_node_detail_refresh_loop(
                client,
                detail_url,
                ranking_url,
                node_ids,
                data.clone(),
                notifier,
                stop_flag.clone(),
            ),
            run_daily_summary_loop(data, daily_summary_notifier, stop_flag),
        );
    } else {
        run_node_detail_refresh_loop(
            client,
            detail_url,
            ranking_url,
            node_ids,
            data,
            notifier,
            stop_flag,
        )
        .await;
    }

    Ok(())
}

async fn run_node_detail_refresh_loop(
    client: reqwest::Client,
    detail_url: String,
    ranking_url: String,
    node_ids: Vec<String>,
    data: SharedData,
    notifier: Option<Arc<TelegramNotifier>>,
    stop_flag: Arc<AtomicBool>,
) {
    let mut interval = time::interval(NODE_DETAIL_REFRESH_INTERVAL);

    fetch_all_node_details(&client, &detail_url, &node_ids, data.clone()).await;
    fetch_node_rankings(&client, &ranking_url, &node_ids, data.clone(), notifier.clone()).await;
    interval.tick().await;

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        interval.tick().await;

        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        fetch_all_node_details(&client, &detail_url, &node_ids, data.clone()).await;
        fetch_node_rankings(&client, &ranking_url, &node_ids, data.clone(), notifier.clone()).await;
    }
}

async fn run_daily_summary_loop(
    data: SharedData,
    notifier: Arc<TelegramNotifier>,
    stop_flag: Arc<AtomicBool>,
) {
    let mut schedule = next_daily_summary_schedule();

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        let stop_poll = time::sleep(DAILY_SUMMARY_STOP_POLL_INTERVAL);
        let daily_summary_sleep = time::sleep_until(schedule.deadline);
        tokio::pin!(stop_poll);
        tokio::pin!(daily_summary_sleep);

        tokio::select! {
            biased;
            _ = &mut daily_summary_sleep => {
                send_daily_summary(&data, &notifier, schedule.date).await;
                schedule = next_daily_summary_schedule();
            }
            _ = &mut stop_poll => {},
        }
    }
}

async fn send_daily_summary(
    data: &SharedData,
    notifier: &Arc<TelegramNotifier>,
    scheduled_date: NaiveDate,
) {
    let node_details = {
        let data = lock_or_panic(data);
        data.node_details()
    };
    let snapshot_store = DailyNodeSnapshotStore::default();
    let daily_summary_details = snapshot_store.daily_summary_details(scheduled_date, &node_details);

    if let Err(err) = snapshot_store.save_snapshot(scheduled_date, &node_details) {
        warn_with_status(
            data,
            format!("Failed to persist daily node snapshot for {}: {}", scheduled_date, err),
        );
    }

    notifier.notify_daily_node_snapshot(&scheduled_date.to_string(), &daily_summary_details).await;
}

async fn fetch_all_node_details(
    client: &reqwest::Client,
    url: &str,
    node_ids: &[String],
    data: SharedData,
) {
    let requests = node_ids.iter().map(|node_id| {
        let data = data.clone();
        async move {
            match tokio::time::timeout(
                NODE_DETAIL_REQUEST_TIMEOUT,
                fetch_node_detail(client, url, node_id, data.clone()),
            )
            .await
            {
                Ok(Ok(())) => None,
                Ok(Err(message)) => {
                    warn!("{message}");
                    let mut data = lock_or_panic(&data);
                    data.remove_node_detail(node_id);
                    Some(node_id.clone())
                },
                Err(err) => {
                    warn!(
                        "Node detail request timed out after {:?} for {}: {}",
                        NODE_DETAIL_REQUEST_TIMEOUT, node_id, err
                    );
                    let mut data = lock_or_panic(&data);
                    data.remove_node_detail(node_id);
                    Some(node_id.clone())
                },
            }
        }
    });

    let failures: Vec<String> =
        futures::future::join_all(requests).await.into_iter().flatten().collect();

    {
        let mut data = lock_or_panic(&data);
        data.mark_node_details_loaded();
    }

    if !failures.is_empty() {
        record_status_message(&data, StatusLevel::Warn, summarize_node_detail_failures(&failures));
    }
}

async fn fetch_node_rankings(
    client: &reqwest::Client,
    url: &str,
    node_ids: &[String],
    data: SharedData,
    notifier: Option<Arc<TelegramNotifier>>,
) {
    match tokio::time::timeout(
        NODE_RANKING_REQUEST_TIMEOUT,
        fetch_node_rankings_once(client, url, node_ids, data.clone(), notifier.clone()),
    )
    .await
    {
        Ok(()) => {},
        Err(err) => {
            warn_with_status(
                &data,
                format!(
                    "Node ranking request timed out after {:?}: {}",
                    NODE_RANKING_REQUEST_TIMEOUT, err
                ),
            );
        },
    }
}

async fn fetch_node_rankings_once(
    client: &reqwest::Client,
    url: &str,
    node_ids: &[String],
    data: SharedData,
    notifier: Option<Arc<TelegramNotifier>>,
) {
    let body = serde_json::json!({
        "pageNo": 1,
        "pageSize": 300,
        "key": "",
        "queryStatus": "all",
    });

    debug!("fetch node ranking: {}", url);

    match client.post(url).header("content-type", "application/json").json(&body).send().await {
        Ok(resp) => {
            debug!("Reponse: {}", resp.status());
            if !resp.status().is_success() {
                warn_with_status(
                    &data,
                    format!("Node ranking API returned error status: {}", resp.status()),
                );
                return;
            }
            let body_bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn_with_status(&data, format!("Failed to read response body: {}", e));
                    return;
                },
            };
            let node_list_resp: types::NodeListResponse = match serde_json::from_slice(&body_bytes)
            {
                Ok(node_list_resp) => node_list_resp,
                Err(e) => {
                    warn_with_status(&data, format!("Failed to parse response JSON: {}", e));
                    return;
                },
            };
            debug!("Node list response: {:?}", node_list_resp);

            if node_list_resp.code == 0 {
                if let Some(data_obj) = node_list_resp.data {
                    let ranking_observations = {
                        let mut data = lock_or_panic(&data);
                        let mut ranking_observations = Vec::new();

                        for node_id in node_ids {
                            let ranking = parse_node_ranking(&data_obj, node_id);
                            data.merge_node_ranking_for(node_id, ranking);

                            let Some(ranking) = ranking.filter(|ranking| *ranking > 0) else {
                                continue;
                            };

                            let node_name = data
                                .node_detail_for(node_id)
                                .map(|detail| detail.node_name)
                                .unwrap_or_default();

                            ranking_observations.push((node_id.clone(), node_name, ranking));
                        }

                        ranking_observations
                    };

                    if let Some(notifier) = notifier.as_ref() {
                        for (node_id, node_name, ranking) in ranking_observations {
                            notifier
                                .notify_node_ranking_change(&node_id, &node_name, ranking)
                                .await;
                        }
                    }
                } else {
                    warn_with_status(&data, "Node ranking response missing data field");
                }
            } else {
                warn_with_status(
                    &data,
                    format!(
                        "Node ranking API returned error code: {}, err_msg: {}",
                        node_list_resp.code, node_list_resp.err_msg
                    ),
                );
            }
        },
        Err(e) => {
            warn_with_status(&data, format!("Failed to fetch node ranking: {}", e));
        },
    }
}

async fn fetch_node_detail(
    client: &reqwest::Client,
    url: &str,
    node_id: &str,
    data: SharedData,
) -> std::result::Result<(), String> {
    let body = serde_json::json!({
        "nodeId": node_id
    });

    debug!("fetch node detail: {}", url);

    match client.post(url).header("content-type", "application/json").json(&body).send().await {
        Ok(resp) => {
            debug!("Reponse: {}", resp.status());
            if !resp.status().is_success() {
                return Err(format!(
                    "Node detail API returned error status for {}: {}",
                    node_id,
                    resp.status()
                ));
            }
            let body_bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Err(format!(
                        "Failed to read node detail response body for {}: {}",
                        node_id, e
                    ));
                },
            };
            let node_detail_resp: types::NodeDetailResponse =
                match serde_json::from_slice(&body_bytes) {
                    Ok(node_detail_resp) => node_detail_resp,
                    Err(e) => {
                        return Err(format!(
                            "Failed to parse node detail JSON for {}: {}",
                            node_id, e
                        ));
                    },
                };
            debug!("Node detail response: {:?}", node_detail_resp);

            if node_detail_resp.code == 0 {
                if let Some(detail) = node_detail_resp.data {
                    let node_detail = parse_node_detail(node_id, &detail);
                    let mut data = lock_or_panic(&data);
                    data.merge_node_detail_for(node_id, Some(node_detail));
                } else {
                    return Err(format!("Node detail response missing data field for {}", node_id));
                }
            } else {
                return Err(format!(
                    "Node detail API returned error code for {}: {}, err_msg: {}",
                    node_id, node_detail_resp.code, node_detail_resp.err_msg
                ));
            }
        },
        Err(e) => {
            return Err(format!("Failed to fetch node detail for {}: {}", node_id, e));
        },
    }

    Ok(())
}

fn parse_node_detail(
    node_id: &str,
    node_detail: &types::NodeDetail,
) -> NodeDetail {
    let node_name = node_detail.node_name.clone();
    let block_qty = u64::try_from(node_detail.block_qty).unwrap_or(0);
    let expect_block_qty = u64::try_from(node_detail.expect_block_qty).unwrap_or(0);
    let block_rate = if expect_block_qty > 0 {
        let rate = block_qty as f64 / expect_block_qty as f64;
        format!("{:.2}%", rate * 100.0)
    } else {
        "0.00%".to_string()
    };
    let daily_block_rate = node_detail.gen_blocks_rate.clone();
    let reward_per = node_detail.reward_per.parse::<f64>().ok().unwrap_or(0.0);
    let reward_value = node_detail.reward_value.parse::<f64>().ok().unwrap_or(0.0);
    let reward_address = node_detail.benefit_addr.clone();
    let verifier_time = u64::try_from(node_detail.verifier_time).unwrap_or(0);

    NodeDetail {
        node_id: node_id.to_string(),
        node_name,
        ranking: 0,
        block_qty,
        block_rate,
        daily_block_rate,
        reward_per,
        reward_value,
        reward_address,
        verifier_time,
        last_updated_at: Some(Instant::now()),
    }
}

fn parse_node_ranking(
    data: &[NodeInfo],
    node_id: &str,
) -> Option<i32> {
    data.iter()
        .find(|node| node.node_id == node_id)
        .and_then(|node| i32::try_from(node.ranking).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_node_ranking_missing_returns_none() {
        let ranking = parse_node_ranking(&[], "missing-node");

        assert_eq!(ranking, None);
    }

    #[test]
    fn test_summarize_node_detail_failures_for_single_node() {
        let summary = summarize_node_detail_failures(&["node-a".to_string()]);

        assert_eq!(summary, "Node detail unavailable for node-a");
    }

    #[test]
    fn test_next_daily_summary_schedule_is_in_the_future() {
        let now = Local::now();
        let now_instant = TokioInstant::now();
        let schedule = next_daily_summary_schedule_from(&now, now_instant);

        assert_eq!(schedule.date, next_local_midnight_after(&now).date_naive());
        assert!(schedule.deadline > now_instant);
        assert!(schedule.deadline <= now_instant + Duration::from_secs(25 * 60 * 60));
    }

    #[test]
    fn test_summarize_node_detail_failures_truncates_long_lists() {
        let summary = summarize_node_detail_failures(&[
            "node-a".to_string(),
            "node-b".to_string(),
            "node-c".to_string(),
            "node-d".to_string(),
        ]);

        assert_eq!(
            summary,
            "Node details unavailable for 4 node(s): node-a, node-b, node-c, +1 more"
        );
    }

    #[test]
    fn test_parse_node_detail_clamps_negative_values() {
        let parsed = parse_node_detail(
            "node-a-id",
            &types::NodeDetail {
                node_name: "node-a".to_string(),
                total_value: "0".to_string(),
                delegate_value: "0".to_string(),
                delegate_qty: 0,
                block_qty: -1,
                expect_block_qty: -10,
                gen_blocks_rate: "1/day".to_string(),
                reward_per: "10".to_string(),
                reward_value: "20".to_string(),
                benefit_addr: "addr".to_string(),
                verifier_time: -5,
            },
        );

        assert_eq!(parsed.block_qty, 0);
        assert_eq!(parsed.block_rate, "0.00%");
        assert_eq!(parsed.verifier_time, 0);
    }
}
