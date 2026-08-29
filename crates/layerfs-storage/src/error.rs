use crate::{
    CommitId, HeadMoved, LayerHistoryId, LayerId, ReadOnlyHistory, StackHistoryId, StackId,
    WrongHistory,
};
use layerfs_content::filesystem::ContentConflict;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug)]
pub enum StorageError {
    CommitHeadMoved(HeadMoved<CommitId>),
    StackHeadMoved(HeadMoved<StackId>),
    LayerHeadMoved(HeadMoved<LayerId>),
    WrongStackHistory(WrongHistory<StackHistoryId>),
    WrongLayerHistory(WrongHistory<LayerHistoryId>),
    ReadOnlyStackHistory(ReadOnlyHistory<StackHistoryId>),
    WrongSourceRoute,
    NoCommonBase,
    AmbiguousMergeBase,
    MissingBaseData,
    Conflict(Box<ContentConflict>),
    Integrity(&'static str),
    StoreBusy,
    StoreAlreadyExists,
    StoreMissing,
    WrongStoreRole,
    WrongStoreSchema,
    WrongParent,
    ObservationUnavailable,
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
        Self::Core(value)
    }
}
