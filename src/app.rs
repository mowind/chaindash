use num_rational::Ratio;

use crate::opts::Opts;
use crate::widgets::*;

pub struct App {
    pub widgets: Widgets,
}

pub struct Widgets {
    pub txs: TxsWidget,
    pub time: TimeWidget,
    pub node: NodeWidget,
}

pub fn setup_app(opts: &Opts, program_name: &str) -> App {
    let urls: Vec<&str> = opts.url.as_str().split(",").collect();
    let interval = Ratio::from_integer(1);
    let txs = TxsWidget::new(interval, urls[0]);
    let time = TimeWidget::new(interval, urls[0]);
    let node = NodeWidget::new(&urls);

    App {
        widgets: Widgets { txs, time, node },
    }
}
