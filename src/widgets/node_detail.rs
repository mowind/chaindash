use std::time::{
    Duration as StdDuration,
    Instant,
};

use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::{
        Constraint,
        Direction,
        Layout,
        Rect,
    },
    style::{
        Modifier,
        Style,
    },
    text::{
        Line,
        Span,
    },
    widgets::{
        Cell,
        Paragraph,
        Row,
        Table,
        Widget,
    },
};

use crate::{
    collect::{
        NodeDetail,
        SharedData,
    },
    sync::lock_or_panic,
    update::UpdatableWidget,
    widgets::{
        block,
        helpers::{
            format_grouped_u64,
            prefix_chars,
            select_prioritized_lines,
            suffix_chars,
            PriorityLines,
        },
    },
};

type DoublePriorityLines = (PriorityLines, PriorityLines);

pub struct NodeDetailWidget {
    title: String,
    update_interval: Ratio<u64>,
    loading: bool,
    node_details: Vec<NodeDetail>,

    collect_data: SharedData,
}

impl NodeDetailWidget {
    const COMPACT_LAYOUT_WIDTH: u16 = 110;
    const COMPACT_TWO_COLUMN_WIDTH: u16 = 72;
    const STACKED_LAYOUT_WIDTH: u16 = 150;
    const STACKED_LAYOUT_HEIGHT: u16 = 9;
    const HEADING_LAYOUT_HEIGHT: u16 = 5;
    const INLINE_RIGHT_PADDING: u16 = 3;
    const TABLE_LAYOUT_MIN_WIDTH: u16 = 98;
    const FRESH_DETAIL_MAX_AGE_SECS: u64 = 30;
    const WARN_DETAIL_MAX_AGE_SECS: u64 = 5 * 60;

    pub fn new(collect_data: SharedData) -> NodeDetailWidget {
        NodeDetailWidget {
            title: " Node Details ".to_string(),
            update_interval: Ratio::from_integer(1),
            loading: true,
            node_details: Vec::new(),
            collect_data,
        }
    }

    fn flexible_width(
        area_width: u16,
        reserved_width: u16,
        min_width: u16,
    ) -> u16 {
        area_width.saturating_sub(2).saturating_sub(reserved_width).max(min_width)
    }

    fn display_name(detail: &NodeDetail) -> String {
        if detail.node_name.is_empty() {
            if detail.node_id.is_empty() {
                "-".to_string()
            } else {
                detail.node_id.clone()
            }
        } else {
            detail.node_name.clone()
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
        format_grouped_u64(value)
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

    fn format_ranking(ranking: i32) -> String {
        if ranking <= 0 {
            "-".to_string()
        } else {
            Self::format_number(ranking as u64)
        }
    }

    fn format_elapsed(elapsed: StdDuration) -> String {
        let seconds = elapsed.as_secs();
        if seconds < 60 {
            format!("{seconds}s")
        } else if seconds < 60 * 60 {
            format!("{}m", seconds / 60)
        } else if seconds < 60 * 60 * 24 {
            format!("{}h", seconds / (60 * 60))
        } else {
            format!("{}d", seconds / (60 * 60 * 24))
        }
    }

    fn format_updated_at(updated_at: Option<Instant>) -> String {
        let Some(updated_at) = updated_at else {
            return "-".to_string();
        };

        Self::format_elapsed(Instant::now().saturating_duration_since(updated_at))
    }

    fn detail_age_secs(detail: &NodeDetail) -> Option<u64> {
        detail
            .last_updated_at
            .map(|updated_at| Instant::now().saturating_duration_since(updated_at).as_secs())
    }

    fn detail_status(detail: &NodeDetail) -> &'static str {
        match Self::detail_age_secs(detail) {
            None => "UNKNOWN",
            Some(elapsed) if elapsed <= Self::FRESH_DETAIL_MAX_AGE_SECS => "OK",
            Some(_) => "STALE",
        }
    }

    fn name_value_style() -> Style {
        block::content_style().add_modifier(Modifier::BOLD)
    }

    fn status_value_style(detail: &NodeDetail) -> Style {
        match Self::detail_age_secs(detail) {
            None => block::muted_style(),
            Some(elapsed) if elapsed <= Self::FRESH_DETAIL_MAX_AGE_SECS => {
                block::accent_style(block::METRIC_POSITIVE)
            },
            Some(elapsed) if elapsed <= Self::WARN_DETAIL_MAX_AGE_SECS => {
                block::accent_style(block::ACCENT_WARN)
            },
            Some(_) => block::accent_style(block::ACCENT_ERROR),
        }
    }

    fn updated_value_style(updated_at: Option<Instant>) -> Style {
        let Some(updated_at) = updated_at else {
            return block::muted_style();
        };

        let elapsed = Instant::now().saturating_duration_since(updated_at).as_secs();
        if elapsed <= Self::FRESH_DETAIL_MAX_AGE_SECS {
            Self::metric_value_style()
        } else if elapsed <= Self::WARN_DETAIL_MAX_AGE_SECS {
            block::accent_style(block::ACCENT_WARN)
        } else {
            block::accent_style(block::ACCENT_ERROR)
        }
    }

    fn shorten_address(address: &str) -> String {
        const MAX_LEN: usize = 24;
        const PREFIX_LEN: usize = 10;
        const SUFFIX_LEN: usize = 8;

        if address.chars().count() <= MAX_LEN {
            return address.to_string();
        }

        format!("{}…{}", prefix_chars(address, PREFIX_LEN), suffix_chars(address, SUFFIX_LEN))
    }

    fn shorten_address_for_width(
        address: &str,
        max_len: usize,
    ) -> String {
        const MIN_PREFIX_LEN: usize = 8;
        const MAX_SUFFIX_LEN: usize = 8;
        const MIN_SUFFIX_LEN: usize = 6;

        if max_len == 0 || address.chars().count() <= max_len {
            return address.to_string();
        }

        if max_len <= MIN_PREFIX_LEN + MIN_SUFFIX_LEN + 1 {
            return Self::shorten_address(address);
        }

        let suffix_len = ((max_len - 1) / 3).clamp(MIN_SUFFIX_LEN, MAX_SUFFIX_LEN);
        let prefix_len = max_len.saturating_sub(suffix_len + 1);

        format!("{}…{}", prefix_chars(address, prefix_len), suffix_chars(address, suffix_len))
    }

    fn compact_list_line(
        detail: &NodeDetail,
        area_width: u16,
    ) -> Line<'static> {
        let mut spans = vec![
            Span::styled(Self::display_name(detail), Self::name_value_style()),
            Span::raw(" "),
            Span::styled(
                format!("R{}", Self::format_ranking(detail.ranking)),
                Self::metric_value_style(),
            ),
        ];

        if area_width >= 44 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("B{}", Self::format_number(detail.block_qty)),
                Self::metric_value_style(),
            ));
        }

        if area_width >= 62 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(detail.block_rate.clone(), Self::reward_value_style()));
        }

        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            Self::detail_status(detail).to_string(),
            Self::status_value_style(detail),
        ));

        if area_width >= 78 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                Self::format_updated_at(detail.last_updated_at),
                Self::updated_value_style(detail.last_updated_at),
            ));
        }

        Line::from(spans)
    }

    fn compact_list_lines(
        &self,
        area_width: u16,
        max_rows: u16,
    ) -> Vec<Line<'static>> {
        if max_rows == 0 {
            return Vec::new();
        }

        let show_more = max_rows > 1 && self.node_details.len() > max_rows as usize;
        let visible_details = max_rows.saturating_sub(if show_more { 1 } else { 0 }) as usize;
        let mut lines = self
            .node_details
            .iter()
            .take(visible_details)
            .map(|detail| Self::compact_list_line(detail, area_width))
            .collect::<Vec<_>>();

        if show_more {
            lines.push(Line::from(vec![Span::styled(
                format!(
                    "+{} more node details",
                    self.node_details.len().saturating_sub(visible_details)
                ),
                block::muted_style(),
            )]));
        }

        lines
    }

    fn select_prioritized_lines(
        specs: PriorityLines,
        max_rows: u16,
    ) -> Vec<Line<'static>> {
        select_prioritized_lines(specs, max_rows)
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
        block::accent_style(block::METRIC_PRIMARY)
    }

    fn reward_value_style() -> ratatui::style::Style {
        block::accent_style(block::METRIC_POSITIVE)
    }

    fn address_value_style() -> ratatui::style::Style {
        block::highlight_style()
    }

    fn detail_column_specs(
        detail: &NodeDetail,
        show_section_headings: bool,
        address_max_len: usize,
    ) -> DoublePriorityLines {
        let metric_style = Self::metric_value_style();
        let reward_style = Self::reward_value_style();
        let address_style = Self::address_value_style();

        let mut left = Vec::new();
        if show_section_headings {
            left.push((9, Self::section_heading("Node")));
        }
        left.extend([
            (1, Self::detail_line("Name", Self::display_name(detail))),
            (
                2,
                Self::detail_line_with_style(
                    "Ranking",
                    Self::format_ranking(detail.ranking),
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

        let updated_style = Self::updated_value_style(detail.last_updated_at);
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
            (
                6,
                Self::detail_line_with_style(
                    "Updated",
                    Self::format_updated_at(detail.last_updated_at),
                    updated_style,
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
        let updated_style = Self::updated_value_style(detail.last_updated_at);
        let mut lines = Vec::new();

        if show_section_headings {
            lines.push((20, Self::section_heading("Node")));
        }
        lines.push((1, Self::detail_line("Name", Self::display_name(detail))));
        lines.push((
            2,
            Self::detail_line_with_style(
                "Ranking",
                Self::format_ranking(detail.ranking),
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
        lines.push((
            11,
            Self::detail_line_with_style(
                "Updated",
                Self::format_updated_at(detail.last_updated_at),
                updated_style,
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

        let Some(detail) = self.node_details.first() else {
            Paragraph::new(vec![Line::raw(self.empty_message())])
                .style(block::empty_state_style())
                .render(content, buf);
            return;
        };

        let show_section_headings = content.height >= Self::HEADING_LAYOUT_HEIGHT;
        if content.width < Self::STACKED_LAYOUT_WIDTH {
            if content.height >= Self::STACKED_LAYOUT_HEIGHT {
                let address_max_len = Self::inline_value_max_len(content.width, "Address: ");
                let lines = Self::visible_stacked_lines(
                    detail,
                    show_section_headings,
                    address_max_len,
                    content.height,
                );
                Paragraph::new(lines).render(content, buf);
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
            Self::detail_column_specs(detail, show_section_headings, address_max_len);
        let left_lines = Self::select_prioritized_lines(left_specs, left_area.height);
        let right_lines = Self::select_prioritized_lines(right_specs, right_area.height);

        Paragraph::new(left_lines).render(left_area, buf);
        Paragraph::new(right_lines).render(right_area, buf);
    }

    fn compact_line_specs(
        detail: &NodeDetail,
        address_max_len: usize,
    ) -> PriorityLines {
        let metric_style = Self::metric_value_style();
        let reward_style = Self::reward_value_style();
        let address_style = Self::address_value_style();
        let updated_style = Self::updated_value_style(detail.last_updated_at);

        vec![
            (1, Self::detail_line("Name", Self::display_name(detail))),
            (
                2,
                Self::detail_line_with_style(
                    "Rank",
                    Self::format_ranking(detail.ranking),
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
            (
                11,
                Self::detail_line_with_style(
                    "Updated",
                    Self::format_updated_at(detail.last_updated_at),
                    updated_style,
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
        match self.node_details.first() {
            Some(detail) => Self::visible_compact_lines(detail, 24, u16::MAX)
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
        let updated_style = Self::updated_value_style(detail.last_updated_at);
        let left = vec![
            (1, Self::detail_line("Name", Self::display_name(detail))),
            (
                2,
                Self::detail_line_with_style(
                    "Rank",
                    Self::format_ranking(detail.ranking),
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
            (
                6,
                Self::detail_line_with_style(
                    "Updated",
                    Self::format_updated_at(detail.last_updated_at),
                    updated_style,
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

        let Some(detail) = self.node_details.first() else {
            Paragraph::new(vec![Line::raw(self.empty_message())])
                .style(block::empty_state_style())
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
                detail,
                address_max_len,
                left_area.height,
                right_area.height,
            );

            Paragraph::new(left_lines).render(left_area, buf);
            Paragraph::new(right_lines).render(right_area, buf);
            return;
        }

        let address_max_len = Self::inline_value_max_len(inner.width, "Address: ");
        let lines = Self::visible_compact_lines(detail, address_max_len, inner.height);
        Paragraph::new(lines).render(inner, buf);
    }

    fn render_compact_detail_list(
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

        let lines = self.compact_list_lines(inner.width, inner.height);
        Paragraph::new(lines).render(inner, buf);
    }

    fn table_row_values(
        detail: &NodeDetail,
        address_max_len: usize,
    ) -> Vec<String> {
        vec![
            format!(" {}", Self::display_name(detail)),
            Self::format_ranking(detail.ranking),
            Self::format_number(detail.block_qty),
            detail.block_rate.clone(),
            detail.daily_block_rate.clone(),
            format!("{:.2}%", detail.reward_per),
            Self::detail_status(detail).to_string(),
            Self::format_updated_at(detail.last_updated_at),
            Self::shorten_address_for_width(&detail.reward_address, address_max_len),
        ]
    }

    fn table_row_cells(
        detail: &NodeDetail,
        address_max_len: usize,
    ) -> Vec<Cell<'static>> {
        let values = Self::table_row_values(detail, address_max_len);

        vec![
            Cell::from(values[0].clone()).style(Self::name_value_style()),
            Cell::from(values[1].clone()).style(Self::metric_value_style()),
            Cell::from(values[2].clone()).style(Self::metric_value_style()),
            Cell::from(values[3].clone()).style(Self::reward_value_style()),
            Cell::from(values[4].clone()).style(Self::metric_value_style()),
            Cell::from(values[5].clone()).style(Self::reward_value_style()),
            Cell::from(values[6].clone()).style(Self::status_value_style(detail)),
            Cell::from(values[7].clone()).style(Self::updated_value_style(detail.last_updated_at)),
            Cell::from(values[8].clone()).style(Self::address_value_style()),
        ]
    }

    fn render_table(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let address_width = Self::flexible_width(area.width, 87, 14);
        let address_max_len = address_width.saturating_sub(1) as usize;
        let header =
            [" Name", "Rank", "Blocks", "Rate", "24H", "Ratio", "Status", "Updated", "Address"];
        let rows = self
            .node_details
            .iter()
            .map(|detail| Row::new(Self::table_row_cells(detail, address_max_len)));
        let header_row = Row::new(header.iter().copied()).style(block::header_style());

        Table::new(
            rows,
            &[
                Constraint::Length(16),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(address_width),
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
        let data = lock_or_panic(&self.collect_data);
        self.node_details = data.node_details();
        self.loading = self.node_details.is_empty() && !data.node_details_loaded();
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

        if self.node_details.len() > 1 {
            if area.width < NodeDetailWidget::TABLE_LAYOUT_MIN_WIDTH {
                self.render_compact_detail_list(area, buf);
                return;
            }

            self.render_table(area, buf);
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
            node_id: "node-a-id".to_string(),
            node_name: "node-a".to_string(),
            ranking: 7,
            block_qty: 123_456,
            block_rate: "12.34%".to_string(),
            daily_block_rate: "3/day".to_string(),
            reward_per: 5.0,
            reward_value: 12_345.67,
            reward_address: "lat1zytcgvw35sagn722cneh6sz92y8j3dp8gqj5h".to_string(),
            verifier_time: 9,
            last_updated_at: Some(Instant::now()),
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
    fn test_node_detail_widget_shows_empty_state_after_initial_fetch() {
        let shared_data = create_shared_data();
        {
            let mut data = shared_data.lock().expect("mutex poisoned");
            data.mark_node_details_loaded();
        }

        let mut widget = NodeDetailWidget::new(shared_data);
        widget.update();

        assert!(!widget.loading);
        assert_eq!(widget.empty_message(), "No node details found");
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
    fn test_format_ranking_uses_dash_for_unknown_rank() {
        assert_eq!(NodeDetailWidget::format_ranking(0), "-");
        assert_eq!(NodeDetailWidget::format_ranking(-1), "-");
        assert_eq!(NodeDetailWidget::format_ranking(7), "7");
    }

    #[test]
    fn test_format_elapsed_uses_human_readable_units() {
        assert_eq!(NodeDetailWidget::format_elapsed(StdDuration::from_secs(5)), "5s");
        assert_eq!(NodeDetailWidget::format_elapsed(StdDuration::from_secs(65)), "1m");
        assert_eq!(NodeDetailWidget::format_elapsed(StdDuration::from_secs(7_200)), "2h");
        assert_eq!(NodeDetailWidget::format_elapsed(StdDuration::from_secs(172_800)), "2d");
    }

    #[test]
    fn test_table_row_values_include_status_and_updated_columns() {
        let mut detail = sample_detail();
        detail.last_updated_at = None;

        let row = NodeDetailWidget::table_row_values(&detail, 18);

        assert_eq!(row[0], " node-a");
        assert_eq!(row[6], "UNKNOWN");
        assert_eq!(row[7], "-");
    }

    #[test]
    fn test_updated_value_style_reflects_staleness() {
        assert_eq!(NodeDetailWidget::updated_value_style(None).fg, block::muted_style().fg,);
        assert_eq!(
            NodeDetailWidget::updated_value_style(Some(Instant::now())).fg,
            NodeDetailWidget::metric_value_style().fg,
        );
        assert_eq!(
            NodeDetailWidget::updated_value_style(Some(
                Instant::now() - StdDuration::from_secs(90)
            ))
            .fg,
            Some(block::ACCENT_WARN),
        );
        assert_eq!(
            NodeDetailWidget::updated_value_style(Some(
                Instant::now() - StdDuration::from_secs(600)
            ))
            .fg,
            Some(block::ACCENT_ERROR),
        );
    }

    #[test]
    fn test_table_highlight_style_helpers_match_expected_colors() {
        let detail = sample_detail();

        assert_eq!(NodeDetailWidget::metric_value_style().fg, Some(block::METRIC_PRIMARY));
        assert_eq!(NodeDetailWidget::reward_value_style().fg, Some(block::METRIC_POSITIVE));
        assert_eq!(NodeDetailWidget::status_value_style(&detail).fg, Some(block::METRIC_POSITIVE));
        assert_eq!(NodeDetailWidget::address_value_style().fg, Some(block::CONTENT_HIGHLIGHT),);
    }

    #[test]
    fn test_detail_status_reflects_staleness() {
        let mut detail = sample_detail();
        detail.last_updated_at = None;
        assert_eq!(NodeDetailWidget::detail_status(&detail), "UNKNOWN");
        assert_eq!(NodeDetailWidget::status_value_style(&detail).fg, block::muted_style().fg);

        detail.last_updated_at = Some(Instant::now() - StdDuration::from_secs(90));
        assert_eq!(NodeDetailWidget::detail_status(&detail), "STALE");
        assert_eq!(NodeDetailWidget::status_value_style(&detail).fg, Some(block::ACCENT_WARN));
    }

    #[test]
    fn test_shorten_address_preserves_prefix_and_suffix() {
        assert_eq!(
            NodeDetailWidget::shorten_address("lat1zytcgvw35sagn722cneh6sz92y8j3dp8gqj5h"),
            "lat1zytcgv…dp8gqj5h"
        );
    }

    #[test]
    fn test_shorten_address_for_width_handles_unicode_safely() {
        let shortened = NodeDetailWidget::shorten_address_for_width(
            "地址前缀-1234567890-验证节点-额外字段",
            16,
        );

        assert!(shortened.starts_with("地址前缀-123"));
        assert!(shortened.ends_with("额外字段"));
        assert!(shortened.contains('…'));
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
        let detail = sample_detail();
        let (left, right) = NodeDetailWidget::detail_column_specs(&detail, true, 19);

        assert_eq!(line_text(&left[0].1), "Node");
        assert_eq!(left[3].1.spans[1].content, "123,456");
        assert_eq!(line_text(&right[0].1), "Rewards");
        assert_eq!(right[3].1.spans[1].content, "12,345.67 LAT");
        assert_eq!(right[5].1.spans[1].content, "lat1zytcgvw3…8gqj5h");
        assert!(line_text(&right[6].1).starts_with("Updated: "));
    }

    #[test]
    fn test_detail_columns_can_hide_section_headings() {
        let (left, right) = NodeDetailWidget::detail_column_specs(&sample_detail(), false, 19);

        assert_eq!(left[0].1.spans[0].content, "Name: ");
        assert_eq!(left.len(), 5);
        assert_eq!(right[0].1.spans[0].content, "Verifier Time: ");
        assert_eq!(right.len(), 6);
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
        assert!(line_text(&lines[13]).starts_with("Updated: "));
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
        assert!(line_text(&right[5]).starts_with("Updated: "));
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

        let mut widget = NodeDetailWidget::new(shared_data);
        widget.update();
        let lines = widget.compact_lines();

        assert_eq!(lines[0], "Name: node-a");
        assert_eq!(lines[1], "Rank: 7");
        assert_eq!(lines[2], "Blocks: 123,456");
        assert_eq!(lines[3], "Rate: 12.34%");
        assert_eq!(lines[4], "24H: 3/day");
        assert_eq!(lines[8], "Rewards: 11,728.39 LAT");
        assert_eq!(lines[9], "Address: lat1zytcgvw35sag…p8gqj5h");
        assert!(lines[10].starts_with("Updated: "));
    }

    #[test]
    fn test_compact_lines_show_dash_for_unknown_rank() {
        let shared_data = create_shared_data();
        {
            let mut data = shared_data.lock().expect("mutex poisoned");
            let mut detail = sample_detail();
            detail.ranking = 0;
            data.update_node_detail(Some(detail));
        }

        let mut widget = NodeDetailWidget::new(shared_data);
        widget.update();
        let lines = widget.compact_lines();

        assert_eq!(lines[1], "Rank: -");
    }

    #[test]
    fn test_update_collects_multiple_node_details() {
        let shared_data = create_shared_data();
        {
            let mut data = shared_data.lock().expect("mutex poisoned");
            let mut detail_a = sample_detail();
            detail_a.ranking = 7;
            let mut detail_b = sample_detail();
            detail_b.node_id = "node-b-id".to_string();
            detail_b.node_name = "node-b".to_string();
            detail_b.ranking = 2;
            let mut detail_c = sample_detail();
            detail_c.node_id = "node-c-id".to_string();
            detail_c.node_name = "node-c".to_string();
            detail_c.ranking = 0;

            data.merge_node_detail_for(&detail_a.node_id.clone(), Some(detail_a));
            data.merge_node_detail_for(&detail_b.node_id.clone(), Some(detail_b));
            data.merge_node_detail_for(&detail_c.node_id.clone(), Some(detail_c));
        }

        let mut widget = NodeDetailWidget::new(shared_data);
        widget.update();

        assert_eq!(widget.node_details.len(), 3);
        assert_eq!(widget.node_details[0].node_name, "node-b");
        assert_eq!(widget.node_details[1].node_name, "node-a");
        assert_eq!(widget.node_details[2].node_name, "node-c");
    }

    #[test]
    fn test_compact_list_lines_include_more_indicator() {
        let shared_data = create_shared_data();
        let mut widget = NodeDetailWidget::new(shared_data);
        widget.node_details = vec![sample_detail(), sample_detail(), sample_detail()];

        let lines = widget.compact_list_lines(72, 2);

        assert_eq!(lines.len(), 2);
        assert!(line_text(&lines[0]).contains("node-a R7 B123,456 12.34% OK"));
        assert_eq!(line_text(&lines[1]), "+2 more node details");
    }
}
