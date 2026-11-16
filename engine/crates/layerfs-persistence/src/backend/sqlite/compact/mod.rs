//! §7 SQLite Store compaction: mark, rebuild, immutable verification and the
//! atomic install.
//!
//! One call takes the Store's writer lane before it registers its admission
//! intent, runs the T7.2 mark in the same exclusive window, seals the file
//! (checkpoint, checked close, sidecar check and `(size, mtime, change
//! counter)` capture), and then either stops on the §3 `none` path or builds
//! `<store>.compact`, verifies it through an `immutable=1` URI, and renames it
//! into place at `G → G+1`. The run's phase and mark tuple are kept beside the
//! Store in `<store>.compact-op`, the SQLite equivalent of the PostgreSQL
//! `gc_op` row (§11); the next `store.compact` judges a leftover by the Store
//! generation (§9) and removes the artifact set it names.
//!
//! The T7.2 mark already resolves every logical reference and delta base, so
//! the bytes of packs that hold no reached object are the refined dead bytes:
//! §7's step-4 payload sweep cannot discover another live object on SQLite.
//! `deadBytesRefined` therefore equals `deadBytesBound`, as §11 requires for
//! the `none` path.
//!
//! The module is split by phase: [`types`] (the §7/§11 shapes), [`op`] (the
//! `.compact-op` sidecar), [`build`], [`verify`], [`install`], [`receipt`] and
//! [`run`] (the orchestration on `SqliteHandles`).

mod build;
mod install;
mod op;
mod receipt;
mod run;
mod types;
mod verify;

pub use build::{journal_bound_bytes, peak_temp_bytes};
pub use op::{compact_op_status, CompactOpStatus};
pub use types::io_error;
pub use types::{cap_delta_references, DELTA_BASE_REFERENCES_CAP};
pub use types::{
    CompactFailpoint, CompactKeptWaste, CompactPath, CompactReceipt, CompactRequest,
    CompactResidue, CompactZeroReason,
};
