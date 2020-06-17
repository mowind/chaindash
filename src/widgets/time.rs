use num_rational::Ratio;
use tui::buffer::Buffer;
use tui::layout::Rect;
use tui::style::Style;
use tui::symbols::Marker;
use tui::widgets::{Axis, Chart, Dataset, GraphType, Widget};
use web3::futures::Future;
use web3::transports::{EventLoopHandle, Http};
use web3::types::{Block, BlockId, H256, U64};

use crate::update::UpdatableWidget;
use crate::widgets::block;

pub struct TimeWidget {
    title: String,
    update_interval: Ratio<u64>,

    eloop: EventLoopHandle,
    web3: web3::Web3<Http>,

    update_count: u64,
    cur_num: u64,
    prev_timestamp: u64,
    cur_time: u64,
    max_time: u64,
    data: Vec<(f64, f64)>,
}

impl TimeWidget {
    pub fn new(update_interval: Ratio<u64>, url: &str) -> TimeWidget {
        let update_count = 0;

        let (eloop, transport) = web3::transports::Http::new(url).unwrap();
        let web3 = web3::Web3::new(transport);

        let mut time_widget = TimeWidget {
            title: " Block Time(ms) ".to_string(),
            update_interval,

            eloop,
            web3,

            update_count,
            cur_num: 0,
            prev_timestamp: 0,
            cur_time: 0,
            max_time: 0,
            data: vec![(0.0, 0.0)],
        };

        time_widget
    }
}

impl UpdatableWidget for TimeWidget {
    fn update(&mut self) {
        self.update_count += 1;
        let platon = self.web3.platon();

        if self.cur_num == 0 {
            let block_num = platon.block_number().wait().unwrap();

            if block_num.as_u64() == 0 {
                self.data.push((self.update_count as f64, 0.0));
                return;
            }
            self.cur_num = block_num.as_u64();

            let block = platon.block(BlockId::from(block_num)).wait().unwrap();
            if let Some(block) = block {
                self.prev_timestamp = block.timestamp.as_u64();
            }
        } else {
            let block_num = platon.block_number().wait().unwrap();

            if block_num.as_u64() > self.cur_num {
                let block = platon
                    .block(BlockId::from(U64::from(self.cur_num + 1)))
                    .wait()
                    .unwrap();
                if let Some(block) = block {
                    self.cur_time = block.timestamp.as_u64() - self.prev_timestamp;
                    self.prev_timestamp = block.timestamp.as_u64();
                    self.cur_num = block_num.as_u64();
                }
            }
        }

        if self.cur_time > self.max_time {
            self.max_time = self.cur_time
        }

        self.data
            .push((self.update_count as f64, self.cur_time as f64));
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &TimeWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut dataset = Vec::new();
        dataset.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .data(&self.data),
        );

        Chart::<String, String>::default()
            .block(block::new(&self.title))
            .x_axis(Axis::default().bounds([
                self.update_count as f64 - 25.0,
                self.update_count as f64 + 1.0,
            ]))
            .y_axis(Axis::default().bounds([0.0, 10000.0]))
            .datasets(&dataset)
            .render(area, buf);

        buf.set_string(
            area.x + 3,
            area.y + 1,
            format!("CUR   {}", self.cur_time),
            Style::default(),
        );

        buf.set_string(
            area.x + 3,
            area.y + 2,
            format!("MAX   {}", self.max_time),
            Style::default(),
        );

        buf.set_string(
            area.x + 3,
            area.y + 3,
            format!("BLOCK {}", self.cur_num),
            Style::default(),
        );
    }
}
