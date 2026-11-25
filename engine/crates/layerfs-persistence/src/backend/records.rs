//! Owned bounded scalar inputs/results, independent of database engines.
#[derive(Clone, Debug)]
pub(crate) enum Param {
    I64(i64),
    Text(String),
    Bytes(Vec<u8>),
    OptionalBytes(Option<Vec<u8>>),
    OptionalText(Option<String>),
}
#[derive(Clone, Debug)]
pub(crate) enum Value {
    Null,
    Integer(i64),
    Text(String),
    Bytes(Vec<u8>),
}
pub(crate) struct Record {
    pub(crate) cells: Vec<Value>,
}
pub(crate) trait FromValue: Sized {
    fn from_value(value: &Value) -> Option<Self>;
}
impl FromValue for i64 {
    fn from_value(v: &Value) -> Option<Self> {
        if let Value::Integer(v) = v {
            Some(*v)
        } else {
            None
        }
    }
}
impl FromValue for Vec<u8> {
    fn from_value(v: &Value) -> Option<Self> {
        if let Value::Bytes(v) = v {
            Some(v.clone())
        } else {
            None
        }
    }
}
impl FromValue for String {
    fn from_value(v: &Value) -> Option<Self> {
        if let Value::Text(v) = v {
            Some(v.clone())
        } else {
            None
        }
    }
}
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Null => Some(None),
            _ => T::from_value(v).map(Some),
        }
    }
}
impl Record {
    pub(crate) fn take_bytes(&mut self, index: usize) -> Result<Vec<u8>, BackendError> {
        match self
            .cells
            .get_mut(index)
            .map(|v| std::mem::replace(v, Value::Null))
        {
            Some(Value::Bytes(v)) => Ok(v),
            _ => Err(BackendError::Integrity),
        }
    }
    pub(crate) fn get<T: FromValue>(&self, index: usize) -> Result<T, BackendError> {
        self.cells
            .get(index)
            .and_then(T::from_value)
            .ok_or(BackendError::Integrity)
    }
}
use layerfs_storage::{
    location::{PackDomain, PackInfo},
    policy,
    port::ObjectKey,
};
/// One checked pack descriptor from its stored columns, on either engine.
pub(crate) fn pack_info(
    pack_id: i64,
    domain: i64,
    digest: &[u8],
    length: i64,
) -> Result<PackInfo, BackendError> {
    let domain = match domain {
        0 => PackDomain::Metadata,
        1 => PackDomain::Payload,
        _ => return Err(BackendError::Integrity),
    };
    let digest = digest.try_into().map_err(|_| BackendError::Integrity)?;
    let length = usize::try_from(length).map_err(|_| BackendError::Integrity)?;
    if pack_id <= 0
        || !(32..=policy::SINGLETON_PACK_LIMIT).contains(&length)
        || domain == PackDomain::Metadata && length > policy::PACK_LIMIT
    {
        return Err(BackendError::Integrity);
    }
    Ok(PackInfo {
        pack_id,
        domain,
        key: ObjectKey::from_bytes(digest),
        length,
    })
}
/// The typed failure of one backend call, shared by both engines.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendError {
    /// A lock or session was held; the call had no effect.
    Busy,
    /// The Store was closed while the call waited for, or before, its turn.
    Closed,
    /// The server answered a new connection with an error (no such
    /// database, failed authentication): no session, so nothing ran.
    Rejected,
    /// A signal interrupted the cross-process writer wait (Linux or macOS).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Cancelled,
    /// The Store or its session is read-only; the call had no effect.
    ReadOnly,
    /// §10: the Store path no longer names the file this handle opened.
    Stale,
    /// The Store definitely ran out of a resource; the call had no effect.
    Capacity,
    /// The reply or stored layout violated a checked invariant.
    Integrity,
    /// A SQLite Store of schema version 1 (before #91): refused at open with
    /// the offline conversion named, never misread.
    SchemaVersion1,
    /// A SQLite Store of schema version 2 (before #776's Commit attempt):
    /// refused at open by name, never converted.
    SchemaVersion2,
    /// A PostgreSQL Store of schema version 2, written before 2.0's ancestor
    /// lists and pack directories (#758): refused at open, never converted.
    PostgresSchema2,
    /// A PostgreSQL Store of schema version 3, written before each Commit
    /// recorded its attempt (#776 R1): refused at open, never converted.
    PostgresSchema3,
    /// Stored SQLite layout is not implemented; this is not a corruption verdict.
    UnsupportedSchemaVersion(i64),
    /// The outcome is not proven: the failure may have committed.
    Unknown,
}
/// The refusal a schema version 1 SQLite Store gets at open.
pub(crate) const SCHEMA_VERSION_1: &str =
    "schema version 1 Store (one row per inode value and binding hint, before #91): \
convert a closed copy offline with tools/store_blocks.py migrate";
/// The refusal a schema version 2 SQLite Store gets at open (#776 R1).
pub(crate) const SCHEMA_VERSION_2: &str =
    "SQLite Store schema version 2 (before each Commit recorded its attempt, #776): \
not supported by this release and not migrated; create a new Store";
/// The refusal a PostgreSQL Store of schema version 2 gets at open (#758 D5).
pub(crate) const POSTGRES_SCHEMA_2: &str =
    "PostgreSQL Store schema version 2 (before 2.0's ancestor lists and pack directories, #758): \
not supported by this release and not migrated; create a new Store";
/// The refusal a PostgreSQL Store of schema version 3 gets at open (#776 R1).
pub(crate) const POSTGRES_SCHEMA_3: &str =
    "PostgreSQL Store schema version 3 (before each Commit recorded its attempt, #776): \
not supported by this release and not migrated; create a new Store";
/// The refusals of a Store of an older schema version that this release
/// neither reads nor migrates: its remedy is a new Store.
const OUTDATED: [&str; 3] = [SCHEMA_VERSION_2, POSTGRES_SCHEMA_2, POSTGRES_SCHEMA_3];
/// The open refusal of an older-format Store that `text` carries, if any:
/// one of the exact statuses such a Store is refused with at open
/// ([`OUTDATED`]), as it is or inside the text of an error that wraps it.
pub fn outdated_store(text: &str) -> Option<&'static str> {
    OUTDATED.into_iter().find(|status| text.contains(status))
}
/// Whether older-format refusal `text` names a Store `store upgrade` brings
/// to the latest format (SQLite 2, PostgreSQL 3), as against one whose
/// remedy is `store remove` / `store disconnect` (#804).
pub fn upgradable_store(text: &str) -> bool {
    [SCHEMA_VERSION_2, POSTGRES_SCHEMA_3]
        .into_iter()
        .any(|status| text.contains(status))
}
impl From<BackendError> for layerfs_storage::port::PersistenceError {
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::Unknown => Self::Uncertain,
            BackendError::Busy => Self::Busy,
            BackendError::Integrity => Self::Malformed,
            BackendError::SchemaVersion1 => Self::Refused {
                status: SCHEMA_VERSION_1.to_owned(),
            },
            BackendError::SchemaVersion2 => Self::Refused {
                status: SCHEMA_VERSION_2.to_owned(),
            },
            BackendError::PostgresSchema2 => Self::Refused {
                status: POSTGRES_SCHEMA_2.to_owned(),
            },
            BackendError::PostgresSchema3 => Self::Refused {
                status: POSTGRES_SCHEMA_3.to_owned(),
            },
            BackendError::UnsupportedSchemaVersion(version) => Self::Refused {
                status: format!("unsupported SQLite Store schema version {version}; supported versions are 56/57/58 and 62/63/64; no migration performed"),
            },
            // §10: the definite out-of-space refusal, named explicitly so a
            // new variant cannot silently inherit a capacity verdict.
            BackendError::Capacity => Self::Refused {
                status: "Capacity".to_owned(),
            },
            _ => Self::Refused {
                status: format!("{e:?}"),
            },
        }
    }
}
impl From<BackendError> for layerfs_history::HistoryError {
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::Busy => Self::Busy,
            // The writer was never obtained; nothing was attempted.
            BackendError::Closed | BackendError::Rejected | BackendError::Cancelled => {
                Self::OwnershipUnavailable
            }
            BackendError::ReadOnly => Self::ContinuityUnavailable,
            // §10: the file was replaced or moved under this handle.
            BackendError::Stale => Self::StaleStore,
            BackendError::Capacity => Self::Capacity("persistence"),
            BackendError::Integrity => Self::Integrity("persistence"),
            BackendError::SchemaVersion1 => Self::Integrity(SCHEMA_VERSION_1),
            BackendError::SchemaVersion2 => Self::Integrity(SCHEMA_VERSION_2),
            BackendError::PostgresSchema2 => Self::Integrity(POSTGRES_SCHEMA_2),
            BackendError::PostgresSchema3 => Self::Integrity(POSTGRES_SCHEMA_3),
            BackendError::UnsupportedSchemaVersion(_) => {
                Self::Unsupported("SQLite Store schema version")
            }
            BackendError::Unknown => Self::UnknownOutcome,
        }
    }
}

/// Recognizes uncertain outcomes without inspecting error text or guessing rollback.
pub(crate) trait OutcomeError {
    fn uncertain(&self) -> bool;
}
impl OutcomeError for BackendError {
    fn uncertain(&self) -> bool {
        *self == Self::Unknown
    }
}
impl OutcomeError for layerfs_storage::port::PersistenceError {
    fn uncertain(&self) -> bool {
        matches!(self, Self::Uncertain)
    }
}
impl OutcomeError for layerfs_history::HistoryError {
    fn uncertain(&self) -> bool {
        self.unknown()
    }
}
