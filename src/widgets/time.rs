use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{
        Color,
        Style,
    },
    widgets::Widget,
};

use crate::{
    collect::SharedData,
    update::UpdatableWidget,
    widgets::{
        block,
        chart,
    },
};

const OUTER_TITLE: &str = " Block Time ";
const BOX_TITLE: &str = "time";
const MIN_Y_AXIS_MAX_MS: f64 = 1000.0;
const AVERAGE_WINDOW_DATA_POINTS: usize = 10;
const TIME_PLOT_FILL_COLOR: Color = Color::Rgb(194, 88, 188);
const TIME_PLOT_CREST_COLOR: Color = Color::Rgb(224, 210, 248);
const METRIC_PALETTE: chart::StandardMetricPalette = chart::StandardMetricPalette {
    trend_up: Color::LightRed,
    trend_down: Color::LightGreen,
    current_fallback: Color::Indexed(70),
    top_fallback: Color::Indexed(145),
    avg: Color::Indexed(109),
    block: Color::DarkGray,
};
const Y_AXIS_STEPS_MS: [(f64, f64); 4] =
    [(5000.0, 500.0), (20000.0, 1000.0), (50000.0, 5000.0), (f64::MAX, 10000.0)];

pub struct TimeWidget {
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
            update_interval,

            collect_data,
            update_count: 0,
            cur_num: 0,
            cur_time: 0,
            max_time: 0,
            data: vec![(0.0, 0.0)],
        }
    }

    fn metric_rows(
        &self,
        labels: (&str, &str, &str, &str),
        trend: &str,
        avg_trend: &str,
        avg_time: u64,
    ) -> (Vec<chart::LabeledBoxRow>, Vec<chart::SegmentGridRow>) {
        let (_cur_label, max_label, _avg_label, blk_label) = labels;
        let max_time = format_block_time(self.max_time);
        let avg_time = format_block_time(avg_time);
        let cur_time = format_block_time(self.cur_time);
        let cur_block = chart::format_grouped_number(self.cur_num);

        let values = chart::StandardMetricValues {
            trend,
            current_box_value: format!(" {cur_time}"),
            current_fallback_value: format!(" {:>7}", cur_time),
            top_box_value: max_time.clone(),
            top_fallback: format!("{max_label} {:>7}", max_time),
            avg_trend,
            avg_box_value: format!(" {avg_time}"),
            avg_fallback_value: format!(" {:>7}", avg_time),
            block_box_value: cur_block.clone(),
            block_fallback: format!("{blk_label} {:>12}", cur_block),
        };

        chart::standard_metric_rows(labels, &values, METRIC_PALETTE)
    }
}

impl UpdatableWidget for TimeWidget {
    fn update(&mut self) {
        let mut collect_data = self.collect_data.lock().expect("mutex poisoned - recovering");
        self.cur_num = collect_data.cur_block_number();
        self.cur_time = collect_data.cur_interval();
        self.max_time = collect_data.max_interval();

        let data = collect_data.intervals_and_clear();
        chart::append_u64_samples(&mut self.data, &mut self.update_count, data);
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
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

impl Widget for &TimeWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let y_max = chart::y_axis_upper_bound(&self.data, MIN_Y_AXIS_MAX_MS, &Y_AXIS_STEPS_MS);
        let avg_time =
            chart::average_recent_nonzero_rounded(&self.data, AVERAGE_WINDOW_DATA_POINTS);
        let trend = chart::recent_trend_symbol(&self.data);
        let avg_trend = chart::recent_window_trend_symbol(&self.data, AVERAGE_WINDOW_DATA_POINTS);
        let labels = chart::info_labels(area.width);
        let top_label = format_block_time(y_max.round() as u64);

        let (section_rows, info_rows) = self.metric_rows(labels, trend, avg_trend, avg_time);
        let max_metrics = area.height.saturating_sub(6).clamp(1, 4) as usize;
        let (section_rows, info_rows) =
            chart::limit_standard_metric_rows(&section_rows, &info_rows, max_metrics);
        let mut panel = chart::default_metric_panel(
            OUTER_TITLE,
            BOX_TITLE,
            y_max,
            &top_label,
            &section_rows,
            &info_rows,
        );
        panel.box_options.start_y_offset = 1;
        panel.band_rows = Some(chart::lighter_band_rows(area.height.saturating_sub(2)));
        panel.plot_fill_style = Style::default().fg(TIME_PLOT_FILL_COLOR).bg(block::PANEL_BG);
        panel.plot_crest_style = Style::default().fg(TIME_PLOT_CREST_COLOR).bg(block::PANEL_BG);

        chart::render_metric_panel(buf, area, &self.data, &panel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_widget_title_constants() {
        assert_eq!(OUTER_TITLE, " Block Time ");
        assert_eq!(BOX_TITLE, "time");
    }

    #[test]
    fn test_y_axis_upper_bound_has_minimum() {
        assert_eq!(
            chart::y_axis_upper_bound(&[], MIN_Y_AXIS_MAX_MS, &Y_AXIS_STEPS_MS),
            MIN_Y_AXIS_MAX_MS
        );
        assert_eq!(
            chart::y_axis_upper_bound(&[(0.0, 800.0)], MIN_Y_AXIS_MAX_MS, &Y_AXIS_STEPS_MS),
            MIN_Y_AXIS_MAX_MS,
        );
    }

    #[test]
    fn test_y_axis_upper_bound_rounds_up_with_headroom() {
        assert_eq!(
            chart::y_axis_upper_bound(&[(0.0, 1200.0)], MIN_Y_AXIS_MAX_MS, &Y_AXIS_STEPS_MS),
            1500.0
        );
        assert_eq!(
            chart::y_axis_upper_bound(&[(0.0, 6100.0)], MIN_Y_AXIS_MAX_MS, &Y_AXIS_STEPS_MS),
            7000.0
        );
        assert_eq!(
            chart::y_axis_upper_bound(&[(0.0, 21000.0)], MIN_Y_AXIS_MAX_MS, &Y_AXIS_STEPS_MS),
            25000.0
        );
    }

    #[test]
    fn test_format_block_time_uses_ms_and_seconds() {
        assert_eq!(format_block_time(950), "950ms");
        assert_eq!(format_block_time(1200), "1.20s");
        assert_eq!(format_block_time(12500), "12.5s");
        assert_eq!(format_block_time(120000), "120s");
    }

    #[test]
    fn test_trend_style_maps_symbols() {
        assert_eq!(METRIC_PALETTE.trend_style("↑").fg, Some(Color::LightRed));
        assert_eq!(METRIC_PALETTE.trend_style("↓").fg, Some(Color::LightGreen));
        assert_eq!(METRIC_PALETTE.trend_style("→").fg, Some(Color::DarkGray));
    }

    #[test]
    fn test_average_recent_block_time_ignores_placeholder_zero() {
        let data = vec![(0.0, 0.0), (1.0, 1000.0), (2.0, 2000.0), (3.0, 3000.0)];
        assert_eq!(chart::average_recent_nonzero_rounded(&data, 10), 2000);
    }

    #[test]
    fn test_average_recent_block_time_uses_recent_window() {
        let data = vec![(0.0, 1000.0), (1.0, 2000.0), (2.0, 3000.0), (3.0, 7000.0)];
        assert_eq!(chart::average_recent_nonzero_rounded(&data, 2), 5000);
    }
}
