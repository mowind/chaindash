mod block;
#[cfg(target_family = "unix")]
mod disk_list;
mod node;
mod node_detail;
#[cfg(target_family = "unix")]
mod system_summary;
mod time;
mod txs;

#[cfg(target_family = "unix")]
pub use self::disk_list::DiskListWidget;
#[cfg(target_family = "unix")]
pub use self::system_summary::SystemSummaryWidget;
pub use self::{
    node::NodeWidget,
    node_detail::NodeDetailWidget,
    time::TimeWidget,
    txs::TxsWidget,
};
