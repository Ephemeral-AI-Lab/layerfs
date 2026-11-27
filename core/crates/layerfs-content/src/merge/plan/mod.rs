//! The merge plan (#355): its rows, byte forms, spilled state and the merged root.
//!
//! Entry module: declarations only.

pub(crate) mod build;
pub(crate) mod codec;
pub(crate) mod rows;
pub(crate) mod spill;
