//! Private backend-neutral LayerFS storage engine.
//!
//! L0 owns the checked canonical format/error/path surface and custody tests.
//! L1 incrementally adds the private BLAKE3 identity, FastCDC, immutable
//! admission, dense-pack, and structural COW runtime without exposing the
//! later public SDK, workspace, authority, or publication contracts.

#![forbid(unsafe_code)]

mod error;

#[allow(dead_code)]
pub(crate) mod cas;
pub mod cdc;
#[allow(dead_code)]
pub(crate) mod content;
#[allow(dead_code)]
pub(crate) mod cow;
pub mod format;
pub mod identity;
#[cfg(feature = "operation-polymorphism")]
#[allow(dead_code)]
pub(crate) mod lifecycle;
#[allow(dead_code)]
pub(crate) mod limits;
pub mod object;
#[allow(dead_code)]
pub(crate) mod pack;
pub mod profile;
#[cfg(feature = "operation-polymorphism")]
#[allow(dead_code)]
pub(crate) mod read;

pub use error::{CoreError, CoreResult, OutcomeCode};

// These accepted storage tests exercise private production boundaries from
// inside this crate. Keeping them internal prevents test plumbing from turning
// concrete FsCas, pack, COW, resource, or complete-operation machinery into a dependent-crate
// SDK.
#[cfg(test)]
extern crate self as layerfs_storage;

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;

#[cfg(all(test, feature = "operation-polymorphism"))]
#[path = "../tests/c3_fscas.rs"]
mod c3_fscas_tests;
#[cfg(all(test, feature = "operation-polymorphism"))]
#[path = "../tests/c3_mutation.rs"]
mod c3_mutation_tests;
#[cfg(all(test, feature = "operation-polymorphism"))]
#[path = "../tests/c3_operation.rs"]
mod c3_operation_tests;
#[cfg(all(test, feature = "operation-polymorphism"))]
#[path = "../tests/l1_cas.rs"]
mod l1_cas_tests;
#[cfg(test)]
#[path = "../tests/l1_content.rs"]
mod l1_content_tests;
#[cfg(all(test, feature = "operation-polymorphism"))]
#[path = "../tests/l1_pack.rs"]
mod l1_pack_tests;
#[cfg(test)]
#[path = "../tests/l1_resources.rs"]
mod l1_resources_tests;
#[cfg(test)]
#[path = "../tests/l1_tree.rs"]
mod l1_tree_tests;
#[cfg(test)]
#[path = "../tests/l1_update.rs"]
mod l1_update_tests;
