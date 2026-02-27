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

pub struct DiskListWidget {
    update_interval: Ratio<u64>,
    collect_data: SharedData,
    system_stats: SystemStats,
}

impl DiskListWidget {
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
    use super::*;
    use crate::collect::Data;

    fn create_shared_data() -> SharedData {
        Data::new()
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
}
