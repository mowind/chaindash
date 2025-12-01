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

pub struct SystemWidget {
    title: String,
    update_interval: Ratio<u64>,
    collect_data: SharedData,
    system_stats: SystemStats,
}

impl SystemWidget {
    pub fn new(collect_data: SharedData) -> SystemWidget {
        SystemWidget {
            title: " System Stats ".to_string(),
            update_interval: Ratio::from_integer(2),
            collect_data,
            system_stats: SystemStats::default(),
        }
    }
}

impl UpdatableWidget for SystemWidget {
    fn update(&mut self) {
        let collect_data = self.collect_data.lock().unwrap();
        self.system_stats = collect_data.system_stats();
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &SystemWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        if area.height < 3 {
            return;
        }

        let stats = &self.system_stats;

        // 如果有磁盘详情，需要更多空间显示
        let show_disk_details = !stats.disk_details.is_empty() && area.height >= 8;

        if show_disk_details {
            self.render_with_disk_details(area, buf);
        } else {
            self.render_summary_only(area, buf);
        }
    }
}

impl SystemWidget {
    /// 渲染仅包含摘要信息的视图
    fn render_summary_only(
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

    /// 渲染包含磁盘详情的视图
    fn render_with_disk_details(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let stats = &self.system_stats;
        let disk_details = &stats.disk_details;
        let current_index = stats.current_disk_index.min(disk_details.len().saturating_sub(1));

        // 分割区域：上半部分显示系统摘要，下半部分显示磁盘详情
        let summary_height = 3;
        let details_height = area.height.saturating_sub(summary_height + 1);

        if details_height < 5 {
            // 空间不足，回退到仅显示摘要
            self.render_summary_only(area, buf);
            return;
        }

        let summary_area = Rect::new(area.x, area.y, area.width, summary_height);
        let details_area = Rect::new(area.x, area.y + summary_height, area.width, details_height);

        // 渲染系统摘要
        self.render_summary_only(summary_area, buf);

        // 渲染磁盘详情
        self.render_disk_details(details_area, buf, current_index);
    }

    /// 渲染磁盘详情区域
    fn render_disk_details(
        &self,
        area: Rect,
        buf: &mut Buffer,
        current_index: usize,
    ) {
        let stats = &self.system_stats;
        let disk_details = &stats.disk_details;

        if disk_details.is_empty() {
            return;
        }

        let current_disk = &disk_details[current_index];

        // 创建磁盘详情标题，包含Tab切换指示器
        let title = if disk_details.len() > 1 {
            format!(" Disk Details [{}/{}] (Tab to switch) ", current_index + 1, disk_details.len())
        } else {
            " Disk Details ".to_string()
        };

        // 格式化磁盘信息
        let total_gb = current_disk.total as f64 / 1024.0 / 1024.0 / 1024.0;
        let used_gb = current_disk.used as f64 / 1024.0 / 1024.0 / 1024.0;
        let available_gb = current_disk.available as f64 / 1024.0 / 1024.0 / 1024.0;

        // 确定颜色：告警状态用红色，网络挂载点用黄色
        let mut style = Style::default().fg(Color::Indexed(249 as u8));
        if current_disk.is_alert {
            style = style.fg(Color::Red);
        } else if current_disk.is_network {
            style = style.fg(Color::Yellow);
        }

        // 创建详情行
        let rows = vec![
            Row::StyledData(
                vec![
                    " Mount Point:".to_string(),
                    format!(" {}", current_disk.mount_point),
                ]
                .into_iter(),
                style,
            ),
            Row::StyledData(
                vec![
                    " Filesystem:".to_string(),
                    format!(" {}", current_disk.filesystem),
                ]
                .into_iter(),
                style,
            ),
            Row::StyledData(
                vec![
                    " Device:".to_string(),
                    format!(" {}", current_disk.device),
                ]
                .into_iter(),
                style,
            ),
            Row::StyledData(
                vec![
                    " Total:".to_string(),
                    format!(" {:.2} GB", total_gb),
                ]
                .into_iter(),
                style,
            ),
            Row::StyledData(
                vec![
                    " Used:".to_string(),
                    format!(" {:.2} GB ({:.1}%)", used_gb, current_disk.usage_percent),
                ]
                .into_iter(),
                style,
            ),
            Row::StyledData(
                vec![
                    " Available:".to_string(),
                    format!(" {:.2} GB", available_gb),
                ]
                .into_iter(),
                style,
            ),
        ];

        let header = ["", ""];
        Table::new(header.iter(), rows.into_iter())
            .block(block::new(&title))
            .header_style(
                Style::default()
                    .fg(Color::Indexed(249 as u8))
                    .bg(Color::Reset)
                    .modifier(Modifier::BOLD),
            )
            .widths(&[
                Constraint::Length(15),
                Constraint::Length(area.width.saturating_sub(15)),
            ])
            .column_spacing(1)
            .header_gap(0)
            .render(area, buf);
    }
}
