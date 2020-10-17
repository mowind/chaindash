use std::collections::HashMap;

use log::{debug, error, info, trace, warn};
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration};
use web3::futures::{self, Future, StreamExt};
use web3::transports::{self, Http, WebSocket};
use web3::types::BlockId;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct ConsensusState {
    pub name: String,
    pub current_number: u64,
    pub epoch: u64,
    pub view: u64,
    pub committed: u64,
    pub locked: u64,
    pub qc: u64,
    pub validator: bool,
}

#[derive(Debug)]
pub struct Data {
    cur_block_number: u64,
    cur_block_time: u64,
    prev_block_time: u64,
    cur_txs: u64,
    max_txs: u64,

    txns: Vec<u64>,
    intervals: Vec<u64>,

    cur_interval: u64,
    max_interval: u64,

    states: HashMap<String, ConsensusState>,
}

pub type SharedData = Arc<Mutex<Data>>;

#[derive(Debug)]
pub struct Collector {
    data: SharedData,
    urls: Vec<String>,
}

pub async fn run(collector: Collector) -> Result<()> {
    tokio::select! {
        res = collector.run() => {
            res
        }
    }
}

impl Default for ConsensusState {
    fn default() -> Self {
        ConsensusState {
            name: String::from(""),
            current_number: 0,
            epoch: 0,
            view: 0,
            committed: 0,
            locked: 0,
            qc: 0,
            validator: false,
        }
    }
}

impl Default for Data {
    fn default() -> Data {
        Data {
            cur_block_number: 0,
            cur_block_time: 0,
            prev_block_time: 0,
            cur_txs: 0,
            max_txs: 0,
            txns: vec![0],
            intervals: vec![0],
            cur_interval: 0,
            max_interval: 0,
            states: HashMap::new(),
        }
    }
}

impl Data {
    pub fn new() -> SharedData {
        Arc::new(Mutex::new(Data::default()))
    }

    pub fn cur_block_number(&self) -> u64 {
        self.cur_block_number
    }

    pub fn cur_block_time(&self) -> u64 {
        self.cur_block_time
    }

    pub fn prev_block_time(&self) -> u64 {
        self.prev_block_time
    }

    pub fn cur_txs(&self) -> u64 {
        self.cur_txs
    }

    pub fn max_txs(&self) -> u64 {
        self.max_txs
    }

    pub fn txns_and_clear(&mut self) -> Vec<u64> {
        let txns = self.txns.clone();
        self.txns.clear();
        txns
    }

    pub fn intervals_and_clear(&mut self) -> Vec<u64> {
        let intervals = self.intervals.clone();
        self.intervals.clear();
        intervals
    }

    pub fn cur_interval(&self) -> u64 {
        self.cur_interval
    }

    pub fn max_interval(&self) -> u64 {
        self.max_interval
    }

    pub fn states(&self) -> Vec<ConsensusState> {
        let states: Vec<ConsensusState> = self.states.iter().map(|(_, val)| val.clone()).collect();
        states
    }
}

impl Collector {
    pub fn new(urls: Vec<String>, data: SharedData) -> Self {
        Collector { data, urls }
    }

    pub(crate) async fn run(&self) -> Result<()> {
        let ws = WebSocket::new(self.urls[0].as_str()).await?;
        let web3 = web3::Web3::new(ws.clone());
        let mut sub = web3.platon_subscribe().subscribe_new_heads().await?;

        let urls = self.urls.clone();
        let _: Vec<_> = urls
            .into_iter()
            .map(|url| {
                tokio::spawn(collect_node_state(url.clone(), self.data.clone()));
            })
            .collect();

        loop {
            tokio::select! {
                Some(head) = (&mut sub).next() => {
                    let head = head.unwrap();
                    let number = head.number.unwrap();
                    let number = BlockId::from(number);
                    let txs = web3.platon().block_transaction_count(number).await?;
                    let txs = txs.unwrap().as_u64();

                    let mut data = self.data.lock().unwrap();
                    data.cur_block_number = head.number.unwrap().as_u64();
                    if data.cur_block_time > 0 {
                        data.prev_block_time = data.cur_block_time;
                    }
                    data.cur_block_time = head.timestamp.as_u64();
                    data.cur_txs = txs;

                    if txs > data.max_txs {
                        data.max_txs = txs
                    }

                    data.txns.push(txs);
                    if data.prev_block_time > 0 {
                        let interval = data.cur_block_time - data.prev_block_time;
                        data.cur_interval = interval;
                        if interval > data.max_interval {
                            data.max_interval = interval
                        }
                        data.intervals.push(interval);
                    }
                }
            }
        }
    }
}

async fn collect_node_state(url: String, data: SharedData) -> Result<()> {
    let ws = WebSocket::new(url.as_str()).await?;
    let web3 = web3::Web3::new(ws.clone());
    let debug = web3.debug();
    let platon = web3.platon();

    let mut interval = time::interval(Duration::from_secs(1));

    let name = url.replace("ws://", "");

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let state = debug.consensus_status().await?;
                let cur_number = platon.block_number().await?;
                let node = ConsensusState{
                    name: name.clone(),
                    current_number: cur_number.as_u64(),
                    epoch: state.state.view.epoch,
                    view: state.state.view.view,
                    committed: state.state.committed.number,
                    locked: state.state.locked.number,
                    qc: state.state.qc.number,
                    validator: state.validator,
                };

                let mut data = data.lock().unwrap();
                data.states.insert(name.clone(), node);
            }
        }
    }
}
