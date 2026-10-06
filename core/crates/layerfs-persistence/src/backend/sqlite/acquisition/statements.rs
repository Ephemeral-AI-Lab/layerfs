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
statement!(BIND_NATIVE, "bind_native");
statement!(ENTRY_DEPENDENCIES, "entry_dependencies");
statement!(PUT_ENTRIES, "put_entries");
statement!(PUT_NATIVE, "put_native");
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
statement!(SET_DIRECTORY_ROOT, "set_directory_root");
statement!(DISCARD_ENTRIES, "discard_entries");
statement!(DISCARD_NATIVE, "discard_native");
statement!(DISCARD_ENTRY_KEYS, "discard_entry_keys");
statement!(DISCARD_NATIVE_KEYS, "discard_native_keys");
statement!(DISCARD_ENTRY_TAIL, "discard_entry_tail");
statement!(DISCARD_NATIVE_TAIL, "discard_native_tail");
statement!(ABANDONED, "abandoned");
statement!(RELEASE_ABANDONED, "release_abandoned");

/// Every shipped statement by name, in a fixed order, for plan inspection.
pub(crate) const ALL: [(&str, &str); 34] = [
    ("begin", BEGIN),
    ("claim_epoch", CLAIM_EPOCH),
    ("owner", OWNER),
    ("charge", CHARGE),
    ("credit", CREDIT),
    ("advance", ADVANCE),
    ("release", RELEASE),
    ("put_entry", PUT_ENTRY),
    ("bind_native", BIND_NATIVE),
    ("entry_dependencies", ENTRY_DEPENDENCIES),
    ("put_entries", PUT_ENTRIES),
    ("put_native", PUT_NATIVE),
    ("unplaced_first", UNPLACED_FIRST),
    ("unplaced_next", UNPLACED_NEXT),
    ("place_entry", PLACE_ENTRY),
    ("directories", DIRECTORIES),
    ("directory_sizes", DIRECTORY_SIZES),
    ("directory_path", DIRECTORY_PATH),
    ("jobs", JOBS),
    ("job_sizes", JOB_SIZES),
    ("job", JOB),
    ("complete_file", COMPLETE_FILE),
    ("file_roots", FILE_ROOTS),
    ("entries_first", ENTRIES_FIRST),
    ("entries_next", ENTRIES_NEXT),
    ("set_directory_root", SET_DIRECTORY_ROOT),
    ("discard_entries", DISCARD_ENTRIES),
    ("discard_native", DISCARD_NATIVE),
    ("discard_entry_keys", DISCARD_ENTRY_KEYS),
    ("discard_native_keys", DISCARD_NATIVE_KEYS),
    ("discard_entry_tail", DISCARD_ENTRY_TAIL),
    ("discard_native_tail", DISCARD_NATIVE_TAIL),
    ("abandoned", ABANDONED),
    ("release_abandoned", RELEASE_ABANDONED),
];

/// Highest numbered parameter a statement binds.
fn parameters(sql: &str) -> usize {
    sql.split('?')
        .skip(1)
        .filter_map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<usize>().ok()
        })
        .max()
        .unwrap_or(0)
}

/// The engine's query plan of every shipped statement on this database.
pub(crate) fn explain(
    tx: &crate::backend::Transaction<'_>,
) -> Result<Vec<(&'static str, Vec<String>)>, BackendError> {
    let mut plans = Vec::with_capacity(ALL.len());
    for (name, sql) in ALL {
        let bound = vec![crate::backend::records::Param::I64(1); parameters(sql)];
        let plan = tx
            .query(&format!("EXPLAIN QUERY PLAN {sql}"), bound)?
            .iter()
            .map(|row| row.get::<String>(3))
            .collect::<Result<Vec<_>, _>>()?;
        plans.push((name, plan));
    }
    Ok(plans)
}

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
