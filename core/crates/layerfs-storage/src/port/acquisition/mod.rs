//! Operation-scoped working state of one initial root acquisition.
//!
//! The acquiring domain states bounded semantic units over typed rows; the
//! provider owns the engine, its transactions and every physical access path.
//! No SQL text, table name, cursor or connection crosses this boundary.
mod contract;
mod rows;
mod work;
pub use contract::{
    Acquisition, AcquisitionError, AcquisitionResult, Limits, READ_WINDOW_BYTES, READ_WINDOW_ROWS,
    WRITE_ROW_BYTES, WRITE_WINDOW_BYTES, WRITE_WINDOW_ROWS,
};
pub use rows::{
    Abandoned, Begin, Directory, Entry, EntryKey, FileRoot, Job, NativeIdentity, NewEntry, Owner,
    Phase, Placed, Unplaced, EVIDENCE_BYTES,
};
pub use work::{AcquisitionWork, Discarded};
