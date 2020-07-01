use num_rational::Ratio;
use tui::buffer::Buffer;
use tui::layout::Rect;
use tui::style::{Color, Style};
use tui::symbols::Marker;
use tui::widgets::{Axis, Chart, Dataset, GraphType, Widget};
use web3::futures::Future;
use web3::transports::{EventLoopHandle, Http};
use web3::types::{Block, BlockId, H256, U64};

use crate::update::UpdatableWidget;
use crate::widgets::block;

pub struct TxsWidget {
    title: String,
    update_interval: Ratio<u64>,

    eloop: EventLoopHandle,
    web3: web3::Web3<Http>,

    update_count: u64,
    cur_num: u64,
    cur_txs: u64,
    max: u64,
    data: Vec<(f64, f64)>,
}

impl TxsWidget {
    pub fn new(update_interval: Ratio<u64>, url: &str) -> TxsWidget {
        let update_count = 0;

        let (eloop, transport) = web3::transports::Http::new(url).unwrap();
        let web3 = web3::Web3::new(transport);

        let mut txs_widgets = TxsWidget {
            title: " Block Transactions ".to_string(),
            update_interval,

            eloop: eloop,
            web3: web3,

            update_count,
            cur_num: 0,
            cur_txs: 0,
            max: 0,
            data: vec![(0.0, 0.0)],
        };

        txs_widgets
    }
}

impl UpdatableWidget for TxsWidget {
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

            let txs_count = platon
                .block_transaction_count(BlockId::from(block_num))
                .wait()
                .unwrap();
            match txs_count {
                Some(txs_count) => {
                    self.cur_txs = txs_count.as_u64();
                }
                _ => self.cur_txs = 0,
            }
        } else {
            let block_num = platon.block_number().wait().unwrap();

            if block_num.as_u64() > self.cur_num {
                let txs_count = platon
                    .block_transaction_count(BlockId::from(U64::from(self.cur_num + 1)))
                    .wait()
                    .unwrap();
                match txs_count {
                    Some(txs_count) => {
                        self.cur_txs = txs_count.as_u64();
                        self.cur_num += 1
                    }
                    _ => self.cur_txs = 0,
                }
            }

            if self.cur_txs > self.max {
                self.max = self.cur_txs
            }
            self.data
                .push((self.update_count as f64, self.cur_txs as f64));
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
            .y_axis(Axis::default().bounds([0.0, 5000.0]))
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
            format!("MAX   {}", self.max),
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
