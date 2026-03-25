use num_rational::Ratio;
use ratatui::{
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
    text::Line,
    widgets::{
        Paragraph,
        Row,
        Table,
        Widget,
        Wrap,
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

pub struct DiskListWidget {
    update_interval: Ratio<u64>,
    collect_data: SharedData,
    system_stats: SystemStats,
}

impl DiskListWidget {
    const COMPACT_LAYOUT_WIDTH: u16 = 50;

    pub fn new(collect_data: SharedData) -> DiskListWidget {
        DiskListWidget {
            update_interval: Ratio::from_integer(2),
            collect_data,
            system_stats: SystemStats::default(),
        }
    }
}

impl UpdatableWidget for DiskListWidget {
    fn update(&mut self) {
        let collect_data = self.collect_data.lock().expect("mutex poisoned - recovering");
        self.system_stats = collect_data.system_stats();
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &DiskListWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        if area.height < 3 {
            return;
        }

        self.render_disk_list(area, buf);
    }
}

impl DiskListWidget {
    /// 渲染磁盘列表视图
    fn render_disk_list(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let stats = &self.system_stats;
        let disk_details = &stats.disk_details;
        let current_index = stats.current_disk_index.min(disk_details.len().saturating_sub(1));

        if disk_details.is_empty() {
            // 无磁盘数据时显示友好消息
            let title = " Disk Details ".to_string();
            let row = Row::new(vec![" No disk mount points found"])
                .style(Style::default().fg(Color::Indexed(249_u8)).bg(Color::Reset));

            Table::new(vec![row], &[Constraint::Length(area.width)])
                .block(block::new(&title))
                .column_spacing(1)
                .render(area, buf);
            return;
        }

        // 创建标题，包含Tab切换指示器
        let title = if disk_details.len() > 1 {
            format!(" Disk Details [{}/{}] (Tab to switch) ", current_index + 1, disk_details.len())
        } else {
            " Disk Details ".to_string()
        };

        if area.width < Self::COMPACT_LAYOUT_WIDTH {
            self.render_compact_disk_list(area, buf, &title, current_index);
            return;
        }

        // 创建表头 - 类似 df -h 的格式
        let header = ["Mounted on", "Size", "Used", "Avail", "Use%"];

        // 创建行数据
        let mut rows = Vec::new();
        for (index, disk) in disk_details.iter().enumerate() {
            // 格式化大小为人类可读格式
            let total_str = Self::format_size(disk.total);
            let used_str = Self::format_size(disk.used);
            let available_str = Self::format_size(disk.available);

            // 确定颜色：告警状态用红色，网络挂载点用黄色，当前选中行高亮
            let mut style = Style::default().fg(Color::Indexed(249_u8));
            if disk.is_alert {
                style = style.fg(Color::Red);
            } else if disk.is_network {
                style = style.fg(Color::Yellow);
            }

            // 如果是当前选中的行，添加高亮
            if index == current_index {
                style = style.add_modifier(Modifier::BOLD);
            }

            let row = Row::new(vec![
                format!(" {}", disk.mount_point),
                format!(" {}", total_str),
                format!(" {}", used_str),
                format!(" {}", available_str),
                format!(" {:.1}%", disk.usage_percent),
            ])
            .style(style);
            rows.push(row);
        }

        // 动态计算列宽，基于可用空间
        let col_width = area.width / 5; // 5列
        let header_row = Row::new(header.iter().copied()).style(
            Style::default()
                .fg(Color::Indexed(249_u8))
                .bg(Color::Reset)
                .add_modifier(Modifier::BOLD),
        );

        Table::new(
            rows,
            &[
                Constraint::Length(col_width),
                Constraint::Length(col_width),
                Constraint::Length(col_width),
                Constraint::Length(col_width),
                Constraint::Length(col_width),
            ],
        )
        .block(block::new(&title))
        .header(header_row)
        .column_spacing(1)
        .render(area, buf);
    }

    fn compact_lines(
        &self,
        current_index: usize,
    ) -> Vec<String> {
        let disk = &self.system_stats.disk_details[current_index];
        vec![
            format!("Mount: {}", disk.mount_point),
            format!("Use: {:.1}%", disk.usage_percent),
            format!("Used: {} / {}", Self::format_size(disk.used), Self::format_size(disk.total)),
            format!("Avail: {}", Self::format_size(disk.available)),
        ]
    }

    fn render_compact_disk_list(
        &self,
        area: Rect,
        buf: &mut Buffer,
        title: &str,
        current_index: usize,
    ) {
        let disk = &self.system_stats.disk_details[current_index];
        let lines: Vec<Line> =
            self.compact_lines(current_index).into_iter().map(Line::raw).collect();

        let mut paragraph = Paragraph::new(lines)
            .block(block::new(title))
            .style(Style::default().fg(Color::Indexed(249_u8)).bg(Color::Reset))
            .wrap(Wrap { trim: true });

        if disk.is_alert {
            paragraph = paragraph.style(Style::default().fg(Color::Red).bg(Color::Reset));
        } else if disk.is_network {
            paragraph = paragraph.style(Style::default().fg(Color::Yellow).bg(Color::Reset));
        }

        paragraph.render(area, buf);
    }

    /// 格式化大小为人类可读格式（类似 df -h）
    fn format_size(bytes: u64) -> String {
        const KB: f64 = 1024.0;
        const MB: f64 = KB * 1024.0;
        const GB: f64 = MB * 1024.0;
        const TB: f64 = GB * 1024.0;

        let bytes_f64 = bytes as f64;

        if bytes_f64 >= TB {
            format!("{:.1}T", bytes_f64 / TB)
        } else if bytes_f64 >= GB {
            format!("{:.1}G", bytes_f64 / GB)
        } else if bytes_f64 >= MB {
            format!("{:.1}M", bytes_f64 / MB)
        } else if bytes_f64 >= KB {
            format!("{:.1}K", bytes_f64 / KB)
        } else {
            format!("{bytes}B")
        }
    }
}

#[cfg(test)]
#[cfg(target_family = "unix")]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::collect::{
        Data,
        DiskDetail,
    };

    fn create_shared_data() -> SharedData {
        Data::new()
    }

    fn create_disk_detail(mount_point: &str) -> DiskDetail {
        DiskDetail {
            mount_point: mount_point.to_string(),
            filesystem: "ext4".to_string(),
            total: 100 * 1024 * 1024 * 1024,
            used: 40 * 1024 * 1024 * 1024,
            available: 60 * 1024 * 1024 * 1024,
            usage_percent: 40.0,
            device: "/dev/sda1".to_string(),
            is_alert: false,
            is_network: false,
            last_updated: Instant::now(),
        }
    }

    #[test]
    fn test_disk_list_widget_new() {
        let shared_data = create_shared_data();
        let widget = DiskListWidget::new(shared_data);
        assert_eq!(widget.update_interval, Ratio::from_integer(2));
    }

    #[test]
    fn test_disk_list_widget_update_interval() {
        let shared_data = create_shared_data();
        let widget = DiskListWidget::new(shared_data);
        let interval = widget.get_update_interval();
        assert_eq!(interval, Ratio::from_integer(2));
    }

    #[test]
    fn test_disk_list_widget_update_with_empty_data() {
        let shared_data = create_shared_data();
        let mut widget = DiskListWidget::new(shared_data);
        widget.update();
        assert!(widget.system_stats.disk_details.is_empty());
    }

    #[test]
    fn test_disk_list_format_size_bytes() {
        assert_eq!(DiskListWidget::format_size(512), "512B");
        assert_eq!(DiskListWidget::format_size(0), "0B");
    }

    #[test]
    fn test_disk_list_format_size_kilobytes() {
        assert_eq!(DiskListWidget::format_size(1024), "1.0K");
        assert_eq!(DiskListWidget::format_size(1536), "1.5K");
    }

    #[test]
    fn test_disk_list_format_size_megabytes() {
        assert_eq!(DiskListWidget::format_size(1048576), "1.0M");
        assert_eq!(DiskListWidget::format_size(1572864), "1.5M");
    }

    #[test]
    fn test_disk_list_format_size_gigabytes() {
        assert_eq!(DiskListWidget::format_size(1073741824), "1.0G");
        assert_eq!(DiskListWidget::format_size(1610612736), "1.5G");
    }

    #[test]
    fn test_disk_list_format_size_terabytes() {
        assert_eq!(DiskListWidget::format_size(1099511627776), "1.0T");
        assert_eq!(DiskListWidget::format_size(1649267441664), "1.5T");
    }

    #[test]
    fn test_compact_lines_show_selected_disk_summary() {
        let shared_data = create_shared_data();
        let mut widget = DiskListWidget::new(shared_data);
        widget.system_stats.disk_details = vec![create_disk_detail("/data")];

        let lines = widget.compact_lines(0);

        assert_eq!(lines[0], "Mount: /data");
        assert_eq!(lines[1], "Use: 40.0%");
        assert_eq!(lines[2], "Used: 40.0G / 100.0G");
        assert_eq!(lines[3], "Avail: 60.0G");
    }
}
