use std::iter::IntoIterator;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{select, tick, unbounded, Sender};
use num_rational::Ratio;
use tui::buffer::Buffer;
use tui::layout::{Constraint, Rect};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Row, Table, Widget};
use web3::futures::Future;
use web3::transports::{EventLoopHandle, Http};

use crate::update::UpdatableWidget;
use crate::widgets::block;

struct NodeEndpoint {
    url: String,
    eloop: EventLoopHandle,
    web3: web3::Web3<Http>,
}

#[derive(Clone)]
struct Node {
    name: String,
    block_number: u64,
    epoch: u64,
    view_number: u64,
    committed: u64,
    locked: u64,
    qc: u64,
    validator: bool,
}

struct Collector {
    nodes: Vec<Node>,
    handle: Option<JoinHandle<()>>,
    sender: Sender<()>,
}

impl Collector {
    fn new(urls: &Vec<&str>) -> Arc<Mutex<Box<Collector>>> {
        let (sender, recver) = unbounded();
        let collector = Arc::new(Mutex::new(Box::new(Collector {
            nodes: Vec::new(),
            handle: None,
            sender,
        })));

        let mut new_urls = Vec::new();
        for url in urls {
            new_urls.push(url.to_string());
        }

        let collector_clone = Arc::downgrade(&collector);
        let handle = thread::spawn(move || {
            let urls = new_urls.to_owned();

            let mut endpoints = Vec::new();
            for url in urls {
                let (eloop, transport) = web3::transports::Http::new(url.as_str()).unwrap();
                let web3 = web3::Web3::new(transport);

                endpoints.push(NodeEndpoint {
                    url: url.to_string(),
                    eloop,
                    web3,
                });
            }

            let endpoints = Box::new(endpoints);

            let ticker = tick(Duration::from_secs(1));
            loop {
                let collector = collector_clone.clone();
                select! {
                    recv(recver) -> _ => {
                        break;
                    }
                    recv(ticker) -> _ => {
                        let mut nodes = Vec::new();
                        for ep in endpoints.as_ref() {
                            let platon = ep.web3.platon();
                            let block_number = platon.block_number().wait().unwrap();
                            let debug = ep.web3.debug();
                            let status = debug.consensus_status().wait().unwrap();

                            nodes.push(Node {
                                name: ep.url.clone(),
                                block_number: block_number.as_u64(),
                                epoch: status.state.view.epoch,
                                view_number: status.state.view.view,
                                committed: status.state.committed.number,
                                locked: status.state.locked.number,
                                qc: status.state.qc.number,
                                validator: status.validator,
                            });
                        }

                        let collector = collector.upgrade().unwrap();
                        collector.lock().map(|mut c| {c.nodes = nodes}).unwrap();
                    }
                }
            }
        });
        collector
            .lock()
            .map(|mut c| c.handle = Some(handle))
            .unwrap();

        collector
    }

    fn get_nodes(&self) -> Vec<Node> {
        self.nodes.clone()
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.sender.send(()).unwrap();
        self.handle.take().map(|h| h.join().unwrap()).unwrap();
    }
}

pub struct NodeWidget {
    title: String,
    update_interval: Ratio<u64>,
    //endpoints: HashMap<String, NodeEndpoint>,
    nodes: Vec<Node>,
    //grouped_nodes: HashMap<String, Node>,
    collector: Arc<Mutex<Box<Collector>>>,
}

impl NodeWidget {
    pub fn new(urls: &Vec<&str>) -> NodeWidget {
        NodeWidget {
            title: " Nodes ".to_string(),
            update_interval: Ratio::from_integer(1),

            nodes: Vec::new(),

            collector: Collector::new(urls),
            //grouped_nodes: HashMap::new(),
        }
    }
}

impl UpdatableWidget for NodeWidget {
    fn update(&mut self) {
        self.nodes = self.collector.lock().unwrap().get_nodes();
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &NodeWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 {
            return;
        }

        let header = [
            " Name",
            "BlockNumber",
            "Epoch",
            "ViewNumber",
            "Committed",
            "Locked",
            "QC",
            "Validator",
        ];

        let nodes = self.nodes.clone();

        Table::new(
            header.iter(),
            nodes.into_iter().map(|node| {
                Row::StyledData(
                    vec![
                        format!(" {}", node.name),
                        format!("{}", node.block_number),
                        format!("{}", node.epoch),
                        format!("{}", node.view_number),
                        format!("{}", node.committed),
                        format!("{}", node.locked),
                        format!("{}", node.qc),
                        format!("{}", node.validator),
                    ]
                    .into_iter(),
                    Style::default()
                        .fg(Color::Indexed(249 as u8))
                        .bg(Color::Reset),
                )
            }),
        )
        .block(block::new(&self.title))
        .header_style(
            Style::default()
                .fg(Color::Indexed(249 as u8))
                .bg(Color::Reset)
                .modifier(Modifier::BOLD),
        )
        .widths(&[
            Constraint::Length(20),
            Constraint::Length(u16::max((area.width as i16 - 2 - 80 - 8) as u16, 10)),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
        ])
        .column_spacing(1)
        .header_gap(0)
        .render(area, buf);
    }
}
