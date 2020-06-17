use num_rational::Ratio;

use crate::opts::Opts;
use crate::widgets::*;

pub struct App {
    pub widgets: Widgets,
}

pub struct Widgets {
    pub txs: TxsWidget,
}

pub fn setup_app(opts: &Opts, program_name: &str) -> App {
    let interval = Ratio::from_integer(1);
    let txs = TxsWidget::new(interval, opts.url.as_str());

    App {
        widgets: Widgets { txs },
    }
}
