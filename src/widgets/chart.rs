use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{
        Color,
        Modifier,
        Style,
    },
    widgets::Widget,
};
use unicode_width::UnicodeWidthStr;

use crate::widgets::{
    block,
    helpers::format_grouped_u64,
};

pub const MAX_DATA_POINTS: usize = 200;
pub const NARROW_CHART_WIDTH: u16 = 40;
pub const ULTRA_NARROW_CHART_WIDTH: u16 = 32;
pub const PLOT_FILL_COLOR: Color = Color::Rgb(214, 106, 206);
pub const PLOT_CREST_COLOR: Color = Color::Rgb(232, 224, 255);
pub const AXIS_LABEL_COLOR: Color = block::PANEL_MUTED;
pub const INFO_FRAME_COLOR: Color = block::PANEL_BORDER;
pub const INFO_LABEL_COLOR: Color = block::PANEL_MUTED;
const INLINE_FRAME_WIDTH: u16 = 2;

pub type StyledSegment = (String, Style);
pub type StyledSegments = Vec<StyledSegment>;
pub type LabeledBoxRow = (String, Style, StyledSegments);
pub type SegmentGridRow = Vec<StyledSegments>;

#[derive(Clone, Copy)]
pub struct StandardMetricPalette {
    pub trend_up: Color,
    pub trend_down: Color,
    pub current_fallback: Color,
    pub top_fallback: Color,
    pub avg: Color,
    pub block: Color,
}

impl StandardMetricPalette {
    pub fn trend_style(
        self,
        trend: &str,
    ) -> Style {
        trend_style(trend, self.trend_up, self.trend_down)
    }

    pub fn current_box_style(self) -> Style {
        block::content_style().fg(PLOT_FILL_COLOR).add_modifier(Modifier::BOLD)
    }

    pub fn current_fallback_style(self) -> Style {
        block::content_style().fg(self.current_fallback).add_modifier(Modifier::BOLD)
    }

    pub fn top_box_style(self) -> Style {
        block::content_style().fg(PLOT_CREST_COLOR).add_modifier(Modifier::BOLD)
    }

    pub fn top_fallback_style(self) -> Style {
        block::content_style().fg(self.top_fallback).add_modifier(Modifier::BOLD)
    }

    pub fn avg_style(self) -> Style {
        block::content_style().fg(self.avg).add_modifier(Modifier::BOLD)
    }

    pub fn block_style(self) -> Style {
        block::content_style().fg(self.block).add_modifier(Modifier::BOLD)
    }
}

pub struct StandardMetricValues<'a> {
    pub trend: &'a str,
    pub current_box_value: String,
    pub current_fallback_value: String,
    pub top_box_value: String,
    pub top_fallback: String,
    pub avg_trend: &'a str,
    pub avg_box_value: String,
    pub avg_fallback_value: String,
    pub block_box_value: String,
    pub block_fallback: String,
}

pub fn styled_segment<S>(
    text: S,
    style: Style,
) -> StyledSegment
where
    S: Into<String>,
{
    (text.into(), style)
}

pub fn single_segment<S>(
    text: S,
    style: Style,
) -> StyledSegments
where
    S: Into<String>,
{
    vec![styled_segment(text, style)]
}

pub fn labeled_info_row(
    label: &str,
    value_segments: StyledSegments,
) -> LabeledBoxRow {
    (label.to_string(), Style::default().fg(INFO_LABEL_COLOR).bg(block::PANEL_BG), value_segments)
}

pub fn two_column_segment_grid(
    top_left: StyledSegments,
    top_right: StyledSegments,
    bottom_left: StyledSegments,
    bottom_right: StyledSegments,
) -> Vec<SegmentGridRow> {
    vec![vec![top_left, top_right], vec![bottom_left, bottom_right]]
}

pub fn standard_metric_rows(
    labels: (&str, &str, &str, &str),
    values: &StandardMetricValues<'_>,
    palette: StandardMetricPalette,
) -> (Vec<LabeledBoxRow>, Vec<SegmentGridRow>) {
    let (cur_label, _max_label, avg_label, _blk_label) = labels;

    let box_rows = vec![
        labeled_info_row(
            "now:",
            vec![
                styled_segment(values.trend, palette.trend_style(values.trend)),
                styled_segment(values.current_box_value.clone(), palette.current_box_style()),
            ],
        ),
        labeled_info_row(
            "top:",
            single_segment(values.top_box_value.clone(), palette.top_box_style()),
        ),
        labeled_info_row(
            "avg:",
            vec![
                styled_segment(values.avg_trend, palette.trend_style(values.avg_trend)),
                styled_segment(values.avg_box_value.clone(), palette.avg_style()),
            ],
        ),
        labeled_info_row(
            "blk:",
            single_segment(values.block_box_value.clone(), palette.block_style()),
        ),
    ];

    let fallback_rows = two_column_segment_grid(
        vec![
            styled_segment(format!("{cur_label} "), palette.current_fallback_style()),
            styled_segment(values.trend, palette.trend_style(values.trend)),
            styled_segment(values.current_fallback_value.clone(), palette.current_fallback_style()),
        ],
        single_segment(values.top_fallback.clone(), palette.top_fallback_style()),
        vec![
            styled_segment(format!("{avg_label} "), palette.avg_style()),
            styled_segment(values.avg_trend, palette.trend_style(values.avg_trend)),
            styled_segment(values.avg_fallback_value.clone(), palette.avg_style()),
        ],
        single_segment(values.block_fallback.clone(), palette.block_style()),
    );

    (box_rows, fallback_rows)
}

pub fn limit_standard_metric_rows(
    box_rows: &[LabeledBoxRow],
    fallback_rows: &[SegmentGridRow],
    max_metrics: usize,
) -> (Vec<LabeledBoxRow>, Vec<SegmentGridRow>) {
    let max_metrics = max_metrics.clamp(1, 4);

    let limited_box_rows = match max_metrics {
        1 => vec![box_rows[0].clone()],
        2 => vec![box_rows[0].clone(), box_rows[1].clone()],
        3 => vec![box_rows[0].clone(), box_rows[1].clone(), box_rows[2].clone()],
        _ => box_rows.to_vec(),
    };

    let limited_fallback_rows = match max_metrics {
        1 => vec![vec![fallback_rows[0][0].clone()]],
        2 => vec![vec![fallback_rows[0][0].clone(), fallback_rows[0][1].clone()]],
        3 => vec![
            vec![fallback_rows[0][0].clone(), fallback_rows[0][1].clone()],
            vec![fallback_rows[1][0].clone()],
        ],
        _ => fallback_rows.to_vec(),
    };

    (limited_box_rows, limited_fallback_rows)
}

pub struct MetricPanel<'a> {
    pub outer_title: &'a str,
    pub y_max: f64,
    pub top_label: &'a str,
    pub box_rows: &'a [LabeledBoxRow],
    pub box_options: LabeledBoxOptions<'a>,
    pub fallback_rows: &'a [SegmentGridRow],
    pub plot_fill_style: Style,
    pub plot_crest_style: Style,
    pub band_rows: Option<u16>,
}

pub struct LabeledBoxOptions<'a> {
    pub start_y_offset: u16,
    pub title: &'a str,
    pub title_style: Style,
    pub frame_style: Style,
    pub background_style: Style,
    pub column_gap: u16,
    pub right_inset: u16,
}

pub fn default_labeled_box_options<'a>(title: &'a str) -> LabeledBoxOptions<'a> {
    LabeledBoxOptions {
        start_y_offset: 2,
        title,
        title_style: block::header_style(),
        frame_style: Style::default().fg(INFO_FRAME_COLOR).bg(block::PANEL_BG),
        background_style: block::content_style(),
        column_gap: 2,
        right_inset: 2,
    }
}

pub fn lower_band_rows(area_height: u16) -> u16 {
    area_height.saturating_mul(3).saturating_div(4).max(2)
}

pub fn lighter_band_rows(area_height: u16) -> u16 {
    area_height.saturating_mul(2).saturating_div(3).max(2)
}

pub fn info_labels(area_width: u16) -> (&'static str, &'static str, &'static str, &'static str) {
    if area_width < ULTRA_NARROW_CHART_WIDTH {
        ("C", "M", "A", "B")
    } else {
        ("CUR", "MAX", "AVG", "BLK")
    }
}

pub fn format_grouped_number(value: u64) -> String {
    format_grouped_u64(value)
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn clear_area(
    buf: &mut Buffer,
    area: Rect,
    style: Style,
) {
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            buf.get_mut(x, y).set_symbol(" ").set_style(style);
        }
    }
}

pub fn render_left_axis_labels(
    buf: &mut Buffer,
    area: Rect,
    top_label: &str,
    bottom_label: &str,
    style: Style,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return area;
    }

    let label_width = display_width(top_label).max(display_width(bottom_label)) as u16;
    let gutter_width = label_width.saturating_add(1);
    if label_width == 0 || area.width.saturating_sub(gutter_width) < 4 {
        return area;
    }

    let label_area_width = gutter_width.saturating_sub(1) as usize;
    let top_x = area.x + label_area_width.saturating_sub(display_width(top_label)) as u16;
    buf.set_stringn(top_x, area.y, top_label, label_area_width, style);

    if area.height > 1 {
        let bottom_y = area.y + area.height - 1;
        let bottom_x = area.x + label_area_width.saturating_sub(display_width(bottom_label)) as u16;
        buf.set_stringn(bottom_x, bottom_y, bottom_label, label_area_width, style);
    }

    Rect::new(area.x + gutter_width, area.y, area.width.saturating_sub(gutter_width), area.height)
}

fn braille_fill_symbol(filled_rows: u16) -> &'static str {
    match filled_rows {
        0 => " ",
        1 => "⣀",
        2 => "⣤",
        3 => "⣶",
        _ => "⣿",
    }
}

pub fn render_bottom_band_dotted_plot(
    buf: &mut Buffer,
    area: Rect,
    data: &[(f64, f64)],
    y_max: f64,
    band_rows: u16,
    fill_style: Style,
    crest_style: Style,
) {
    if area.width == 0 || area.height == 0 || data.is_empty() || y_max <= 0.0 {
        return;
    }

    let visible_width = area.width as usize;
    let visible_count = visible_width.min(data.len());
    let start_index = data.len().saturating_sub(visible_count);
    let start_x = area.x + area.width.saturating_sub(visible_count as u16);
    let band_rows = band_rows.clamp(1, area.height);
    let band_units = band_rows.saturating_mul(4);

    let visible_data = &data[start_index..];

    for (column_index, (_, value)) in visible_data.iter().enumerate() {
        let x = start_x + column_index as u16;
        let prev = if column_index > 0 {
            visible_data[column_index - 1].1
        } else {
            *value
        };
        let next = if column_index + 1 < visible_data.len() {
            visible_data[column_index + 1].1
        } else {
            *value
        };
        let smoothed_value = ((prev + *value * 2.0 + next) / 4.0).max(0.0).min(y_max);
        let ratio = (smoothed_value / y_max).clamp(0.0, 1.0);
        let mut filled_units = (ratio * f64::from(band_units)).round() as u16;
        if smoothed_value > 0.0 && filled_units == 0 {
            filled_units = 1;
        }

        for cell_offset in 0..band_rows {
            let y = area.y + area.height - 1 - cell_offset;
            let cell_start = cell_offset.saturating_mul(4);
            let filled_rows = filled_units.saturating_sub(cell_start).min(4);
            if filled_rows > 0 {
                let is_crest = filled_units > cell_start && filled_units <= cell_start + 4;
                let style = if is_crest { crest_style } else { fill_style };
                buf.get_mut(x, y).set_symbol(braille_fill_symbol(filled_rows)).set_style(style);
            }
        }
    }
}

pub fn default_metric_panel<'a>(
    outer_title: &'a str,
    box_title: &'a str,
    y_max: f64,
    top_label: &'a str,
    box_rows: &'a [LabeledBoxRow],
    fallback_rows: &'a [SegmentGridRow],
) -> MetricPanel<'a> {
    MetricPanel {
        outer_title,
        y_max,
        top_label,
        box_rows,
        box_options: default_labeled_box_options(box_title),
        fallback_rows,
        plot_fill_style: Style::default().fg(PLOT_FILL_COLOR).bg(block::PANEL_BG),
        plot_crest_style: Style::default().fg(PLOT_CREST_COLOR).bg(block::PANEL_BG),
        band_rows: None,
    }
}

pub fn render_metric_panel(
    buf: &mut Buffer,
    area: Rect,
    data: &[(f64, f64)],
    panel: &MetricPanel<'_>,
) {
    buf.set_style(area, block::content_style());

    let outer_block = block::new(panel.outer_title);
    let inner = outer_block.inner(area);
    outer_block.render(area, buf);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    clear_area(buf, inner, block::content_style());

    let plot_area = render_left_axis_labels(
        buf,
        inner,
        panel.top_label,
        "0",
        Style::default().fg(AXIS_LABEL_COLOR).bg(block::PANEL_BG),
    );
    let band_rows = panel.band_rows.unwrap_or_else(|| lower_band_rows(plot_area.height));

    render_bottom_band_dotted_plot(
        buf,
        plot_area,
        data,
        panel.y_max,
        band_rows,
        panel.plot_fill_style,
        panel.plot_crest_style,
    );

    if !render_right_aligned_labeled_box(buf, inner, panel.box_rows, &panel.box_options) {
        render_right_aligned_segment_grid(buf, inner, 0, panel.fallback_rows, 2);
    }
}

pub fn render_right_aligned_labeled_box(
    buf: &mut Buffer,
    area: Rect,
    rows: &[LabeledBoxRow],
    options: &LabeledBoxOptions<'_>,
) -> bool {
    let inner_width = area.width.saturating_sub(2);
    if inner_width == 0 || rows.is_empty() {
        return false;
    }

    let label_width = rows.iter().map(|(label, _, _)| display_width(label)).max().unwrap_or(0);
    let value_width = rows
        .iter()
        .map(|(_, _, value)| value.iter().map(|(text, _)| display_width(text)).sum::<usize>())
        .max()
        .unwrap_or(0);
    let content_width = label_width
        .saturating_add(options.column_gap as usize)
        .saturating_add(value_width)
        .max(display_width(options.title).saturating_add(2));
    if content_width == 0 {
        return false;
    }

    let available_content_width = inner_width.saturating_sub(2) as usize;
    if content_width > available_content_width {
        return false;
    }

    let total_rows = rows.len().saturating_add(2) as u16;
    let available_rows = area.height.saturating_sub(options.start_y_offset.saturating_add(1));
    if total_rows == 0 || total_rows > available_rows {
        return false;
    }

    let content_width = content_width as u16;
    let label_width = label_width as u16;
    let total_width = content_width.saturating_add(2);
    let box_x = area.x + area.width.saturating_sub(total_width.saturating_add(options.right_inset));
    let content_x = box_x.saturating_add(1);
    let top_y = area.y + options.start_y_offset;
    let bottom_y = top_y + total_rows - 1;

    for y in top_y..=bottom_y {
        for x in box_x..box_x + total_width {
            buf.get_mut(x, y).set_symbol(" ").set_style(options.background_style);
        }
    }

    buf.get_mut(box_x, top_y).set_symbol("╭").set_style(options.frame_style);
    buf.get_mut(box_x + total_width - 1, top_y).set_symbol("╮").set_style(options.frame_style);
    for x in box_x + 1..box_x + total_width - 1 {
        buf.get_mut(x, top_y).set_symbol("─").set_style(options.frame_style);
    }

    let title_text = format!(" {} ", options.title);
    let title_width = display_width(&title_text) as u16;
    let title_x = content_x + if content_width > title_width { 1 } else { 0 };
    buf.set_stringn(title_x, top_y, &title_text, title_text.len(), options.title_style);

    for (index, (label, label_style, value_segments)) in rows.iter().enumerate() {
        let y = top_y + index as u16 + 1;
        buf.get_mut(box_x, y).set_symbol("│").set_style(options.frame_style);
        buf.get_mut(box_x + total_width - 1, y).set_symbol("│").set_style(options.frame_style);

        buf.set_stringn(content_x, y, label, label_width as usize, *label_style);

        let row_width = value_segments
            .iter()
            .map(|(text, _)| display_width(text))
            .sum::<usize>()
            .min(content_width as usize);
        let mut text_x = content_x + content_width.saturating_sub(row_width as u16);
        for (text, style) in value_segments {
            let segment_width = display_width(text);
            buf.set_stringn(text_x, y, text, segment_width, *style);
            text_x += segment_width as u16;
        }
    }

    buf.get_mut(box_x, bottom_y).set_symbol("╰").set_style(options.frame_style);
    buf.get_mut(box_x + total_width - 1, bottom_y).set_symbol("╯").set_style(options.frame_style);
    for x in box_x + 1..box_x + total_width - 1 {
        buf.get_mut(x, bottom_y).set_symbol("─").set_style(options.frame_style);
    }

    true
}

pub fn trim_data_points(
    data: &mut Vec<(f64, f64)>,
    max_data_points: usize,
) {
    if data.len() > max_data_points {
        data.drain(0..data.len() - max_data_points);
    }
}

pub fn append_u64_samples<I>(
    data: &mut Vec<(f64, f64)>,
    update_count: &mut u64,
    samples: I,
) where
    I: IntoIterator<Item = u64>,
{
    for sample in samples {
        data.push((*update_count as f64, sample as f64));
        *update_count += 1;
    }

    trim_data_points(data, MAX_DATA_POINTS);
}

pub fn trend_style(
    trend: &str,
    up_color: Color,
    down_color: Color,
) -> Style {
    match trend {
        "↑" => Style::default().fg(up_color).add_modifier(Modifier::BOLD),
        "↓" => Style::default().fg(down_color).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    }
}

pub fn y_axis_upper_bound(
    data: &[(f64, f64)],
    min_y_axis_max: f64,
    steps: &[(f64, f64)],
) -> f64 {
    let max_value = data.iter().map(|(_, value)| *value).fold(0.0, f64::max);

    if max_value <= min_y_axis_max {
        return min_y_axis_max;
    }

    let value_with_headroom = max_value * 1.1;
    let step = steps
        .iter()
        .find(|(threshold, _)| value_with_headroom <= *threshold)
        .map(|(_, step)| *step)
        .or_else(|| steps.last().map(|(_, step)| *step))
        .unwrap_or(1.0);

    (value_with_headroom / step).ceil() * step
}

fn rounded_nonzero_values(data: &[(f64, f64)]) -> Vec<u64> {
    data.iter()
        .filter_map(|(_, value)| {
            if *value > 0.0 {
                Some(value.round() as u64)
            } else {
                None
            }
        })
        .collect()
}

pub fn average_recent_nonzero_rounded(
    data: &[(f64, f64)],
    sample_count: usize,
) -> u64 {
    let recent_values: Vec<u64> =
        rounded_nonzero_values(data).into_iter().rev().take(sample_count).collect();

    if recent_values.is_empty() {
        return 0;
    }

    let sum: u64 = recent_values.iter().sum();
    sum / recent_values.len() as u64
}

pub fn recent_trend_symbol(data: &[(f64, f64)]) -> &'static str {
    let recent_values: Vec<u64> = rounded_nonzero_values(data).into_iter().rev().take(2).collect();

    if recent_values.len() < 2 {
        return "→";
    }

    if recent_values[0] > recent_values[1] {
        "↑"
    } else if recent_values[0] < recent_values[1] {
        "↓"
    } else {
        "→"
    }
}

pub fn recent_window_trend_symbol(
    data: &[(f64, f64)],
    sample_count: usize,
) -> &'static str {
    let values = rounded_nonzero_values(data);

    if sample_count == 0 || values.len() < sample_count * 2 {
        return "→";
    }

    let recent = &values[values.len() - sample_count..];
    let previous = &values[values.len() - sample_count * 2..values.len() - sample_count];

    let recent_avg = recent.iter().sum::<u64>() / sample_count as u64;
    let previous_avg = previous.iter().sum::<u64>() / sample_count as u64;

    if recent_avg > previous_avg {
        "↑"
    } else if recent_avg < previous_avg {
        "↓"
    } else {
        "→"
    }
}

#[cfg(test)]
pub fn render_right_aligned_text_lines(
    buf: &mut Buffer,
    area: Rect,
    start_y_offset: u16,
    lines: &[(String, Style)],
) {
    let inner_width = area.width.saturating_sub(2);
    if inner_width == 0 || lines.is_empty() {
        return;
    }

    let max_y_offset = area.height.saturating_sub(1);
    let has_frame = inner_width > INLINE_FRAME_WIDTH;
    let content_width = lines
        .iter()
        .map(|(text, _)| display_width(text))
        .max()
        .unwrap_or(0)
        .min(inner_width.saturating_sub(if has_frame { INLINE_FRAME_WIDTH } else { 0 }) as usize)
        as u16;
    if content_width == 0 {
        return;
    }

    let total_width = content_width + if has_frame { INLINE_FRAME_WIDTH } else { 0 };
    let box_x = area.x + area.width.saturating_sub(total_width + 1);
    let content_x = box_x + if has_frame { 1 } else { 0 };

    for (index, (text, style)) in lines.iter().enumerate() {
        let y_offset = start_y_offset + index as u16;
        if y_offset >= max_y_offset {
            break;
        }

        let y = area.y + y_offset;
        for x in box_x..box_x + total_width {
            buf.get_mut(x, y).set_symbol(" ").set_style(Style::default());
        }

        if has_frame {
            let (left, right) = inline_frame_symbols(index, lines.len());
            let frame_style = Style::default().fg(Color::DarkGray);
            buf.get_mut(box_x, y).set_symbol(left).set_style(frame_style);
            buf.get_mut(box_x + total_width - 1, y).set_symbol(right).set_style(frame_style);
        }

        let text_width = display_width(text).min(content_width as usize) as u16;
        let text_x = content_x + content_width.saturating_sub(text_width);
        buf.set_stringn(text_x, y, text, content_width as usize, *style);
    }
}

fn inline_frame_symbols(
    row_index: usize,
    row_count: usize,
) -> (&'static str, &'static str) {
    if row_count <= 1 {
        return ("[", "]");
    }

    if row_index == 0 {
        ("┌", "┐")
    } else if row_index + 1 == row_count {
        ("└", "┘")
    } else {
        ("│", "│")
    }
}

pub fn render_right_aligned_segment_grid(
    buf: &mut Buffer,
    area: Rect,
    start_y_offset: u16,
    rows: &[SegmentGridRow],
    column_gap: u16,
) {
    let inner_width = area.width.saturating_sub(2);
    if inner_width == 0 || rows.is_empty() {
        return;
    }

    let max_y_offset = area.height.saturating_sub(1);
    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    if column_count == 0 {
        return;
    }

    let mut column_widths = vec![0usize; column_count];
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            let cell_width = cell.iter().map(|(text, _)| display_width(text)).sum::<usize>();
            column_widths[index] = column_widths[index].max(cell_width);
        }
    }

    let content_width =
        column_widths.iter().sum::<usize>() + column_gap as usize * column_count.saturating_sub(1);
    if content_width == 0 {
        return;
    }

    let has_frame = inner_width > INLINE_FRAME_WIDTH;
    let available_content_width =
        inner_width.saturating_sub(if has_frame { INLINE_FRAME_WIDTH } else { 0 }) as usize;
    if content_width > available_content_width {
        return;
    }

    let total_width = content_width as u16 + if has_frame { INLINE_FRAME_WIDTH } else { 0 };
    let box_x = area.x + area.width.saturating_sub(total_width + 1);
    let content_x = box_x + if has_frame { 1 } else { 0 };

    for (row_index, row) in rows.iter().enumerate() {
        let y_offset = start_y_offset + row_index as u16;
        if y_offset >= max_y_offset {
            break;
        }

        let y = area.y + y_offset;
        for x in box_x..box_x + total_width {
            buf.get_mut(x, y).set_symbol(" ").set_style(block::content_style());
        }

        if has_frame {
            let (left, right) = inline_frame_symbols(row_index, rows.len());
            let frame_style = Style::default().fg(INFO_FRAME_COLOR).bg(block::PANEL_BG);
            buf.get_mut(box_x, y).set_symbol(left).set_style(frame_style);
            buf.get_mut(box_x + total_width - 1, y).set_symbol(right).set_style(frame_style);
        }

        let mut cursor_x = content_x + content_width as u16;
        for column_index in (0..column_count).rev() {
            let width = column_widths[column_index] as u16;
            let cell_start = cursor_x.saturating_sub(width);

            if let Some(cell) = row.get(column_index) {
                let cell_width =
                    cell.iter().map(|(text, _)| display_width(text)).sum::<usize>() as u16;
                let mut text_x = cell_start + width.saturating_sub(cell_width);
                for (text, style) in cell {
                    buf.set_stringn(text_x, y, text, display_width(text), *style);
                    text_x += display_width(text) as u16;
                }
            }

            cursor_x = cell_start.saturating_sub(column_gap);
        }
    }
}

#[cfg(test)]
pub fn render_right_aligned_text_grid(
    buf: &mut Buffer,
    area: Rect,
    start_y_offset: u16,
    rows: &[Vec<(String, Style)>],
    column_gap: u16,
) {
    let inner_width = area.width.saturating_sub(2);
    if inner_width == 0 || rows.is_empty() {
        return;
    }

    let max_y_offset = area.height.saturating_sub(1);
    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    if column_count == 0 {
        return;
    }

    let mut column_widths = vec![0usize; column_count];
    for row in rows {
        for (index, (text, _)) in row.iter().enumerate() {
            column_widths[index] = column_widths[index].max(display_width(text));
        }
    }

    let content_width =
        column_widths.iter().sum::<usize>() + column_gap as usize * column_count.saturating_sub(1);
    if content_width == 0 {
        return;
    }

    let has_frame = inner_width > INLINE_FRAME_WIDTH;
    let available_content_width =
        inner_width.saturating_sub(if has_frame { INLINE_FRAME_WIDTH } else { 0 }) as usize;
    if content_width > available_content_width {
        let lines: Vec<(String, Style)> = rows
            .iter()
            .flat_map(|row| row.iter().filter(|(text, _)| !text.is_empty()).cloned())
            .collect();
        render_right_aligned_text_lines(buf, area, start_y_offset, &lines);
        return;
    }

    let total_width = content_width as u16 + if has_frame { INLINE_FRAME_WIDTH } else { 0 };
    let box_x = area.x + area.width.saturating_sub(total_width + 1);
    let content_x = box_x + if has_frame { 1 } else { 0 };

    for (row_index, row) in rows.iter().enumerate() {
        let y_offset = start_y_offset + row_index as u16;
        if y_offset >= max_y_offset {
            break;
        }

        let y = area.y + y_offset;
        for x in box_x..box_x + total_width {
            buf.get_mut(x, y).set_symbol(" ").set_style(Style::default());
        }

        if has_frame {
            let (left, right) = inline_frame_symbols(row_index, rows.len());
            let frame_style = Style::default().fg(Color::DarkGray);
            buf.get_mut(box_x, y).set_symbol(left).set_style(frame_style);
            buf.get_mut(box_x + total_width - 1, y).set_symbol(right).set_style(frame_style);
        }

        let mut cursor_x = content_x + content_width as u16;
        for column_index in (0..column_count).rev() {
            let width = column_widths[column_index] as u16;
            let cell_start = cursor_x.saturating_sub(width);

            if let Some((text, style)) = row.get(column_index) {
                let available_width = cursor_x.saturating_sub(cell_start) as usize;
                let text_width = display_width(text).min(available_width) as u16;
                let text_x = cell_start + width.saturating_sub(text_width);
                buf.set_stringn(text_x, y, text, available_width, *style);
            }

            cursor_x = cell_start.saturating_sub(column_gap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_width_uses_terminal_cell_width() {
        assert_eq!(display_width("ABC"), 3);
        assert_eq!(display_width("↑"), 1);
        assert_eq!(display_width("节点"), 4);
    }

    #[test]
    fn test_braille_fill_symbol_progression() {
        assert_eq!(braille_fill_symbol(0), " ");
        assert_eq!(braille_fill_symbol(1), "⣀");
        assert_eq!(braille_fill_symbol(2), "⣤");
        assert_eq!(braille_fill_symbol(3), "⣶");
        assert_eq!(braille_fill_symbol(4), "⣿");
        assert_eq!(braille_fill_symbol(9), "⣿");
    }

    #[test]
    fn test_clear_area_replaces_symbols_and_style() {
        let area = Rect::new(0, 0, 3, 2);
        let mut buf = Buffer::empty(area);
        buf.set_string(0, 0, "XXX", Style::default().fg(Color::Red));
        let style = Style::default().fg(Color::Green);

        clear_area(&mut buf, area, style);

        assert_eq!(buf.get(0, 0).symbol(), " ");
        assert_eq!(buf.get(2, 1).symbol(), " ");
        assert_eq!(buf.get(0, 0).fg, Color::Green);
        assert_eq!(buf.get(2, 1).fg, Color::Green);
    }

    #[test]
    fn test_format_grouped_number_adds_grouping_separators() {
        assert_eq!(format_grouped_number(0), "0");
        assert_eq!(format_grouped_number(1_234), "1,234");
        assert_eq!(format_grouped_number(144_706_819), "144,706,819");
    }

    fn sample_standard_metric_values<'a>() -> StandardMetricValues<'a> {
        StandardMetricValues {
            trend: "↑",
            current_box_value: " 5.2s".to_string(),
            current_fallback_value: "   5.2s".to_string(),
            top_box_value: "8.1s".to_string(),
            top_fallback: "MAX    8.1s".to_string(),
            avg_trend: "→",
            avg_box_value: " 6.0s".to_string(),
            avg_fallback_value: "   6.0s".to_string(),
            block_box_value: "144,706,819".to_string(),
            block_fallback: "BLK  144,706,819".to_string(),
        }
    }

    fn sample_standard_metric_palette() -> StandardMetricPalette {
        StandardMetricPalette {
            trend_up: block::METRIC_POSITIVE,
            trend_down: block::ACCENT_ERROR,
            current_fallback: block::METRIC_PRIMARY,
            top_fallback: block::METRIC_PEAK,
            avg: block::METRIC_SECONDARY,
            block: block::CONTENT_HIGHLIGHT,
        }
    }

    #[test]
    fn test_segment_helpers_build_expected_shapes() {
        let value = single_segment("123", block::accent_style(block::ACCENT_WARN));
        let row = labeled_info_row("now:", value.clone());
        let grid = two_column_segment_grid(value.clone(), value.clone(), value.clone(), value);

        assert_eq!(styled_segment("x", Style::default()).0, "x");
        assert_eq!(row.0, "now:");
        assert_eq!(row.1.fg, Some(INFO_LABEL_COLOR));
        assert_eq!(row.2[0].0, "123");
        assert_eq!(grid.len(), 2);
        assert_eq!(grid[0].len(), 2);
        assert_eq!(grid[1].len(), 2);
    }

    #[test]
    fn test_standard_metric_rows_builds_box_and_fallback_layouts() {
        let values = sample_standard_metric_values();
        let palette = sample_standard_metric_palette();

        let (box_rows, fallback_rows) =
            standard_metric_rows(("CUR", "MAX", "AVG", "BLK"), &values, palette);

        assert_eq!(box_rows.len(), 4);
        assert_eq!(box_rows[0].0, "now:");
        assert_eq!(box_rows[0].2[0].0, "↑");
        assert_eq!(box_rows[0].2[1].0, " 5.2s");
        assert_eq!(box_rows[0].2[0].1.fg, Some(block::METRIC_POSITIVE));
        assert_eq!(box_rows[3].2[0].0, "144,706,819");
        assert_eq!(fallback_rows.len(), 2);
        assert_eq!(fallback_rows[0][0][0].0, "CUR ");
        assert_eq!(fallback_rows[0][1][0].0, "MAX    8.1s");
        assert_eq!(fallback_rows[1][0][0].0, "AVG ");
        assert_eq!(fallback_rows[1][1][0].0, "BLK  144,706,819");
    }

    #[test]
    fn test_limit_standard_metric_rows_reduces_metrics_by_priority() {
        let values = sample_standard_metric_values();
        let palette = sample_standard_metric_palette();

        let (box_rows, fallback_rows) =
            standard_metric_rows(("CUR", "MAX", "AVG", "BLK"), &values, palette);
        let (limited_box, limited_fallback) =
            limit_standard_metric_rows(&box_rows, &fallback_rows, 3);

        assert_eq!(limited_box.len(), 3);
        assert_eq!(limited_box[0].0, "now:");
        assert_eq!(limited_box[1].0, "top:");
        assert_eq!(limited_box[2].0, "avg:");
        assert_eq!(limited_fallback.len(), 2);
        assert_eq!(limited_fallback[0].len(), 2);
        assert_eq!(limited_fallback[1].len(), 1);
    }

    #[test]
    fn test_render_left_axis_labels_reserves_gutter() {
        let area = Rect::new(0, 0, 12, 4);
        let mut buf = Buffer::empty(area);
        let plot_area = render_left_axis_labels(
            &mut buf,
            area,
            "2.5s",
            "0",
            Style::default().fg(AXIS_LABEL_COLOR).bg(block::PANEL_BG),
        );

        assert_eq!(plot_area, Rect::new(5, 0, 7, 4));
        assert_eq!(buf.get(0, 0).symbol(), "2");
        assert_eq!(buf.get(0, 0).fg, AXIS_LABEL_COLOR);
        assert_eq!(buf.get(3, 0).symbol(), "s");
        assert_eq!(buf.get(3, 3).symbol(), "0");
    }

    #[test]
    fn test_render_left_axis_labels_skips_when_too_narrow() {
        let area = Rect::new(0, 0, 6, 4);
        let mut buf = Buffer::empty(area);
        let plot_area = render_left_axis_labels(&mut buf, area, "9999", "0", Style::default());

        assert_eq!(plot_area, area);
        assert_eq!(buf.get(0, 0).symbol(), " ");
    }

    #[test]
    fn test_render_bottom_band_dotted_plot_only_draws_bottom_band() {
        let area = Rect::new(0, 0, 3, 4);
        let mut buf = Buffer::empty(area);
        let data = vec![(0.0, 100.0)];

        render_bottom_band_dotted_plot(
            &mut buf,
            area,
            &data,
            100.0,
            2,
            Style::default().fg(Color::Magenta),
            Style::default().fg(Color::Cyan),
        );

        assert_eq!(buf.get(2, 3).fg, Color::Magenta);
        assert_eq!(buf.get(2, 2).fg, Color::Cyan);
        assert_eq!(buf.get(2, 1).symbol(), " ");
        assert_eq!(buf.get(2, 0).symbol(), " ");
    }

    #[test]
    fn test_render_right_aligned_labeled_box_aligns_labels_and_values() {
        let area = Rect::new(0, 0, 26, 8);
        let mut buf = Buffer::empty(area);
        let rows = vec![
            (
                "now:".to_string(),
                block::muted_style(),
                vec![("5.2s".to_string(), block::accent_style(block::METRIC_POSITIVE))],
            ),
            (
                "blk:".to_string(),
                block::muted_style(),
                vec![("123,456".to_string(), block::highlight_style())],
            ),
        ];

        let options = LabeledBoxOptions {
            start_y_offset: 1,
            title: "block time",
            title_style: block::header_style(),
            frame_style: Style::default().fg(INFO_FRAME_COLOR).bg(block::PANEL_BG),
            background_style: block::content_style(),
            column_gap: 2,
            right_inset: 1,
        };

        let rendered = render_right_aligned_labeled_box(&mut buf, area, &rows, &options);

        assert!(rendered);
        assert_eq!(buf.get(10, 1).symbol(), "╭");
        assert_eq!(buf.get(10, 1).fg, INFO_FRAME_COLOR);
        assert_eq!(buf.get(15, 1).symbol(), "o");
        assert_eq!(buf.get(15, 1).fg, block::PANEL_TITLE);
        assert_eq!(buf.get(11, 2).symbol(), "n");
        assert_eq!(buf.get(20, 2).symbol(), "5");
        assert_eq!(buf.get(17, 3).symbol(), "1");
        assert_eq!(buf.get(17, 3).fg, block::CONTENT_HIGHLIGHT);
        assert_eq!(buf.get(24, 4).symbol(), "╯");
    }

    #[test]
    fn test_trim_data_points_keeps_latest_values() {
        let mut data: Vec<(f64, f64)> = (0..5).map(|i| (i as f64, i as f64)).collect();
        trim_data_points(&mut data, 3);
        assert_eq!(data, vec![(2.0, 2.0), (3.0, 3.0), (4.0, 4.0)]);
    }

    #[test]
    fn test_append_u64_samples_increments_x_and_trims_to_max() {
        let mut data: Vec<(f64, f64)> =
            (0..MAX_DATA_POINTS).map(|i| (i as f64, i as f64)).collect();
        let mut update_count = MAX_DATA_POINTS as u64;

        append_u64_samples(&mut data, &mut update_count, vec![200, 201]);

        assert_eq!(update_count, 202);
        assert_eq!(data.len(), MAX_DATA_POINTS);
        assert_eq!(data[0], (2.0, 2.0));
        assert_eq!(data[MAX_DATA_POINTS - 1], (201.0, 201.0));
    }

    #[test]
    fn test_trend_style_uses_configured_colors() {
        assert_eq!(trend_style("↑", Color::Green, Color::Red).fg, Some(Color::Green));
        assert_eq!(trend_style("↓", Color::Green, Color::Red).fg, Some(Color::Red));
        assert_eq!(trend_style("→", Color::Green, Color::Red).fg, Some(Color::DarkGray));
    }

    #[test]
    fn test_average_recent_nonzero_rounded_ignores_placeholders() {
        let data = vec![(0.0, 0.0), (1.0, 10.0), (2.0, 20.0), (3.0, 30.0)];

        assert_eq!(average_recent_nonzero_rounded(&data, 2), 25);
        assert_eq!(average_recent_nonzero_rounded(&data, 10), 20);
        assert_eq!(average_recent_nonzero_rounded(&data, 0), 0);
    }

    #[test]
    fn test_recent_trend_symbol_detects_direction() {
        assert_eq!(recent_trend_symbol(&[(0.0, 0.0), (1.0, 100.0), (2.0, 200.0)]), "↑");
        assert_eq!(recent_trend_symbol(&[(0.0, 0.0), (1.0, 200.0), (2.0, 100.0)]), "↓");
        assert_eq!(recent_trend_symbol(&[(0.0, 0.0), (1.0, 100.0), (2.0, 100.0)]), "→");
        assert_eq!(recent_trend_symbol(&[(0.0, 0.0), (1.0, 100.0)]), "→");
    }

    #[test]
    fn test_recent_window_trend_symbol_detects_direction() {
        let rising = vec![(0.0, 10.0), (1.0, 20.0), (2.0, 30.0), (3.0, 40.0)];
        let falling = vec![(0.0, 40.0), (1.0, 30.0), (2.0, 20.0), (3.0, 10.0)];
        let flat = vec![(0.0, 10.0), (1.0, 20.0), (2.0, 10.0), (3.0, 20.0)];

        assert_eq!(recent_window_trend_symbol(&rising, 2), "↑");
        assert_eq!(recent_window_trend_symbol(&falling, 2), "↓");
        assert_eq!(recent_window_trend_symbol(&flat, 2), "→");
        assert_eq!(recent_window_trend_symbol(&[(0.0, 10.0), (1.0, 20.0)], 2), "→");
    }

    #[test]
    fn test_render_right_aligned_text_lines_uses_inner_right_edge() {
        let area = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::empty(area);
        let lines = vec![("ABC".to_string(), Style::default())];
        render_right_aligned_text_lines(&mut buf, area, 1, &lines);

        assert_eq!(buf.get(4, 1).symbol(), "[");
        assert_eq!(buf.get(5, 1).symbol(), "A");
        assert_eq!(buf.get(6, 1).symbol(), "B");
        assert_eq!(buf.get(7, 1).symbol(), "C");
        assert_eq!(buf.get(8, 1).symbol(), "]");
    }

    #[test]
    fn test_render_right_aligned_text_lines_clears_box_area() {
        let area = Rect::new(0, 0, 12, 5);
        let mut buf = Buffer::empty(area);
        buf.set_string(3, 1, "XXXXXX", Style::default());
        let lines =
            vec![("LONG".to_string(), Style::default()), ("S".to_string(), Style::default())];

        render_right_aligned_text_lines(&mut buf, area, 1, &lines);

        assert_eq!(buf.get(5, 1).symbol(), "┌");
        assert_eq!(buf.get(6, 1).symbol(), "L");
        assert_eq!(buf.get(9, 1).symbol(), "G");
        assert_eq!(buf.get(10, 1).symbol(), "┐");
        assert_eq!(buf.get(6, 2).symbol(), " ");
        assert_eq!(buf.get(9, 2).symbol(), "S");
        assert_eq!(buf.get(10, 2).symbol(), "┘");
    }

    #[test]
    fn test_render_right_aligned_text_grid_places_two_columns() {
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::empty(area);
        let rows = vec![
            vec![("A1".to_string(), Style::default()), ("B222".to_string(), Style::default())],
            vec![("A333".to_string(), Style::default()), ("B4".to_string(), Style::default())],
        ];

        render_right_aligned_text_grid(&mut buf, area, 1, &rows, 2);

        assert_eq!(buf.get(7, 1).symbol(), "┌");
        assert_eq!(buf.get(10, 1).symbol(), "A");
        assert_eq!(buf.get(11, 1).symbol(), "1");
        assert_eq!(buf.get(14, 1).symbol(), "B");
        assert_eq!(buf.get(17, 1).symbol(), "2");
        assert_eq!(buf.get(18, 1).symbol(), "┐");
        assert_eq!(buf.get(7, 2).symbol(), "└");
        assert_eq!(buf.get(8, 2).symbol(), "A");
        assert_eq!(buf.get(11, 2).symbol(), "3");
        assert_eq!(buf.get(16, 2).symbol(), "B");
        assert_eq!(buf.get(17, 2).symbol(), "4");
        assert_eq!(buf.get(18, 2).symbol(), "┘");
    }

    #[test]
    fn test_render_right_aligned_text_grid_falls_back_to_single_column_when_narrow() {
        let area = Rect::new(0, 0, 10, 6);
        let mut buf = Buffer::empty(area);
        let rows = vec![
            vec![("A1".to_string(), Style::default()), ("B222".to_string(), Style::default())],
            vec![("A333".to_string(), Style::default()), ("B4".to_string(), Style::default())],
        ];

        render_right_aligned_text_grid(&mut buf, area, 1, &rows, 2);

        assert_eq!(buf.get(3, 1).symbol(), "┌");
        assert_eq!(buf.get(6, 1).symbol(), "A");
        assert_eq!(buf.get(7, 1).symbol(), "1");
        assert_eq!(buf.get(8, 1).symbol(), "┐");
        assert_eq!(buf.get(3, 2).symbol(), "│");
        assert_eq!(buf.get(4, 2).symbol(), "B");
        assert_eq!(buf.get(7, 2).symbol(), "2");
        assert_eq!(buf.get(8, 2).symbol(), "│");
        assert_eq!(buf.get(3, 3).symbol(), "│");
        assert_eq!(buf.get(4, 3).symbol(), "A");
        assert_eq!(buf.get(7, 3).symbol(), "3");
        assert_eq!(buf.get(3, 4).symbol(), "└");
        assert_eq!(buf.get(6, 4).symbol(), "B");
        assert_eq!(buf.get(7, 4).symbol(), "4");
        assert_eq!(buf.get(8, 4).symbol(), "┘");
    }
}
