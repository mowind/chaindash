use crate::{
    collect::{
        Data,
        SharedData,
    },
    opts::Opts,
    sync::lock_or_panic,
    widgets::{
        DiskListWidget,
        NodeDetailWidget,
        NodeWidget,
        SystemSummaryWidget,
        TimeWidget,
        TxsWidget,
    },
};

pub struct App {
    pub widgets: Widgets,
    pub data: SharedData,
}

impl App {
    /// 处理Tab键事件，切换当前选中的磁盘
    #[cfg(target_family = "unix")]
    pub fn handle_tab_key(&self) {
        let mut data = lock_or_panic(&self.data);
        let stats = data.system_stats();
        let disk_count = stats.disk_details.len();
        if disk_count > 0 {
            // 获取当前索引并计算下一个索引
            let current_index = stats.current_disk_index;
            let next_index = (current_index + 1) % disk_count;

            // 更新索引
            data.update_disk_index(next_index);
        }
    }

    /// 处理Tab键事件，切换当前选中的磁盘
    #[cfg(not(target_family = "unix"))]
    pub fn handle_tab_key(&self) {}

    /// 处理Shift+Tab键事件，切换到上一个磁盘
    #[cfg(target_family = "unix")]
    pub fn handle_shift_tab_key(&self) {
        let mut data = lock_or_panic(&self.data);
        let stats = data.system_stats();
        let disk_count = stats.disk_details.len();

        if disk_count > 0 {
            // 获取当前索引并计算上一个索引
            let current_index = stats.current_disk_index;
            let prev_index = if current_index == 0 {
                disk_count - 1
            } else {
                current_index - 1
            };

            // 更新索引
            data.update_disk_index(prev_index);
        }
    }

    /// 处理Shift+Tab键事件，切换到上一个磁盘
    #[cfg(not(target_family = "unix"))]
    pub fn handle_shift_tab_key(&self) {}
}

pub struct Widgets {
    pub txs: TxsWidget,
    pub time: TimeWidget,
    pub node: NodeWidget,
    #[cfg(target_family = "unix")]
    pub system_summary: SystemSummaryWidget,
    #[cfg(target_family = "unix")]
    pub disk_list: DiskListWidget,
    pub node_details: NodeDetailWidget,
}

pub fn setup_app(opts: &Opts) -> App {
    let data = Data::new();
    let txs = TxsWidget::new(opts.interval, data.clone());
    let time = TimeWidget::new(opts.interval, data.clone());
    let node = NodeWidget::new(data.clone());

    #[cfg(target_family = "unix")]
    let system_summary = SystemSummaryWidget::new(data.clone());

    #[cfg(target_family = "unix")]
    let disk_list = DiskListWidget::new(data.clone());

    let node_details = NodeDetailWidget::new(data.clone());

    App {
        widgets: Widgets {
            txs,
            time,
            node,
            #[cfg(target_family = "unix")]
            system_summary,
            #[cfg(target_family = "unix")]
            disk_list,
            node_details,
        },
        data,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use clap::Parser;

    use super::*;
    use crate::{
        collect::DiskDetail,
        Opts,
    };

    fn create_test_opts() -> Opts {
        Opts::parse_from(["test", "--url", "test@ws://127.0.0.1:6789"])
    }

    #[cfg(target_family = "unix")]
    fn create_test_disk_detail(mount_point: &str) -> DiskDetail {
        DiskDetail {
            mount_point: mount_point.to_string(),
            filesystem: "ext4".to_string(),
            total: 100_000_000_000,
            used: 50_000_000_000,
            available: 50_000_000_000,
            usage_percent: 50.0,
            device: "/dev/sda1".to_string(),
            is_alert: false,
            is_network: false,
            last_updated: Instant::now(),
        }
    }

    #[test]
    fn test_setup_app_creates_app_with_widgets() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        assert_eq!(app.data.lock().expect("mutex poisoned").cur_block_number(), 0);
    }

    #[test]
    fn test_setup_app_initializes_shared_data() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        let data = app.data.lock().expect("mutex poisoned");
        assert_eq!(data.cur_block_number(), 0);
        assert_eq!(data.cur_txs(), 0);
        assert_eq!(data.max_txs(), 0);
    }

    #[test]
    fn test_app_data_is_shared_with_widgets() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        let data_clone = app.data.clone();
        {
            let mut data = data_clone.lock().expect("mutex poisoned");
            data.update_node_detail(Some(crate::collect::NodeDetail {
                node_id: "test-node-id".to_string(),
                node_name: "test-node".to_string(),
                ranking: 1,
                block_qty: 100,
                block_rate: "50%".to_string(),
                daily_block_rate: "10/day".to_string(),
                reward_per: 10.0,
                reward_value: 1000.0,
                reward_address: "0x123".to_string(),
                verifier_time: 3600,
                last_updated_at: None,
            }));
        }

        let data = app.data.lock().expect("mutex poisoned");
        assert!(data.node_detail().is_some());
        let detail = data.node_detail().unwrap();
        assert_eq!(detail.node_name, "test-node");
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_tab_key_empty_disk_list() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        let index_before = {
            let data = app.data.lock().expect("mutex poisoned");
            data.current_disk_index_for_test()
        };

        app.handle_tab_key();

        let index_after = {
            let data = app.data.lock().expect("mutex poisoned");
            data.current_disk_index_for_test()
        };

        assert_eq!(index_before, index_after);
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_tab_key_single_disk() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        {
            let mut data = app.data.lock().expect("mutex poisoned");
            data.set_disk_details_for_test(vec![create_test_disk_detail("/")]);
        }

        app.handle_tab_key();

        let index_after = {
            let data = app.data.lock().expect("mutex poisoned");
            data.current_disk_index_for_test()
        };

        assert_eq!(index_after, 0);
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_tab_key_multiple_disks() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        {
            let mut data = app.data.lock().expect("mutex poisoned");
            data.set_disk_details_for_test(vec![
                create_test_disk_detail("/"),
                create_test_disk_detail("/home"),
                create_test_disk_detail("/opt"),
            ]);
        }

        let indices: Vec<usize> = {
            let mut result = Vec::new();
            for _ in 0..5 {
                let index = {
                    let data = app.data.lock().expect("mutex poisoned");
                    data.current_disk_index_for_test()
                };
                result.push(index);
                app.handle_tab_key();
            }
            result
        };

        assert_eq!(indices, vec![0, 1, 2, 0, 1]);
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_shift_tab_key_empty_disk_list() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        let index_before = {
            let data = app.data.lock().expect("mutex poisoned");
            data.current_disk_index_for_test()
        };

        app.handle_shift_tab_key();

        let index_after = {
            let data = app.data.lock().expect("mutex poisoned");
            data.current_disk_index_for_test()
        };

        assert_eq!(index_before, index_after);
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_shift_tab_key_single_disk() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        {
            let mut data = app.data.lock().expect("mutex poisoned");
            data.set_disk_details_for_test(vec![create_test_disk_detail("/")]);
        }

        app.handle_shift_tab_key();

        let index_after = {
            let data = app.data.lock().expect("mutex poisoned");
            data.current_disk_index_for_test()
        };

        assert_eq!(index_after, 0);
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_shift_tab_key_wrap_around() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        {
            let mut data = app.data.lock().expect("mutex poisoned");
            data.set_disk_details_for_test(vec![
                create_test_disk_detail("/"),
                create_test_disk_detail("/home"),
                create_test_disk_detail("/opt"),
            ]);
        }

        app.handle_shift_tab_key();

        let index_after = {
            let data = app.data.lock().expect("mutex poisoned");
            data.current_disk_index_for_test()
        };

        assert_eq!(index_after, 2);
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_shift_tab_key_multiple_navigation() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        {
            let mut data = app.data.lock().expect("mutex poisoned");
            data.set_disk_details_for_test(vec![
                create_test_disk_detail("/"),
                create_test_disk_detail("/home"),
                create_test_disk_detail("/opt"),
            ]);
        }

        let indices: Vec<usize> = {
            let mut result = Vec::new();
            for _ in 0..5 {
                let index = {
                    let data = app.data.lock().expect("mutex poisoned");
                    data.current_disk_index_for_test()
                };
                result.push(index);
                app.handle_shift_tab_key();
            }
            result
        };

        assert_eq!(indices, vec![0, 2, 1, 0, 2]);
    }

    #[test]
    fn test_widgets_struct_fields() {
        let opts = create_test_opts();
        let app = setup_app(&opts);

        let _txs_ref = &app.widgets.txs;
        let _time_ref = &app.widgets.time;
        let _node_ref = &app.widgets.node;
        let _node_details_ref = &app.widgets.node_details;

        #[cfg(target_family = "unix")]
        {
            let _system_summary_ref = &app.widgets.system_summary;
            let _disk_list_ref = &app.widgets.disk_list;
        }
    }
}
