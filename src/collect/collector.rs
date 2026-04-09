use std::sync::{
    atomic::{
        AtomicBool,
        Ordering,
    },
    Arc,
};

use log::{
    debug,
    warn,
};
use tokio::{
    task::JoinSet,
    time::Duration,
};

#[cfg(target_family = "unix")]
use super::system_stats::collect_system_stats;
use super::{
    block_subscription::{
        is_websocket_endpoint,
        run_block_subscription_loop,
    },
    data::SharedData,
    node_detail::collect_node_details,
    node_state::collect_node_state,
};
use crate::{
    error::{
        ChaindashError,
        Result,
    },
    opts::Opts,
};

const COLLECTOR_RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct Collector {
    data: SharedData,
    urls: Vec<(String, String)>,
    disk_mount_points: Vec<String>,
    disk_auto_discovery: bool,
    disk_alert_threshold: f32,
    disk_refresh_interval: u64,
    node_ids: Vec<String>,
    explorer_api_url: String,
    stop_flag: Arc<AtomicBool>,
}

pub async fn run(collector: Arc<Collector>) -> Result<()> {
    collector.run().await
}

impl Collector {
    pub fn new(
        opts: &Opts,
        data: SharedData,
    ) -> Result<Self> {
        let urls: Vec<&str> = opts.url.as_str().split(',').collect();
        let urls: Vec<(String, String)> = urls
            .into_iter()
            .map(|url: &str| {
                let Some((name, endpoint)) = url.split_once('@') else {
                    return Err(format!("invalid url format: {url}").into());
                };
                if !is_websocket_endpoint(endpoint) {
                    return Err(ChaindashError::Other(format!(
                        "invalid websocket url for {name}: {endpoint}",
                    )));
                }
                Ok((name.into(), endpoint.into()))
            })
            .collect::<Result<Vec<_>>>()?;
        let disk_mount_points = opts.disk_mount_points.clone();
        let disk_auto_discovery = opts.disk_auto_discovery;
        let disk_alert_threshold = opts.disk_alert_threshold;
        let disk_refresh_interval = opts.disk_refresh_interval;
        let mut node_ids = Vec::new();
        for node_id in &opts.node_id {
            let node_id = node_id.trim();
            if !node_id.is_empty() && !node_ids.iter().any(|existing| existing == node_id) {
                node_ids.push(node_id.to_string());
            }
        }
        let explorer_api_url = opts.explorer_api_url.clone();

        Ok(Collector {
            data,
            urls,
            disk_mount_points,
            disk_auto_discovery,
            disk_alert_threshold,
            disk_refresh_interval,
            node_ids,
            explorer_api_url,
            stop_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    fn spawn_background_tasks(
        &self,
        background_tasks: &mut JoinSet<()>,
    ) {
        for (name, url_str) in self.urls.clone() {
            let data = self.data.clone();
            let stop_flag = self.stop_flag.clone();
            background_tasks.spawn(async move {
                if let Err(e) = collect_node_state(
                    name.clone(),
                    url_str.clone(),
                    data,
                    stop_flag,
                    COLLECTOR_RETRY_DELAY,
                )
                .await
                {
                    warn!("collect_node_state failed for {}: {}", name, e);
                }
            });
        }

        if !self.node_ids.is_empty() {
            debug!("start collect node detail: {:?}", self.node_ids);
            let node_ids = self.node_ids.clone();
            let explorer_api_url = self.explorer_api_url.clone();
            let data = self.data.clone();
            let stop_flag = self.stop_flag.clone();
            background_tasks.spawn(async move {
                if let Err(e) =
                    collect_node_details(node_ids, data, explorer_api_url, stop_flag).await
                {
                    warn!("collect_node_details failed: {}", e);
                }
            });
        }

        #[cfg(target_family = "unix")]
        {
            let data = self.data.clone();
            let disk_mount_points = self.disk_mount_points.clone();
            let disk_auto_discovery = self.disk_auto_discovery;
            let disk_alert_threshold = self.disk_alert_threshold;
            let disk_refresh_interval = self.disk_refresh_interval;
            let stop_flag = self.stop_flag.clone();
            background_tasks.spawn(async move {
                if let Err(e) = collect_system_stats(
                    data,
                    disk_mount_points,
                    disk_auto_discovery,
                    disk_alert_threshold,
                    disk_refresh_interval,
                    stop_flag,
                )
                .await
                {
                    warn!("collect_system_stats failed: {}", e);
                }
            });
        }
    }

    async fn join_background_tasks(background_tasks: &mut JoinSet<()>) -> Result<()> {
        while let Some(join_result) = background_tasks.join_next().await {
            join_result.map_err(|err| {
                ChaindashError::Other(format!("collector background task join error: {err}"))
            })?;
        }

        Ok(())
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    pub(crate) async fn run(&self) -> Result<()> {
        let mut background_tasks = JoinSet::new();
        self.spawn_background_tasks(&mut background_tasks);

        let run_result = run_block_subscription_loop(
            &self.urls,
            &self.data,
            &self.stop_flag,
            COLLECTOR_RETRY_DELAY,
        )
        .await;

        self.stop();
        let join_result = Self::join_background_tasks(&mut background_tasks).await;

        run_result.and(join_result)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        Arc,
        Mutex,
    };

    use clap::Parser;
    use tokio::{
        task::JoinSet,
        time::{
            self,
            Duration,
        },
    };

    use super::Collector;
    use crate::{
        collect::{
            Data,
            SharedData,
        },
        Opts,
    };

    #[test]
    fn test_collector_new_invalid_url_no_at_sign() {
        let opts = Opts::parse_from(["test", "--url", "invalid_url"]);
        let data: SharedData = Arc::new(Mutex::new(Data::default()));

        let result = Collector::new(&opts, data);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid url format"));
    }

    #[test]
    fn test_collector_new_valid_url() {
        let opts = Opts::parse_from(["test", "--url", "test@ws://127.0.0.1:6789"]);
        let data: SharedData = Arc::new(Mutex::new(Data::default()));

        let result = Collector::new(&opts, data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_collector_new_rejects_non_websocket_url() {
        let opts = Opts::parse_from(["test", "--url", "test@http://127.0.0.1:6789"]);
        let data: SharedData = Arc::new(Mutex::new(Data::default()));

        let result = Collector::new(&opts, data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid websocket url"));
    }

    #[test]
    fn test_collector_new_rejects_invalid_endpoint_in_list() {
        let opts = Opts::parse_from([
            "test",
            "--url",
            "main@ws://127.0.0.1:6789,backup@http://127.0.0.1:6790",
        ]);
        let data: SharedData = Arc::new(Mutex::new(Data::default()));

        let result = Collector::new(&opts, data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid websocket url for backup"));
    }

    #[tokio::test]
    async fn test_join_background_tasks_waits_for_task_completion() {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let mut background_tasks = JoinSet::new();
        let (sender, receiver) = tokio::sync::oneshot::channel();

        let stop_flag_clone = Arc::clone(&stop_flag);
        background_tasks.spawn(async move {
            loop {
                if stop_flag_clone.load(Ordering::Relaxed) {
                    let _ = sender.send(());
                    break;
                }

                time::sleep(Duration::from_millis(10)).await;
            }
        });

        stop_flag.store(true, Ordering::Relaxed);
        Collector::join_background_tasks(&mut background_tasks)
            .await
            .expect("background tasks should join successfully");

        receiver.await.expect("task should report completion before join returns");
    }

    #[tokio::test]
    async fn test_join_background_tasks_reports_join_errors() {
        let mut background_tasks = JoinSet::new();
        background_tasks.spawn(async move {
            panic!("boom");
        });

        let err = Collector::join_background_tasks(&mut background_tasks)
            .await
            .expect_err("join error should be reported");

        assert!(err.to_string().contains("collector background task join error"));
    }
}
