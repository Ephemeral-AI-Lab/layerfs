#![forbid(unsafe_code)]

mod client;
mod connection;
mod query;
mod request;
mod result;
mod topology;

pub use client::Client;
pub use connection::{ConnectionContext, LayerStackEndpoint};
pub use layerfs_branch_store::BranchStore;
pub use layerfs_layerstack_store::LayerStackStore;
pub use layerfs_monitor::{
    DedupAnalysis, ExactOrUnavailable, LocalCasAnalysis, MonitorError, MonitorSnapshot,
    OperationFamily, OperationReceipt, PlacementAnalysis, SemanticOperation, TransferAnalysis,
};
pub use layerfs_storage::{
    BranchFact, BranchId, BranchRecord, BranchScope, BranchScopeRecord, CommitId, CommitRecord,
    DiffAspects, DiffEntry, DiffRequest, EntityName, Fact, FactKind, InitializeLayerStackResult,
    LayerId, LayerRecord, LayerStackFact, LayerStackId, LayerStackInitialization, LayerStackRecord,
    LayerStackScopeRecord, LocalForkSource, NodeSummary, PullBranchResult, PullLayerResult,
    PushResult, RemotePlacement, ServingMode, StorageError, StoreId,
};
pub use layerfs_workspace::{
    ConflictCursor, ConflictId, ConflictKind, ConflictPage, ContainerId, CreateWorkspaceSession,
    EndWorkspaceMode, ExecutionEvent, ExecutionId, ExecutionReceipt, ExecutionSummary, NonEmpty,
    OutputChunk, OutputPage, OutputReader, OutputStream, ResolveChoice, ResolveResult,
    WorkspaceCommitResult, WorkspaceConflict, WorkspaceDetail, WorkspaceEndResult, WorkspaceError,
    WorkspaceExecution, WorkspaceId, WorkspacePlacement, WorkspaceProjection, WorkspaceState,
    WorkspaceSummary,
};
pub use query::{Query, QueryItem, QueryKind, QueryPage, WorkspaceQueryItem};
pub use request::{DiffPage, OperationHandle};
pub use result::{AddLayerResult, Result, SdkError};
