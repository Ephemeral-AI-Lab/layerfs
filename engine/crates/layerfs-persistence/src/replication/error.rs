//! Why an export, apply or base mark was refused or failed.
use layerfs_storage::port::PersistenceError;
use std::{error::Error, fmt, io};

/// Why an export, apply or base mark was refused or failed.
#[derive(Debug)]
pub enum ChangeError {
    /// The Store is not an export base: it records no change state.
    NotBase,
    /// The cursor does not name a state this Store or bundle can use.
    Cursor(&'static str),
    /// The replica was written after its recorded state.
    Diverged,
    /// The Store or bundle is not consistent.
    Integrity(&'static str),
    /// The bundle path already exists.
    Exists,
    /// The Store refused its connection settings: it is not in WAL mode
    /// (`Malformed`), another connection holds it (`Busy`), or SQLite refused
    /// a setting.
    Profile(PersistenceError),
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
            Self::Cursor(why) => write!(f, "cursor refused: {why}"),
            Self::Diverged => write!(f, "the replica was written after its recorded state"),
            Self::Integrity(why) => write!(f, "integrity: {why}"),
            Self::Exists => write!(f, "the bundle path already exists"),
            Self::Profile(_) => write!(f, "the Store refused its connection settings"),
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
            Self::Profile(error) => Some(error),
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
