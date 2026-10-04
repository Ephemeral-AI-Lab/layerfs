//! Bounded immutable-pack saves through the bounded persistence port.
mod operation;
mod pooled;
mod provider;
mod publication;
mod reservation;
mod seal;
mod select;
mod source;
mod state;
mod wave;
pub use operation::{Save, SaveSink};
pub use state::WriteOutcome;

mod work;
pub use work::{SaveHistory, SaveWork, StageWork};
pub(crate) use work::{Stage, Work};

mod batch;
mod profile;
pub(crate) use batch::PendingBatch;
pub use profile::{PoolCounters, SaveProfile};
