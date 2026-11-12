//! Why an export, apply or change to an export base was refused or failed.
use layerfs_storage::port::PersistenceError;
use std::{error::Error, fmt, io};

/// Why an export, apply or change to an export base was refused or failed.
#[derive(Debug)]
pub enum ChangeError {
    /// The Store is not an export base: it records no change state.
    NotBase,
    /// Another writer holds the Store: its writer lane, or SQLite's write lock
    /// for a connection that bypasses the lane. One attempt was made and
    /// nothing was changed.
    Busy,
    /// The cursor does not name a state this Store or bundle can use.
    Cursor(&'static str),
    /// The replica was written after its recorded state.
    Diverged,
    /// The bundle was exported from a Store whose schema differs from the
    /// replica's: another schema version, layout, migration or table
    /// definition. Nothing was installed.
    Schema,
    /// The Store or bundle is not consistent.
    Integrity(&'static str),
    /// The bundle path already exists.
    Exists,
    /// The Store's own code refused it: its connection settings (a Store not
    /// in WAL mode is `Malformed`, another connection can make it `Busy`),
    /// its writer lane (the Store's directory could not be opened and locked)
    /// or its schema definition.
    Store(PersistenceError),
    /// Unlike every other variant, the change committed. SQLite then did not
    /// copy the write-ahead log into the Store file: another connection was
    /// reading the Store, or the checkpoint failed (the source). Until a later
    /// checkpoint, the Store file holds the change only together with its
    /// write-ahead log.
    NotCheckpointed(Option<rusqlite::Error>),
    /// SQLite failed.
    Sqlite(rusqlite::Error),
    /// The filesystem failed.
    Io(io::Error),
}

impl fmt::Display for ChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotBase => write!(f, "the Store is not an export base"),
            Self::Busy => write!(f, "another writer holds the Store"),
            Self::Cursor(why) => write!(f, "cursor refused: {why}"),
            Self::Diverged => write!(f, "the replica was written after its recorded state"),
            Self::Schema => write!(f, "the bundle's Store and the replica differ in schema"),
            Self::Integrity(why) => write!(f, "integrity: {why}"),
            Self::Exists => write!(f, "the bundle path already exists"),
            Self::Store(_) => write!(f, "the Store refused the connection"),
            Self::NotCheckpointed(_) => write!(
                f,
                "the change committed, but the Store file holds it only with its write-ahead log"
            ),
            Self::Sqlite(_) => write!(f, "SQLite failed"),
            Self::Io(_) => write!(f, "the filesystem failed"),
        }
    }
}

impl Error for ChangeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::NotCheckpointed(Some(error)) | Self::Sqlite(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for ChangeError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<io::Error> for ChangeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
