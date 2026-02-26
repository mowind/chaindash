use num_rational::Ratio;
use tui::{
    buffer::Buffer,
    layout::Rect,
    style::{
        Color,
        Style,
    },
    symbols::Marker,
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
    widgets::block,
};

/// Maximum number of data points to keep in memory.
/// Older data points are discarded to prevent unbounded memory growth.
const MAX_DATA_POINTS: usize = 200;

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
        let time_widget = TimeWidget {
            title: " Block Time(ms) ".to_string(),
            update_interval,

            collect_data,
            update_count: 0,
            cur_num: 0,
            cur_time: 0,
            max_time: 0,
            data: vec![(0.0, 0.0)],
        };

        time_widget
    }
}

impl UpdatableWidget for TimeWidget {
    fn update(&mut self) {
        let mut collect_data = self.collect_data.lock().unwrap();
        self.cur_num = collect_data.cur_block_number();
        self.cur_time = collect_data.cur_interval();
        self.max_time = collect_data.max_interval();

        let data = collect_data.intervals_and_clear();
        for interval in data {
            self.data.push((self.update_count as f64, interval as f64));
            self.update_count += 1;
        }

        if self.data.len() > MAX_DATA_POINTS {
            self.data.drain(0..self.data.len() - MAX_DATA_POINTS);
        }
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &TimeWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let mut dataset = Vec::new();
        dataset.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Indexed(70)))
                .data(&self.data),
        );

        Chart::<String, String>::default()
            .block(block::new(&self.title))
            .x_axis(
                Axis::default()
                    .bounds([self.update_count as f64 - 25.0, self.update_count as f64 + 1.0]),
            )
            .y_axis(Axis::default().bounds([0.0, 20000.0]))
            .datasets(&dataset)
            .render(area, buf);

        buf.set_string(
            area.x + 2,
            area.y + 1,
            format!("CUR   {}", self.cur_time),
            Style::default().fg(Color::Indexed(70)),
        );

        buf.set_string(
            area.x + 2,
            area.y + 2,
            format!("MAX   {}", self.max_time),
            Style::default().fg(Color::Indexed(141)),
        );

        buf.set_string(
            area.x + 2,
            area.y + 3,
            format!("BLOCK {}", self.cur_num),
            Style::default().fg(Color::Indexed(208)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_data_points_constant() {
        assert_eq!(MAX_DATA_POINTS, 200);
    }

    #[test]
    fn test_data_truncation_at_boundary() {
        assert_eq!(MAX_DATA_POINTS, 200);

        let mut data: Vec<(f64, f64)> = (0..200).map(|i| (i as f64, i as f64)).collect();
        assert_eq!(data.len(), MAX_DATA_POINTS);

        data.push((200.0, 200.0));
        assert_eq!(data.len(), MAX_DATA_POINTS + 1);

        if data.len() > MAX_DATA_POINTS {
            data.drain(0..data.len() - MAX_DATA_POINTS);
        }

        assert_eq!(data.len(), MAX_DATA_POINTS);
        assert_eq!(data[0], (1.0, 1.0));
        assert_eq!(data[199], (200.0, 200.0));
    }

    #[test]
    fn test_data_truncation_multiple_excess() {
        let mut data: Vec<(f64, f64)> = (0..250).map(|i| (i as f64, i as f64)).collect();

        if data.len() > MAX_DATA_POINTS {
            data.drain(0..data.len() - MAX_DATA_POINTS);
        }

        assert_eq!(data.len(), MAX_DATA_POINTS);
        assert_eq!(data[0], (50.0, 50.0));
        assert_eq!(data[199], (249.0, 249.0));
    }

    #[test]
    fn test_data_no_truncation_when_below_limit() {
        let mut data: Vec<(f64, f64)> = (0..100).map(|i| (i as f64, i as f64)).collect();

        if data.len() > MAX_DATA_POINTS {
            data.drain(0..data.len() - MAX_DATA_POINTS);
        }

        assert_eq!(data.len(), 100);
        assert_eq!(data[0], (0.0, 0.0));
        assert_eq!(data[99], (99.0, 99.0));
    }
}
