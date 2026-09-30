//! Typed append-only construction facts and their supplied-state boundary.
//!
//! Declarations and reexports only. This initial format owns DirectoryRoots;
//! it supplies no mutable graph/draft capability or physical memory claim.
mod cursor;
mod directory_roots;
mod page;
mod port;
mod records;
mod resident;
mod seal;
mod selection;

pub use cursor::StateCursor;
pub(crate) use directory_roots::DirectoryRoots;
pub use page::{PageLimit, StatePage, STATE_PAGE_HEADER_BYTES};
pub use port::IndexedState;
pub use records::{
    StateCapacity, StateKey, StateRecord, DIRECTORY_ROOT_RECORD_BYTES, STATE_APPEND_HEADER_BYTES,
    STATE_KEY_BYTES, STATE_MAX_KEY_BYTES, STATE_MAX_PAGE_BYTES, STATE_MAX_PAGE_RECORDS,
    STATE_MAX_VALUE_BYTES,
};
pub use resident::ResidentState;
pub use seal::{StateLedger, StateSeal, STATE_SEAL_BYTES};
pub use selection::{StateScope, StateSelection, StateTable, STATE_SCOPE_BYTES};
