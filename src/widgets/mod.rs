mod block;
mod node;
#[cfg(target_family = "unix")]
mod system_summary;
#[cfg(target_family = "unix")]
mod disk_list;
mod time;
mod txs;

#[cfg(target_family = "unix")]
pub use self::system_summary::SystemSummaryWidget;
#[cfg(target_family = "unix")]
pub use self::disk_list::DiskListWidget;
pub use self::{
    node::NodeWidget,
    time::TimeWidget,
    txs::TxsWidget,
};
