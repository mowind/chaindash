use log::debug;
use num_rational::Ratio;
use tui::{
    buffer::Buffer,
    layout::{
        Constraint,
        Rect,
    },
    style::{
        Color,
        Modifier,
        Style,
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
        let collect_data = self.collect_data.lock().unwrap();
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

        // 确定颜色：高使用率显示警告颜色
        let _cpu_color = if stats.cpu_usage > 80.0 {
            Color::Red
        } else {
            Color::Indexed(249 as u8)
        };
        let _memory_color = if stats.memory_usage_percent > 80.0 {
            Color::Red
        } else {
            Color::Indexed(249 as u8)
        };
        let _disk_color = if stats.has_disk_alerts {
            Color::Red
        } else {
            Color::Indexed(249 as u8)
        };

        let rows = vec![Row::StyledData(
            vec![
                format!(" \u{f085}  {:.2}%", stats.cpu_usage),
                format!("\u{f233} {:.2}%", stats.memory_usage_percent),
                format!("\u{f02a1} {:.2}GB / {:.2}GB", memory_used_gb, memory_total_gb),
                disk_usage_text,
                format!("\u{f0a0f} {:.2}GB / {:.2}GB", disk_used_gb, disk_total_gb),
                format!("\u{f0045} {:.2} MB/s", network_rx_mb),
                format!("\u{f005d} {:.2} MB/s", network_tx_mb),
            ]
            .into_iter(),
            Style::default().fg(Color::Indexed(249 as u8)).bg(Color::Reset),
        )];

        Table::new(header.iter(), rows.into_iter())
            .block(block::new(&self.title))
            .header_style(
                Style::default()
                    .fg(Color::Indexed(249 as u8))
                    .bg(Color::Reset)
                    .modifier(Modifier::BOLD),
            )
            .widths(&[
                Constraint::Length(15),
                Constraint::Length(15),
                Constraint::Length(25),
                Constraint::Length(15),
                Constraint::Length(25),
                Constraint::Length(15),
                Constraint::Length(15),
            ])
            .column_spacing(1)
            .header_gap(0)
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
        let _memory_used_gb = stats.memory_used as f64 / 1024.0 / 1024.0 / 1024.0;
        let _memory_total_gb = stats.memory_total as f64 / 1024.0 / 1024.0 / 1024.0;
        let network_rx_mb = stats.network_rx as f64 / 1024.0 / 1024.0;
        let network_tx_mb = stats.network_tx as f64 / 1024.0 / 1024.0;

        // 确定颜色：高使用率显示警告颜色
        let _cpu_color = if stats.cpu_usage > 80.0 {
            Color::Red
        } else {
            Color::Indexed(249 as u8)
        };
        let _memory_color = if stats.memory_usage_percent > 80.0 {
            Color::Red
        } else {
            Color::Indexed(249 as u8)
        };
        let _disk_color = if stats.has_disk_alerts {
            Color::Red
        } else {
            Color::Indexed(249 as u8)
        };

        let rows = vec![Row::StyledData(
            vec![
                format!(" {:.1}%", stats.cpu_usage),
                format!(" {:.1}%", stats.memory_usage_percent),
                format!(" {:.1}%", stats.disk_usage_percent),
                format!(" {:.1}M", network_rx_mb),
                format!(" {:.1}M", network_tx_mb),
            ]
            .into_iter(),
            Style::default().fg(Color::Indexed(249 as u8)).bg(Color::Reset),
        )];

        Table::new(header.iter(), rows.into_iter())
            .block(block::new(&self.title))
            .header_style(
                Style::default()
                    .fg(Color::Indexed(249 as u8))
                    .bg(Color::Reset)
                    .modifier(Modifier::BOLD),
            )
            .widths(&[
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
            ])
            .column_spacing(1)
            .header_gap(0)
            .render(area, buf);
    }
}
