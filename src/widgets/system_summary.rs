use log::debug;
use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::{
        Constraint,
        Rect,
    },
    style::{
        Color,
        Style,
    },
    text::{
        Line,
        Span,
    },
    widgets::{
        Cell,
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
    const FULL_SUMMARY_MIN_WIDTH: u16 = 133;

    fn disk_usage_color(stats: &SystemStats) -> Color {
        if stats.has_disk_alerts {
            block::ACCENT_WARN
        } else {
            block::METRIC_POSITIVE
        }
    }

    pub fn new(collect_data: SharedData) -> SystemSummaryWidget {
        SystemSummaryWidget {
            title: " System Stats ".to_string(),
            update_interval: Ratio::from_integer(2),
            collect_data,
            system_stats: SystemStats::default(),
        }
    }

    fn content_area(area: Rect) -> Rect {
        let outer_block = block::new(" System Stats ");
        let inner = outer_block.inner(area);
        Rect::new(inner.x, inner.y.saturating_add(1), inner.width, inner.height.saturating_sub(1))
    }

    fn metric_value_style(color: Color) -> Style {
        block::accent_style(color)
    }

    fn metric_cell(
        icon: &str,
        value: impl Into<String>,
        value_style: Style,
    ) -> Cell<'static> {
        Cell::from(Line::from(vec![
            Span::styled(icon.to_string(), block::muted_style()),
            Span::raw(" "),
            Span::styled(value.into(), value_style),
        ]))
    }

    fn compact_metric_cell(
        value: impl Into<String>,
        value_style: Style,
    ) -> Cell<'static> {
        Cell::from(Line::from(vec![Span::styled(value.into(), value_style)]))
    }

    fn disk_usage_cell(stats: &SystemStats) -> Cell<'static> {
        let value_style = Self::metric_value_style(Self::disk_usage_color(stats));
        let mut spans = vec![
            Span::styled("\u{f1c0}".to_string(), block::muted_style()),
            Span::raw(" "),
            Span::styled(format!("{:.2}%", stats.disk_usage_percent), value_style),
        ];
        if stats.has_disk_alerts {
            spans.push(Span::styled(" !", block::accent_style(block::ACCENT_WARN)));
        }

        Cell::from(Line::from(spans))
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
        let use_compact_layout = area.width < Self::FULL_SUMMARY_MIN_WIDTH;

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
            "CPU Usage",
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

        let rows = vec![Row::new(vec![
            Self::metric_cell(
                "\u{f085}",
                format!("{:.2}%", stats.cpu_usage),
                Self::metric_value_style(block::METRIC_PRIMARY),
            ),
            Self::metric_cell(
                "\u{f233}",
                format!("{:.2}%", stats.memory_usage_percent),
                Self::metric_value_style(block::METRIC_SECONDARY),
            ),
            Self::metric_cell(
                "\u{f02a1}",
                format!("{:.2}GB / {:.2}GB", memory_used_gb, memory_total_gb),
                Self::metric_value_style(block::METRIC_TERTIARY),
            ),
            Self::disk_usage_cell(stats),
            Self::metric_cell(
                "\u{f0a0f}",
                format!("{:.2}GB / {:.2}GB", disk_used_gb, disk_total_gb),
                Self::metric_value_style(block::METRIC_POSITIVE),
            ),
            Self::metric_cell(
                "\u{f0045}",
                format!("{:.2} MB/s", network_rx_mb),
                Self::metric_value_style(block::METRIC_NETWORK),
            ),
            Self::metric_cell(
                "\u{f005d}",
                format!("{:.2} MB/s", network_tx_mb),
                Self::metric_value_style(block::METRIC_NETWORK),
            ),
        ])];

        let header_row = Row::new(header.iter().copied()).style(block::header_style());

        let outer_block = block::new(&self.title);
        let content = Self::content_area(area);
        outer_block.render(area, buf);

        if content.width == 0 || content.height == 0 {
            return;
        }

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
        .header(header_row)
        .column_spacing(1)
        .render(content, buf);
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
            Self::compact_metric_cell(
                format!("{:.1}%", stats.cpu_usage),
                Self::metric_value_style(block::METRIC_PRIMARY),
            ),
            Self::compact_metric_cell(
                format!("{:.1}%", stats.memory_usage_percent),
                Self::metric_value_style(block::METRIC_SECONDARY),
            ),
            Self::compact_metric_cell(
                format!("{:.1}%", stats.disk_usage_percent),
                Self::metric_value_style(Self::disk_usage_color(stats)),
            ),
            Self::compact_metric_cell(
                format!("{:.1}M", network_rx_mb),
                Self::metric_value_style(block::METRIC_NETWORK),
            ),
            Self::compact_metric_cell(
                format!("{:.1}M", network_tx_mb),
                Self::metric_value_style(block::METRIC_NETWORK),
            ),
        ])];

        let header_row = Row::new(header.iter().copied()).style(block::header_style());

        let outer_block = block::new(&self.title);
        let content = Self::content_area(area);
        outer_block.render(area, buf);

        if content.width == 0 || content.height == 0 {
            return;
        }

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
        .header(header_row)
        .column_spacing(1)
        .render(content, buf);
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

    #[test]
    fn test_disk_usage_color_uses_shared_warning_and_positive_palette() {
        let mut stats = SystemStats::default();

        assert_eq!(SystemSummaryWidget::disk_usage_color(&stats), block::METRIC_POSITIVE);

        stats.has_disk_alerts = true;
        assert_eq!(SystemSummaryWidget::disk_usage_color(&stats), block::ACCENT_WARN);
    }
}
