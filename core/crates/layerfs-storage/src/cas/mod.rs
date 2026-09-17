//! Content-addressed save and read: one owner, bounded batches, real persistence.
//!
//! Entry module: declarations and re-exports only.

mod batch;
mod dependencies;
mod finish;
mod membership;
mod owner;
mod provider;
mod read;
mod save;
mod store;

pub use owner::{OutcomeCounters, PoolCounters};
pub use provider::StoreProvider;
pub use read::ReadCounters;
pub use store::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreReadCounters};
