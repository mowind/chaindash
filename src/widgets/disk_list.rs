use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::{
        Constraint,
        Rect,
    },
    style::{
        Modifier,
        Style,
    },
    text::{
        Line,
        Span,
    },
    widgets::{
        Cell,
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
    const SUMMARY_WIDTH: u16 = 72;
    const SUMMARY_HEIGHT: u16 = 6;

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
    fn mount_value_style(is_selected: bool) -> Style {
        if is_selected {
            block::highlight_style().add_modifier(Modifier::BOLD)
        } else {
            block::content_style().add_modifier(Modifier::BOLD)
        }
    }

    fn size_value_style() -> Style {
        block::accent_style(block::METRIC_PRIMARY)
    }

    fn available_value_style() -> Style {
        block::accent_style(block::METRIC_POSITIVE)
    }

    fn percent_value_style(is_alert: bool) -> Style {
        let color = if is_alert {
            block::ACCENT_WARN
        } else {
            block::METRIC_POSITIVE
        };
        block::accent_style(color)
    }

    fn filesystem_value_style() -> Style {
        block::highlight_style()
    }

    fn network_tag_style() -> Style {
        block::accent_style(block::ACCENT_WARN)
    }

    fn table_row_values(
        mount: String,
        total: String,
        used: String,
        available: String,
        usage: String,
    ) -> Vec<String> {
        vec![mount, total, used, available, usage]
    }

    fn render_disk_list(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let stats = &self.system_stats;
        let disk_details = &stats.disk_details;
        let current_index = stats.current_disk_index.min(disk_details.len().saturating_sub(1));

        if disk_details.is_empty() {
            let outer_block = block::new(" Disk Details ");
            let inner = outer_block.inner(area);
            outer_block.render(area, buf);

            if inner.width == 0 || inner.height == 0 {
                return;
            }

            let content = Rect::new(
                inner.x,
                inner.y.saturating_add(1),
                inner.width,
                inner.height.saturating_sub(1),
            );
            Paragraph::new(vec![Line::raw("No disk mount points found")])
                .style(block::empty_state_style())
                .wrap(Wrap { trim: true })
                .render(content, buf);
            return;
        }

        let title = if disk_details.len() > 1 {
            format!(" Disk Details [{}/{}] (Tab to switch) ", current_index + 1, disk_details.len())
        } else {
            " Disk Details ".to_string()
        };

        if area.width < Self::SUMMARY_WIDTH || area.height <= Self::SUMMARY_HEIGHT {
            self.render_selected_disk_summary(area, buf, &title, current_index);
            return;
        }

        let header = ["Mounted on", "Size", "Used", "Avail", "Use%"];
        let rows = disk_details.iter().enumerate().map(|(index, disk)| {
            let is_selected = index == current_index;
            let mount = if is_selected {
                format!("> {}", disk.mount_point)
            } else {
                format!("  {}", disk.mount_point)
            };
            let values = Self::table_row_values(
                mount,
                Self::format_size(disk.total),
                Self::format_size(disk.used),
                Self::format_size(disk.available),
                format!("{:.1}%", disk.usage_percent),
            );

            Row::new(vec![
                Cell::from(values[0].clone()).style(Self::mount_value_style(is_selected)),
                Cell::from(values[1].clone()).style(Self::size_value_style()),
                Cell::from(values[2].clone()).style(Self::size_value_style()),
                Cell::from(values[3].clone()).style(Self::available_value_style()),
                Cell::from(values[4].clone()).style(Self::percent_value_style(disk.is_alert)),
            ])
        });

        Table::new(
            rows,
            &[
                Constraint::Length(Self::mount_column_width(area.width)),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(7),
            ],
        )
        .block(block::new(&title))
        .header(Row::new(header.iter().copied()).style(block::header_style()))
        .column_spacing(1)
        .render(area, buf);
    }

    fn mount_column_width(area_width: u16) -> u16 {
        area_width.saturating_sub(2).saturating_sub(41).max(14)
    }

    #[cfg(test)]
    fn summary_lines(
        &self,
        current_index: usize,
    ) -> Vec<String> {
        let disk = &self.system_stats.disk_details[current_index];
        let mount_label = if disk.is_network {
            format!("Mount: {} [net]", disk.mount_point)
        } else {
            format!("Mount: {}", disk.mount_point)
        };
        let usage_label = if disk.is_alert {
            format!("{:.1}% [alert]", disk.usage_percent)
        } else {
            format!("{:.1}%", disk.usage_percent)
        };

        vec![
            mount_label,
            format!(
                "Used: {} / {} · {}",
                Self::format_size(disk.used),
                Self::format_size(disk.total),
                usage_label
            ),
            format!("Free: {} · FS {}", Self::format_size(disk.available), disk.filesystem),
        ]
    }

    fn render_selected_disk_summary(
        &self,
        area: Rect,
        buf: &mut Buffer,
        title: &str,
        current_index: usize,
    ) {
        let disk = &self.system_stats.disk_details[current_index];
        let mount_line = if disk.is_network {
            Line::from(vec![
                Span::styled("Mount: ", block::muted_style()),
                Span::styled(disk.mount_point.clone(), Self::mount_value_style(true)),
                Span::styled(" [net]", Self::network_tag_style()),
            ])
        } else {
            Line::from(vec![
                Span::styled("Mount: ", block::muted_style()),
                Span::styled(disk.mount_point.clone(), Self::mount_value_style(true)),
            ])
        };
        let used_line = if disk.is_alert {
            Line::from(vec![
                Span::styled("Used: ", block::muted_style()),
                Span::styled(Self::format_size(disk.used), Self::size_value_style()),
                Span::styled(" / ", block::muted_style()),
                Span::styled(Self::format_size(disk.total), Self::size_value_style()),
                Span::styled(" · ", block::muted_style()),
                Span::styled(
                    format!("{:.1}%", disk.usage_percent),
                    Self::percent_value_style(true),
                ),
                Span::styled(" [alert]", Self::percent_value_style(true)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Used: ", block::muted_style()),
                Span::styled(Self::format_size(disk.used), Self::size_value_style()),
                Span::styled(" / ", block::muted_style()),
                Span::styled(Self::format_size(disk.total), Self::size_value_style()),
                Span::styled(" · ", block::muted_style()),
                Span::styled(
                    format!("{:.1}%", disk.usage_percent),
                    Self::percent_value_style(false),
                ),
            ])
        };
        let free_line = Line::from(vec![
            Span::styled("Free: ", block::muted_style()),
            Span::styled(Self::format_size(disk.available), Self::available_value_style()),
            Span::styled(" · FS ", block::muted_style()),
            Span::styled(disk.filesystem.clone(), Self::filesystem_value_style()),
        ]);
        let lines = vec![mount_line, used_line, free_line];

        let outer_block = block::new(title);
        let inner = outer_block.inner(area);
        outer_block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let content = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );

        Paragraph::new(lines).wrap(Wrap { trim: true }).render(content, buf);
    }

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
    fn test_mount_column_width_saturates_for_narrow_area() {
        assert_eq!(DiskListWidget::mount_column_width(40), 14);
        assert_eq!(DiskListWidget::mount_column_width(80), 37);
    }

    #[test]
    fn test_table_row_values_include_expected_columns() {
        let values = DiskListWidget::table_row_values(
            "> /data".to_string(),
            "100.0G".to_string(),
            "40.0G".to_string(),
            "60.0G".to_string(),
            "40.0%".to_string(),
        );

        assert_eq!(values, vec!["> /data", "100.0G", "40.0G", "60.0G", "40.0%"]);
    }

    #[test]
    fn test_disk_highlight_style_helpers_match_expected_colors() {
        assert_eq!(DiskListWidget::mount_value_style(true).fg, Some(block::CONTENT_HIGHLIGHT));
        assert_eq!(DiskListWidget::size_value_style().fg, Some(block::METRIC_PRIMARY));
        assert_eq!(DiskListWidget::available_value_style().fg, Some(block::METRIC_POSITIVE));
        assert_eq!(DiskListWidget::percent_value_style(false).fg, Some(block::METRIC_POSITIVE));
        assert_eq!(DiskListWidget::percent_value_style(true).fg, Some(block::ACCENT_WARN));
        assert_eq!(DiskListWidget::filesystem_value_style().fg, Some(block::CONTENT_HIGHLIGHT));
    }

    #[test]
    fn test_summary_lines_show_selected_disk_summary() {
        let shared_data = create_shared_data();
        let mut widget = DiskListWidget::new(shared_data);
        widget.system_stats.disk_details = vec![create_disk_detail("/data")];

        let lines = widget.summary_lines(0);

        assert_eq!(lines[0], "Mount: /data");
        assert_eq!(lines[1], "Used: 40.0G / 100.0G · 40.0%");
        assert_eq!(lines[2], "Free: 60.0G · FS ext4");
    }
}
