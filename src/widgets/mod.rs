mod block;
mod node;
mod time;
mod txs;
#[cfg(target_family = "unix")]
mod system;

pub use self::node::NodeWidget;
pub use self::time::TimeWidget;
pub use self::txs::TxsWidget;
#[cfg(target_family = "unix")]
pub use self::system::SystemWidget;
