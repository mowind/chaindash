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
        let collect_data = self.collect_data.lock().unwrap();
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
            let rows = vec![Row::StyledData(
                vec![" No disk mount points found".to_string()]
                    .into_iter(),
                Style::default().fg(Color::Indexed(249 as u8)).bg(Color::Reset),
            )];

            Table::new([""].iter(), rows.into_iter())
                .block(block::new(&title))
                .header_style(
                    Style::default()
                        .fg(Color::Indexed(249 as u8))
                        .bg(Color::Reset)
                        .modifier(Modifier::BOLD),
                )
                .widths(&[Constraint::Length(area.width)])
                .column_spacing(1)
                .header_gap(0)
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
        let header = [
            "Mounted on",
            "Size",
            "Used",
            "Avail",
            "Use%",
        ];

        // 创建行数据
        let mut rows = Vec::new();
        for (index, disk) in disk_details.iter().enumerate() {
            // 格式化大小为人类可读格式
            let total_str = Self::format_size(disk.total);
            let used_str = Self::format_size(disk.used);
            let available_str = Self::format_size(disk.available);

            // 确定颜色：告警状态用红色，网络挂载点用黄色，当前选中行高亮
            let mut style = Style::default().fg(Color::Indexed(249 as u8));
            if disk.is_alert {
                style = style.fg(Color::Red);
            } else if disk.is_network {
                style = style.fg(Color::Yellow);
            }

            // 如果是当前选中的行，添加高亮
            if index == current_index {
                style = style.modifier(Modifier::BOLD);
            }

            let row = Row::StyledData(
                vec![
                    format!(" {}", disk.mount_point),
                    format!(" {}", total_str),
                    format!(" {}", used_str),
                    format!(" {}", available_str),
                    format!(" {:.1}%", disk.usage_percent),
                ]
                .into_iter(),
                style,
            );
            rows.push(row);
        }

        // 动态计算列宽，基于可用空间
        let col_width = area.width / 5; // 5列
        Table::new(header.iter(), rows.into_iter())
            .block(block::new(&title))
            .header_style(
                Style::default()
                    .fg(Color::Indexed(249 as u8))
                    .bg(Color::Reset)
                    .modifier(Modifier::BOLD),
            )
            .widths(&[
                Constraint::Length(col_width),
                Constraint::Length(col_width),
                Constraint::Length(col_width),
                Constraint::Length(col_width),
                Constraint::Length(col_width),
            ])
            .column_spacing(1)
            .header_gap(0)
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
            format!("{}B", bytes)
        }
    }
}