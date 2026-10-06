//! Authenticated native delivery; logical object/history/control codecs are separate.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
pub mod codec;
pub mod contract;
#[cfg(feature = "native")]
pub mod native;
