//! Payload DELTA: record framing, candidate acquisition, selection and chains.
//!
//! Entry module: declarations and re-exports only.

pub mod candidates;
pub mod edge;
pub(crate) mod full;
pub mod program_page;
pub mod read;
pub mod record;
pub mod select;

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::encoding::delta::program_page as branch; // #828 compat
