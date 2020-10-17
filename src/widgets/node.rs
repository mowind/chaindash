use std::iter::IntoIterator;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{select, tick, unbounded, Sender};
use num_rational::Ratio;
use tui::buffer::Buffer;
use tui::layout::{Constraint, Rect};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Row, Table, Widget};

use crate::collect::{ConsensusState, SharedData};
use crate::update::UpdatableWidget;
use crate::widgets::block;

pub struct NodeWidget {
    title: String,
    update_interval: Ratio<u64>,

    collect_data: SharedData,

    nodes: Vec<ConsensusState>,
}

impl NodeWidget {
    pub fn new(collect_data: SharedData) -> NodeWidget {
        NodeWidget {
            title: " Nodes ".to_string(),
            update_interval: Ratio::from_integer(1),

            collect_data,
            nodes: Vec::new(),
        }
    }
}

impl UpdatableWidget for NodeWidget {
    fn update(&mut self) {
        let collect_data = self.collect_data.lock().unwrap();
        self.nodes = collect_data.states();
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

        let header = [
            " Name",
            "BlockNumber",
            "Epoch",
            "ViewNumber",
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
            Constraint::Length(u16::max((area.width as i16 - 2 - 80 - 8) as u16, 10)),
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
}
