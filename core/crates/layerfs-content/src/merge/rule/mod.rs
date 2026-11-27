//! Merge rules: last-writer-wins from `/.lww`, merge units and unit reverts.
//!
//! Entry module: declarations only.

pub mod lww;
pub(crate) mod revert;
pub(crate) mod units;
