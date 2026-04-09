use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::Widget,
};

use crate::{
    collect::SharedData,
    sync::lock_or_panic,
    update::UpdatableWidget,
    widgets::{
        block,
        chart,
    },
};

const OUTER_TITLE: &str = " Block Transactions ";
const BOX_TITLE: &str = "txs";
const MIN_Y_AXIS_MAX_TXS: f64 = 10.0;
const AVERAGE_WINDOW_DATA_POINTS: usize = 10;
const METRIC_PALETTE: chart::StandardMetricPalette = chart::StandardMetricPalette {
    trend_up: block::METRIC_POSITIVE,
    trend_down: block::ACCENT_ERROR,
    current_fallback: block::METRIC_TERTIARY,
    top_fallback: block::METRIC_PEAK,
    avg: block::METRIC_POSITIVE,
    block: block::CONTENT_HIGHLIGHT,
};
const Y_AXIS_STEPS_TXS: [(f64, f64); 6] = [
    (100.0, 10.0),
    (500.0, 50.0),
    (1000.0, 100.0),
    (5000.0, 500.0),
    (20000.0, 1000.0),
    (f64::MAX, 5000.0),
];

pub struct TxsWidget {
    update_interval: Ratio<u64>,

    collect_data: SharedData,

    update_count: u64,
    cur_num: u64,
    cur_txs: u64,
    max: u64,
    max_block_number: u64,
    data: Vec<(f64, f64)>,
}

impl TxsWidget {
    pub fn new(
        update_interval: Ratio<u64>,
        collect_data: SharedData,
    ) -> TxsWidget {
        TxsWidget {
            update_interval,

            collect_data,

            update_count: 0,
            cur_num: 0,
            cur_txs: 0,
            max: 0,
            max_block_number: 0,
            data: vec![(0.0, 0.0)],
        }
    }

    fn metric_rows(
        &self,
        labels: (&str, &str, &str, &str),
        trend: &str,
        avg_trend: &str,
        avg_txs: u64,
        area_width: u16,
    ) -> (Vec<chart::LabeledBoxRow>, Vec<chart::SegmentGridRow>) {
        let (_cur_label, max_label, _avg_label, blk_label) = labels;
        let cur_txs = format_tx_count(self.cur_txs);
        let avg_txs = format_tx_count(avg_txs);
        let cur_block = chart::format_grouped_number(self.cur_num);

        let values = chart::StandardMetricValues {
            trend,
            current_box_value: format!(" {cur_txs}"),
            current_fallback_value: format!(" {:>7}", cur_txs),
            top_box_value: format_max_txs_box(self.max, self.max_block_number, area_width),
            top_fallback: format_max_txs(self.max, self.max_block_number, area_width, max_label),
            avg_trend,
            avg_box_value: format!(" {avg_txs}"),
            avg_fallback_value: format!(" {:>7}", avg_txs),
            block_box_value: cur_block.clone(),
            block_fallback: format!("{blk_label} {:>12}", cur_block),
        };

        chart::standard_metric_rows(labels, &values, METRIC_PALETTE)
    }
}

impl UpdatableWidget for TxsWidget {
    fn update(&mut self) {
        let mut collect_data = lock_or_panic(&self.collect_data);
        self.cur_num = collect_data.cur_block_number();
        self.cur_txs = collect_data.cur_txs();
        self.max = collect_data.max_txs();
        self.max_block_number = collect_data.max_txs_block_number();
        let data = collect_data.txns_and_clear();

        chart::append_u64_samples(&mut self.data, &mut self.update_count, data);
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

fn format_tx_count(value: u64) -> String {
    if value < 1000 {
        return value.to_string();
    }

    if value < 10000 {
        return format!("{:.1}k", value as f64 / 1000.0);
    }

    if value < 1_000_000 {
        return format!("{}k", value / 1000);
    }

    format!("{:.1}m", value as f64 / 1_000_000.0)
}

fn format_max_txs(
    max_txs: u64,
    max_block_number: u64,
    area_width: u16,
    max_label: &str,
) -> String {
    if area_width < chart::NARROW_CHART_WIDTH {
        return format!("{max_label} {:>7}", format_tx_count(max_txs));
    }

    format!("{max_label} {:>7} #{max_block_number}", format_tx_count(max_txs))
}

fn format_max_txs_box(
    max_txs: u64,
    max_block_number: u64,
    area_width: u16,
) -> String {
    if area_width < chart::NARROW_CHART_WIDTH {
        return format_tx_count(max_txs);
    }

    format!("{} #{}", format_tx_count(max_txs), chart::format_grouped_number(max_block_number))
}

impl Widget for &TxsWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let y_max = chart::y_axis_upper_bound(&self.data, MIN_Y_AXIS_MAX_TXS, &Y_AXIS_STEPS_TXS);
        let trend = chart::recent_trend_symbol(&self.data);
        let avg_txs = chart::average_recent_nonzero_rounded(&self.data, AVERAGE_WINDOW_DATA_POINTS);
        let avg_trend = chart::recent_window_trend_symbol(&self.data, AVERAGE_WINDOW_DATA_POINTS);
        let labels = chart::info_labels(area.width);
        let top_label = format_tx_count(y_max.round() as u64);

        let (section_rows, info_rows) =
            self.metric_rows(labels, trend, avg_trend, avg_txs, area.width);
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

        chart::render_metric_panel(buf, area, &self.data, &panel);
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
    fn test_txs_widget_title_constants() {
        assert_eq!(OUTER_TITLE, " Block Transactions ");
        assert_eq!(BOX_TITLE, "txs");
    }

    #[test]
    fn test_txs_widget_update_interval() {
        let shared_data = create_shared_data();
        let interval = Ratio::from_integer(5);
        let widget = TxsWidget::new(interval, shared_data);
        assert_eq!(widget.get_update_interval(), Ratio::from_integer(5));
    }

    #[test]
    fn test_txs_widget_initial_state() {
        let shared_data = create_shared_data();
        let interval = Ratio::from_integer(1);
        let widget = TxsWidget::new(interval, shared_data);
        assert_eq!(widget.update_count, 0);
        assert_eq!(widget.cur_num, 0);
        assert_eq!(widget.cur_txs, 0);
        assert_eq!(widget.max, 0);
        assert_eq!(widget.max_block_number, 0);
        assert_eq!(widget.data, vec![(0.0, 0.0)]);
    }

    #[test]
    fn test_txs_widget_update_with_empty_data() {
        let shared_data = create_shared_data();
        let interval = Ratio::from_integer(1);
        let mut widget = TxsWidget::new(interval, shared_data);
        widget.update();
        assert_eq!(widget.cur_num, 0);
        assert_eq!(widget.cur_txs, 0);
    }

    #[test]
    fn test_y_axis_upper_bound_has_minimum() {
        assert_eq!(
            chart::y_axis_upper_bound(&[], MIN_Y_AXIS_MAX_TXS, &Y_AXIS_STEPS_TXS),
            MIN_Y_AXIS_MAX_TXS
        );
        assert_eq!(
            chart::y_axis_upper_bound(&[(0.0, 5.0)], MIN_Y_AXIS_MAX_TXS, &Y_AXIS_STEPS_TXS),
            MIN_Y_AXIS_MAX_TXS,
        );
    }

    #[test]
    fn test_y_axis_upper_bound_rounds_up_with_headroom() {
        assert_eq!(
            chart::y_axis_upper_bound(&[(0.0, 1200.0)], MIN_Y_AXIS_MAX_TXS, &Y_AXIS_STEPS_TXS),
            1500.0
        );
        assert_eq!(
            chart::y_axis_upper_bound(&[(0.0, 6100.0)], MIN_Y_AXIS_MAX_TXS, &Y_AXIS_STEPS_TXS),
            7000.0
        );
        assert_eq!(
            chart::y_axis_upper_bound(&[(0.0, 21000.0)], MIN_Y_AXIS_MAX_TXS, &Y_AXIS_STEPS_TXS),
            25000.0
        );
    }

    #[test]
    fn test_format_tx_count_uses_compact_units() {
        assert_eq!(format_tx_count(950), "950");
        assert_eq!(format_tx_count(1200), "1.2k");
        assert_eq!(format_tx_count(12500), "12k");
        assert_eq!(format_tx_count(2_500_000), "2.5m");
    }

    #[test]
    fn test_trend_style_maps_symbols() {
        assert_eq!(METRIC_PALETTE.trend_style("↑").fg, Some(block::METRIC_POSITIVE));
        assert_eq!(METRIC_PALETTE.trend_style("↓").fg, Some(block::ACCENT_ERROR));
        assert_eq!(METRIC_PALETTE.trend_style("→").fg, Some(ratatui::style::Color::DarkGray));
        assert_eq!(METRIC_PALETTE.current_fallback, block::METRIC_TERTIARY);
        assert_eq!(METRIC_PALETTE.top_fallback, block::METRIC_PEAK);
        assert_eq!(METRIC_PALETTE.avg, block::METRIC_POSITIVE);
        assert_eq!(METRIC_PALETTE.block, block::CONTENT_HIGHLIGHT);
    }

    #[test]
    fn test_average_recent_txs_ignores_placeholder_zero() {
        let data = vec![(0.0, 0.0), (1.0, 10.0), (2.0, 20.0), (3.0, 30.0)];
        assert_eq!(chart::average_recent_nonzero_rounded(&data, 10), 20);
    }

    #[test]
    fn test_average_recent_txs_uses_recent_window() {
        let data = vec![(0.0, 10.0), (1.0, 20.0), (2.0, 30.0), (3.0, 70.0)];
        assert_eq!(chart::average_recent_nonzero_rounded(&data, 2), 50);
    }

    #[test]
    fn test_format_max_txs_hides_block_number_on_narrow_area() {
        assert_eq!(
            format_max_txs(12_500, 12345, chart::NARROW_CHART_WIDTH - 1, "MAX"),
            "MAX     12k"
        );
        assert_eq!(
            format_max_txs(12_500, 12345, chart::NARROW_CHART_WIDTH, "MAX"),
            "MAX     12k #12345"
        );
    }
}
