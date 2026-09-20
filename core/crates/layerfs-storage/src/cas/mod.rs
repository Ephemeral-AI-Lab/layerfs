//! Content-addressed save and read: one owner, bounded batches, real persistence.
//!
//! Entry module: declarations and re-exports only.

mod batch;
mod dependencies;
mod finish;
mod lifecycle;
mod membership;
mod owner;
mod placement;
mod pool_lane;
mod provider;
mod read;
mod save;
mod selection;
mod store;

pub use owner::{OutcomeCounters, SaveProfile};
pub use pool_lane::PoolCounters;
pub use provider::StoreProvider;
pub use read::ReadCounters;
pub use store::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreReadCounters};
