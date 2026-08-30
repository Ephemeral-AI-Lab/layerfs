#[derive(Debug)]
pub enum SdkError {
    Storage(layerfs_storage::StorageError),
    Workspace(layerfs_workspace::WorkspaceError),
    Monitor(layerfs_monitor::MonitorError),
    InvalidContext,
    InvalidRequest(&'static str),
}

pub type Result<T> = std::result::Result<T, SdkError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddLayerResult {
    Added {
        layer_id: layerfs_storage::LayerId,
    },
    UpToDate {
        layer_id: layerfs_storage::LayerId,
    },
    NoChanges {
        head_layer_id: layerfs_storage::LayerId,
    },
    NotPushed {
        branch_id: layerfs_storage::BranchId,
        head_commit_id: layerfs_storage::CommitId,
    },
    LayerNotPulled {
        layer_id: layerfs_storage::LayerId,
    },
    NeedsResolution {
        workspace_id: layerfs_workspace::WorkspaceId,
        old_base_layer_id: layerfs_storage::LayerId,
        current_layer_id: layerfs_storage::LayerId,
        conflict_count: u64,
    },
    HeadMoved {
        expected: layerfs_storage::LayerId,
        actual: layerfs_storage::LayerId,
    },
}

impl std::fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SdkError {}

impl From<layerfs_storage::StorageError> for SdkError {
    fn from(value: layerfs_storage::StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<layerfs_workspace::WorkspaceError> for SdkError {
    fn from(value: layerfs_workspace::WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<layerfs_monitor::MonitorError> for SdkError {
    fn from(value: layerfs_monitor::MonitorError) -> Self {
        Self::Monitor(value)
    }
}
