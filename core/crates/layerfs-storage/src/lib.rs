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
// crate cannot be: the pinned zstd codec needs the C FFI, and a lint that is
// `forbid`ed cannot be allowed back on for one module (E0453). `unsafe` is
// therefore denied crate-wide and allowed on exactly one audited module,
// `encoding::codec` (see its documentation for the FFI inventory), and the
// product boundary guard rejects `unsafe` anywhere else in this crate. The
// deviation from the siblings' literal `forbid` is recorded as a design note
// in `physical-encoding-and-packing.md`.
#![deny(unsafe_code)]

pub mod cas;
pub mod encoding;
pub mod error;
pub mod pack;
pub mod policy;
pub mod sqlite;

pub use cas::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreProvider, StoreReadCounters};
pub use error::{StorageError, StorageResult};
pub use policy::{
    SchemaIdentity, StorageCapacities, StoragePolicy, SCHEMA_IDENTITY, STORE_CACHE_PAGES,
    STORE_PAGE_SIZE_BYTES,
};
