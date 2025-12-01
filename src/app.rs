use crate::{
    collect::{
        Data,
        SharedData,
    },
    opts::Opts,
    widgets::*,
};

pub struct App {
    pub widgets: Widgets,
    pub data: SharedData,
}

impl App {
    /// 处理Tab键事件，切换当前选中的磁盘
    pub fn handle_tab_key(&self) {
        let mut data = self.data.lock().unwrap();
        let disk_count = data.system_stats().disk_details.len();
        if disk_count > 0 {
            // 获取当前索引并计算下一个索引
            let current_index = data.system_stats().current_disk_index;
            let next_index = (current_index + 1) % disk_count;

            // 更新索引
            data.update_disk_index(next_index);
        }
    }

    /// 处理Shift+Tab键事件，切换到上一个磁盘
    pub fn handle_shift_tab_key(&self) {
        let mut data = self.data.lock().unwrap();
        let disk_count = data.system_stats().disk_details.len();

        if disk_count > 0 {
            // 获取当前索引并计算上一个索引
            let current_index = data.system_stats().current_disk_index;
            let prev_index = if current_index == 0 {
                disk_count - 1
            } else {
                current_index - 1
            };

            // 更新索引
            data.update_disk_index(prev_index);
        }
    }
}

pub struct Widgets {
    pub txs: TxsWidget,
    pub time: TimeWidget,
    pub node: NodeWidget,
    #[cfg(target_family = "unix")]
    pub system: SystemWidget,
}

pub fn setup_app(
    opts: &Opts,
    _program_name: &str,
) -> App {
    let data = Data::new();
    let txs = TxsWidget::new(opts.interval, data.clone());
    let time = TimeWidget::new(opts.interval, data.clone());
    let node = NodeWidget::new(data.clone());

    #[cfg(target_family = "unix")]
    let system = SystemWidget::new(data.clone());

    App {
        widgets: Widgets {
            txs,
            time,
            node,
            #[cfg(target_family = "unix")]
            system,
        },
        data,
    }
}
