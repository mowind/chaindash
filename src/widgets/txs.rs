use num_rational::Ratio;
use tui::buffer::Buffer;
use tui::layout::Rect;
use tui::style::{Color, Style};
use tui::symbols::Marker;
use tui::widgets::{Axis, Chart, Dataset, GraphType, Widget};

use crate::collect::{Data, SharedData};
use crate::update::UpdatableWidget;
use crate::widgets::block;

pub struct TxsWidget {
    title: String,
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
    pub fn new(update_interval: Ratio<u64>, collect_data: SharedData) -> TxsWidget {
        let update_count = 0;

        let mut txs_widgets = TxsWidget {
            title: " Block Transactions ".to_string(),
            update_interval,

            collect_data,

            update_count,
            cur_num: 0,
            cur_txs: 0,
            max: 0,
            max_block_number: 0,
            data: vec![(0.0, 0.0)],
        };

        txs_widgets
    }
}

impl UpdatableWidget for TxsWidget {
    fn update(&mut self) {
        let mut collect_data = self.collect_data.lock().unwrap();
        self.cur_num = collect_data.cur_block_number();
        self.cur_txs = collect_data.cur_txs();
        self.max = collect_data.max_txs();
        self.max_block_number = collect_data.max_txs_block_number();
        let data = collect_data.txns_and_clear();

        for txs in data {
            self.data.push((self.update_count as f64, txs as f64));
            self.update_count += 1;
        }
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &TxsWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut datasets = Vec::new();
        datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Indexed(81)))
                .data(&self.data),
        );

        Chart::<String, String>::default()
            .block(block::new(&self.title))
            .x_axis(Axis::default().bounds([
                self.update_count as f64 - 25.0,
                self.update_count as f64 + 1.0,
            ]))
            .y_axis(Axis::default().bounds([0.0, 50000.0]))
            .datasets(&datasets)
            .render(area, buf);

        buf.set_string(
            area.x + 2,
            area.y + 1,
            format!("CUR   {}", self.cur_txs),
            Style::default().fg(Color::Indexed(81 as u8)),
        );

        buf.set_string(
            area.x + 2,
            area.y + 2,
            format!("MAX   {}({})", self.max, self.max_block_number),
            Style::default().fg(Color::Indexed(141 as u8)),
        );

        buf.set_string(
            area.x + 2,
            area.y + 3,
            format!("BLOCK {}", self.cur_num),
            Style::default().fg(Color::Indexed(208 as u8)),
        );
    }
}
