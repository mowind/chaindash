use crate::collect::{Data, SharedData};
use crate::opts::Opts;
use crate::widgets::*;

pub struct App {
    pub widgets: Widgets,
    pub data: SharedData,
}

pub struct Widgets {
    pub txs: TxsWidget,
    pub time: TimeWidget,
    pub node: NodeWidget,
}

pub fn setup_app(opts: &Opts, _program_name: &str) -> App {
    let data = Data::new();
    let txs = TxsWidget::new(opts.interval, data.clone());
    let time = TimeWidget::new(opts.interval, data.clone());
    let node = NodeWidget::new(data.clone());

    App {
        widgets: Widgets { txs, time, node },
        data,
    }
}
