//! Private backend-neutral LayerFS storage engine.
//!
//! L0 owns the checked canonical format/error/path surface and custody tests.
//! L1 incrementally adds the private BLAKE3 identity, FastCDC, immutable
//! admission, dense-pack, and structural COW runtime without exposing the
//! later public SDK, workspace, authority, or publication contracts.

#![forbid(unsafe_code)]

mod error;

#[cfg(feature = "c3-polymorphism")]
pub mod cas;
#[cfg(not(feature = "c3-polymorphism"))]
pub(crate) mod cas;
pub mod cdc;
pub mod content;
pub mod cow;
pub mod format;
pub mod identity;
pub mod limits;
pub mod object;
#[cfg(feature = "c3-polymorphism")]
pub mod pack;
#[cfg(not(feature = "c3-polymorphism"))]
pub(crate) mod pack;
pub mod profile;

pub use error::{CoreError, CoreResult, OutcomeCode};
