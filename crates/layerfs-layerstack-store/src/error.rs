use crate::{BranchId, CommitId, EntityName, LayerId, LayerStackId};
use layerfs_content::ObjectId;

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug)]
pub enum StoreError {
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
    MissingObject(ObjectId),
    Integrity(&'static str),
    StoreBusy,
    StoreAlreadyExists,
    StoreMissing,
    WrongStoreSchema,
    NotFound(&'static str),
    InvalidInput(&'static str),
    Database(String),
    Io(std::io::Error),
    Core(layerfs_content::CoreError),
}

impl PartialEq for StoreError {
    fn eq(&self, other: &Self) -> bool {
        format!("{self:?}") == format!("{other:?}")
    }
}

impl Eq for StoreError {}

impl std::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        match value {
            rusqlite::Error::SqliteFailure(error, _)
                if matches!(
                    error.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                ) =>
            {
                Self::StoreBusy
            }
            rusqlite::Error::FromSqlConversionFailure(_, _, source) => {
                match source.downcast::<StoreError>() {
                    Ok(error) => *error,
                    Err(source) => Self::Database(source.to_string()),
                }
            }
            error => Self::Database(error.to_string()),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<layerfs_content::CoreError> for StoreError {
    fn from(value: layerfs_content::CoreError) -> Self {
        match value {
            layerfs_content::CoreError::InvalidRecord(message) => Self::Integrity(message),
            layerfs_content::CoreError::IdentityMismatch => Self::Integrity("object identity"),
            layerfs_content::CoreError::MissingObject => Self::Integrity("visible object missing"),
            error => Self::Core(error),
        }
    }
}
