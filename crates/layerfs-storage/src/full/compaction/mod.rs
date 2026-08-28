//! Full-generation compaction persistence.

mod copy;
mod legacy;
pub mod reachability;
pub mod verify;

pub(crate) use copy::VerifiedFullCopy;
