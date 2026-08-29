#![forbid(unsafe_code)]

mod binding;
mod direct;
mod endpoint;
mod stacked;

pub use direct::Direct;
pub use endpoint::RemoteEndpoint;
pub use layerfs_storage_core::{
    AddLayerSource, AddResult, BranchId, BranchRecord, Change, CommitId, LayerHistoryId,
    LayerHistoryRecord, LayerId, LayerRecord, MergeOutcome, RefOutcome, Result, StackHistoryId,
    StackHistoryRecord, StackId, StackRecord, StorageError,
};
pub use layerfs_workspace::{
    Attr, Kind, NodeId, ReadPlan, ResourcePolicy, Workspace, WorkspaceState, ROOT,
};
pub use stacked::Stacked;
