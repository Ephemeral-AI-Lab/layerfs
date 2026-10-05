//! Logical operations and authenticated native delivery; no C1/C2 dependency.
#![forbid(unsafe_code)]
#[cfg(feature = "native")]
pub mod adapters;
pub mod contract;
