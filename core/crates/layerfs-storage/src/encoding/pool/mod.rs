//! Pooled physical metadata: pooled leaf records over stored inode values.
//!
//! Entry module: declarations and re-exports only.

pub mod counters;
pub mod delta;
pub mod leaf;
mod memo;
pub mod program_page;
pub mod read;

pub use counters::PoolReadCounters;
pub use leaf::{PooledRecord, POOLED_DELTA_TAG, POOLED_FULL_TAG};
pub use read::PoolReader;
