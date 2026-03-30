use std::collections::HashMap;

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
        Paragraph,
        Row,
        Table,
        Widget,
        Wrap,
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

const NODE_VALUE_COLOR: Color = Color::Indexed(153);
const RUNTIME_VALUE_COLOR: Color = Color::Indexed(150);

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

    fn flexible_width(
        area_width: u16,
        reserved_width: u16,
        min_width: u16,
    ) -> u16 {
        area_width.saturating_sub(2).saturating_sub(reserved_width).max(min_width)
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

    fn format_gigabytes(value: u64) -> String {
        format!("{:.2}G", value as f64 / 1024.0 / 1024.0 / 1024.0)
    }

    fn section_heading(title: &str) -> Line<'static> {
        Line::from(vec![Span::styled(title.to_string(), block::header_style())])
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

    fn role_badge(node: &ConsensusState) -> (&'static str, Color) {
        if node.validator {
            ("VALIDATOR", Color::LightGreen)
        } else {
            ("OBSERVER", Color::Yellow)
        }
    }

    fn render_single_node(
        &self,
        area: Rect,
        buf: &mut Buffer,
        node: &ConsensusState,
        stat: Option<&NodeStats>,
    ) {
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

        let columns = if stat.is_some() {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(31),
                        Constraint::Percentage(34),
                        Constraint::Percentage(35),
                    ]
                    .as_ref(),
                )
                .split(content)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(34),
                        Constraint::Percentage(33),
                        Constraint::Percentage(33),
                    ]
                    .as_ref(),
                )
                .split(content)
        };

        let show_section_headings = content.height >= 7;
        let node_value_style = block::content_style().add_modifier(Modifier::BOLD);
        let metric_value_style =
            block::content_style().fg(NODE_VALUE_COLOR).add_modifier(Modifier::BOLD);
        let runtime_value_style =
            block::content_style().fg(RUNTIME_VALUE_COLOR).add_modifier(Modifier::BOLD);
        let (role_text, role_color) = Self::role_badge(node);

        let mut left_lines = Vec::new();
        if show_section_headings {
            left_lines.push(Self::section_heading("Node"));
        }
        left_lines.push(Self::info_line_with_style("Name", node.name.clone(), node_value_style));
        left_lines.push(Self::info_line_with_style(
            "Host",
            node.host.clone(),
            block::content_style(),
        ));
        left_lines.push(Line::from(vec![
            Span::styled("Role: ", block::muted_style()),
            Span::styled(
                role_text.to_string(),
                block::content_style().fg(role_color).add_modifier(Modifier::BOLD),
            ),
        ]));

        let chain_primary_lines = [
            Self::info_line_with_style(
                "Block",
                Self::format_number(node.current_number),
                metric_value_style,
            ),
            Self::info_line_with_style(
                "Epoch",
                Self::format_number(node.epoch),
                metric_value_style,
            ),
            Self::info_line_with_style("View", Self::format_number(node.view), metric_value_style),
        ];
        let chain_secondary_lines = [
            Self::info_line_with_style("QC", Self::format_number(node.qc), metric_value_style),
            Self::info_line_with_style(
                "Locked",
                Self::format_number(node.locked),
                metric_value_style,
            ),
            Self::info_line_with_style(
                "Committed",
                Self::format_number(node.committed),
                metric_value_style,
            ),
        ];

        let mut middle_lines = Vec::new();
        if show_section_headings {
            middle_lines.push(Self::section_heading("Chain"));
        }
        middle_lines.extend(chain_primary_lines.clone());
        if stat.is_some() {
            middle_lines.extend(chain_secondary_lines.clone());
        }

        let left =
            Paragraph::new(left_lines).style(block::content_style()).wrap(Wrap { trim: true });
        let middle =
            Paragraph::new(middle_lines).style(block::content_style()).wrap(Wrap { trim: true });

        left.render(columns[0], buf);
        middle.render(columns[1], buf);

        if let Some(stat) = stat {
            let mem = Self::format_gigabytes(stat.mem);
            let mem_limit = Self::format_gigabytes(stat.mem_limit);
            let mut right_lines = Vec::new();
            if show_section_headings {
                right_lines.push(Self::section_heading("Runtime"));
            }
            right_lines.extend([
                Self::info_line_with_style(
                    "CPU",
                    format!("{:.2}%", stat.cpu_percent),
                    runtime_value_style,
                ),
                Self::info_line_with_style(
                    "Mem",
                    format!("{:.2}%  {mem}/{mem_limit}", stat.mem_percent),
                    runtime_value_style,
                ),
                Self::info_line_with_style(
                    "RX",
                    format!("{:.2} MB/s", stat.network_rx as f64 / 1024.0 / 1024.0),
                    runtime_value_style,
                ),
                Self::info_line_with_style(
                    "TX",
                    format!("{:.2} MB/s", stat.network_tx as f64 / 1024.0 / 1024.0),
                    runtime_value_style,
                ),
                Self::info_line_with_style(
                    "Read",
                    Self::format_gigabytes(stat.blk_read),
                    runtime_value_style,
                ),
                Self::info_line_with_style(
                    "Write",
                    Self::format_gigabytes(stat.blk_write),
                    runtime_value_style,
                ),
            ]);
            Paragraph::new(right_lines)
                .style(block::content_style())
                .wrap(Wrap { trim: true })
                .render(columns[2], buf);
        } else {
            let mut right_lines = Vec::new();
            if show_section_headings {
                right_lines.push(Self::section_heading("Consensus"));
            }
            right_lines.extend(chain_secondary_lines);
            Paragraph::new(right_lines)
                .style(block::content_style())
                .wrap(Wrap { trim: true })
                .render(columns[2], buf);
        }
    }

    fn render_without_stats(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let header =
            [" Name", "Host", "Block", "Epoch", "View", "QC", "Locked", "Committed", "Role"];

        let rows = self.nodes.iter().map(|node| {
            let (role_text, _) = Self::role_badge(node);
            Row::new(vec![
                format!(" {}", &node.name),
                node.host.clone(),
                Self::format_number(node.current_number),
                Self::format_number(node.epoch),
                Self::format_number(node.view),
                Self::format_number(node.qc),
                Self::format_number(node.locked),
                Self::format_number(node.committed),
                role_text.to_string(),
            ])
            .style(block::content_style())
        });

        let header_row = Row::new(header.iter().copied()).style(block::header_style());

        Table::new(
            rows,
            &[
                Constraint::Length(16),
                Constraint::Length(Self::flexible_width(area.width, 88, 18)),
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
            "QC",
            "Locked",
            "Committed",
            "Role",
            "CPU",
            "Memory",
            "RX",
            "TX",
        ];

        let rows = self.nodes.iter().map(|node| {
            let stat = stats.get(&node.name).unwrap_or_default();
            let mem = Self::format_gigabytes(stat.mem);
            let mem_limit = Self::format_gigabytes(stat.mem_limit);
            let (role_text, _role_color) = Self::role_badge(node);
            Row::new(vec![
                format!(" {}", &node.name),
                node.host.clone(),
                Self::format_number(node.current_number),
                Self::format_number(node.epoch),
                Self::format_number(node.view),
                Self::format_number(node.qc),
                Self::format_number(node.locked),
                Self::format_number(node.committed),
                role_text.to_string(),
                format!("{:.1}%", stat.cpu_percent),
                format!("{:.1}% {mem}/{mem_limit}", stat.mem_percent),
                format!("{:.2}M", stat.network_rx as f64 / 1024.0 / 1024.0),
                format!("{:.2}M", stat.network_tx as f64 / 1024.0 / 1024.0),
            ])
            .style(block::content_style())
        });

        let header_row = Row::new(header.iter().copied()).style(block::header_style());

        Table::new(
            rows,
            &[
                Constraint::Length(14),
                Constraint::Length(Self::flexible_width(area.width, 135, 18)),
                Constraint::Length(14),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(14),
                Constraint::Length(14),
                Constraint::Length(14),
                Constraint::Length(11),
                Constraint::Length(8),
                Constraint::Length(20),
                Constraint::Length(8),
                Constraint::Length(8),
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

        if self.nodes.len() == 1 {
            let node = &self.nodes[0];
            self.render_single_node(area, buf, node, self.stats.get(&node.name));
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
        assert!(!widget.stats.contains_key("nonexistent"));
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
}
