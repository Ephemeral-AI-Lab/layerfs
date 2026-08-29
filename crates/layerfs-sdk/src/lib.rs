#![forbid(unsafe_code)]

mod branch;
mod client;
mod connection;
mod layer;
mod location;
mod result;
mod stack;
mod topology;

pub use client::{Client, FactId, Query, QueryCursor, QueryPage, QueryResult, QueryScope};
pub use connection::{
    BranchConnection, BranchConnectionId, BranchParent, ConnectionContext, LayerConnection,
    LayerConnectionId, StackConnection, StackConnectionId,
};
pub use layerfs_branch_store::BranchStore;
pub use layerfs_layer_store::LayerStore;
pub use layerfs_monitor::{
    BranchStoreId, CliInvocationReceipt, DedupAnalysis, Monitor, MonitorError, MonitorScope,
    MonitorSnapshot, MonitoredRoute, OperationId, OperationOutcome, OperationReceipt,
    TimingFragment,
};
pub use layerfs_stack_store::StackStore;
pub use layerfs_storage::{
    AddResult, AddResultRecord, AdmissionSetReceipt, BaseId, BranchCommit, BranchId, BranchRecord,
    BranchSource, CommitId, CommitRecord, CreatedStack, DatabaseReceipt, Fact, FactKind,
    InitializedLayer, LayerHistoryId, LayerHistoryRecord, LayerId, LayerInitialization,
    LayerRecord, LayerSource, LocalAdmissionReceipt, LocalObjectReceipt, MergeOutcome,
    ObjectTransferReceipt, PulledBranch, RefOutcome, Result, ResultId, SourceId, StackHistoryId,
    StackHistoryRecord, StackId, StackRecord, StorageError, StorageReceipt, TransferReceipt,
    TransferSetReceipt, TransportReceipt,
};
pub use layerfs_workspace::{
    ContainerId, CreateWorkspaceSession, EndWorkspaceMode, ExecutionEvent, ExecutionId,
    ExecutionReceipt, ExecutionSummary, NonEmpty, OutputChunk, OutputPage, OutputReader,
    OutputStream, WorkspaceCommitResult, WorkspaceDetail, WorkspaceDiff, WorkspaceEndResult,
    WorkspaceError, WorkspaceExecution, WorkspacePlacement, WorkspaceProjection, WorkspaceResult,
    WorkspaceSession, WorkspaceSessionId, WorkspaceState, WorkspaceSummary, Workspaces,
};
pub use location::StoreLocation;
pub use result::SdkError;
