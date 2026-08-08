//! Private backend-neutral LayerFS storage engine.
//!
//! L0 owns the checked, dependency-free canonical format/error/path surface
//! and the custody tests for the frozen M6.1.2/M6.0 artifacts. Later stages
//! add BLAKE3 identity admission, FastCDC, packs, copy-on-write trees,
//! workspaces, structural diff, and generic publication primitives.

#![forbid(unsafe_code)]

mod error;

pub mod format;

pub use error::{CoreError, CoreResult, OutcomeCode};
