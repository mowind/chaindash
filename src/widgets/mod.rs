mod block;
mod node;
#[cfg(target_family = "unix")]
mod system;
mod time;
mod txs;

#[cfg(target_family = "unix")]
pub use self::system::SystemWidget;
pub use self::{
    node::NodeWidget,
    time::TimeWidget,
    txs::TxsWidget,
};
