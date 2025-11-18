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

        let header =
            [" CPU Usage", "Memory Usage", "Memory Used/Total", "Network RX", "Network TX"];

        let stats = &self.system_stats;

        // 格式化数据
        let memory_used_gb = stats.memory_used as f64 / 1024.0 / 1024.0 / 1024.0;
        let memory_total_gb = stats.memory_total as f64 / 1024.0 / 1024.0 / 1024.0;
        let network_rx_mb = stats.network_rx as f64 / 1024.0 / 1024.0;
        let network_tx_mb = stats.network_tx as f64 / 1024.0 / 1024.0;

        let rows = vec![Row::StyledData(
            vec![
                format!(" \u{f0ee0} {:.2}%", stats.cpu_usage),
                format!("\u{f035b} {:.2}%", stats.memory_usage_percent),
                format!(" {:.2}GB / {:.2}GB", memory_used_gb, memory_total_gb),
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
                Constraint::Length(15),
            ])
            .column_spacing(1)
            .header_gap(0)
            .render(area, buf);
    }
}
