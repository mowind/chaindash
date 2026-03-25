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
    text::Line,
    widgets::{
        Paragraph,
        Row,
        Table,
        Widget,
        Wrap,
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
    const COMPACT_LAYOUT_WIDTH: u16 = 110;

    pub fn new(collect_data: SharedData) -> NodeDetailWidget {
        NodeDetailWidget {
            title: " Node Details".to_string(),
            update_interval: Ratio::from_integer(1),
            loading: true,
            collect_data,
        }
    }

    fn empty_message(&self) -> &'static str {
        if self.loading {
            "Loading..."
        } else {
            "No node details found"
        }
    }

    fn render_node_details(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        if area.width < Self::COMPACT_LAYOUT_WIDTH {
            self.render_compact_node_details(area, buf);
            return;
        }

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

        let rows: Vec<Row> = match node_detail {
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
            None => vec![Row::new(vec![
                self.empty_message().to_string(),
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
            .style(Style::default().fg(Color::Indexed(249_u8)).bg(Color::Reset))],
        };

        let header_row = Row::new(headers.iter().copied()).style(
            Style::default()
                .fg(Color::Indexed(249_u8))
                .bg(Color::Reset)
                .add_modifier(Modifier::BOLD),
        );

        Table::new(
            rows,
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

    fn compact_lines(&self) -> Vec<String> {
        let data = self.collect_data.lock().expect("mutex poisoned - recovering");
        match data.node_detail() {
            Some(detail) => vec![
                format!("Name: {}", detail.node_name),
                format!("Rank: {}    Validator: {}", detail.ranking, detail.verifier_time),
                format!("Blocks: {}    Rate: {}", detail.block_qty, detail.block_rate),
                format!("24H: {}", detail.daily_block_rate),
                format!("Reward Ratio: {:.2}%", detail.reward_per),
                format!("System Reward: {:.2} LAT", detail.reward_value),
                format!("Reward Address: {}", detail.reward_address),
                format!("Rewards: {:.2} LAT", detail.rewards()),
            ],
            None => vec![self.empty_message().to_string()],
        }
    }

    fn render_compact_node_details(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let lines: Vec<Line> = self.compact_lines().into_iter().map(Line::raw).collect();

        Paragraph::new(lines)
            .block(block::new(&self.title))
            .style(Style::default().fg(Color::Indexed(249_u8)).bg(Color::Reset))
            .wrap(Wrap { trim: true })
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
    use crate::collect::{
        Data,
        NodeDetail,
    };

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

    #[test]
    fn test_empty_message_reflects_loading_state() {
        let shared_data = create_shared_data();
        let mut widget = NodeDetailWidget::new(shared_data);
        assert_eq!(widget.empty_message(), "Loading...");

        widget.loading = false;
        assert_eq!(widget.empty_message(), "No node details found");
    }

    #[test]
    fn test_compact_lines_without_data_uses_empty_message() {
        let shared_data = create_shared_data();
        let widget = NodeDetailWidget::new(shared_data);

        assert_eq!(widget.compact_lines(), vec!["Loading...".to_string()]);
    }

    #[test]
    fn test_compact_lines_with_data_include_key_fields() {
        let shared_data = create_shared_data();
        {
            let mut data = shared_data.lock().expect("mutex poisoned");
            data.update_node_detail(Some(NodeDetail {
                node_name: "node-a".to_string(),
                ranking: 7,
                block_qty: 123,
                block_rate: "12.34%".to_string(),
                daily_block_rate: "3/day".to_string(),
                reward_per: 5.0,
                reward_value: 42.5,
                reward_address: "addr".to_string(),
                verifier_time: 9,
            }));
        }

        let widget = NodeDetailWidget::new(shared_data);
        let lines = widget.compact_lines();

        assert_eq!(lines[0], "Name: node-a");
        assert_eq!(lines[1], "Rank: 7    Validator: 9");
        assert_eq!(lines[2], "Blocks: 123    Rate: 12.34%");
        assert_eq!(lines[7], "Rewards: 40.38 LAT");
    }
}
