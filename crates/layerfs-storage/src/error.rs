use crate::{BranchId, CommitId, EntityName, LayerId, LayerStackId};
use layerfs_content::ObjectId;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug)]
pub enum StorageError {
    CommitHeadMoved {
        expected: Option<CommitId>,
        actual: Option<CommitId>,
    },
    LayerHeadMoved {
        expected: LayerId,
        actual: LayerId,
    },
    LayerStackNameConflict {
        name: EntityName,
        existing_id: LayerStackId,
        incoming_id: LayerStackId,
    },
    BranchNameConflict {
        layer_stack_id: LayerStackId,
        name: EntityName,
        existing_id: BranchId,
        incoming_id: BranchId,
    },
    ReadOnlyBranch(BranchId),
    MissingObject(ObjectId),
    Integrity(&'static str),
    StoreBusy,
    StoreAlreadyExists,
    StoreMissing,
    WrongStoreRole,
    WrongStoreSchema,
    WrongParent,
    Unavailable,
    NotFound(&'static str),
    InvalidInput(&'static str),
    Database(String),
    Io(std::io::Error),
    Core(layerfs_content::CoreError),
}

impl PartialEq for StorageError {
    fn eq(&self, other: &Self) -> bool {
        format!("{self:?}") == format!("{other:?}")
    }
}

impl Eq for StorageError {}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value.to_string())
    }
}

impl From<std::io::Error> for StorageError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<layerfs_content::CoreError> for StorageError {
    fn from(value: layerfs_content::CoreError) -> Self {
        match value {
            layerfs_content::CoreError::InvalidRecord(message) => Self::Integrity(message),
            layerfs_content::CoreError::ValidationAuthorityUnavailable => Self::Unavailable,
            error => Self::Core(error),
        }
    }
}
