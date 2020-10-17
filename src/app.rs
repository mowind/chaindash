use num_rational::Ratio;

use crate::collect::{Data, SharedData};
use crate::opts::Opts;
use crate::widgets::*;

pub struct App {
    pub urls: Vec<String>,
    pub widgets: Widgets,
    pub data: SharedData,
}

pub struct Widgets {
    pub txs: TxsWidget,
    pub time: TimeWidget,
    pub node: NodeWidget,
}

pub fn setup_app(opts: &Opts, _program_name: &str) -> App {
    let urls: Vec<&str> = opts.url.as_str().split(",").collect();
    let data = Data::new();
    let interval = Ratio::from_integer(1);
    let txs = TxsWidget::new(interval, data.clone());
    let time = TimeWidget::new(interval, data.clone());
    let node = NodeWidget::new(data.clone());

    let s_urls = urls.into_iter().map(|url| String::from(url)).collect();

    App {
        urls: s_urls,
        widgets: Widgets { txs, time, node },
        data,
    }
}
