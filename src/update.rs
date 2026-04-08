use num_rational::Ratio;

use crate::app::Widgets;

pub trait UpdatableWidget {
    fn update(&mut self);
    fn get_update_interval(&self) -> Ratio<u64>;
}

fn should_update_widget_at(
    seconds: Ratio<u64>,
    interval: Ratio<u64>,
) -> bool {
    interval != Ratio::from_integer(0) && seconds % interval == Ratio::from_integer(0)
}

fn update_widget_if_due(
    widget: &mut dyn UpdatableWidget,
    seconds: Ratio<u64>,
) {
    if should_update_widget_at(seconds, widget.get_update_interval()) {
        widget.update();
    }
}

pub fn update_widgets(
    widgets: &mut Widgets,
    seconds: Ratio<u64>,
) {
    let mut widgets_to_update: Vec<&mut dyn UpdatableWidget> =
        vec![&mut widgets.txs, &mut widgets.time, &mut widgets.node, &mut widgets.node_details];

    #[cfg(target_family = "unix")]
    {
        widgets_to_update.push(&mut widgets.system_summary);
        widgets_to_update.push(&mut widgets.disk_list);
    }

    for widget in widgets_to_update {
        update_widget_if_due(widget, seconds);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        Mutex,
    };

    use super::*;

    /// Mock widget for testing interval-based update logic
    struct MockWidget {
        update_interval: Ratio<u64>,
        update_count: Arc<Mutex<u64>>,
    }

    impl MockWidget {
        fn new(update_interval: Ratio<u64>) -> (Self, Arc<Mutex<u64>>) {
            let counter = Arc::new(Mutex::new(0));
            (
                MockWidget {
                    update_interval,
                    update_count: counter.clone(),
                },
                counter,
            )
        }
    }

    impl UpdatableWidget for MockWidget {
        fn update(&mut self) {
            let mut count = self.update_count.lock().expect("mutex poisoned");
            *count += 1;
        }

        fn get_update_interval(&self) -> Ratio<u64> {
            self.update_interval
        }
    }

    // ========================================
    // Mock Widget Tests - Basic trait behavior
    // ========================================

    #[test]
    fn test_mock_widget_update_increments_counter() {
        let (mut widget, counter) = MockWidget::new(Ratio::from_integer(1));

        assert_eq!(*counter.lock().expect("mutex poisoned"), 0);
        widget.update();
        assert_eq!(*counter.lock().expect("mutex poisoned"), 1);
        widget.update();
        assert_eq!(*counter.lock().expect("mutex poisoned"), 2);
    }

    #[test]
    fn test_mock_widget_get_update_interval() {
        let (widget, _) = MockWidget::new(Ratio::from_integer(5));
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(5));
    }

    #[test]
    fn test_mock_widget_interval_zero() {
        let (widget, _) = MockWidget::new(Ratio::from_integer(0));
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(0));
    }

    #[test]
    fn test_should_update_widget_at_zero_interval_is_false() {
        assert!(!should_update_widget_at(Ratio::from_integer(10), Ratio::from_integer(0),));
    }

    #[test]
    fn test_update_widget_if_due_skips_zero_interval() {
        let (mut widget, counter) = MockWidget::new(Ratio::from_integer(0));

        update_widget_if_due(&mut widget, Ratio::from_integer(10));

        assert_eq!(*counter.lock().expect("mutex poisoned"), 0);
    }

    #[test]
    fn test_should_update_widget_at_long_interval() {
        let interval = Ratio::from_integer(90);

        assert!(!should_update_widget_at(Ratio::from_integer(60), interval));
        assert!(should_update_widget_at(Ratio::from_integer(90), interval));
        assert!(should_update_widget_at(Ratio::from_integer(180), interval));
    }

    // ========================================
    // Interval Logic Tests
    // ========================================

    #[test]
    fn test_interval_divisible_at_interval() {
        let interval = Ratio::from_integer(5);
        let seconds = Ratio::from_integer(10);
        assert!(seconds % interval == Ratio::from_integer(0));
    }

    #[test]
    fn test_interval_not_divisible_before_interval() {
        let interval = Ratio::from_integer(5);
        let seconds = Ratio::from_integer(3);
        assert!(seconds % interval != Ratio::from_integer(0));
    }

    #[test]
    fn test_interval_at_zero_seconds() {
        let interval = Ratio::from_integer(5);
        let seconds = Ratio::from_integer(0);
        assert!(seconds % interval == Ratio::from_integer(0));
    }

    #[test]
    fn test_interval_at_exact_multiple() {
        for interval in [1, 2, 3, 5, 10].iter() {
            for multiplier in [0, 1, 2, 5, 10].iter() {
                let seconds = Ratio::from_integer(interval * multiplier);
                let interval_ratio = Ratio::from_integer(*interval);
                assert!(
                    seconds % interval_ratio == Ratio::from_integer(0),
                    "Expected {} to be divisible by {}",
                    seconds,
                    interval
                );
            }
        }
    }

    #[test]
    fn test_interval_one_always_updates() {
        let interval = Ratio::from_integer(1);
        for seconds in 0..10 {
            let seconds_ratio = Ratio::from_integer(seconds);
            assert!(
                seconds_ratio % interval == Ratio::from_integer(0),
                "Interval 1 should update at every second"
            );
        }
    }

    #[test]
    fn test_interval_two_updates_alternating() {
        let interval = Ratio::from_integer(2);
        for seconds in 0..10 {
            let seconds_ratio = Ratio::from_integer(seconds);
            let should_update = seconds_ratio % interval == Ratio::from_integer(0);
            assert_eq!(should_update, seconds % 2 == 0);
        }
    }

    // ========================================
    // Real Widget Update Interval Tests
    // ========================================

    use crate::{
        collect::Data,
        widgets::{
            DiskListWidget,
            NodeDetailWidget,
            NodeWidget,
            SystemSummaryWidget,
            TimeWidget,
            TxsWidget,
        },
    };

    fn create_shared_data() -> Arc<Mutex<Data>> {
        Data::new()
    }

    #[test]
    fn test_time_widget_default_interval() {
        let data = create_shared_data();
        let widget = TimeWidget::new(Ratio::from_integer(1), data);
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(1));
    }

    #[test]
    fn test_time_widget_custom_interval() {
        let data = create_shared_data();
        let widget = TimeWidget::new(Ratio::from_integer(5), data);
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(5));
    }

    #[test]
    fn test_txs_widget_default_interval() {
        let data = create_shared_data();
        let widget = TxsWidget::new(Ratio::from_integer(1), data);
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(1));
    }

    #[test]
    fn test_txs_widget_custom_interval() {
        let data = create_shared_data();
        let widget = TxsWidget::new(Ratio::from_integer(10), data);
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(10));
    }

    #[test]
    fn test_node_widget_interval_is_one() {
        let data = create_shared_data();
        let widget = NodeWidget::new(data);
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(1));
    }

    #[test]
    fn test_node_detail_widget_interval_is_one() {
        let data = create_shared_data();
        let widget = NodeDetailWidget::new(data);
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(1));
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_disk_list_widget_interval_is_two() {
        let data = create_shared_data();
        let widget = DiskListWidget::new(data);
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(2));
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_system_summary_widget_interval_is_two() {
        let data = create_shared_data();
        let widget = SystemSummaryWidget::new(data);
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(2));
    }

    // ========================================
    // Widget Update Behavior Tests
    // ========================================

    #[test]
    fn test_time_widget_update_with_no_data() {
        let data = create_shared_data();
        let mut widget = TimeWidget::new(Ratio::from_integer(1), data.clone());
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(1));
        widget.update();
    }

    #[test]
    fn test_txs_widget_update_with_no_data() {
        let data = create_shared_data();
        let mut widget = TxsWidget::new(Ratio::from_integer(1), data.clone());
        widget.update();
    }

    #[test]
    fn test_node_widget_update_with_no_data() {
        let data = create_shared_data();
        let mut widget = NodeWidget::new(data.clone());
        widget.update();
    }

    #[test]
    fn test_node_detail_widget_update_with_no_data() {
        let data = create_shared_data();
        let mut widget = NodeDetailWidget::new(data.clone());
        widget.update();
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_disk_list_widget_update_with_no_data() {
        let data = create_shared_data();
        let mut widget = DiskListWidget::new(data.clone());
        widget.update();
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_system_summary_widget_update_with_no_data() {
        let data = create_shared_data();
        let mut widget = SystemSummaryWidget::new(data.clone());
        widget.update();
    }

    // ========================================
    // TimeWidget Update Logic Tests
    // ========================================

    #[test]
    fn test_time_widget_update_accumulates_data() {
        let data = create_shared_data();
        let mut widget = TimeWidget::new(Ratio::from_integer(1), data);
        for _ in 0..5 {
            widget.update();
        }
    }

    // ========================================
    // TxsWidget Update Logic Tests
    // ========================================

    #[test]
    fn test_txs_widget_update_accumulates_data() {
        let data = create_shared_data();
        let mut widget = TxsWidget::new(Ratio::from_integer(1), data);
        for _ in 0..5 {
            widget.update();
        }
    }

    // ========================================
    // NodeWidget Update Logic Tests
    // ========================================

    #[test]
    fn test_node_widget_update_fetches_states() {
        let data = create_shared_data();
        let mut widget = NodeWidget::new(data);
        widget.update();
    }

    // ========================================
    // NodeDetailWidget Update Logic Tests
    // ========================================

    #[test]
    fn test_node_detail_widget_loading_state_without_data() {
        let data = create_shared_data();
        let mut widget = NodeDetailWidget::new(data);
        widget.update();
    }

    #[test]
    fn test_node_detail_widget_with_data() {
        use crate::collect::NodeDetail;

        let data = create_shared_data();
        {
            let mut d = data.lock().expect("mutex poisoned");
            d.update_node_detail(Some(NodeDetail {
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

        let mut widget = NodeDetailWidget::new(data);
        widget.update();
    }

    // ========================================
    // Unix-specific Widget Tests
    // ========================================

    #[cfg(target_family = "unix")]
    mod unix_tests {
        use std::time::Instant;

        use super::*;
        use crate::collect::DiskDetail;

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
        fn test_disk_list_widget_with_disk_data() {
            let data = create_shared_data();
            {
                let mut d = data.lock().expect("mutex poisoned");
                d.set_disk_details_for_test(vec![
                    create_test_disk_detail("/"),
                    create_test_disk_detail("/home"),
                ]);
            }

            let mut widget = DiskListWidget::new(data);
            widget.update();
        }

        #[test]
        fn test_system_summary_widget_update() {
            let data = create_shared_data();
            let mut widget = SystemSummaryWidget::new(data);

            widget.update();
        }

        #[test]
        fn test_disk_list_widget_multiple_updates() {
            let data = create_shared_data();

            {
                let mut d = data.lock().expect("mutex poisoned");
                d.set_disk_details_for_test(vec![create_test_disk_detail("/")]);
            }

            let mut widget = DiskListWidget::new(data);

            for _ in 0..5 {
                widget.update();
            }
        }
    }

    // ========================================
    // Ratio Arithmetic Tests
    // ========================================

    #[test]
    fn test_ratio_modulo_basic() {
        let a = Ratio::from_integer(10);
        let b = Ratio::from_integer(3);
        let result = a % b;
        assert_eq!(result, Ratio::from_integer(1));
    }

    #[test]
    fn test_ratio_modulo_zero() {
        let a = Ratio::from_integer(10);
        let b = Ratio::from_integer(5);
        let result = a % b;
        assert_eq!(result, Ratio::from_integer(0));
    }

    #[test]
    fn test_ratio_equality() {
        let a = Ratio::from_integer(5);
        let b = Ratio::from_integer(5);
        assert_eq!(a, b);
    }

    #[test]
    fn test_ratio_from_integer() {
        let r = Ratio::from_integer(42);
        assert_eq!(*r.numer(), 42);
        assert_eq!(*r.denom(), 1);
    }
}
