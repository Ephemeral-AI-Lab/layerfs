use crate::{LayerStackId, Result, StoreError, WorkspaceReadReceipt};
use std::cell::RefCell;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureMode {
    Live,
    Materialized,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CandidateReceipt {
    pub candidate_objects: u64,
    pub candidate_bytes: u64,
    pub inserted_objects: u64,
    pub inserted_bytes: u64,
    pub reused_objects: u64,
    pub reused_bytes: u64,
    pub batch_inserted_objects: u64,
    pub batch_inserted_bytes: u64,
    pub final_inserted_objects: u64,
    pub final_inserted_bytes: u64,
    pub preexisting_reused_objects: u64,
    pub preexisting_reused_bytes: u64,
    pub admission_transactions: u64,
    pub max_transaction_objects: u64,
    pub max_transaction_bytes: u64,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayerStackInitializationReceipt {
    pub layer_stack_id: LayerStackId,
    pub scanned_files: u64,
    pub scanned_bytes: u64,
}

impl CandidateReceipt {
    fn validate(self) -> Result<()> {
        self.validate_with_max_objects(crate::objects::ADMISSION_BATCH_COUNT as u64)
    }

    fn validate_with_max_objects(self, max_objects: u64) -> Result<()> {
        if self.candidate_objects != self.inserted_objects + self.reused_objects
            || self.candidate_bytes != self.inserted_bytes + self.reused_bytes
            || self.inserted_objects != self.batch_inserted_objects + self.final_inserted_objects
            || self.inserted_bytes != self.batch_inserted_bytes + self.final_inserted_bytes
            || self.reused_objects != self.preexisting_reused_objects
            || self.reused_bytes != self.preexisting_reused_bytes
            || self.max_transaction_objects > max_objects
            || self.max_transaction_bytes >= crate::objects::OBJECT_PAGE_BYTES as u64
        {
            return Err(StoreError::Integrity("candidate equation"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FuseWriteReceipt {
    pub max_write_bytes: u64,
    pub kernel_write_requests: u64,
    pub kernel_write_bytes: u64,
    pub kernel_write_le_4k: u64,
    pub kernel_write_le_64k: u64,
    pub kernel_write_le_256k: u64,
    pub kernel_write_le_1m: u64,
    pub kernel_write_gt_1m: u64,
    pub client_request_copy_bytes: u64,
    pub frame_payload_copy_bytes: u64,
    pub client_frame_bytes: u64,
    pub encode_ns: u64,
    pub socket_write_ns: u64,
    pub host_frame_bytes: u64,
    pub socket_read_ns: u64,
    pub decode_ns: u64,
    pub host_decode_copy_bytes: u64,
    pub host_dispatch_ns: u64,
    pub spool_write_bytes: u64,
    pub spool_write_open_count: u64,
    pub spool_write_ns: u64,
    pub workspace_fence_count: u64,
    pub workspace_fence_ns: u64,
    pub collection_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceCommitReceipt {
    pub total_ns: u64,
    pub pause_fence_ns: u64,
    pub quiesce_ns: u64,
    pub capture_ns: u64,
    pub capture_mode: Option<CaptureMode>,
    pub captured_files: u64,
    pub captured_bytes: u64,
    pub candidate_plan_ns: u64,
    pub dirty_compare_ns: u64,
    pub content_ns: u64,
    pub namespace_ns: u64,
    pub candidate_finish_ns: u64,
    pub local_admission_ns: u64,
    pub object_admission_ns: u64,
    pub object_admission_transactions: u64,
    pub max_admission_transaction_objects: u64,
    pub max_admission_transaction_bytes: u64,
    pub object_admission_begin_ns: u64,
    pub object_admission_insert_ns: u64,
    pub object_admission_commit_ns: u64,
    pub publication_ns: u64,
    pub publication_begin_ns: u64,
    pub publication_payload_ns: u64,
    pub publication_insert_ns: u64,
    pub publication_metadata_ns: u64,
    pub publication_commit_ns: u64,
    pub in_place_rebase_ns: u64,
    pub resume_ns: u64,
    pub unattributed_ns: u64,
    pub snapshot_database_calls: u64,
    pub snapshot_database_rows: u64,
    pub snapshot_database_bytes: u64,
    pub payload_bytes_read: u64,
    pub cdc_bytes_scanned: u64,
    pub edit_count: u64,
    pub edit_piece_count: u64,
    pub edit_piece_height: u64,
    pub edit_piece_logical_charge: u64,
    pub edit_spool_allocated_bytes: u64,
    pub edit_spool_live_bytes: u64,
    pub edit_spool_superseded_bytes: u64,
    pub edit_tree_visits: u64,
    pub edit_metric_nodes_scanned: u64,
}

impl WorkspaceCommitReceipt {
    fn attributed_ns(self) -> u64 {
        [
            self.pause_fence_ns,
            self.quiesce_ns,
            self.capture_ns,
            self.candidate_plan_ns,
            self.dirty_compare_ns,
            self.content_ns,
            self.namespace_ns,
            self.candidate_finish_ns,
            self.local_admission_ns,
            self.object_admission_ns,
            self.publication_ns,
            self.in_place_rebase_ns,
            self.resume_ns,
        ]
        .into_iter()
        .fold(0_u64, u64::saturating_add)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceCommitPhase {
    PauseFence,
    Quiesce,
    Capture,
    CandidatePlan,
    DirtyCompare,
    Content,
    Namespace,
    CandidateFinish,
    LocalAdmission,
    ObjectAdmission,
    Publication,
    InPlaceRebase,
    Resume,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WorkspaceLifecycleKind {
    #[default]
    Attach,
    End,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceLifecycleReceipt {
    pub kind: WorkspaceLifecycleKind,
    pub total_ns: u64,
    pub proxy_ns: u64,
    pub docker_setup_ns: u64,
    pub helper_copy_ns: u64,
    pub mount_ready_ns: u64,
    pub unmount_ns: u64,
    pub wait_ns: u64,
    pub cleanup_ns: u64,
    pub unattributed_ns: u64,
    pub docker_calls: u64,
    pub snapshot_database_calls: u64,
    pub snapshot_database_rows: u64,
    pub snapshot_database_bytes: u64,
    pub snapshot_cache_rows_at_create: u64,
    pub snapshot_cache_bytes_at_create: u64,
    pub snapshot_store_wide_scans: u64,
    pub small_file_prefetch_eligible: u64,
    pub small_file_prefetch_bytes: u64,
    pub anchor_prefetch_count: u64,
}

impl WorkspaceLifecycleReceipt {
    fn validate(self) -> Result<()> {
        if self.total_ns
            != self
                .proxy_ns
                .saturating_add(self.docker_setup_ns)
                .saturating_add(self.helper_copy_ns)
                .saturating_add(self.mount_ready_ns)
                .saturating_add(self.unmount_ns)
                .saturating_add(self.wait_ns)
                .saturating_add(self.cleanup_ns)
                .saturating_add(self.unattributed_ns)
        {
            return Err(StoreError::Integrity("Workspace lifecycle timing equation"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageReceipt {
    Candidate(CandidateReceipt),
    WorkspaceCommit(WorkspaceCommitReceipt),
    WorkspaceLifecycle(WorkspaceLifecycleReceipt),
    FuseWrite(FuseWriteReceipt),
    WorkspaceRead(WorkspaceReadReceipt),
}

thread_local! {
    static RECEIPTS: RefCell<Vec<StorageReceipt>> = const { RefCell::new(Vec::new()) };
    static LAYERSTACK_INITIALIZATIONS: RefCell<Vec<LayerStackInitializationReceipt>> = const { RefCell::new(Vec::new()) };
    static WORKSPACE_COMMIT: RefCell<Option<WorkspaceCommitReceipt>> = const { RefCell::new(None) };
}

pub struct WorkspaceCommitTimer(Instant);

impl Drop for WorkspaceCommitTimer {
    fn drop(&mut self) {
        WORKSPACE_COMMIT.with(|current| {
            let Some(mut receipt) = current.borrow_mut().take() else {
                return;
            };
            receipt.total_ns = elapsed_ns(self.0);
            receipt.unattributed_ns = receipt.total_ns.saturating_sub(receipt.attributed_ns());
            RECEIPTS.with(|receipts| {
                receipts
                    .borrow_mut()
                    .push(StorageReceipt::WorkspaceCommit(receipt));
            });
        });
    }
}

pub fn begin_workspace_commit(mode: CaptureMode) -> Result<WorkspaceCommitTimer> {
    WORKSPACE_COMMIT.with(|current| {
        let mut current = current.borrow_mut();
        if current.is_some() {
            return Err(StoreError::Integrity("nested Workspace Commit timing"));
        }
        *current = Some(WorkspaceCommitReceipt {
            capture_mode: Some(mode),
            ..WorkspaceCommitReceipt::default()
        });
        Ok(WorkspaceCommitTimer(Instant::now()))
    })
}

#[allow(clippy::too_many_arguments)]
pub fn note_workspace_commit_edit_state(
    edit_count: u64,
    piece_count: u64,
    piece_height: u64,
    piece_logical_charge: u64,
    spool_allocated_bytes: u64,
    spool_live_bytes: u64,
    spool_superseded_bytes: u64,
    metric_nodes_scanned: u64,
) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.edit_count = edit_count;
            receipt.edit_piece_count = piece_count;
            receipt.edit_piece_height = piece_height;
            receipt.edit_piece_logical_charge = piece_logical_charge;
            receipt.edit_spool_allocated_bytes = spool_allocated_bytes;
            receipt.edit_spool_live_bytes = spool_live_bytes;
            receipt.edit_spool_superseded_bytes = spool_superseded_bytes;
            receipt.edit_metric_nodes_scanned = metric_nodes_scanned;
        }
    });
}

pub fn note_workspace_commit_tree_visits(visits: u64) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.edit_tree_visits = receipt.edit_tree_visits.saturating_add(visits);
        }
    });
}

pub fn note_workspace_commit_phase(phase: WorkspaceCommitPhase, elapsed_ns: u64) {
    WORKSPACE_COMMIT.with(|current| {
        let mut current = current.borrow_mut();
        let Some(receipt) = current.as_mut() else {
            return;
        };
        let target = match phase {
            WorkspaceCommitPhase::PauseFence => &mut receipt.pause_fence_ns,
            WorkspaceCommitPhase::Quiesce => &mut receipt.quiesce_ns,
            WorkspaceCommitPhase::Capture => &mut receipt.capture_ns,
            WorkspaceCommitPhase::CandidatePlan => &mut receipt.candidate_plan_ns,
            WorkspaceCommitPhase::DirtyCompare => &mut receipt.dirty_compare_ns,
            WorkspaceCommitPhase::Content => &mut receipt.content_ns,
            WorkspaceCommitPhase::Namespace => &mut receipt.namespace_ns,
            WorkspaceCommitPhase::CandidateFinish => &mut receipt.candidate_finish_ns,
            WorkspaceCommitPhase::LocalAdmission => &mut receipt.local_admission_ns,
            WorkspaceCommitPhase::ObjectAdmission => &mut receipt.object_admission_ns,
            WorkspaceCommitPhase::Publication => &mut receipt.publication_ns,
            WorkspaceCommitPhase::InPlaceRebase => &mut receipt.in_place_rebase_ns,
            WorkspaceCommitPhase::Resume => &mut receipt.resume_ns,
        };
        *target = target.saturating_add(elapsed_ns);
    });
}

pub(crate) fn note_workspace_commit_cdc(bytes: u64) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.cdc_bytes_scanned = bytes;
        }
    });
}

pub(crate) fn note_workspace_admission(
    transactions: u64,
    max_transaction_objects: u64,
    max_transaction_bytes: u64,
    begin_ns: u64,
    insert_ns: u64,
    commit_ns: u64,
) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.object_admission_transactions = transactions;
            receipt.max_admission_transaction_objects = max_transaction_objects;
            receipt.max_admission_transaction_bytes = max_transaction_bytes;
            receipt.object_admission_begin_ns = begin_ns;
            receipt.object_admission_insert_ns = insert_ns;
            receipt.object_admission_commit_ns = commit_ns;
        }
    });
}

pub fn note_workspace_commit_reads(
    before: WorkspaceReadReceipt,
    after: WorkspaceReadReceipt,
) -> Result<()> {
    WORKSPACE_COMMIT.with(|current| {
        let mut current = current.borrow_mut();
        let receipt = current
            .as_mut()
            .ok_or(StoreError::Integrity("Workspace Commit timing"))?;
        receipt.snapshot_database_calls = after
            .snapshot_database_calls
            .checked_sub(before.snapshot_database_calls)
            .ok_or(StoreError::Integrity("Workspace Commit read metrics"))?;
        receipt.snapshot_database_rows = after
            .snapshot_database_rows
            .checked_sub(before.snapshot_database_rows)
            .ok_or(StoreError::Integrity("Workspace Commit read metrics"))?;
        receipt.snapshot_database_bytes = after
            .snapshot_database_bytes
            .checked_sub(before.snapshot_database_bytes)
            .ok_or(StoreError::Integrity("Workspace Commit read metrics"))?;
        receipt.payload_bytes_read = after
            .payload_bytes_read
            .checked_sub(before.payload_bytes_read)
            .ok_or(StoreError::Integrity("Workspace Commit read metrics"))?;
        Ok(())
    })
}

pub fn note_workspace_capture(captured_files: u64, captured_bytes: u64) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.captured_files = receipt.captured_files.saturating_add(captured_files);
            receipt.captured_bytes = receipt.captured_bytes.saturating_add(captured_bytes);
        }
    });
}

pub(crate) fn note_workspace_publication(
    begin_ns: u64,
    payload_ns: u64,
    insert_ns: u64,
    metadata_ns: u64,
    commit_ns: u64,
) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.publication_begin_ns = begin_ns;
            receipt.publication_payload_ns = payload_ns;
            receipt.publication_insert_ns = insert_ns;
            receipt.publication_metadata_ns = metadata_ns;
            receipt.publication_commit_ns = commit_ns;
        }
    });
}

pub fn record_workspace_lifecycle(receipt: WorkspaceLifecycleReceipt) -> Result<()> {
    receipt.validate()?;
    RECEIPTS.with(|receipts| {
        receipts
            .borrow_mut()
            .push(StorageReceipt::WorkspaceLifecycle(receipt));
    });
    Ok(())
}

pub fn note_workspace_create_snapshot(
    read: WorkspaceReadReceipt,
    cache_rows: u64,
    cache_bytes: u64,
) -> Result<()> {
    RECEIPTS.with(|receipts| {
        let mut receipts = receipts.borrow_mut();
        let lifecycle = receipts
            .iter_mut()
            .rev()
            .find_map(|receipt| match receipt {
                StorageReceipt::WorkspaceLifecycle(receipt)
                    if receipt.kind == WorkspaceLifecycleKind::Attach =>
                {
                    Some(receipt)
                }
                _ => None,
            })
            .ok_or(StoreError::Integrity("Workspace Create lifecycle receipt"))?;
        lifecycle.snapshot_database_calls = read.snapshot_database_calls;
        lifecycle.snapshot_database_rows = read.snapshot_database_rows;
        lifecycle.snapshot_database_bytes = read.snapshot_database_bytes;
        lifecycle.snapshot_cache_rows_at_create = cache_rows;
        lifecycle.snapshot_cache_bytes_at_create = cache_bytes;
        Ok(())
    })
}

pub(crate) fn record_candidate(receipt: CandidateReceipt) -> Result<()> {
    receipt.validate()?;
    RECEIPTS.with(|receipts| {
        receipts
            .borrow_mut()
            .push(StorageReceipt::Candidate(receipt));
    });
    Ok(())
}

pub(crate) fn record_initialization_candidate(receipt: CandidateReceipt) -> Result<()> {
    receipt
        .validate_with_max_objects(crate::objects::INITIALIZATION_ADMISSION_BATCH_COUNT as u64)?;
    RECEIPTS.with(|receipts| {
        receipts
            .borrow_mut()
            .push(StorageReceipt::Candidate(receipt));
    });
    Ok(())
}

pub(crate) fn record_layerstack_initialization(receipt: LayerStackInitializationReceipt) {
    LAYERSTACK_INITIALIZATIONS.with(|receipts| receipts.borrow_mut().push(receipt));
}

pub fn record_fuse_write(receipt: FuseWriteReceipt) -> Result<()> {
    RECEIPTS.with(|receipts| {
        receipts
            .borrow_mut()
            .push(StorageReceipt::FuseWrite(receipt));
    });
    Ok(())
}

pub fn record_workspace_read(receipt: WorkspaceReadReceipt) -> Result<()> {
    RECEIPTS.with(|receipts| {
        receipts
            .borrow_mut()
            .push(StorageReceipt::WorkspaceRead(receipt));
    });
    Ok(())
}

pub fn take_storage_receipts() -> Vec<StorageReceipt> {
    RECEIPTS.with(|receipts| std::mem::take(&mut *receipts.borrow_mut()))
}

pub(crate) fn take_layerstack_initialization_receipts() -> Vec<LayerStackInitializationReceipt> {
    LAYERSTACK_INITIALIZATIONS.with(|receipts| std::mem::take(&mut *receipts.borrow_mut()))
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}
