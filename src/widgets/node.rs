use std::collections::HashMap;

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
        ConsensusState,
        NodeStats,
        SharedData,
    },
    update::UpdatableWidget,
    widgets::block,
};

pub struct NodeWidget {
    title: String,
    update_interval: Ratio<u64>,

    collect_data: SharedData,

    nodes: Vec<ConsensusState>,
    stats: HashMap<String, NodeStats>,
}

impl NodeWidget {
    pub fn new(collect_data: SharedData) -> NodeWidget {
        NodeWidget {
            title: " Nodes ".to_string(),
            update_interval: Ratio::from_integer(1),

            collect_data,
            nodes: Vec::new(),
            stats: HashMap::new(),
        }
    }

    fn render_without_stats(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let header =
            [" Name", "Host", "Block", "Epoch", "View", "Committed", "Locked", "QC", "Validator"];

        let rows = self.nodes.iter().map(|node| {
            Row::new(vec![
                format!(" {}", &node.name),
                format!("{}", &node.host),
                format!("{}", node.current_number),
                format!("{}", node.epoch),
                format!("{}", node.view),
                format!("{}", node.committed),
                format!("{}", node.locked),
                format!("{}", node.qc),
                format!("{}", node.validator),
            ])
            .style(Style::default().fg(Color::Indexed(249_u8)).bg(Color::Reset))
        });

        let header_row = Row::new(header.iter().copied()).style(
            Style::default()
                .fg(Color::Indexed(249_u8))
                .bg(Color::Reset)
                .add_modifier(Modifier::BOLD),
        );

        Table::new(
            rows,
            &[
                Constraint::Length(20),
                Constraint::Length(20),
                Constraint::Length(u16::max((area.width as i16 - 2 - 100 - 8) as u16, 10)),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        )
        .block(block::new(&self.title))
        .header(header_row)
        .column_spacing(1)
        .render(area, buf);
    }

    fn render_with_stats(
        &self,
        area: Rect,
        buf: &mut Buffer,
        stats: &HashMap<String, NodeStats>,
    ) {
        let header = [
            " Name",
            "Host",
            "Block",
            "Epoch",
            "View",
            "Committed",
            "Locked",
            "QC",
            "Validator",
            "CPU",
            "Memory",
            "Traffic In",
            "Traffic Out",
            "Disc Read",
            "Disc Write",
        ];

        let rows = self.nodes.iter().map(|node| {
            let stat = stats.get(&node.name).unwrap_or_default();
            let mem = stat.mem as f64 / 1024.0 / 1024.0 / 1024.0;
            let mem_limit = stat.mem_limit as f64 / 1024.0 / 1024.0 / 1024.0;
            let blk_read = stat.blk_read as f64 / 1024.0 / 1024.0 / 1024.0;
            let blk_write = stat.blk_write as f64 / 1024.0 / 1024.0 / 1024.0;
            let rx = stat.network_rx as f64 / 1024.0 / 1024.0 / 1024.0;
            let tx = stat.network_tx as f64 / 1024.0 / 1024.0 / 1024.0;
            Row::new(vec![
                format!(" {}", &node.name),
                format!("{}", &node.host),
                format!("{}", node.current_number),
                format!("{}", node.epoch),
                format!("{}", node.view),
                format!("{}", node.committed),
                format!("{}", node.locked),
                format!("{}", node.qc),
                format!("{}", node.validator),
                format!("{:.2}%", stat.cpu_percent),
                format!("{:.2}% [{:.2}GB/{:.2}GB]", stat.mem_percent, mem, mem_limit),
                format!("{:.2}GB", rx),
                format!("{:.2}GB", tx),
                format!("{:.2}GB", blk_read),
                format!("{:.2}GB", blk_write),
            ])
            .style(Style::default().fg(Color::Indexed(249_u8)).bg(Color::Reset))
        });

        let header_row = Row::new(header.iter().copied()).style(
            Style::default()
                .fg(Color::Indexed(249_u8))
                .bg(Color::Reset)
                .add_modifier(Modifier::BOLD),
        );

        Table::new(
            rows,
            &[
                Constraint::Length(20),
                Constraint::Length(20),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(u16::max((area.width as i16 - 2 - 184 - 7) as u16, 10)),
                Constraint::Length(10),
                Constraint::Length(25),
                Constraint::Length(10),
                Constraint::Length(11),
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

impl UpdatableWidget for NodeWidget {
    fn update(&mut self) {
        let collect_data = self.collect_data.lock().expect("mutex poisoned - recovering");
        self.nodes = collect_data.states();
        self.stats = collect_data.stats();
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &NodeWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        if area.height < 3 {
            return;
        }

        if !self.stats.is_empty() {
            self.render_with_stats(area, buf, &self.stats);
        } else {
            self.render_without_stats(area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::Data;

    fn create_shared_data() -> SharedData {
        Data::new()
    }

    #[test]
    fn test_node_widget_new() {
        let shared_data = create_shared_data();
        let widget = NodeWidget::new(shared_data);
        assert_eq!(widget.title, " Nodes ");
    }

    #[test]
    fn test_node_widget_update_interval() {
        let shared_data = create_shared_data();
        let widget = NodeWidget::new(shared_data);
        let interval = widget.get_update_interval();
        assert_eq!(interval, Ratio::from_integer(1));
    }

    #[test]
    fn test_node_widget_update_with_empty_data() {
        let shared_data = create_shared_data();
        let mut widget = NodeWidget::new(shared_data);
        widget.update();
        assert!(widget.nodes.is_empty());
        assert!(widget.stats.is_empty());
    }

    #[test]
    fn test_node_widget_initial_state() {
        let shared_data = create_shared_data();
        let widget = NodeWidget::new(shared_data);
        assert!(widget.nodes.is_empty());
        assert!(widget.stats.is_empty());
    }

    #[test]
    fn test_node_widget_stats_default() {
        let shared_data = create_shared_data();
        let widget = NodeWidget::new(shared_data);
        assert!(widget.stats.get("nonexistent").is_none());
    }
}
