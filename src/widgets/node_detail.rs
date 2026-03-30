use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::{
        Constraint,
        Direction,
        Layout,
        Rect,
    },
    style::Modifier,
    text::{
        Line,
        Span,
    },
    widgets::{
        Paragraph,
        Widget,
        Wrap,
    },
};

use crate::{
    collect::{
        NodeDetail,
        SharedData,
    },
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
            title: " Node Details ".to_string(),
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

    fn format_number(value: u64) -> String {
        let digits = value.to_string();
        let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
        for (index, ch) in digits.chars().rev().enumerate() {
            if index > 0 && index % 3 == 0 {
                formatted.push(',');
            }
            formatted.push(ch);
        }
        formatted.chars().rev().collect()
    }

    fn format_amount(value: f64) -> String {
        let rounded = format!("{value:.2}");
        let Some((integer, fraction)) = rounded.split_once('.') else {
            return rounded;
        };
        let integer = integer
            .parse::<u64>()
            .ok()
            .map(Self::format_number)
            .unwrap_or_else(|| integer.to_string());
        format!("{integer}.{fraction}")
    }

    fn shorten_address(address: &str) -> String {
        const MAX_LEN: usize = 24;
        const PREFIX_LEN: usize = 10;
        const SUFFIX_LEN: usize = 8;

        if address.len() <= MAX_LEN {
            return address.to_string();
        }

        format!(
            "{}…{}",
            &address[..PREFIX_LEN.min(address.len())],
            &address[address.len().saturating_sub(SUFFIX_LEN)..]
        )
    }

    fn section_heading(title: &str) -> Line<'static> {
        Line::from(vec![Span::styled(title.to_string(), block::header_style())])
    }

    fn detail_line(
        label: &str,
        value: impl Into<String>,
    ) -> Line<'static> {
        Self::detail_line_with_style(
            label,
            value,
            block::content_style().add_modifier(Modifier::BOLD),
        )
    }

    fn detail_line_with_style(
        label: &str,
        value: impl Into<String>,
        value_style: ratatui::style::Style,
    ) -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("{label}: "), block::muted_style()),
            Span::styled(value.into(), value_style),
        ])
    }

    fn detail_columns(detail: &NodeDetail) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
        let metric_style = block::content_style()
            .fg(ratatui::style::Color::LightCyan)
            .add_modifier(Modifier::BOLD);
        let reward_style = block::content_style()
            .fg(ratatui::style::Color::LightGreen)
            .add_modifier(Modifier::BOLD);
        let address_style = block::content_style().fg(block::PANEL_TITLE);

        let left = vec![
            Self::section_heading("Node"),
            Self::detail_line("Name", detail.node_name.clone()),
            Self::detail_line_with_style(
                "Ranking",
                Self::format_number(detail.ranking.max(0) as u64),
                metric_style,
            ),
            Self::detail_line_with_style(
                "Blocks",
                Self::format_number(detail.block_qty),
                metric_style,
            ),
            Self::detail_line_with_style("Block Rate", detail.block_rate.clone(), reward_style),
            Self::detail_line_with_style("24H Rate", detail.daily_block_rate.clone(), metric_style),
        ];
        let right = vec![
            Self::section_heading("Rewards"),
            Self::detail_line_with_style(
                "Verifier Time",
                Self::format_number(detail.verifier_time),
                metric_style,
            ),
            Self::detail_line_with_style(
                "Reward Ratio",
                format!("{:.2}%", detail.reward_per),
                reward_style,
            ),
            Self::detail_line_with_style(
                "System Reward",
                format!("{} LAT", Self::format_amount(detail.reward_value)),
                metric_style,
            ),
            Self::detail_line_with_style(
                "Rewards",
                format!("{} LAT", Self::format_amount(detail.rewards())),
                reward_style,
            ),
            Self::detail_line_with_style(
                "Reward Address",
                Self::shorten_address(&detail.reward_address),
                address_style,
            ),
        ];

        (left, right)
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

        let outer_block = block::new(&self.title);
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
        if content.width == 0 || content.height == 0 {
            return;
        }

        let detail = {
            let data = self.collect_data.lock().expect("mutex poisoned - recovering");
            data.node_detail()
        };

        let Some(detail) = detail else {
            Paragraph::new(vec![Line::raw(self.empty_message())])
                .style(block::content_style())
                .wrap(Wrap { trim: true })
                .render(content, buf);
            return;
        };

        let (left_lines, right_lines) = Self::detail_columns(&detail);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(44), Constraint::Percentage(56)].as_ref())
            .split(content);
        let left_area = Rect::new(
            columns[0].x,
            columns[0].y,
            columns[0].width.saturating_sub(1),
            columns[0].height,
        );
        let right_area = Rect::new(
            columns[1].x.saturating_add(1),
            columns[1].y,
            columns[1].width.saturating_sub(1),
            columns[1].height,
        );

        Paragraph::new(left_lines)
            .style(block::content_style())
            .wrap(Wrap { trim: true })
            .render(left_area, buf);
        Paragraph::new(right_lines)
            .style(block::content_style())
            .wrap(Wrap { trim: true })
            .render(right_area, buf);
    }

    fn compact_lines(&self) -> Vec<String> {
        let data = self.collect_data.lock().expect("mutex poisoned - recovering");
        match data.node_detail() {
            Some(detail) => vec![
                format!("Name: {}", detail.node_name),
                format!("Rank: {}    Verifier: {}", detail.ranking, detail.verifier_time),
                format!(
                    "Blocks: {}    Rate: {}",
                    Self::format_number(detail.block_qty),
                    detail.block_rate
                ),
                format!("24H: {}", detail.daily_block_rate),
                format!("Reward Ratio: {:.2}%", detail.reward_per),
                format!("System Reward: {} LAT", Self::format_amount(detail.reward_value)),
                format!("Reward Address: {}", Self::shorten_address(&detail.reward_address)),
                format!("Rewards: {} LAT", Self::format_amount(detail.rewards())),
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
            .style(block::content_style())
            .wrap(Wrap { trim: true })
            .render(area, buf);
    }
}

impl UpdatableWidget for NodeDetailWidget {
    fn update(&mut self) {
        let data = self.collect_data.lock().expect("mutex poisoned - recovering");
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

    fn sample_detail() -> NodeDetail {
        NodeDetail {
            node_name: "node-a".to_string(),
            ranking: 7,
            block_qty: 123_456,
            block_rate: "12.34%".to_string(),
            daily_block_rate: "3/day".to_string(),
            reward_per: 5.0,
            reward_value: 12_345.67,
            reward_address: "lat1zytcgvw35sagn722cneh6sz92y8j3dp8gqj5h".to_string(),
            verifier_time: 9,
        }
    }

    #[test]
    fn test_node_detail_widget_new() {
        let shared_data = create_shared_data();
        let widget = NodeDetailWidget::new(shared_data);
        assert_eq!(widget.title, " Node Details ");
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
    fn test_format_number_adds_grouping_separators() {
        assert_eq!(NodeDetailWidget::format_number(123_456_789), "123,456,789");
    }

    #[test]
    fn test_format_amount_adds_grouping_separators() {
        assert_eq!(NodeDetailWidget::format_amount(12_345.67), "12,345.67");
    }

    #[test]
    fn test_shorten_address_preserves_prefix_and_suffix() {
        assert_eq!(
            NodeDetailWidget::shorten_address("lat1zytcgvw35sagn722cneh6sz92y8j3dp8gqj5h"),
            "lat1zytcgv…dp8gqj5h"
        );
    }

    #[test]
    fn test_detail_columns_show_formatted_values() {
        let (left, right) = NodeDetailWidget::detail_columns(&sample_detail());

        assert_eq!(left[0].spans[0].content, "Node");
        assert_eq!(left[3].spans[1].content, "123,456");
        assert_eq!(right[0].spans[0].content, "Rewards");
        assert_eq!(right[3].spans[1].content, "12,345.67 LAT");
        assert_eq!(right[5].spans[1].content, "lat1zytcgv…dp8gqj5h");
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
            data.update_node_detail(Some(sample_detail()));
        }

        let widget = NodeDetailWidget::new(shared_data);
        let lines = widget.compact_lines();

        assert_eq!(lines[0], "Name: node-a");
        assert_eq!(lines[1], "Rank: 7    Verifier: 9");
        assert_eq!(lines[2], "Blocks: 123,456    Rate: 12.34%");
        assert_eq!(lines[7], "Rewards: 11,728.39 LAT");
    }
}
