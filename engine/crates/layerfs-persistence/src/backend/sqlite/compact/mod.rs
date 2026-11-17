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
//! generation (§9) and removes the artifact set it names. A record whose owner
//! is provably live is not a leftover: the next call refuses `Busy` instead of
//! adopting the state of a run that released the writer lane but has not yet
//! settled, and a settle removes the record only while it still names the
//! operation being settled.
//!
//! The T7.2 mark already resolves every logical reference and delta base, so
//! the bytes of packs that hold no reached object are the refined dead bytes:
//! §7's step-4 payload sweep cannot discover another live object on SQLite.
//! `deadBytesRefined` therefore equals `deadBytesBound` on the `none` path (as
//! §11 requires) and on a `rebuild` too, because there is no second payload
//! sweep to refine the bound with (#42 A10).
//!
//! The module is split by phase: [`types`] (the §7/§11 shapes), [`op`] (the
//! `.compact-op` sidecar), [`build`] with [`space`] (the fresh file's size
//! during the load), [`verify`], [`install`], [`receipt`] and [`run`] (the
//! orchestration on `SqliteHandles`).

mod build;
mod install;
mod op;
mod receipt;
mod run;
mod space;
mod types;
mod verify;

pub use build::{peak_temp_bytes, source_mmap, SRC_MMAP_BYTES};
pub use op::{compact_op_status, CompactOpStatus};
pub use receipt::receipt;
pub use space::{reserve, trim};
pub use types::io_error;
pub use types::{cap_delta_references, DELTA_BASE_REFERENCES_CAP};
pub use types::{
    CompactFailpoint, CompactKeptWaste, CompactPath, CompactReceipt, CompactRequest,
    CompactResidue, CompactZeroReason, OwnerLiveness,
};
