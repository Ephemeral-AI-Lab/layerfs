//! Private backend-neutral LayerFS storage engine.
//!
//! L0 owns the checked canonical format/error/path surface and custody tests.
//! L1 incrementally adds the private BLAKE3 identity, FastCDC, immutable
//! admission, dense-pack, and structural COW runtime without exposing the
//! later public SDK, workspace, authority, or publication contracts.

#![forbid(unsafe_code)]

mod cas_stream;
mod error;

pub mod cas;
pub mod cdc;
pub mod content;
pub mod format;
pub mod identity;
pub mod limits;
pub mod object;
pub mod pack;
pub mod profile;
pub mod tree;
pub mod update;

pub use error::{CoreError, CoreResult, OutcomeCode};
