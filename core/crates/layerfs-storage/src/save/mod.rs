//! Bounded immutable-pack saves through the bounded persistence port.
mod operation;
mod provider;
mod state;
pub use operation::{PreparedTail, Save, SaveSink};
pub(crate) use publish::reservation::IdBlock;
pub use state::WriteOutcome;

pub(crate) mod prepare;
pub(crate) mod publish;
pub(crate) mod select;
mod work;
pub use work::{SaveHistory, SaveWork, StageWork};
pub(crate) use work::{Stage, Work};

pub(crate) use prepare::batch::PendingBatch;
pub use select::counters::{PoolCounters, SaveProfile};
