//! §7/§11 compaction types, sidecar constants and the error classification.

use crate::backend::sqlite::session::rows;
use layerfs_history::error::HistoryError;
use layerfs_storage::port::PersistenceError;

/// The sidecar record suffix (§11: SQLite keeps its phase outside the sealed
/// database).
pub(crate) const OP_SUFFIX: &str = ".compact-op";

/// The fresh rebuild Store beside the live one.
pub(crate) const COMPACT_SUFFIX: &str = ".compact";

/// The §6.3 visited scratch database.
pub(crate) const VISITED_SUFFIX: &str = ".compact-visited";

/// The full SQLite artifact set of §9, without the variable `-mj*` journals:
/// the fixed staged fresh file and visited artifact plus the incomplete op
/// record (`.compact-op.tmp`), which an op write that died before its rename
/// leaves. The fresh file is built with `journal_mode=OFF`, so a crash leaves a
/// partial `.compact` with no rollback journal; the settle deletes it.
pub(crate) const ARTIFACT_SUFFIXES: [&str; 7] = [
    COMPACT_SUFFIX,
    ".compact-journal",
    ".compact-wal",
    ".compact-shm",
    VISITED_SUFFIX,
    ".compact-visited-journal",
    ".compact-op.tmp",
];

/// The mark tuple a compaction records before its first checkpoint (§8 Phase M
/// on PostgreSQL; the same first-statement tuple in the SQLite sidecar here).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MarkTuple {
    pub(crate) catalog_id: Vec<u8>,
    pub(crate) generation: i64,
    pub(crate) next_history_serial: i64,
    pub(crate) next_pack_id: i64,
    pub(crate) next_ordinal: i64,
    pub(crate) logical: i64,
    pub(crate) canonical: i64,
    pub(crate) stored: i64,
    pub(crate) metadata: i64,
    pub(crate) stamp: i64,
    pub(crate) history_rows: i64,
}

/// §7 step 3's capacity classification: `ENOSPC`, `EDQUOT` and `EFBIG` are
/// a `Capacity` refusal; every other I/O failure stays `Integrity`.
pub fn io_error(error: std::io::Error) -> HistoryError {
    match error.raw_os_error() {
        Some(28) | Some(122) | Some(27) | Some(69) => HistoryError::Capacity("disk space"),
        _ => HistoryError::Integrity("Store compaction I/O"),
    }
}

pub(crate) fn db_error(error: rusqlite::Error) -> HistoryError {
    rows::error(error).into()
}

pub(crate) fn seal_error(error: PersistenceError) -> HistoryError {
    match error {
        PersistenceError::Busy => HistoryError::Busy,
        PersistenceError::Refused { status } if status == "Capacity" => {
            HistoryError::Capacity("Store compaction")
        }
        PersistenceError::Refused { status } if status == "Store sidecar remains after seal" => {
            HistoryError::Conflict("Store sidecar remains after seal")
        }
        PersistenceError::Refused { .. } | PersistenceError::Malformed => {
            HistoryError::Integrity("Store seal")
        }
        PersistenceError::Uncertain => HistoryError::UnknownOutcome,
        PersistenceError::Missing | PersistenceError::BackendUnavailable => {
            HistoryError::Unsupported("SQLite Store compaction")
        }
    }
}
