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

        let data = self.collect_data.lock().expect("mutex poisoned - recovering");
        let node_detail = data.node_detail();

        debug!("node detail: {:?}", node_detail);

        let raws: Vec<Row> = match node_detail {
            Some(detail) => {
                let reward_per_str = format!("{:.2}%", detail.reward_per);
                let reward_value_str = format!("{:.2}", detail.reward_value);
                let rewards_str = format!("{:.2}", detail.rewards());
                vec![Row::new(vec![
                    format!(" {}", detail.node_name),
                    detail.ranking.to_string(),
                    detail.verifier_time.to_string(), // Elected Validator
                    detail.block_qty.to_string(),
                    detail.block_rate.clone(),
                    detail.daily_block_rate.clone(),
                    reward_per_str,
                    reward_value_str,
                    detail.reward_address.clone(),
                    rewards_str,
                ])
                .style(Style::default().fg(Color::Indexed(249_u8)).bg(Color::Reset))]
            },
            None => {
                let message = if self.loading {
                    "Loading..."
                } else {
                    "No node details found"
                };
                vec![Row::new(vec![
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
                ])
                .style(Style::default().fg(Color::Indexed(249_u8)).bg(Color::Reset))]
            },
        };

        let header_row = Row::new(headers.iter().copied()).style(
            Style::default()
                .fg(Color::Indexed(249_u8))
                .bg(Color::Reset)
                .add_modifier(Modifier::BOLD),
        );

        Table::new(
            raws,
            &[
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
            ],
        )
        .block(block::new(&self.title))
        .header(header_row)
        .column_spacing(1)
        .render(area, buf);
    }
}

impl UpdatableWidget for NodeDetailWidget {
    fn update(&mut self) {
        let data = self.collect_data.lock().expect("mutex poisoned - recovering");
        // Update loading state: loading is true only when data is None
        // This handles the case when data fetch fails after initial success
        self.loading = data.node_detail().is_none();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::Data;

    fn create_shared_data() -> SharedData {
        Data::new()
    }

    #[test]
    fn test_node_detail_widget_new() {
        let shared_data = create_shared_data();
        let widget = NodeDetailWidget::new(shared_data);
        assert_eq!(widget.title, " Node Details");
    }

    #[test]
    fn test_node_detail_widget_update_interval() {
        let shared_data = create_shared_data();
        let widget = NodeDetailWidget::new(shared_data);
        let interval = widget.get_update_interval();
        assert_eq!(interval, Ratio::from_integer(1));
    }

    #[test]
    fn test_node_detail_widget_initial_loading_state() {
        let shared_data = create_shared_data();
        let widget = NodeDetailWidget::new(shared_data);
        assert!(widget.loading);
    }

    #[test]
    fn test_node_detail_widget_update_with_no_detail() {
        let shared_data = create_shared_data();
        let mut widget = NodeDetailWidget::new(shared_data);
        widget.update();
        assert!(widget.loading);
    }
}
