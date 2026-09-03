#![forbid(unsafe_code)]

mod client;
mod query;
mod request;
mod result;

pub use client::Client;
pub use query::{Query, QueryItem, QueryKind, QueryPage, WorkspaceQueryItem};
pub use request::{DiffPage, OperationHandle};
pub use result::{Result, SdkError};

pub use layerfs_layerstack_store::take_workspace_commit_diagnostics;
pub use layerfs_layerstack_store::{
    AddLayerResult, BranchId, BranchRecord, BuildCounters, CandidateReceipt, CanonicalStorage,
    CommitId, CommitRecord, DiffAspects, DiffEntry, DiffRequest, EntityName, FuseWriteReceipt,
    InitializeLayerStackResult, LayerId, LayerRecord, LayerStackId, LayerStackInitialization,
    LayerStackInitializationReceipt, LayerStackRecord, LayerStackStore, LocalForkSource,
    NodeSummary, ObjectSource, ReconcileChoice, ReconcileConflict, ReconcileConflictKind,
    StorageReceipt, StoreCounts, StoreError, WorkspaceCommitDiagnostics, WorkspaceCommitReceipt,
    WorkspaceLifecycleKind, WorkspaceLifecycleReceipt,
};
pub use layerfs_monitor::{
    CandidateStats, CandidateTotals, DatabaseSnapshot, DedupAnalysis, MonitorError,
    MonitorSnapshot, OperationFamily, OperationId, OperationOutcome, OperationReceipt,
    SemanticOperation, TimingFragment,
};
pub use layerfs_workspace::{
    ConflictCursor, ConflictId, ConflictKind, ConflictPage, ContainerBinding, ContainerCreate,
    ContainerError, ContainerId, ContainerLimits, ContainerManager, ContainerResult,
    ContainerStatus, CreateWorkspaceSession, CreatedContainer, DaemonTiming, EndWorkspaceMode,
    ExecutionEvent, ExecutionId, ExecutionReceipt, ExecutionSummary, ExecutionTransport, NonEmpty,
    OutputChunk, OutputPage, OutputReader, OutputStream, ResolveChoice, ResolveResult,
    RunningContainer, WorkspaceCommitResult, WorkspaceCommitStatus, WorkspaceConflict,
    WorkspaceDetail, WorkspaceEndResult, WorkspaceError, WorkspaceExecution,
    WorkspaceFileRangeEdit, WorkspaceFileReplacement, WorkspaceId, WorkspacePlacement,
    WorkspaceProjection, WorkspaceState, WorkspaceSummary,
};
