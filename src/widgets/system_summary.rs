use log::debug;
use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::{
        Constraint,
        Rect,
    },
    widgets::{
        Row,
        Table,
        Widget,
    },
};

use crate::{
    collect::{
        SharedData,
        SystemStats,
    },
    update::UpdatableWidget,
    widgets::block,
};

pub struct SystemSummaryWidget {
    title: String,
    update_interval: Ratio<u64>,
    collect_data: SharedData,
    system_stats: SystemStats,
}

impl SystemSummaryWidget {
    pub fn new(collect_data: SharedData) -> SystemSummaryWidget {
        SystemSummaryWidget {
            title: " System Stats ".to_string(),
            update_interval: Ratio::from_integer(2),
            collect_data,
            system_stats: SystemStats::default(),
        }
    }
}

impl UpdatableWidget for SystemSummaryWidget {
    fn update(&mut self) {
        let collect_data = self.collect_data.lock().expect("mutex poisoned - recovering");
        self.system_stats = collect_data.system_stats();
        debug!("update system stats: {:?}", &self.system_stats);
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &SystemSummaryWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        debug!("area.height: {}", area.height);
        if area.height < 3 {
            return;
        }

        self.render_summary_only(area, buf);
    }
}

impl SystemSummaryWidget {
    /// 渲染仅包含摘要信息的视图
    fn render_summary_only(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        // 根据可用宽度动态调整布局
        let use_compact_layout = area.width < 100;

        debug!("render data: {:?}", &self.system_stats);

        if use_compact_layout {
            self.render_compact_summary(area, buf);
        } else {
            self.render_full_summary(area, buf);
        }
    }

    /// 渲染完整摘要视图
    fn render_full_summary(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let header = [
            " CPU Usage",
            "Memory Usage",
            "Memory Used/Total",
            "Disk Usage",
            "Disk Used/Total",
            "Network RX",
            "Network TX",
        ];

        let stats = &self.system_stats;

        // 格式化数据
        let memory_used_gb = stats.memory_used as f64 / 1024.0 / 1024.0 / 1024.0;
        let memory_total_gb = stats.memory_total as f64 / 1024.0 / 1024.0 / 1024.0;
        let disk_used_gb = stats.disk_used as f64 / 1024.0 / 1024.0 / 1024.0;
        let disk_total_gb = stats.disk_total as f64 / 1024.0 / 1024.0 / 1024.0;
        let network_rx_mb = stats.network_rx as f64 / 1024.0 / 1024.0;
        let network_tx_mb = stats.network_tx as f64 / 1024.0 / 1024.0;

        // 格式化磁盘使用率，如果有告警则添加标记
        let disk_usage_text = if stats.has_disk_alerts {
            format!("\u{f1c0} {:.2}% [!]", stats.disk_usage_percent)
        } else {
            format!("\u{f1c0} {:.2}%", stats.disk_usage_percent)
        };

        let rows = vec![Row::new(vec![
            format!(" \u{f085}  {:.2}%", stats.cpu_usage),
            format!("\u{f233} {:.2}%", stats.memory_usage_percent),
            format!("\u{f02a1} {:.2}GB / {:.2}GB", memory_used_gb, memory_total_gb),
            disk_usage_text,
            format!("\u{f0a0f} {:.2}GB / {:.2}GB", disk_used_gb, disk_total_gb),
            format!("\u{f0045} {:.2} MB/s", network_rx_mb),
            format!("\u{f005d} {:.2} MB/s", network_tx_mb),
        ])
        .style(block::content_style())];

        let header_row = Row::new(header.iter().copied()).style(block::header_style());

        Table::new(
            rows,
            &[
                Constraint::Length(15),
                Constraint::Length(15),
                Constraint::Length(25),
                Constraint::Length(15),
                Constraint::Length(25),
                Constraint::Length(15),
                Constraint::Length(15),
            ],
        )
        .block(block::new(&self.title))
        .header(header_row)
        .column_spacing(1)
        .render(area, buf);
    }

    /// 渲染紧凑摘要视图（用于小宽度）
    fn render_compact_summary(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let header = ["CPU", "Mem", "Disk", "Net RX", "Net TX"];

        let stats = &self.system_stats;

        // 格式化数据
        let network_rx_mb = stats.network_rx as f64 / 1024.0 / 1024.0;
        let network_tx_mb = stats.network_tx as f64 / 1024.0 / 1024.0;

        let rows = vec![Row::new(vec![
            format!(" {:.1}%", stats.cpu_usage),
            format!(" {:.1}%", stats.memory_usage_percent),
            format!(" {:.1}%", stats.disk_usage_percent),
            format!(" {:.1}M", network_rx_mb),
            format!(" {:.1}M", network_tx_mb),
        ])
        .style(block::content_style())];

        let header_row = Row::new(header.iter().copied()).style(block::header_style());

        Table::new(
            rows,
            &[
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        )
        .block(block::new(&self.title))
        .header(header_row)
        .column_spacing(1)
        .render(area, buf);
    }
}

#[cfg(test)]
#[cfg(target_family = "unix")]
mod tests {
    use super::*;
    use crate::collect::Data;

    fn create_shared_data() -> SharedData {
        Data::new()
    }

    #[test]
    fn test_system_summary_widget_new() {
        let shared_data = create_shared_data();
        let widget = SystemSummaryWidget::new(shared_data);
        assert_eq!(widget.title, " System Stats ");
    }

    #[test]
    fn test_system_summary_widget_update_interval() {
        let shared_data = create_shared_data();
        let widget = SystemSummaryWidget::new(shared_data);
        let interval = widget.get_update_interval();
        assert_eq!(interval, Ratio::from_integer(2));
    }

    #[test]
    fn test_system_summary_widget_update_with_empty_data() {
        let shared_data = create_shared_data();
        let mut widget = SystemSummaryWidget::new(shared_data);
        widget.update();
        assert_eq!(widget.system_stats.cpu_usage, 0.0);
        assert_eq!(widget.system_stats.memory_used, 0);
        assert_eq!(widget.system_stats.memory_total, 0);
    }

    #[test]
    fn test_system_summary_widget_initial_state() {
        let shared_data = create_shared_data();
        let widget = SystemSummaryWidget::new(shared_data);
        assert_eq!(widget.system_stats.cpu_usage, 0.0);
        assert_eq!(widget.system_stats.memory_usage_percent, 0.0);
        assert_eq!(widget.system_stats.disk_usage_percent, 0.0);
    }
}
