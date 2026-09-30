//! Physical content-addressed storage for LayerFS (C2).
//!
//! C2 owns exact CAS reuse, the supported FULL representations, pack framing and
//! placement, and the embedded SQLite schema that records locators. It accepts
//! already-finalized canonical objects from C1 (or from any other producer) and
//! can save and read them without running file construction, a Workspace, a
//! history entity or a mount.
//!
//! The persistence profile is the selected embedded one: MEMORY journal,
//! `synchronous = OFF`, memory temporary storage and zero busy timeout. There is
//! no WAL, no added crash-durability work and no `fsync`/`fdatasync`/`sync_all`/
//! `sync_data` anywhere in the product path. `COMMIT` remains required, and one
//! attempted operation never retries, resends or silently changes route.
//!
//! See the crate `README.md` for the accepted profile, the capacities and the
//! exact commands that exercise the real save and read paths.

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
// `layerfs-content` and `layerfs-telemetry` are `forbid(unsafe_code)`. This
// crate cannot be: the pinned zstd codec and exclusive SQLite startup need C
// FFI, and a `forbid` lint cannot be allowed back on for a module (E0453).
// Unsafe is denied crate-wide and allowed only in the exact audited files
// `encoding/codec.rs` and `engine/ffi.rs`; each owns its safety inventory.
// The product boundary guard rejects unsafe code in every sibling file.
// The codec deviation is recorded in `physical-encoding-and-packing.md`;
// Core AGENTS and the engine contract own the exclusive startup exception.
#![deny(unsafe_code)]

pub mod cas;
pub mod construction_state;
pub mod encoding;
pub mod engine;
pub mod error;
pub mod pack;
pub mod policy;
pub mod sqlite;

pub use cas::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreProvider, StoreReadCounters};
pub use error::{StorageError, StorageResult};
pub use policy::{SchemaIdentity, StorageCapacities, StoragePolicy, SCHEMA_IDENTITY};
