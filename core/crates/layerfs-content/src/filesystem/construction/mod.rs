//! Filesystem construction: checked inputs, validation, reference effects and the operation.
//!
//! Entry module: declarations only.

pub mod captured_bindings;
pub mod input;
pub mod operation;
pub mod reference_count;
pub(crate) mod reference_count_index;
pub(crate) mod serial_state;
pub mod statistics;
pub mod validate;
