use std::collections::HashMap;
use std::iter::IntoIterator;

use num_rational::Ratio;
use tui::buffer::Buffer;
use tui::layout::{Constraint, Rect};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Row, Table, Widget};

use crate::collect::{ConsensusState, NodeStats, SharedData};
use crate::update::UpdatableWidget;
use crate::widgets::block;

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

    fn render_without_stats(&self, area: Rect, buf: &mut Buffer) {
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
        ];

        let nodes = self.nodes.clone();

        Table::new(
            header.iter(),
            nodes.into_iter().map(|node| {
                Row::StyledData(
                    vec![
                        format!(" {}", node.name),
                        format!("{}", node.host),
                        format!("{}", node.current_number),
                        format!("{}", node.epoch),
                        format!("{}", node.view),
                        format!("{}", node.committed),
                        format!("{}", node.locked),
                        format!("{}", node.qc),
                        format!("{}", node.validator),
                    ]
                    .into_iter(),
                    Style::default()
                        .fg(Color::Indexed(249 as u8))
                        .bg(Color::Reset),
                )
            }),
        )
        .block(block::new(&self.title))
        .header_style(
            Style::default()
                .fg(Color::Indexed(249 as u8))
                .bg(Color::Reset)
                .modifier(Modifier::BOLD),
        )
        .widths(&[
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Length(u16::max((area.width as i16 - 2 - 100 - 8) as u16, 10)),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
        ])
        .column_spacing(1)
        .header_gap(0)
        .render(area, buf);
    }

    fn render_with_stats(&self, area: Rect, buf: &mut Buffer, stats: &HashMap<String, NodeStats>) {
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

        let nodes = self.nodes.clone();

        Table::new(
            header.iter(),
            nodes.into_iter().map(|node| {
                let stat = stats.get(&node.name).unwrap_or_default();
                let mem = stat.mem as f64 / 1024.0 / 1024.0 / 1024.0;
                let mem_limit = stat.mem_limit as f64 / 1024.0 / 1024.0 / 1024.0;
                let blk_read = stat.blk_read as f64 / 1024.0 / 1024.0 / 1024.0;
                let blk_write = stat.blk_write as f64 / 1024.0 / 1024.0 / 1024.0;
                let rx = stat.network_tx as f64 / 1024.0 / 1024.0 / 1024.0;
                let tx = stat.network_tx as f64 / 1024.0 / 1024.0 / 1024.0;
                Row::StyledData(
                    vec![
                        format!(" {}", node.name),
                        format!("{}", node.host),
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
                    ]
                    .into_iter(),
                    Style::default()
                        .fg(Color::Indexed(249 as u8))
                        .bg(Color::Reset),
                )
            }),
        )
        .block(block::new(&self.title))
        .header_style(
            Style::default()
                .fg(Color::Indexed(249 as u8))
                .bg(Color::Reset)
                .modifier(Modifier::BOLD),
        )
        .widths(&[
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
        ])
        .column_spacing(1)
        .header_gap(0)
        .render(area, buf);
    }
}

impl UpdatableWidget for NodeWidget {
    fn update(&mut self) {
        let collect_data = self.collect_data.lock().unwrap();
        self.nodes = collect_data.states();
        self.stats = collect_data.stats();
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &NodeWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 {
            return;
        }

        let stats = self.stats.clone();
        if stats.len() > 0 {
            self.render_with_stats(area, buf, &stats);
        } else {
            self.render_without_stats(area, buf);
        }
    }
}
