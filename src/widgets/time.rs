use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{
        Color,
        Modifier,
        Style,
    },
    symbols::Marker,
    text::Span,
    widgets::{
        Axis,
        Chart,
        Dataset,
        GraphType,
        Widget,
    },
};

use crate::{
    collect::SharedData,
    update::UpdatableWidget,
    widgets::{
        block,
        chart,
    },
};

const MIN_Y_AXIS_MAX_MS: f64 = 1000.0;
const AVERAGE_WINDOW_DATA_POINTS: usize = 10;
const Y_AXIS_STEPS_MS: [(f64, f64); 4] =
    [(5000.0, 500.0), (20000.0, 1000.0), (50000.0, 5000.0), (f64::MAX, 10000.0)];

pub struct TimeWidget {
    title: String,
    update_interval: Ratio<u64>,

    collect_data: SharedData,

    update_count: u64,
    cur_num: u64,
    cur_time: u64,
    max_time: u64,
    data: Vec<(f64, f64)>,
}

impl TimeWidget {
    pub fn new(
        update_interval: Ratio<u64>,
        collect_data: SharedData,
    ) -> TimeWidget {
        TimeWidget {
            title: " Block Time ".to_string(),
            update_interval,

            collect_data,
            update_count: 0,
            cur_num: 0,
            cur_time: 0,
            max_time: 0,
            data: vec![(0.0, 0.0)],
        }
    }
}

impl UpdatableWidget for TimeWidget {
    fn update(&mut self) {
        let mut collect_data = self.collect_data.lock().expect("mutex poisoned - recovering");
        self.cur_num = collect_data.cur_block_number();
        self.cur_time = collect_data.cur_interval();
        self.max_time = collect_data.max_interval();

        let data = collect_data.intervals_and_clear();
        for interval in data {
            self.data.push((self.update_count as f64, interval as f64));
            self.update_count += 1;
        }

        chart::trim_data_points(&mut self.data, chart::MAX_DATA_POINTS);
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

fn y_axis_upper_bound(data: &[(f64, f64)]) -> f64 {
    chart::y_axis_upper_bound(data, MIN_Y_AXIS_MAX_MS, &Y_AXIS_STEPS_MS)
}

fn format_block_time(ms: u64) -> String {
    if ms < 1000 {
        return format!("{ms}ms");
    }

    let seconds = ms as f64 / 1000.0;
    if seconds < 10.0 {
        format!("{seconds:.2}s")
    } else if seconds < 100.0 {
        format!("{seconds:.1}s")
    } else {
        format!("{seconds:.0}s")
    }
}

fn y_axis_labels(
    y_max: f64,
    area_width: u16,
) -> Vec<Span<'static>> {
    let label_count = if area_width < chart::NARROW_CHART_WIDTH {
        2
    } else {
        3
    };
    chart::y_axis_labels_with_count(y_max, format_block_time, label_count)
}

fn trend_style(trend: &str) -> Style {
    match trend {
        "↑" => Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD),
        "↓" => Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    }
}

fn info_labels(area_width: u16) -> (&'static str, &'static str, &'static str, &'static str) {
    if area_width < chart::ULTRA_NARROW_CHART_WIDTH {
        ("C", "M", "A", "B")
    } else {
        ("CUR", "MAX", "AVG", "BLK")
    }
}

fn format_block_number(value: u64) -> String {
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

fn average_recent_block_time(
    data: &[(f64, f64)],
    sample_count: usize,
) -> u64 {
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
        .take(sample_count)
        .collect();

    if recent_values.is_empty() {
        return 0;
    }

    let sum: u64 = recent_values.iter().sum();
    sum / recent_values.len() as u64
}

impl Widget for &TimeWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        buf.set_style(area, block::content_style());

        let dataset = Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Indexed(70)))
            .data(&self.data);
        let x_bounds = chart::visible_x_bounds(self.update_count, area.width);
        let y_max = y_axis_upper_bound(&self.data);
        let avg_time = average_recent_block_time(&self.data, AVERAGE_WINDOW_DATA_POINTS);
        let trend = chart::recent_trend_symbol(&self.data);
        let avg_trend = chart::recent_window_trend_symbol(&self.data, AVERAGE_WINDOW_DATA_POINTS);
        let (cur_label, max_label, avg_label, blk_label) = info_labels(area.width);

        Chart::new(vec![dataset])
            .block(block::new(&self.title))
            .x_axis(Axis::default().bounds(x_bounds))
            .y_axis(Axis::default().bounds([0.0, y_max]).labels(y_axis_labels(y_max, area.width)))
            .render(area, buf);

        let info_rows = vec![
            vec![
                vec![
                    (
                        format!("{cur_label} "),
                        Style::default().fg(Color::Indexed(70)).add_modifier(Modifier::BOLD),
                    ),
                    (trend.to_string(), trend_style(trend)),
                    (
                        format!(" {:>7}", format_block_time(self.cur_time)),
                        Style::default().fg(Color::Indexed(70)).add_modifier(Modifier::BOLD),
                    ),
                ],
                vec![(
                    format!("{max_label} {:>7}", format_block_time(self.max_time)),
                    Style::default().fg(Color::Indexed(145)),
                )],
            ],
            vec![
                vec![
                    (format!("{avg_label} "), Style::default().fg(Color::Indexed(109))),
                    (avg_trend.to_string(), trend_style(avg_trend)),
                    (
                        format!(" {:>7}", format_block_time(avg_time)),
                        Style::default().fg(Color::Indexed(109)),
                    ),
                ],
                vec![(
                    format!("{blk_label} {:>12}", format_block_number(self.cur_num)),
                    Style::default().fg(Color::DarkGray),
                )],
            ],
        ];

        chart::render_right_aligned_segment_grid(buf, area, 1, &info_rows, 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_data_points_constant() {
        assert_eq!(chart::MAX_DATA_POINTS, 200);
    }

    #[test]
    fn test_data_truncation_at_boundary() {
        assert_eq!(chart::MAX_DATA_POINTS, 200);

        let mut data: Vec<(f64, f64)> = (0..200).map(|i| (i as f64, i as f64)).collect();
        assert_eq!(data.len(), chart::MAX_DATA_POINTS);

        data.push((200.0, 200.0));
        assert_eq!(data.len(), chart::MAX_DATA_POINTS + 1);

        chart::trim_data_points(&mut data, chart::MAX_DATA_POINTS);

        assert_eq!(data.len(), chart::MAX_DATA_POINTS);
        assert_eq!(data[0], (1.0, 1.0));
        assert_eq!(data[199], (200.0, 200.0));
    }

    #[test]
    fn test_data_truncation_multiple_excess() {
        let mut data: Vec<(f64, f64)> = (0..250).map(|i| (i as f64, i as f64)).collect();

        chart::trim_data_points(&mut data, chart::MAX_DATA_POINTS);

        assert_eq!(data.len(), chart::MAX_DATA_POINTS);
        assert_eq!(data[0], (50.0, 50.0));
        assert_eq!(data[199], (249.0, 249.0));
    }

    #[test]
    fn test_data_no_truncation_when_below_limit() {
        let mut data: Vec<(f64, f64)> = (0..100).map(|i| (i as f64, i as f64)).collect();

        chart::trim_data_points(&mut data, chart::MAX_DATA_POINTS);

        assert_eq!(data.len(), 100);
        assert_eq!(data[0], (0.0, 0.0));
        assert_eq!(data[99], (99.0, 99.0));
    }

    #[test]
    fn test_y_axis_upper_bound_has_minimum() {
        assert_eq!(y_axis_upper_bound(&[]), MIN_Y_AXIS_MAX_MS);
        assert_eq!(y_axis_upper_bound(&[(0.0, 800.0)]), MIN_Y_AXIS_MAX_MS);
    }

    #[test]
    fn test_y_axis_upper_bound_rounds_up_with_headroom() {
        assert_eq!(y_axis_upper_bound(&[(0.0, 1200.0)]), 1500.0);
        assert_eq!(y_axis_upper_bound(&[(0.0, 6100.0)]), 7000.0);
        assert_eq!(y_axis_upper_bound(&[(0.0, 21000.0)]), 25000.0);
    }

    #[test]
    fn test_format_block_time_uses_ms_and_seconds() {
        assert_eq!(format_block_time(950), "950ms");
        assert_eq!(format_block_time(1200), "1.20s");
        assert_eq!(format_block_time(12500), "12.5s");
        assert_eq!(format_block_time(120000), "120s");
    }

    #[test]
    fn test_y_axis_labels_match_bounds() {
        let labels = y_axis_labels(2500.0, chart::NARROW_CHART_WIDTH);
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0].content, "0");
        assert_eq!(labels[1].content, "1.25s");
        assert_eq!(labels[2].content, "2.50s");
    }

    #[test]
    fn test_y_axis_labels_reduce_for_narrow_area() {
        let labels = y_axis_labels(2500.0, chart::NARROW_CHART_WIDTH - 1);
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].content, "0");
        assert_eq!(labels[1].content, "2.50s");
    }

    #[test]
    fn test_trend_style_maps_symbols() {
        assert_eq!(trend_style("↑").fg, Some(Color::LightRed));
        assert_eq!(trend_style("↓").fg, Some(Color::LightGreen));
        assert_eq!(trend_style("→").fg, Some(Color::DarkGray));
    }

    #[test]
    fn test_info_labels_shorten_on_ultra_narrow_area() {
        assert_eq!(info_labels(chart::ULTRA_NARROW_CHART_WIDTH - 1), ("C", "M", "A", "B"));
        assert_eq!(info_labels(chart::ULTRA_NARROW_CHART_WIDTH), ("CUR", "MAX", "AVG", "BLK"));
    }

    #[test]
    fn test_average_recent_block_time_ignores_placeholder_zero() {
        let data = vec![(0.0, 0.0), (1.0, 1000.0), (2.0, 2000.0), (3.0, 3000.0)];
        assert_eq!(average_recent_block_time(&data, 10), 2000);
    }

    #[test]
    fn test_average_recent_block_time_uses_recent_window() {
        let data = vec![(0.0, 1000.0), (1.0, 2000.0), (2.0, 3000.0), (3.0, 7000.0)];
        assert_eq!(average_recent_block_time(&data, 2), 5000);
    }

    #[test]
    fn test_format_block_number_adds_grouping_separators() {
        assert_eq!(format_block_number(144706819), "144,706,819");
    }
}
