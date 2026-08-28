//! Frozen `legacy_full` evaluator bridge.
//!
//! Removal gate: delete this bridge only after the evaluator fixtures carry exact
//! typed base/Branch identities and bind-and-slim migration preserves every
//! legacy evidence field without the legacy named-ref authority.

mod capture_legacy;
mod counters;
mod diagnostics;
mod facade;
mod managed_edit_legacy;
mod materialize_legacy;
mod refresh_legacy;
mod resolver_legacy;
mod session_legacy;
mod types;

pub(crate) use counters::{add_metadata_rope, add_native, add_scratch};
pub(crate) use diagnostics::{CompactionDiagnostics, Diagnostics};
pub(crate) use facade::OpenedLayerFs;
pub(crate) use session_legacy::{ExternalWorkspace, LayerFs, ManagedWorkspace, VfsError};
pub(crate) use types::*;
