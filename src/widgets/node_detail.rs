use std::collections::BTreeSet;

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

type PriorityLine = (u8, Line<'static>);
type PriorityLines = Vec<PriorityLine>;
type DoublePriorityLines = (PriorityLines, PriorityLines);

pub struct NodeDetailWidget {
    title: String,
    update_interval: Ratio<u64>,
    loading: bool,

    collect_data: SharedData,
}

impl NodeDetailWidget {
    const COMPACT_LAYOUT_WIDTH: u16 = 110;
    const COMPACT_TWO_COLUMN_WIDTH: u16 = 72;
    const STACKED_LAYOUT_WIDTH: u16 = 150;
    const STACKED_LAYOUT_HEIGHT: u16 = 9;
    const HEADING_LAYOUT_HEIGHT: u16 = 5;
    const INLINE_RIGHT_PADDING: u16 = 3;

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

    fn shorten_address_for_width(
        address: &str,
        max_len: usize,
    ) -> String {
        const MIN_PREFIX_LEN: usize = 8;
        const MAX_SUFFIX_LEN: usize = 8;
        const MIN_SUFFIX_LEN: usize = 6;

        if max_len == 0 || address.len() <= max_len {
            return address.to_string();
        }

        if max_len <= MIN_PREFIX_LEN + MIN_SUFFIX_LEN + 1 {
            return Self::shorten_address(address);
        }

        let suffix_len = ((max_len - 1) / 3).clamp(MIN_SUFFIX_LEN, MAX_SUFFIX_LEN);
        let prefix_len = max_len.saturating_sub(suffix_len + 1);

        format!(
            "{}…{}",
            &address[..prefix_len.min(address.len())],
            &address[address.len().saturating_sub(suffix_len)..]
        )
    }

    fn select_prioritized_lines(
        specs: PriorityLines,
        max_rows: u16,
    ) -> Vec<Line<'static>> {
        let max_rows = max_rows as usize;
        if max_rows == 0 {
            return Vec::new();
        }

        if specs.len() <= max_rows {
            return specs.into_iter().map(|(_, line)| line).collect();
        }

        let mut ranked: Vec<(usize, u8)> =
            specs.iter().enumerate().map(|(index, (priority, _))| (index, *priority)).collect();
        ranked.sort_by_key(|(index, priority)| (*priority, *index));

        let keep: BTreeSet<usize> =
            ranked.into_iter().take(max_rows).map(|(index, _)| index).collect();

        specs
            .into_iter()
            .enumerate()
            .filter_map(|(index, (_, line))| keep.contains(&index).then_some(line))
            .collect()
    }

    fn inline_value_max_len(
        area_width: u16,
        label: &str,
    ) -> usize {
        area_width.saturating_sub(label.len() as u16).saturating_sub(Self::INLINE_RIGHT_PADDING)
            as usize
    }

    fn section_heading(title: &str) -> Line<'static> {
        Line::from(vec![Span::styled(title.to_string(), block::header_style())])
    }

    fn spacer_line() -> Line<'static> {
        Line::from("")
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

    fn metric_value_style() -> ratatui::style::Style {
        block::content_style().fg(ratatui::style::Color::LightCyan).add_modifier(Modifier::BOLD)
    }

    fn reward_value_style() -> ratatui::style::Style {
        block::content_style().fg(ratatui::style::Color::LightGreen).add_modifier(Modifier::BOLD)
    }

    fn address_value_style() -> ratatui::style::Style {
        block::content_style().fg(block::PANEL_TITLE)
    }

    fn detail_column_specs(
        detail: &NodeDetail,
        show_section_headings: bool,
        address_max_len: usize,
    ) -> DoublePriorityLines {
        let metric_style = block::content_style()
            .fg(ratatui::style::Color::LightCyan)
            .add_modifier(Modifier::BOLD);
        let reward_style = block::content_style()
            .fg(ratatui::style::Color::LightGreen)
            .add_modifier(Modifier::BOLD);
        let address_style = block::content_style().fg(block::PANEL_TITLE);

        let mut left = Vec::new();
        if show_section_headings {
            left.push((9, Self::section_heading("Node")));
        }
        left.extend([
            (1, Self::detail_line("Name", detail.node_name.clone())),
            (
                2,
                Self::detail_line_with_style(
                    "Ranking",
                    Self::format_number(detail.ranking.max(0) as u64),
                    metric_style,
                ),
            ),
            (
                3,
                Self::detail_line_with_style(
                    "Blocks",
                    Self::format_number(detail.block_qty),
                    metric_style,
                ),
            ),
            (
                4,
                Self::detail_line_with_style("Block Rate", detail.block_rate.clone(), reward_style),
            ),
            (
                5,
                Self::detail_line_with_style(
                    "24H Rate",
                    detail.daily_block_rate.clone(),
                    metric_style,
                ),
            ),
        ]);

        let mut right = Vec::new();
        if show_section_headings {
            right.push((9, Self::section_heading("Rewards")));
        }
        right.extend([
            (
                1,
                Self::detail_line_with_style(
                    "Verifier Time",
                    Self::format_number(detail.verifier_time),
                    metric_style,
                ),
            ),
            (
                2,
                Self::detail_line_with_style(
                    "Reward Ratio",
                    format!("{:.2}%", detail.reward_per),
                    reward_style,
                ),
            ),
            (
                3,
                Self::detail_line_with_style(
                    "System Reward",
                    format!("{} LAT", Self::format_amount(detail.reward_value)),
                    metric_style,
                ),
            ),
            (
                4,
                Self::detail_line_with_style(
                    "Rewards",
                    format!("{} LAT", Self::format_amount(detail.rewards())),
                    reward_style,
                ),
            ),
            (
                5,
                Self::detail_line_with_style(
                    "Reward Address",
                    Self::shorten_address_for_width(&detail.reward_address, address_max_len),
                    address_style,
                ),
            ),
        ]);

        (left, right)
    }

    fn stacked_line_specs(
        detail: &NodeDetail,
        show_section_headings: bool,
        address_max_len: usize,
    ) -> PriorityLines {
        let metric_style = Self::metric_value_style();
        let reward_style = Self::reward_value_style();
        let address_style = Self::address_value_style();
        let mut lines = Vec::new();

        if show_section_headings {
            lines.push((20, Self::section_heading("Node")));
        }
        lines.push((1, Self::detail_line("Name", detail.node_name.clone())));
        lines.push((
            2,
            Self::detail_line_with_style(
                "Ranking",
                Self::format_number(detail.ranking.max(0) as u64),
                metric_style,
            ),
        ));
        lines.push((
            3,
            Self::detail_line_with_style(
                "Blocks",
                Self::format_number(detail.block_qty),
                metric_style,
            ),
        ));
        lines.push((
            4,
            Self::detail_line_with_style("Block Rate", detail.block_rate.clone(), reward_style),
        ));
        lines.push((
            5,
            Self::detail_line_with_style("24H", detail.daily_block_rate.clone(), metric_style),
        ));

        if show_section_headings {
            lines.push((30, Self::spacer_line()));
            lines.push((20, Self::section_heading("Rewards")));
        }
        lines.push((
            6,
            Self::detail_line_with_style(
                "Verifier",
                Self::format_number(detail.verifier_time),
                metric_style,
            ),
        ));
        lines.push((
            7,
            Self::detail_line_with_style(
                "Ratio",
                format!("{:.2}%", detail.reward_per),
                reward_style,
            ),
        ));
        lines.push((
            8,
            Self::detail_line_with_style(
                "System",
                format!("{} LAT", Self::format_amount(detail.reward_value)),
                metric_style,
            ),
        ));
        lines.push((
            9,
            Self::detail_line_with_style(
                "Rewards",
                format!("{} LAT", Self::format_amount(detail.rewards())),
                reward_style,
            ),
        ));
        lines.push((
            10,
            Self::detail_line_with_style(
                "Address",
                Self::shorten_address_for_width(&detail.reward_address, address_max_len),
                address_style,
            ),
        ));

        lines
    }

    fn visible_stacked_lines(
        detail: &NodeDetail,
        show_section_headings: bool,
        address_max_len: usize,
        max_rows: u16,
    ) -> Vec<Line<'static>> {
        Self::select_prioritized_lines(
            Self::stacked_line_specs(detail, show_section_headings, address_max_len),
            max_rows,
        )
    }

    #[cfg(test)]
    fn stacked_lines(
        detail: &NodeDetail,
        show_section_headings: bool,
        address_max_len: usize,
    ) -> Vec<Line<'static>> {
        Self::visible_stacked_lines(detail, show_section_headings, address_max_len, u16::MAX)
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

        let content = inner;
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
                .render(content, buf);
            return;
        };

        let show_section_headings = content.height >= Self::HEADING_LAYOUT_HEIGHT;
        if content.width < Self::STACKED_LAYOUT_WIDTH {
            if content.height >= Self::STACKED_LAYOUT_HEIGHT {
                let address_max_len = Self::inline_value_max_len(content.width, "Address: ");
                let lines = Self::visible_stacked_lines(
                    &detail,
                    show_section_headings,
                    address_max_len,
                    content.height,
                );
                Paragraph::new(lines).style(block::content_style()).render(content, buf);
            } else {
                self.render_compact_node_details(area, buf);
            }
            return;
        }

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
        let address_max_len = Self::inline_value_max_len(right_area.width, "Reward Address: ");
        let (left_specs, right_specs) =
            Self::detail_column_specs(&detail, show_section_headings, address_max_len);
        let left_lines = Self::select_prioritized_lines(left_specs, left_area.height);
        let right_lines = Self::select_prioritized_lines(right_specs, right_area.height);

        Paragraph::new(left_lines).style(block::content_style()).render(left_area, buf);
        Paragraph::new(right_lines).style(block::content_style()).render(right_area, buf);
    }

    fn compact_line_specs(
        detail: &NodeDetail,
        address_max_len: usize,
    ) -> PriorityLines {
        let metric_style = Self::metric_value_style();
        let reward_style = Self::reward_value_style();
        let address_style = Self::address_value_style();

        vec![
            (1, Self::detail_line("Name", detail.node_name.clone())),
            (2, Self::detail_line_with_style("Rank", detail.ranking.to_string(), metric_style)),
            (
                3,
                Self::detail_line_with_style(
                    "Blocks",
                    Self::format_number(detail.block_qty),
                    metric_style,
                ),
            ),
            (4, Self::detail_line_with_style("Rate", detail.block_rate.clone(), reward_style)),
            (5, Self::detail_line_with_style("24H", detail.daily_block_rate.clone(), metric_style)),
            (
                6,
                Self::detail_line_with_style(
                    "Verifier",
                    Self::format_number(detail.verifier_time),
                    metric_style,
                ),
            ),
            (
                7,
                Self::detail_line_with_style(
                    "Ratio",
                    format!("{:.2}%", detail.reward_per),
                    reward_style,
                ),
            ),
            (
                8,
                Self::detail_line_with_style(
                    "System",
                    format!("{} LAT", Self::format_amount(detail.reward_value)),
                    metric_style,
                ),
            ),
            (
                9,
                Self::detail_line_with_style(
                    "Rewards",
                    format!("{} LAT", Self::format_amount(detail.rewards())),
                    reward_style,
                ),
            ),
            (
                10,
                Self::detail_line_with_style(
                    "Address",
                    Self::shorten_address_for_width(&detail.reward_address, address_max_len),
                    address_style,
                ),
            ),
        ]
    }

    fn visible_compact_lines(
        detail: &NodeDetail,
        address_max_len: usize,
        max_rows: u16,
    ) -> Vec<Line<'static>> {
        Self::select_prioritized_lines(Self::compact_line_specs(detail, address_max_len), max_rows)
    }

    #[cfg(test)]
    fn compact_lines(&self) -> Vec<String> {
        let data = self.collect_data.lock().expect("mutex poisoned - recovering");
        match data.node_detail() {
            Some(detail) => Self::visible_compact_lines(&detail, 24, u16::MAX)
                .into_iter()
                .map(|line| line.spans.into_iter().map(|span| span.content.into_owned()).collect())
                .collect(),
            None => vec![self.empty_message().to_string()],
        }
    }

    fn compact_summary_column_specs(
        detail: &NodeDetail,
        address_max_len: usize,
    ) -> DoublePriorityLines {
        let metric_style = Self::metric_value_style();
        let reward_style = Self::reward_value_style();
        let address_style = Self::address_value_style();
        let left = vec![
            (1, Self::detail_line("Name", detail.node_name.clone())),
            (2, Self::detail_line_with_style("Rank", detail.ranking.to_string(), metric_style)),
            (
                3,
                Self::detail_line_with_style(
                    "Blocks",
                    Self::format_number(detail.block_qty),
                    metric_style,
                ),
            ),
            (4, Self::detail_line_with_style("Rate", detail.block_rate.clone(), reward_style)),
            (5, Self::detail_line_with_style("24H", detail.daily_block_rate.clone(), metric_style)),
        ];
        let right = vec![
            (
                1,
                Self::detail_line_with_style(
                    "Verifier",
                    Self::format_number(detail.verifier_time),
                    metric_style,
                ),
            ),
            (
                2,
                Self::detail_line_with_style(
                    "Ratio",
                    format!("{:.2}%", detail.reward_per),
                    reward_style,
                ),
            ),
            (
                3,
                Self::detail_line_with_style(
                    "System",
                    format!("{} LAT", Self::format_amount(detail.reward_value)),
                    metric_style,
                ),
            ),
            (
                4,
                Self::detail_line_with_style(
                    "Rewards",
                    format!("{} LAT", Self::format_amount(detail.rewards())),
                    reward_style,
                ),
            ),
            (
                5,
                Self::detail_line_with_style(
                    "Address",
                    Self::shorten_address_for_width(&detail.reward_address, address_max_len),
                    address_style,
                ),
            ),
        ];

        (left, right)
    }

    fn compact_summary_columns(
        detail: &NodeDetail,
        address_max_len: usize,
        max_left_rows: u16,
        max_right_rows: u16,
    ) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
        let (left_specs, right_specs) = Self::compact_summary_column_specs(detail, address_max_len);

        (
            Self::select_prioritized_lines(left_specs, max_left_rows),
            Self::select_prioritized_lines(right_specs, max_right_rows),
        )
    }

    fn render_compact_node_details(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let outer_block = block::new(&self.title);
        let inner = outer_block.inner(area);
        outer_block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let detail = {
            let data = self.collect_data.lock().expect("mutex poisoned - recovering");
            data.node_detail()
        };

        let Some(detail) = detail else {
            Paragraph::new(vec![Line::raw(self.empty_message())])
                .style(block::content_style())
                .render(inner, buf);
            return;
        };

        if inner.width >= Self::COMPACT_TWO_COLUMN_WIDTH && inner.height >= 3 {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(47), Constraint::Percentage(53)].as_ref())
                .split(inner);
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
            let address_max_len = Self::inline_value_max_len(right_area.width, "Address: ");
            let (left_lines, right_lines) = Self::compact_summary_columns(
                &detail,
                address_max_len,
                left_area.height,
                right_area.height,
            );

            Paragraph::new(left_lines).style(block::content_style()).render(left_area, buf);
            Paragraph::new(right_lines).style(block::content_style()).render(right_area, buf);
            return;
        }

        let address_max_len = Self::inline_value_max_len(inner.width, "Address: ");
        let lines = Self::visible_compact_lines(&detail, address_max_len, inner.height);
        Paragraph::new(lines).style(block::content_style()).render(inner, buf);
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

    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|span| span.content.as_ref()).collect()
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
        let (left, right) = NodeDetailWidget::compact_summary_columns(&sample_detail(), 30, 10, 10);

        assert_eq!(line_text(&left[0]), "Name: node-a");
        assert_eq!(line_text(&left[1]), "Rank: 7");
        assert_eq!(line_text(&left[2]), "Blocks: 123,456");
        assert_eq!(line_text(&right[0]), "Verifier: 9");
        assert_eq!(line_text(&right[1]), "Ratio: 5.00%");
        assert_eq!(line_text(&right[4]), "Address: lat1zytcgvw35sagn722c…dp8gqj5h");
    }

    #[test]
    fn test_detail_column_specs_show_formatted_values() {
        let (left, right) = NodeDetailWidget::detail_column_specs(&sample_detail(), true, 19);

        assert_eq!(line_text(&left[0].1), "Node");
        assert_eq!(left[3].1.spans[1].content, "123,456");
        assert_eq!(line_text(&right[0].1), "Rewards");
        assert_eq!(right[3].1.spans[1].content, "12,345.67 LAT");
        assert_eq!(right[5].1.spans[1].content, "lat1zytcgvw3…8gqj5h");
    }

    #[test]
    fn test_detail_columns_can_hide_section_headings() {
        let (left, right) = NodeDetailWidget::detail_column_specs(&sample_detail(), false, 19);

        assert_eq!(left[0].1.spans[0].content, "Name: ");
        assert_eq!(left.len(), 5);
        assert_eq!(right[0].1.spans[0].content, "Verifier Time: ");
        assert_eq!(right.len(), 5);
    }

    #[test]
    fn test_stacked_lines_include_key_fields() {
        let lines = NodeDetailWidget::stacked_lines(&sample_detail(), true, 19);

        assert_eq!(line_text(&lines[0]), "Node");
        assert_eq!(line_text(&lines[1]), "Name: node-a");
        assert_eq!(line_text(&lines[2]), "Ranking: 7");
        assert_eq!(line_text(&lines[3]), "Blocks: 123,456");
        assert_eq!(line_text(&lines[6]), "");
        assert_eq!(line_text(&lines[7]), "Rewards");
        assert_eq!(line_text(&lines[8]), "Verifier: 9");
        assert_eq!(line_text(&lines[12]), "Address: lat1zytcgvw3…8gqj5h");
    }

    #[test]
    fn test_compact_summary_columns_include_key_fields() {
        let (left, right) = NodeDetailWidget::compact_summary_columns(&sample_detail(), 30, 10, 10);

        assert_eq!(line_text(&left[0]), "Name: node-a");
        assert_eq!(line_text(&left[1]), "Rank: 7");
        assert_eq!(line_text(&left[2]), "Blocks: 123,456");
        assert_eq!(line_text(&right[0]), "Verifier: 9");
        assert_eq!(line_text(&right[1]), "Ratio: 5.00%");
        assert_eq!(line_text(&right[4]), "Address: lat1zytcgvw35sagn722c…dp8gqj5h");
    }

    #[test]
    fn test_compact_lines_without_data_uses_empty_message() {
        let shared_data = create_shared_data();
        let widget = NodeDetailWidget::new(shared_data);

        assert_eq!(widget.compact_lines(), vec!["Loading...".to_string()]);
    }

    #[test]
    fn test_visible_compact_lines_prioritize_core_fields() {
        let lines = NodeDetailWidget::visible_compact_lines(&sample_detail(), 24, 4);

        assert_eq!(lines.len(), 4);
        assert_eq!(line_text(&lines[0]), "Name: node-a");
        assert_eq!(line_text(&lines[1]), "Rank: 7");
        assert_eq!(line_text(&lines[2]), "Blocks: 123,456");
        assert_eq!(line_text(&lines[3]), "Rate: 12.34%");
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
        assert_eq!(lines[1], "Rank: 7");
        assert_eq!(lines[2], "Blocks: 123,456");
        assert_eq!(lines[3], "Rate: 12.34%");
        assert_eq!(lines[4], "24H: 3/day");
        assert_eq!(lines[8], "Rewards: 11,728.39 LAT");
        assert_eq!(lines[9], "Address: lat1zytcgvw35sag…p8gqj5h");
    }
}
