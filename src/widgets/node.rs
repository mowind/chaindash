use std::collections::HashMap;

use num_rational::Ratio;
use tui::buffer::Buffer;
use tui::layout::{Constraint, Rect};
use tui::style::Modifier;
use tui::widgets::{Row, Table, Widget};
use web3::futures::Future;
use web3::transports::{EventLoopHandle, Http};
use web3::types::{Block, BlockId, ConsensusStatus, H256, U64};

use crate::update::UpdatableWidget;
use crate::widgets::block;

struct NodeEndpoint {
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

pub struct NodeWidget {
    title: String,
    update_interval: Ratio<u64>,

    endpoints: HashMap<String, NodeEndpoint>,
    nodes: Vec<Node>,
    //grouped_nodes: HashMap<String, Node>,
}

impl NodeWidget {
    pub fn new(urls: &Vec<&str>) -> NodeWidget {
        let mut es: HashMap<String, NodeEndpoint> = HashMap::new();
        for url in urls {
            let (eloop, transport) = web3::transports::Http::new(url).unwrap();
            let web3 = web3::Web3::new(transport);

            es.insert(url.to_string(), NodeEndpoint { eloop, web3 });
        }

        NodeWidget {
            title: " Nodes ".to_string(),
            update_interval: Ratio::from_integer(1),

            endpoints: es,
            nodes: Vec::new(),
            //grouped_nodes: HashMap::new(),
        }
    }

    fn update_node(&self, name: &str, web3: &web3::Web3<Http>) -> Node {
        let platon = web3.platon();
        let block_number = platon.block_number().wait().unwrap();
        let debug = web3.debug();
        let status = debug.consensus_status().wait().unwrap();

        Node {
            name: name.to_string(),
            block_number: block_number.as_u64(),
            epoch: status.state.view.epoch,
            view_number: status.state.view.view,
            committed: status.state.committed.number,
            locked: status.state.committed.number,
            qc: status.state.qc.number,
            validator: status.validator,
        }
    }
}

impl UpdatableWidget for NodeWidget {
    fn update(&mut self) {
        let mut nodes = Vec::new();

        for (k, v) in self.endpoints.iter() {
            nodes.push(self.update_node(k.as_str(), &v.web3))
        }
        self.nodes = nodes
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

        let mut header = [
            " Name",
            "BlockNumber",
            "Epoch",
            "ViewNumber",
            "Committed",
            "Locked",
            "QC",
            "Validator",
        ];

        let mut nodes = self.nodes.clone();

        Table::new(
            header.iter(),
            nodes.into_iter().map(|node| {
                Row::Data(
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
                )
            }),
        )
        .block(block::new(&self.title))
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
