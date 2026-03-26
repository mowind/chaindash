use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{
        Color,
        Style,
    },
    text::Span,
};
use unicode_width::UnicodeWidthStr;

pub const MAX_DATA_POINTS: usize = 200;
pub const MIN_VISIBLE_DATA_POINTS: u64 = 25;
pub const MAX_VISIBLE_DATA_POINTS: u64 = 120;
pub const NARROW_CHART_WIDTH: u16 = 40;
pub const ULTRA_NARROW_CHART_WIDTH: u16 = 32;
const INLINE_FRAME_WIDTH: u16 = 2;

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn trim_data_points(
    data: &mut Vec<(f64, f64)>,
    max_data_points: usize,
) {
    if data.len() > max_data_points {
        data.drain(0..data.len() - max_data_points);
    }
}

pub fn visible_data_points(area_width: u16) -> u64 {
    u64::from(area_width.saturating_sub(2)).clamp(MIN_VISIBLE_DATA_POINTS, MAX_VISIBLE_DATA_POINTS)
}

pub fn visible_x_bounds(
    update_count: u64,
    area_width: u16,
) -> [f64; 2] {
    let visible_data_points = visible_data_points(area_width);
    [update_count.saturating_sub(visible_data_points) as f64, update_count as f64 + 1.0]
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

pub fn y_axis_labels_with_count<F>(
    y_max: f64,
    formatter: F,
    label_count: usize,
) -> Vec<Span<'static>>
where
    F: Fn(u64) -> String,
{
    match label_count {
        0 => Vec::new(),
        1 => vec![Span::raw(formatter(y_max.round() as u64))],
        2 => vec![Span::raw("0"), Span::raw(formatter(y_max.round() as u64))],
        _ => vec![
            Span::raw("0"),
            Span::raw(formatter((y_max / 2.0).round() as u64)),
            Span::raw(formatter(y_max.round() as u64)),
        ],
    }
}

pub fn recent_trend_symbol(data: &[(f64, f64)]) -> &'static str {
    let recent_values: Vec<u64> = data
        .iter()
        .rev()
        .filter_map(|(_, value)| {
            if *value > 0.0 {
                Some(value.round() as u64)
            } else {
                None
            }
        })
        .take(2)
        .collect();

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
    let values: Vec<u64> = data
        .iter()
        .filter_map(|(_, value)| {
            if *value > 0.0 {
                Some(value.round() as u64)
            } else {
                None
            }
        })
        .collect();

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
    rows: &[Vec<Vec<(String, Style)>>],
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
    fn test_trim_data_points_keeps_latest_values() {
        let mut data: Vec<(f64, f64)> = (0..5).map(|i| (i as f64, i as f64)).collect();
        trim_data_points(&mut data, 3);
        assert_eq!(data, vec![(2.0, 2.0), (3.0, 3.0), (4.0, 4.0)]);
    }

    #[test]
    fn test_visible_data_points_scales_with_width() {
        assert_eq!(visible_data_points(10), MIN_VISIBLE_DATA_POINTS);
        assert_eq!(visible_data_points(60), 58);
        assert_eq!(visible_data_points(200), MAX_VISIBLE_DATA_POINTS);
    }

    #[test]
    fn test_visible_x_bounds_avoids_negative_start() {
        assert_eq!(visible_x_bounds(3, 40), [0.0, 4.0]);
        assert_eq!(visible_x_bounds(30, 27), [5.0, 31.0]);
    }

    #[test]
    fn test_y_axis_labels_with_count_supports_two_labels() {
        let labels = y_axis_labels_with_count(2500.0, |value| value.to_string(), 2);
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].content, "0");
        assert_eq!(labels[1].content, "2500");
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
