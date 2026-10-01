//! Content-addressed save and read: one owner, bounded batches, real persistence.
//!
//! Entry module: declarations and re-exports only.

mod batch;
mod collision;
mod dependencies;
mod engine_owner;
mod engine_store;
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
mod working;
mod working_prepared;

pub use owner::{OutcomeCounters, ResolveProfile, SaveProfile};
pub use pool_lane::PoolCounters;
pub use provider::{OwnedReadProfile, StoreProvider};
pub use read::ReadCounters;
pub use store::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreReadCounters};
pub use working::{
    SaveWorkingClass, SaveWorkingOwnerStatus, SaveWorkingStatus, SAVE_INDEX_PAIR_BYTES,
    SAVE_WORKING_BYTES,
};
