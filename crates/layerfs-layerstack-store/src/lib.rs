#![forbid(unsafe_code)]

mod branch;
mod error;
mod ids;
mod layerstack;
mod objects;
mod query;
mod records;
mod schema;
mod statements;
mod store;
mod telemetry;
mod workspace;

pub use error::{Result, StoreError};
pub use ids::{BranchId, CommitId, LayerId, LayerStackId};
pub use objects::{
    apply_changes, apply_reconcile_choices, empty_root, reconcile_candidate,
    reconcile_candidate_with, BuildCounters, BuiltRoot, CandidateReconciliation, CanonicalObject,
    CoreReader, DeferredObjectStore, ObjectBuffer, ObjectSource, SpillableObjectSet,
    OBJECT_PAGE_BYTES, OBJECT_PAGE_COUNT,
};
pub use records::{
    AddLayerResult, BranchRecord, BranchRecordPage, CanonicalStorage, CommitRecord,
    CommitRecordPage, DiffRequest, EntityName, InitializeLayerStackResult, LayerRecord,
    LayerRecordPage, LayerStackInitialization, LayerStackRecord, LayerStackRecordPage,
    LocalForkSource, Page, StoreCounts, StoreStorageSnapshot, WorkspaceReadReceipt,
};
pub use store::LayerStackStore;
pub use telemetry::{
    begin_workspace_commit, note_workspace_capture, note_workspace_commit_edit_state,
    note_workspace_commit_phase, note_workspace_commit_reads, note_workspace_commit_tree_visits,
    note_workspace_create_snapshot, record_fuse_write, record_workspace_lifecycle,
    record_workspace_read, take_storage_receipts, CandidateReceipt, CaptureMode, FuseWriteReceipt,
    LayerStackInitializationReceipt, StorageReceipt, WorkspaceCommitPhase, WorkspaceCommitReceipt,
    WorkspaceCommitTimer, WorkspaceLifecycleKind, WorkspaceLifecycleReceipt,
};
pub use workspace::{
    CommitOutcome, PinnedSnapshot, PreparedReconciliation, SnapshotReader, WorkspaceLease,
};

pub use layerfs_content::filesystem::{
    DiffAspects, DiffEntry, NodeSummary, ReconcileChoice, ReconcileConflict, ReconcileConflictKind,
};

#[cfg(feature = "test-instrumentation")]
pub use objects::{read_batch_counters, reset_read_batch_counters, ReadBatchCounters};
#[cfg(feature = "test-instrumentation")]
pub use schema::{reset_sql_trace, sql_trace};

#[cfg(debug_assertions)]
#[doc(hidden)]
pub use schema::set_transaction_failure_at;
