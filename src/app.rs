use crate::{
    collect::{
        Data,
        SharedData,
    },
    opts::Opts,
    widgets::*,
};

pub struct App {
    pub widgets: Widgets,
    pub data: SharedData,
}

pub struct Widgets {
    pub txs: TxsWidget,
    pub time: TimeWidget,
    pub node: NodeWidget,
    #[cfg(target_family = "unix")]
    pub system: SystemWidget,
}

pub fn setup_app(
    opts: &Opts,
    _program_name: &str,
) -> App {
    let data = Data::new();
    let txs = TxsWidget::new(opts.interval, data.clone());
    let time = TimeWidget::new(opts.interval, data.clone());
    let node = NodeWidget::new(data.clone());

    #[cfg(target_family = "unix")]
    let system = SystemWidget::new(data.clone());

    App {
        widgets: Widgets {
            txs,
            time,
            node,
            #[cfg(target_family = "unix")]
            system,
        },
        data,
    }
}
