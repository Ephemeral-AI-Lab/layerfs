//! Closed statement set and the failure type its units share.
//!
//! Every window bound is written `LIMIT ?n+0`. The engine's planner reads a
//! plainly bound LIMIT, which re-prepares the statement on each execution and
//! lets the plan follow the value. The expression keeps one generic plan: the
//! one `explain` reports.
use crate::backend::records::{BackendError, OutcomeError};
use layerfs_storage::port::{acquisition::AcquisitionError, PersistenceError};

macro_rules! statement {
    ($name:ident, $file:literal) => {
        pub(crate) const $name: &str = include_str!(concat!(
            "../../../../sql/sqlite/acquisition/queries/",
            $file,
            ".sql"
        ));
    };
}
statement!(BEGIN, "begin");
statement!(CLAIM_EPOCH, "claim_epoch");
statement!(OWNER, "owner");
statement!(CHARGE, "charge");
statement!(CREDIT, "credit");
statement!(ADVANCE, "advance");
statement!(RELEASE, "release");
statement!(PUT_ENTRY, "put_entry");
statement!(BIND_HOST_FILE, "bind_host_file");
statement!(ENTRY_DEPENDENCIES, "entry_dependencies");
statement!(PUT_ENTRIES, "put_entries");
statement!(PUT_HOST_FILE, "put_host_file");
statement!(UNPLACED_FIRST, "unplaced_first");
statement!(UNPLACED_NEXT, "unplaced_next");
statement!(PLACE_ENTRY, "place_entry");
statement!(DIRECTORIES, "directories");
statement!(DIRECTORY_SIZES, "directory_sizes");
statement!(DIRECTORY_PATH, "directory_path");
statement!(JOBS, "jobs");
statement!(JOB_SIZES, "job_sizes");
statement!(JOB, "job");
statement!(COMPLETE_FILE, "complete_file");
statement!(FILE_ROOTS, "file_roots");
statement!(ENTRIES_FIRST, "entries_first");
statement!(ENTRIES_NEXT, "entries_next");
statement!(BINDINGS_FIRST, "bindings_first");
statement!(BINDINGS_NEXT, "bindings_next");
statement!(SET_DIRECTORY_ROOT, "set_directory_root");
statement!(DISCARD_ENTRIES, "discard_entries");
statement!(DISCARD_HOST_FILE, "discard_host_file");
statement!(DISCARD_ENTRY_KEYS, "discard_entry_keys");
statement!(DISCARD_HOST_FILE_KEYS, "discard_host_file_keys");
statement!(DISCARD_ENTRY_TAIL, "discard_entry_tail");
statement!(DISCARD_HOST_FILE_TAIL, "discard_host_file_tail");
statement!(ABANDONED, "abandoned");
statement!(RELEASE_ABANDONED, "release_abandoned");

/// One unit's failure before it is reported through the port.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Failure {
    Backend(BackendError),
    /// The Store was created without the acquisition tables.
    Unavailable,
    /// The operation still holds working rows.
    Held,
    Stale,
    Bounds,
    Changed(u64),
}
impl From<BackendError> for Failure {
    fn from(error: BackendError) -> Self {
        Self::Backend(error)
    }
}
impl OutcomeError for Failure {
    fn uncertain(&self) -> bool {
        *self == Self::Backend(BackendError::Unknown)
    }
}
impl From<Failure> for AcquisitionError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Backend(error) => Self::Persistence(error.into()),
            Failure::Unavailable => Self::Persistence(PersistenceError::BackendUnavailable),
            Failure::Held => Self::Persistence(PersistenceError::Refused {
                status: "acquisition working rows remain".to_owned(),
            }),
            Failure::Stale => Self::Stale,
            Failure::Bounds => Self::Bounds,
            Failure::Changed(position) => Self::Changed { position },
        }
    }
}

/// A position as the engine's signed integer; larger values are out of bounds.
pub(crate) fn signed(value: u64) -> Result<i64, Failure> {
    i64::try_from(value).map_err(|_| Failure::Bounds)
}
/// A stored non-negative integer.
pub(crate) fn unsigned(value: i64) -> Result<u64, Failure> {
    u64::try_from(value).map_err(|_| Failure::Backend(BackendError::Integrity))
}
