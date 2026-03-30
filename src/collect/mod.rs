mod collector;
mod types;

// Exported for tests (Unix only)
#[cfg(all(target_family = "unix", test))]
pub use self::collector::DiskDetail;
#[cfg(target_family = "unix")]
pub use self::collector::SystemStats;
pub use self::collector::{
    run,
    Collector,
    ConsensusState,
    Data,
    NodeDetail,
    SharedData,
    StatusLevel,
    StatusMessage,
};
