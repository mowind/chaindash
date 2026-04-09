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
        Color,
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
        ConsensusState,
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
type TriplePriorityLines = (PriorityLines, PriorityLines, PriorityLines);

pub struct NodeWidget {
    title: String,
    update_interval: Ratio<u64>,
    collect_data: SharedData,
    nodes: Vec<ConsensusState>,
}

impl NodeWidget {
    const COMPACT_LAYOUT_WIDTH: u16 = 110;
    const COMPACT_TWO_COLUMN_WIDTH: u16 = 72;
    const STACKED_LAYOUT_WIDTH: u16 = 150;
    const STACKED_LAYOUT_HEIGHT: u16 = 9;
    const HEADING_LAYOUT_HEIGHT: u16 = 6;
    const INLINE_RIGHT_PADDING: u16 = 3;
    const TABLE_LAYOUT_MIN_WIDTH: u16 = 129;

    pub fn new(collect_data: SharedData) -> NodeWidget {
        NodeWidget {
            title: " Nodes ".to_string(),
            update_interval: Ratio::from_integer(1),
            collect_data,
            nodes: Vec::new(),
        }
    }

    fn flexible_width(
        area_width: u16,
        reserved_width: u16,
        min_width: u16,
    ) -> u16 {
        area_width.saturating_sub(2).saturating_sub(reserved_width).max(min_width)
    }

    fn format_number(value: u64) -> String {
        format_grouped_u64(value)
    }

    fn shorten_host_for_width(
        host: &str,
        max_len: usize,
    ) -> String {
        const MIN_SUFFIX_LEN: usize = 4;
        const MAX_SUFFIX_LEN: usize = 8;

        if max_len == 0 {
            return String::new();
        }

        if host.chars().count() <= max_len {
            return host.to_string();
        }

        if let Some((base, port)) = host.rsplit_once(':') {
            if !base.is_empty() && !port.is_empty() && port.chars().all(|ch| ch.is_ascii_digit()) {
                let suffix = format!(":{port}");
                let suffix_len = suffix.chars().count();
                if max_len > suffix_len + 1 {
                    let prefix_len = max_len.saturating_sub(suffix_len + 1);
                    return format!("{}…{}", prefix_chars(base, prefix_len), suffix);
                }
            }
        }

        let suffix_len = ((max_len - 1) / 3).clamp(MIN_SUFFIX_LEN, MAX_SUFFIX_LEN);
        let prefix_len = max_len.saturating_sub(suffix_len + 1);

        format!("{}…{}", prefix_chars(host, prefix_len), suffix_chars(host, suffix_len))
    }

    fn compact_list_line(
        node: &ConsensusState,
        area_width: u16,
    ) -> Line<'static> {
        let (role_text, role_color) = Self::role_badge(node);
        let metric_style = Self::metric_value_style();
        let mut spans = vec![
            Span::styled(node.name.clone(), Self::node_value_style()),
            Span::raw(" "),
            Span::styled(role_text.to_string(), Self::role_value_style(role_color)),
            Span::raw(" "),
            Span::styled(format!("#{}", Self::format_number(node.current_number)), metric_style),
        ];

        if area_width >= 48 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("E{}", Self::format_number(node.epoch)), metric_style));
        }

        if area_width >= 60 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("V{}", Self::format_number(node.view)), metric_style));
        }

        if area_width >= 76 {
            let host_max_len = area_width.saturating_sub(38).max(12) as usize;
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                Self::shorten_host_for_width(&node.host, host_max_len),
                block::content_style(),
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

        let show_more = max_rows > 1 && self.nodes.len() > max_rows as usize;
        let visible_nodes = max_rows.saturating_sub(if show_more { 1 } else { 0 }) as usize;
        let mut lines = self
            .nodes
            .iter()
            .take(visible_nodes)
            .map(|node| Self::compact_list_line(node, area_width))
            .collect::<Vec<_>>();

        if show_more {
            lines.push(Line::from(vec![Span::styled(
                format!("+{} more nodes", self.nodes.len().saturating_sub(visible_nodes)),
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

    fn info_line_with_style(
        label: &str,
        value: impl Into<String>,
        value_style: Style,
    ) -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("{label}: "), block::muted_style()),
            Span::styled(value.into(), value_style),
        ])
    }

    fn empty_message() -> &'static str {
        "No nodes found"
    }

    fn role_badge(node: &ConsensusState) -> (&'static str, Color) {
        if node.validator {
            ("VALIDATOR", block::METRIC_POSITIVE)
        } else {
            ("OBSERVER", block::ACCENT_WARN)
        }
    }

    fn node_value_style() -> Style {
        block::content_style().add_modifier(Modifier::BOLD)
    }

    fn metric_value_style() -> Style {
        block::accent_style(block::METRIC_TERTIARY)
    }

    fn role_value_style(color: Color) -> Style {
        block::accent_style(color)
    }

    fn single_node_column_specs(
        node: &ConsensusState,
        show_section_headings: bool,
        host_max_len: usize,
    ) -> TriplePriorityLines {
        let node_value_style = Self::node_value_style();
        let metric_value_style = Self::metric_value_style();
        let (role_text, role_color) = Self::role_badge(node);

        let mut left_lines = Vec::new();
        if show_section_headings {
            left_lines.push((9, Self::section_heading("Node")));
        }
        left_lines
            .push((1, Self::info_line_with_style("Name", node.name.clone(), node_value_style)));
        left_lines.push((
            3,
            Self::info_line_with_style(
                "Host",
                Self::shorten_host_for_width(&node.host, host_max_len),
                block::content_style(),
            ),
        ));
        left_lines.push((
            2,
            Line::from(vec![
                Span::styled("Role: ", block::muted_style()),
                Span::styled(role_text.to_string(), Self::role_value_style(role_color)),
            ]),
        ));

        let mut middle_lines = Vec::new();
        if show_section_headings {
            middle_lines.push((9, Self::section_heading("Chain")));
        }
        middle_lines.extend([
            (
                1,
                Self::info_line_with_style(
                    "Block",
                    Self::format_number(node.current_number),
                    metric_value_style,
                ),
            ),
            (
                2,
                Self::info_line_with_style(
                    "Epoch",
                    Self::format_number(node.epoch),
                    metric_value_style,
                ),
            ),
            (
                3,
                Self::info_line_with_style(
                    "View",
                    Self::format_number(node.view),
                    metric_value_style,
                ),
            ),
        ]);

        let mut right_lines = Vec::new();
        if show_section_headings {
            right_lines.push((9, Self::section_heading("Consensus")));
        }
        right_lines.extend([
            (2, Self::info_line_with_style("QC", Self::format_number(node.qc), metric_value_style)),
            (
                3,
                Self::info_line_with_style(
                    "Locked",
                    Self::format_number(node.locked),
                    metric_value_style,
                ),
            ),
            (
                1,
                Self::info_line_with_style(
                    "Committed",
                    Self::format_number(node.committed),
                    metric_value_style,
                ),
            ),
        ]);

        (left_lines, middle_lines, right_lines)
    }

    fn render_single_node(
        &self,
        area: Rect,
        buf: &mut Buffer,
        node: &ConsensusState,
    ) {
        if area.width < Self::COMPACT_LAYOUT_WIDTH {
            self.render_compact_single_node(area, buf, node);
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

        let show_section_headings = content.height >= Self::HEADING_LAYOUT_HEIGHT;
        if content.width < Self::STACKED_LAYOUT_WIDTH {
            if content.height >= Self::STACKED_LAYOUT_HEIGHT {
                let host_max_len = Self::inline_value_max_len(content.width, "Host: ");
                let lines = Self::visible_stacked_lines(
                    node,
                    show_section_headings,
                    host_max_len,
                    content.height,
                );
                Paragraph::new(lines).render(content, buf);
            } else {
                self.render_compact_single_node(area, buf, node);
            }
            return;
        }

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(34),
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                ]
                .as_ref(),
            )
            .split(content);

        let left_area = Rect::new(
            columns[0].x,
            columns[0].y,
            columns[0].width.saturating_sub(1),
            columns[0].height,
        );
        let host_max_len = Self::inline_value_max_len(left_area.width, "Host: ");
        let middle_area = columns[1];
        let right_area = columns[2];
        let (left_specs, middle_specs, right_specs) =
            Self::single_node_column_specs(node, show_section_headings, host_max_len);
        let left_lines = Self::select_prioritized_lines(left_specs, left_area.height);
        let middle_lines = Self::select_prioritized_lines(middle_specs, middle_area.height);
        let right_lines = Self::select_prioritized_lines(right_specs, right_area.height);

        Paragraph::new(left_lines).render(left_area, buf);
        Paragraph::new(middle_lines).render(middle_area, buf);
        Paragraph::new(right_lines).render(right_area, buf);
    }

    fn stacked_line_specs(
        node: &ConsensusState,
        show_section_headings: bool,
        host_max_len: usize,
    ) -> PriorityLines {
        let (role_text, role_color) = Self::role_badge(node);
        let node_value_style = Self::node_value_style();
        let metric_value_style = Self::metric_value_style();
        let mut lines = Vec::new();

        if show_section_headings {
            lines.push((20, Self::section_heading("Node")));
        }
        lines.push((1, Self::info_line_with_style("Name", node.name.clone(), node_value_style)));
        lines.push((
            8,
            Self::info_line_with_style(
                "Host",
                Self::shorten_host_for_width(&node.host, host_max_len),
                block::content_style(),
            ),
        ));
        lines.push((
            2,
            Self::info_line_with_style("Role", role_text, Self::role_value_style(role_color)),
        ));

        if show_section_headings {
            lines.push((30, Self::spacer_line()));
            lines.push((20, Self::section_heading("Chain")));
        }
        lines.push((
            3,
            Self::info_line_with_style(
                "Block",
                Self::format_number(node.current_number),
                metric_value_style,
            ),
        ));
        lines.push((
            4,
            Self::info_line_with_style(
                "Epoch",
                Self::format_number(node.epoch),
                metric_value_style,
            ),
        ));
        lines.push((
            7,
            Self::info_line_with_style("View", Self::format_number(node.view), metric_value_style),
        ));

        if show_section_headings {
            lines.push((30, Self::spacer_line()));
            lines.push((20, Self::section_heading("Consensus")));
        }
        lines.push((
            6,
            Self::info_line_with_style("QC", Self::format_number(node.qc), metric_value_style),
        ));
        lines.push((
            9,
            Self::info_line_with_style(
                "Locked",
                Self::format_number(node.locked),
                metric_value_style,
            ),
        ));
        lines.push((
            5,
            Self::info_line_with_style(
                "Committed",
                Self::format_number(node.committed),
                metric_value_style,
            ),
        ));

        lines
    }

    fn visible_stacked_lines(
        node: &ConsensusState,
        show_section_headings: bool,
        host_max_len: usize,
        max_rows: u16,
    ) -> Vec<Line<'static>> {
        Self::select_prioritized_lines(
            Self::stacked_line_specs(node, show_section_headings, host_max_len),
            max_rows,
        )
    }

    #[cfg(test)]
    fn stacked_lines(
        node: &ConsensusState,
        show_section_headings: bool,
        host_max_len: usize,
    ) -> Vec<Line<'static>> {
        Self::visible_stacked_lines(node, show_section_headings, host_max_len, u16::MAX)
    }

    fn compact_line_specs(
        node: &ConsensusState,
        host_max_len: usize,
    ) -> PriorityLines {
        let (role_text, role_color) = Self::role_badge(node);
        let node_value_style = Self::node_value_style();
        let metric_value_style = Self::metric_value_style();

        vec![
            (1, Self::info_line_with_style("Name", node.name.clone(), node_value_style)),
            (
                8,
                Self::info_line_with_style(
                    "Host",
                    Self::shorten_host_for_width(&node.host, host_max_len),
                    block::content_style(),
                ),
            ),
            (2, Self::info_line_with_style("Role", role_text, Self::role_value_style(role_color))),
            (
                3,
                Self::info_line_with_style(
                    "Block",
                    Self::format_number(node.current_number),
                    metric_value_style,
                ),
            ),
            (
                4,
                Self::info_line_with_style(
                    "Epoch",
                    Self::format_number(node.epoch),
                    metric_value_style,
                ),
            ),
            (
                7,
                Self::info_line_with_style(
                    "View",
                    Self::format_number(node.view),
                    metric_value_style,
                ),
            ),
            (6, Self::info_line_with_style("QC", Self::format_number(node.qc), metric_value_style)),
            (
                9,
                Self::info_line_with_style(
                    "Locked",
                    Self::format_number(node.locked),
                    metric_value_style,
                ),
            ),
            (
                5,
                Self::info_line_with_style(
                    "Committed",
                    Self::format_number(node.committed),
                    metric_value_style,
                ),
            ),
        ]
    }

    fn visible_compact_lines(
        node: &ConsensusState,
        host_max_len: usize,
        max_rows: u16,
    ) -> Vec<Line<'static>> {
        Self::select_prioritized_lines(Self::compact_line_specs(node, host_max_len), max_rows)
    }

    #[cfg(test)]
    fn compact_lines(
        node: &ConsensusState,
        host_max_len: usize,
    ) -> Vec<String> {
        Self::visible_compact_lines(node, host_max_len, u16::MAX)
            .into_iter()
            .map(|line| line.spans.into_iter().map(|span| span.content.into_owned()).collect())
            .collect()
    }

    fn compact_summary_column_specs(
        node: &ConsensusState,
        host_max_len: usize,
    ) -> DoublePriorityLines {
        let (role_text, role_color) = Self::role_badge(node);
        let node_value_style = Self::node_value_style();
        let metric_value_style = Self::metric_value_style();

        let left = vec![
            (1, Self::info_line_with_style("Name", node.name.clone(), node_value_style)),
            (
                3,
                Self::info_line_with_style(
                    "Host",
                    Self::shorten_host_for_width(&node.host, host_max_len),
                    block::content_style(),
                ),
            ),
            (2, Self::info_line_with_style("Role", role_text, Self::role_value_style(role_color))),
        ];
        let right = vec![
            (
                1,
                Self::info_line_with_style(
                    "Block",
                    Self::format_number(node.current_number),
                    metric_value_style,
                ),
            ),
            (
                2,
                Self::info_line_with_style(
                    "Epoch",
                    Self::format_number(node.epoch),
                    metric_value_style,
                ),
            ),
            (
                3,
                Self::info_line_with_style(
                    "View",
                    Self::format_number(node.view),
                    metric_value_style,
                ),
            ),
            (4, Self::info_line_with_style("QC", Self::format_number(node.qc), metric_value_style)),
            (
                5,
                Self::info_line_with_style(
                    "Locked",
                    Self::format_number(node.locked),
                    metric_value_style,
                ),
            ),
            (
                6,
                Self::info_line_with_style(
                    "Committed",
                    Self::format_number(node.committed),
                    metric_value_style,
                ),
            ),
        ];

        (left, right)
    }

    fn compact_summary_columns(
        node: &ConsensusState,
        host_max_len: usize,
        max_left_rows: u16,
        max_right_rows: u16,
    ) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
        let (left_specs, right_specs) = Self::compact_summary_column_specs(node, host_max_len);

        (
            Self::select_prioritized_lines(left_specs, max_left_rows),
            Self::select_prioritized_lines(right_specs, max_right_rows),
        )
    }

    fn render_compact_single_node(
        &self,
        area: Rect,
        buf: &mut Buffer,
        node: &ConsensusState,
    ) {
        let outer_block = block::new(&self.title);
        let inner = outer_block.inner(area);
        outer_block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if inner.width >= Self::COMPACT_TWO_COLUMN_WIDTH && inner.height >= 4 {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(42), Constraint::Percentage(58)].as_ref())
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
            let host_max_len = Self::inline_value_max_len(left_area.width, "Host: ");
            let (left_lines, right_lines) = Self::compact_summary_columns(
                node,
                host_max_len,
                left_area.height,
                right_area.height,
            );

            Paragraph::new(left_lines).render(left_area, buf);
            Paragraph::new(right_lines).render(right_area, buf);
            return;
        }

        let host_max_len = Self::inline_value_max_len(inner.width, "Host: ");
        let lines = Self::visible_compact_lines(node, host_max_len, inner.height);
        Paragraph::new(lines).render(inner, buf);
    }

    fn render_compact_node_list(
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
        node: &ConsensusState,
        host_max_len: usize,
    ) -> Vec<String> {
        let (role_text, _) = Self::role_badge(node);

        vec![
            format!(" {}", node.name),
            Self::shorten_host_for_width(&node.host, host_max_len),
            Self::format_number(node.current_number),
            Self::format_number(node.epoch),
            Self::format_number(node.view),
            Self::format_number(node.qc),
            Self::format_number(node.locked),
            Self::format_number(node.committed),
            role_text.to_string(),
        ]
    }

    fn table_row_cells(
        node: &ConsensusState,
        host_max_len: usize,
    ) -> Vec<Cell<'static>> {
        let (_, role_color) = Self::role_badge(node);
        let values = Self::table_row_values(node, host_max_len);

        vec![
            Cell::from(values[0].clone()).style(Self::node_value_style()),
            Cell::from(values[1].clone()).style(block::content_style()),
            Cell::from(values[2].clone()).style(Self::metric_value_style()),
            Cell::from(values[3].clone()).style(Self::metric_value_style()),
            Cell::from(values[4].clone()).style(Self::metric_value_style()),
            Cell::from(values[5].clone()).style(Self::metric_value_style()),
            Cell::from(values[6].clone()).style(Self::metric_value_style()),
            Cell::from(values[7].clone()).style(Self::metric_value_style()),
            Cell::from(values[8].clone()).style(Self::role_value_style(role_color)),
        ]
    }

    fn render_table(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let header =
            [" Name", "Host", "Block", "Epoch", "View", "QC", "Locked", "Committed", "Role"];
        let host_width = Self::flexible_width(area.width, 88, 18);
        let host_max_len = host_width.saturating_sub(1) as usize;

        let rows =
            self.nodes.iter().map(|node| Row::new(Self::table_row_cells(node, host_max_len)));

        let header_row = Row::new(header.iter().copied()).style(block::header_style());

        Table::new(
            rows,
            &[
                Constraint::Length(16),
                Constraint::Length(host_width),
                Constraint::Length(14),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(14),
                Constraint::Length(14),
                Constraint::Length(14),
                Constraint::Length(11),
            ],
        )
        .block(block::new(&self.title))
        .header(header_row)
        .column_spacing(1)
        .render(area, buf);
    }

    fn render_empty_state(
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

        Paragraph::new(vec![Line::raw(Self::empty_message())])
            .style(block::empty_state_style())
            .render(inner, buf);
    }
}

impl UpdatableWidget for NodeWidget {
    fn update(&mut self) {
        let collect_data = lock_or_panic(&self.collect_data);
        self.nodes = collect_data.states();
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

        if self.nodes.is_empty() {
            self.render_empty_state(area, buf);
            return;
        }

        if self.nodes.len() == 1 {
            self.render_single_node(area, buf, &self.nodes[0]);
            return;
        }

        if area.width < NodeWidget::TABLE_LAYOUT_MIN_WIDTH {
            self.render_compact_node_list(area, buf);
            return;
        }

        self.render_table(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        widgets::Widget,
    };

    use super::*;
    use crate::collect::Data;

    fn create_shared_data() -> SharedData {
        Data::new()
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|span| span.content.as_ref()).collect()
    }

    fn sample_node() -> ConsensusState {
        ConsensusState {
            name: "Satyrs".to_string(),
            host: "127.0.0.1:6790".to_string(),
            current_number: 145_333_141,
            epoch: 337_985,
            view: 2,
            committed: 145_333_141,
            locked: 145_333_142,
            qc: 145_333_143,
            validator: false,
        }
    }

    fn sample_long_host_node() -> ConsensusState {
        ConsensusState {
            host: "validator-long-domain.example.internal:6790".to_string(),
            ..sample_node()
        }
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
    }

    #[test]
    fn test_node_widget_initial_state() {
        let shared_data = create_shared_data();
        let widget = NodeWidget::new(shared_data);
        assert!(widget.nodes.is_empty());
    }

    #[test]
    fn test_empty_message_matches_nodes_empty_state() {
        assert_eq!(NodeWidget::empty_message(), "No nodes found");
    }

    #[test]
    fn test_flexible_width_saturates_for_narrow_area() {
        assert_eq!(NodeWidget::flexible_width(20, 108, 10), 10);
        assert_eq!(NodeWidget::flexible_width(140, 108, 30), 30);
    }

    #[test]
    fn test_format_number_adds_grouping_separators() {
        assert_eq!(NodeWidget::format_number(0), "0");
        assert_eq!(NodeWidget::format_number(1234), "1,234");
        assert_eq!(NodeWidget::format_number(123456789), "123,456,789");
    }

    #[test]
    fn test_role_badge_reflects_validator_flag() {
        let validator = ConsensusState {
            validator: true,
            ..Default::default()
        };
        let observer = ConsensusState {
            validator: false,
            ..Default::default()
        };

        assert_eq!(NodeWidget::role_badge(&validator).0, "VALIDATOR");
        assert_eq!(NodeWidget::role_badge(&observer).0, "OBSERVER");
    }

    #[test]
    fn test_stacked_lines_include_key_fields() {
        let lines = NodeWidget::stacked_lines(&sample_node(), true, 20);

        assert_eq!(line_text(&lines[0]), "Node");
        assert_eq!(line_text(&lines[1]), "Name: Satyrs");
        assert_eq!(line_text(&lines[4]), "");
        assert_eq!(line_text(&lines[5]), "Chain");
        assert_eq!(line_text(&lines[6]), "Block: 145,333,141");
        assert_eq!(line_text(&lines[7]), "Epoch: 337,985");
        assert_eq!(line_text(&lines[9]), "");
        assert_eq!(line_text(&lines[10]), "Consensus");
        assert_eq!(line_text(&lines[11]), "QC: 145,333,143");
        assert_eq!(line_text(&lines[13]), "Committed: 145,333,141");
    }

    #[test]
    fn test_compact_summary_columns_include_key_fields() {
        let (left, right) = NodeWidget::compact_summary_columns(&sample_node(), 20, 10, 10);

        assert_eq!(line_text(&left[0]), "Name: Satyrs");
        assert_eq!(line_text(&left[2]), "Role: OBSERVER");
        assert_eq!(line_text(&right[0]), "Block: 145,333,141");
        assert_eq!(line_text(&right[2]), "View: 2");
        assert_eq!(line_text(&right[3]), "QC: 145,333,143");
        assert_eq!(line_text(&right[5]), "Committed: 145,333,141");
    }

    #[test]
    fn test_shorten_host_for_width_preserves_port_suffix() {
        assert_eq!(
            NodeWidget::shorten_host_for_width(&sample_long_host_node().host, 20),
            "validator-long…:6790"
        );
    }

    #[test]
    fn test_shorten_host_for_width_handles_unicode_safely() {
        assert_eq!(
            NodeWidget::shorten_host_for_width("验证节点.example.internal:6790", 16),
            "验证节点.examp…:6790"
        );
    }

    #[test]
    fn test_table_row_values_include_expected_columns() {
        let values = NodeWidget::table_row_values(&sample_node(), 20);

        assert_eq!(values[0], " Satyrs");
        assert_eq!(values[1], "127.0.0.1:6790");
        assert_eq!(values[2], "145,333,141");
        assert_eq!(values[8], "OBSERVER");
    }

    #[test]
    fn test_node_table_highlight_style_helpers_match_expected_colors() {
        let (_, observer_color) = NodeWidget::role_badge(&sample_node());

        assert_eq!(NodeWidget::metric_value_style().fg, Some(block::METRIC_TERTIARY));
        assert_eq!(NodeWidget::role_value_style(observer_color).fg, Some(block::ACCENT_WARN));
    }

    #[test]
    fn test_render_empty_state_uses_muted_style() {
        let widget = NodeWidget::new(create_shared_data());
        let area = Rect::new(0, 0, 24, 5);
        let mut buf = Buffer::empty(area);

        (&widget).render(area, &mut buf);

        assert_eq!(buf.get(1, 1).symbol(), "N");
        assert_eq!(buf.get(1, 1).fg, block::PANEL_MUTED);
        assert_eq!(buf.get(1, 1).bg, block::PANEL_BG);
    }

    #[test]
    fn test_visible_compact_lines_prioritize_core_fields() {
        let lines = NodeWidget::visible_compact_lines(&sample_node(), 20, 3);

        assert_eq!(lines.len(), 3);
        assert_eq!(line_text(&lines[0]), "Name: Satyrs");
        assert_eq!(line_text(&lines[1]), "Role: OBSERVER");
        assert_eq!(line_text(&lines[2]), "Block: 145,333,141");
    }

    #[test]
    fn test_compact_lines_include_key_fields() {
        let lines = NodeWidget::compact_lines(&sample_node(), 20);

        assert_eq!(lines[0], "Name: Satyrs");
        assert_eq!(lines[1], "Host: 127.0.0.1:6790");
        assert_eq!(lines[2], "Role: OBSERVER");
        assert_eq!(lines[3], "Block: 145,333,141");
        assert_eq!(lines[4], "Epoch: 337,985");
        assert_eq!(lines[5], "View: 2");
        assert_eq!(lines[6], "QC: 145,333,143");
        assert_eq!(lines[7], "Locked: 145,333,142");
        assert_eq!(lines[8], "Committed: 145,333,141");
    }

    #[test]
    fn test_compact_list_lines_include_more_indicator() {
        let mut widget = NodeWidget::new(create_shared_data());
        widget.nodes = vec![sample_node(), sample_node(), sample_node()];

        let lines = widget.compact_list_lines(70, 2);

        assert_eq!(lines.len(), 2);
        assert!(line_text(&lines[0]).contains("Satyrs OBSERVER #145,333,141 E337,985 V2"));
        assert_eq!(line_text(&lines[1]), "+2 more nodes");
    }
}
