//! Validation of one filesystem operation: checked inputs, bindings and effective-tree proof.
//!
//! Entry module: declarations only.

pub(crate) mod backed;
pub mod checked;
pub(crate) mod cycles;
pub(crate) mod entries;
pub(crate) mod in_place;
pub(crate) mod incremental;
