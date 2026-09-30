//! Exclusive process startup and observed SQLite native heap enforcement.
//!
//! The engine guard covers SQLite's tracked native allocator. It does not admit
//! complete SQL shapes, protected catalog progress or physical Server memory.

mod ffi;
mod guard;
mod profile;
mod status;

pub use ffi::bootstrap_exclusive;
pub use guard::EngineGuard;
pub use profile::{EngineProfile, ENGINE_HEAP_LIMIT_BYTES, ENGINE_PROBE_REQUEST_BYTES};
pub use status::{
    EngineBootstrapCustody, EngineBootstrapFailure, EngineBootstrapObservation,
    EngineBootstrapStage, EngineObservation, NativeMemory,
};
