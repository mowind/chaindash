use log::debug;
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
    collect::SharedData,
    update::UpdatableWidget,
    widgets::block,
};

pub struct NodeDetailWidget {
    title: String,
    update_interval: Ratio<u64>,
    loading: bool,

    collect_data: SharedData,
}

impl NodeDetailWidget {
    pub fn new(collect_data: SharedData) -> NodeDetailWidget {
        NodeDetailWidget {
            title: " Node Details".to_string(),
            update_interval: Ratio::from_integer(1),
            loading: true,
            collect_data,
        }
    }

    fn render_node_details(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let headers = [
            " Name",
            "Ranking",
            "Elected Validator",
            "Blocks",
            "Block Rate",
            "24H Gen-Blocks Rate",
            "Delegated Reward Ratio",
            "Total System Reward (LAT)",
            "Reward Address",
            "Rewards (LAT)",
        ];

        let data = self.collect_data.lock().unwrap();
        let node_detail = data.node_detail();

        debug!("node detail: {:?}", node_detail);

        let raws = match node_detail {
            Some(detail) => {
                let reward_per_str = format!("{:.2}%", detail.reward_per);
                let reward_value_str = format!("{:.2}", detail.reward_value);
                let rewards_str = format!("{:.2}", detail.rewards());
                vec![Row::StyledData(
                    vec![
                        format!(" {}", detail.node_name),
                        detail.ranking.to_string(),
                        detail.verifier_time.to_string(), // Elected Validator
                        detail.block_qty.to_string(),
                        detail.block_rate,
                        detail.daily_block_rate,
                        reward_per_str,
                        reward_value_str,
                        detail.reward_address,
                        rewards_str,
                    ]
                    .into_iter(),
                    Style::default().fg(Color::Indexed(249 as u8)).bg(Color::Reset),
                )]
            },
            None => {
                let message = if self.loading {
                    "Loading..."
                } else {
                    "No node details found"
                };
                vec![Row::StyledData(
                    vec![
                        message.to_string(),
                        "".to_string(),
                        "".to_string(),
                        "".to_string(),
                        "".to_string(),
                        "".to_string(),
                        "".to_string(),
                        "".to_string(),
                        "".to_string(),
                        "".to_string(),
                    ]
                    .into_iter(),
                    Style::default().fg(Color::Indexed(249 as u8)).bg(Color::Reset),
                )]
            },
        };

        Table::new(headers.iter(), raws.into_iter())
            .block(block::new(&self.title))
            .header_style(
                Style::default()
                    .fg(Color::Indexed(249 as u8))
                    .bg(Color::Reset)
                    .modifier(Modifier::BOLD),
            )
            .widths(&[
                Constraint::Length(15),
                Constraint::Length(10),
                Constraint::Length(20),
                Constraint::Length(15),
                Constraint::Length(15),
                Constraint::Length(25),
                Constraint::Length(25),
                Constraint::Length(25),
                Constraint::Length(45),
                Constraint::Length(15),
            ])
            .column_spacing(1)
            .header_gap(0)
            .render(area, buf);
    }
}

impl UpdatableWidget for NodeDetailWidget {
    fn update(&mut self) {
        let data = self.collect_data.lock().unwrap();
        if data.node_detail().is_some() {
            self.loading = false;
        }
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &NodeDetailWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        if area.height < 3 {
            return;
        }

        self.render_node_details(area, buf);
    }
}
