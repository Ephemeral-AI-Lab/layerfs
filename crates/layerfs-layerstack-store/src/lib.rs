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
    BuildCounters, BuiltRoot, CandidateCleanup, CandidateReconciliation, CanonicalObject,
    CoreReader, DeferredObjectStore, OBJECT_PAGE_BYTES, OBJECT_PAGE_COUNT, ObjectBuffer,
    ObjectSource, SpillableObjectSet, apply_changes, apply_reconcile_choices, empty_root,
    reconcile_candidate, reconcile_candidate_with,
};
pub use records::{
    AddLayerResult, BranchRecord, BranchRecordPage, CanonicalStorage, CommitRecord,
    CommitRecordPage, DiffRequest, EntityName, InitializeLayerStackResult, LayerRecord,
    LayerRecordPage, LayerStackInitialization, LayerStackRecord, LayerStackRecordPage,
    LocalForkSource, Page, StoreCounts, StoreStorageSnapshot, WorkspaceReadReceipt,
};
pub use store::LayerStackStore;
pub use telemetry::{
    CandidateReceipt, CaptureMode, FuseWriteReceipt, LayerStackInitializationReceipt,
    StorageReceipt, WorkspaceCommitDiagnostics, WorkspaceCommitDiagnosticsGuard,
    WorkspaceCommitPhase, WorkspaceCommitReceipt, WorkspaceCommitTimer, WorkspaceLifecycleKind,
    WorkspaceLifecycleReceipt, begin_workspace_commit, capture_workspace_commit_diagnostics,
    note_workspace_capture, note_workspace_commit_edit_state, note_workspace_commit_phase,
    note_workspace_commit_reads, note_workspace_commit_sort, note_workspace_commit_tracking,
    note_workspace_commit_tree_visits, note_workspace_create_snapshot,
    note_workspace_generic_commit, note_workspace_handoff, note_workspace_namespace_visits,
    note_workspace_physical_spool, record_fuse_write, record_workspace_lifecycle,
    record_workspace_read, take_storage_receipts, take_workspace_commit_diagnostics,
};
pub use workspace::{
    CommitOutcome, PinnedSnapshot, PreparedReconciliation, SnapshotReader, WorkspaceLease,
};

pub use layerfs_content::filesystem::{
    DiffAspects, DiffEntry, NodeSummary, ReconcileChoice, ReconcileConflict, ReconcileConflictKind,
};

#[cfg(feature = "test-instrumentation")]
pub use objects::{ReadBatchCounters, read_batch_counters, reset_read_batch_counters};
#[cfg(feature = "test-instrumentation")]
pub use schema::{
    VerificationStoreFault, VerificationStoreFaultReceipt, arm_verification_store_fault,
    take_verification_store_fault_receipt,
};
#[cfg(feature = "test-instrumentation")]
pub use schema::{reset_sql_trace, sql_trace};

#[cfg(debug_assertions)]
#[doc(hidden)]
pub use schema::set_transaction_failure_at;

#[doc(hidden)]
pub use telemetry::note_workspace_base_inode_reads;

#[doc(hidden)]
pub use telemetry::note_workspace_tree_spill;
