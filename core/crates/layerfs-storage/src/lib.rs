//! Engine-independent content storage, framing, selection and bounded saves.
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

pub mod encoding;
mod store;
pub use store::error;
pub use store::location;
pub mod pack;
pub use store::policy;
pub mod port;
pub mod read;
pub mod save;
pub(crate) use store::handle as storage;
pub use store::source;

pub use error::{StorageError, StorageResult};
pub use policy::{StorageCapacities, StoragePolicy};

pub use read::{Diagnostics, Reader};
pub use save::{Save, SaveSink, WriteOutcome};
pub use storage::Storage;
