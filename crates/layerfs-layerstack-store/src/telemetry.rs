use crate::{LayerStackId, Result, StoreError, WorkspaceReadReceipt};
use std::cell::RefCell;
use std::time::Instant;

// Store-local counters include admission workers and mounted host reads. Snapshots
// describe the shared Store interval, not exclusive attribution under concurrency.
// group_fetches includes header/directory extraction rejected by an optional budget.
// encoded_read_bytes counts group payload (including record framing), excluding
// outer pack headers/directories; blob_ranges includes those outer SQL ranges.
// decoded_read_bytes counts full decoded groups including framing. base_fetches
// counts distinct anchors per reader wave plus predecessor-search base fetches.
// full_selected/delta_selected count finally admitted representations; alternative
// byte/call/time counters also include work whose prepared output loses a race.
// Matching counters describe optional work, and *_ns costs are nested in existing
// operation/phase times. These counters must not be added to attributed_ns.
macro_rules! physical_storage_receipt {
    ($($field:ident),+ $(,)?) => {
        #[doc(hidden)]
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
        pub struct PhysicalStorageReceipt { $(pub $field: u64,)+ }

        impl PhysicalStorageReceipt {
            pub fn since(self, before: Self) -> Self {
                Self { $($field: if matches!(stringify!($field), "diag_selected_pack_last_id" | "diag_invalid") { self.$field } else { self.$field.saturating_sub(before.$field) },)+ }
            }
        }

        #[derive(Default)]
        pub(crate) struct PhysicalStorageCounters {
            $($field: std::sync::atomic::AtomicU64,)+
        }

        impl PhysicalStorageCounters {
            pub(crate) fn note(&self, receipt: PhysicalStorageReceipt) {
                $(if receipt.$field != 0 {
                    if stringify!($field) == "diag_selected_pack_last_id" {
                        self.$field.fetch_max(receipt.$field, std::sync::atomic::Ordering::Relaxed);
                    } else if stringify!($field).starts_with("diag_") {
                        if self.$field.fetch_update(std::sync::atomic::Ordering::Relaxed, std::sync::atomic::Ordering::Relaxed,
                            |old| old.checked_add(receipt.$field)).is_err() {
                            self.diag_invalid.store(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    } else {
                        self.$field.fetch_add(receipt.$field, std::sync::atomic::Ordering::Relaxed);
                    }
                })+
            }
            pub(crate) fn snapshot(&self) -> PhysicalStorageReceipt {
                PhysicalStorageReceipt {
                    $($field: self.$field.load(std::sync::atomic::Ordering::Relaxed),)+
                }
            }
        }
    };
}

physical_storage_receipt!(
    diag_invalid,
    diag_limit_memory_first_empty_count,
    diag_limit_memory_first_empty_bytes,
    diag_limit_memory_inherited_empty_count,
    diag_limit_memory_inherited_empty_bytes,
    diag_limit_file_first_empty_count,
    diag_limit_file_first_empty_bytes,
    diag_limit_file_inherited_empty_count,
    diag_limit_file_inherited_empty_bytes,
    diag_limit_operation_first_empty_count,
    diag_limit_operation_first_empty_bytes,
    diag_limit_operation_inherited_empty_count,
    diag_limit_operation_inherited_empty_bytes,
    diag_limit_descriptor_first_empty_count,
    diag_limit_descriptor_first_empty_bytes,
    diag_limit_descriptor_inherited_empty_count,
    diag_limit_descriptor_inherited_empty_bytes,
    diag_limit_memory_first_count,
    diag_limit_memory_first_bytes,
    diag_limit_memory_inherited_count,
    diag_limit_memory_inherited_bytes,
    diag_limit_file_first_count,
    diag_limit_file_first_bytes,
    diag_limit_file_inherited_count,
    diag_limit_file_inherited_bytes,
    diag_limit_operation_first_count,
    diag_limit_operation_first_bytes,
    diag_limit_operation_inherited_count,
    diag_limit_operation_inherited_bytes,
    diag_limit_descriptor_first_count,
    diag_limit_descriptor_first_bytes,
    diag_limit_descriptor_inherited_count,
    diag_limit_descriptor_inherited_bytes,
    diag_hints_0_count,
    diag_hints_0_bytes,
    diag_hints_1_count,
    diag_hints_1_bytes,
    diag_hints_2_count,
    diag_hints_2_bytes,
    diag_hints_3_count,
    diag_hints_3_bytes,
    diag_hints_4_count,
    diag_hints_4_bytes,
    diag_event_base_bytes,
    diag_event_budget_bytes,
    diag_event_candidate_bytes,
    diag_event_mixed_rejection_bytes,
    diag_event_fetch_budget_count,
    diag_event_fetch_budget_bytes,
    diag_event_match_budget_count,
    diag_event_match_budget_bytes,
    diag_event_instruction_budget_count,
    diag_event_instruction_budget_bytes,
    diag_event_memory_budget_count,
    diag_event_memory_budget_bytes,
    diag_cursor_attached,
    diag_cursor_queries,
    diag_cursor_inherited,
    diag_cursor_memory_limit,
    diag_cursor_file_limit,
    diag_cursor_operation_limit,
    diag_cursor_descriptor_limit,
    diag_cursor_grants,
    diag_cursor_query_bytes,
    diag_selected_pack_count,
    diag_selected_pack_last_id,
    diag_selected_pack_bytes,
    diag_selected_pack_groups,
    diag_selected_pack_records,
    diag_selected_unlocated_records,
    diag_occurrence_preexisting_count,
    diag_occurrence_preexisting_bytes,
    diag_occurrence_preexisting_grants,
    diag_occurrence_missing_count,
    diag_occurrence_missing_bytes,
    diag_occurrence_missing_grants,
    diag_occurrence_duplicate_count,
    diag_occurrence_duplicate_bytes,
    diag_occurrence_duplicate_grants,
    diag_eligible_count,
    diag_eligible_bytes,
    diag_no_predecessor_count,
    diag_no_predecessor_bytes,
    diag_missing_span_count,
    diag_missing_span_bytes,
    diag_complete_empty_count,
    diag_complete_empty_bytes,
    diag_limited_empty_count,
    diag_limited_empty_bytes,
    diag_complete_hints_count,
    diag_complete_hints_bytes,
    diag_limited_hints_count,
    diag_limited_hints_bytes,
    diag_new_full_count,
    diag_new_full_bytes,
    diag_new_delta_count,
    diag_new_delta_bytes,
    diag_race_count,
    diag_race_bytes,
    diag_terminal_no_predecessor_count,
    diag_terminal_no_predecessor_bytes,
    diag_terminal_missing_span_count,
    diag_terminal_missing_span_bytes,
    diag_terminal_no_overlap_count,
    diag_terminal_no_overlap_bytes,
    diag_terminal_correspondence_limit_count,
    diag_terminal_correspondence_limit_bytes,
    diag_terminal_base_count,
    diag_terminal_base_bytes,
    diag_terminal_budget_count,
    diag_terminal_budget_bytes,
    diag_terminal_no_delta_count,
    diag_terminal_no_delta_bytes,
    diag_terminal_mixed_rejection_count,
    diag_terminal_mixed_rejection_bytes,
    diag_terminal_delta_count,
    diag_terminal_delta_bytes,
    diag_terminal_unknown_count,
    diag_terminal_unknown_bytes,
    diag_size_lt64_count,
    diag_size_lt64_bytes,
    diag_size_lt256_count,
    diag_size_lt256_bytes,
    diag_size_lt1024_count,
    diag_size_lt1024_bytes,
    diag_size_lt4096_count,
    diag_size_lt4096_bytes,
    diag_size_lt16384_count,
    diag_size_lt16384_bytes,
    diag_size_lt65536_count,
    diag_size_lt65536_bytes,
    diag_size_ge65536_count,
    diag_size_ge65536_bytes,
    diag_event_base,
    diag_event_budget,
    diag_event_candidate,
    diag_event_mixed_rejection,
    diag_file_source_without_chunk,
    diag_nonfile_chunk_count,
    diag_nonfile_chunk_bytes,
    metadata_pool_group_fetches,
    metadata_pool_decoded_bytes,
    metadata_pool_admitted_groups,
    metadata_pool_admitted_values,
    metadata_index_sync_ns,
    group_fetches,
    encoded_read_bytes,
    decoded_read_bytes,
    decompression_calls,
    base_fetches,
    blob_ranges,
    eligible_targets,
    absent_predecessors,
    targets_without_hints,
    usable_bases,
    predecessor_hints,
    candidate_trials,
    correspondence_reserved_bytes,
    correspondence_descriptors,
    correspondence_budget_skips,
    budget_skips,
    fetch_budget_skips,
    match_budget_skips,
    instruction_budget_skips,
    memory_budget_skips,
    match_comparisons,
    seed_hash_bytes,
    matching_ns,
    full_selected,
    delta_selected,
    rejected_mixed_groups,
    full_alternative_bytes,
    mixed_alternative_bytes,
    selected_encoded_bytes,
    encoding_calls,
    encoding_ns,
    native_record_fetches,
    native_request_bytes,
    native_parser_bytes,
    native_raw_decoded_bytes,
    native_decode_calls,
    native_decode_ns,
    native_dependency_edges,
    native_depth_0,
    native_depth_1,
    native_depth_2,
    native_depth_3,
    native_depth_4,
    native_full_encode_calls,
    native_full_encode_ns,
    native_full_frame_count,
    native_full_frame_bytes,
    native_prefix_encode_calls,
    native_prefix_encode_ns,
    native_prefix_frame_count,
    native_prefix_frame_bytes,
    native_fallback_no_hint_count,
    native_fallback_no_hint_bytes,
    native_fallback_unavailable_count,
    native_fallback_unavailable_bytes,
    native_fallback_legacy_delta_count,
    native_fallback_legacy_delta_bytes,
    native_fallback_role_count,
    native_fallback_role_bytes,
    native_fallback_depth_count,
    native_fallback_depth_bytes,
    native_fallback_budget_count,
    native_fallback_budget_bytes,
    native_fallback_full_wins_count,
    native_fallback_full_wins_bytes,
    native_admitted_full_count,
    native_admitted_full_bytes,
    native_admitted_prefix_count,
    native_admitted_prefix_bytes,
);

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
    /// Includes attempted native input work before a construction fallback.
    pub scanned_files: u64,
    pub scanned_bytes: u64,
    /// Zero for Empty; one source construction, or a stopped attempt plus one fallback.
    pub source_passes: u64,
}

impl CandidateReceipt {
    pub(crate) fn validate(self) -> Result<()> {
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
    pub live_backing_calls: u64,
    pub live_backing_request_bytes: u64,
    pub live_backing_wait_ns: u64,
    pub live_backing_queue_ns: u64,
    pub live_write_dispatch_ns: u64,
    pub live_edit_ns: u64,

    /// Host-authority dispatch proof (#144 D1): per-wire-class counts of the
    /// host-authority operations this Workspace dispatched. Zero on routes
    /// without a host authority. Take semantics: counts dispatches since the
    /// previous receipt.
    pub host_authority_dispatches: u64,
    pub host_authority_lookup: u64,
    pub host_authority_attr: u64,
    pub host_authority_readlink: u64,
    pub host_authority_directory: u64,
    pub host_authority_create: u64,
    pub host_authority_mkdir: u64,
    pub host_authority_symlink: u64,
    pub host_authority_link: u64,
    pub host_authority_unlink: u64,
    pub host_authority_rename: u64,
    pub host_authority_pin: u64,
    pub host_authority_unpin: u64,
    pub host_authority_forget: u64,
    pub host_authority_forget_batch: u64,
    pub host_authority_read: u64,
    pub host_authority_read_lease: u64,
    pub host_authority_release_lease: u64,
    pub host_authority_write: u64,
    pub host_authority_truncate: u64,
    pub host_authority_chmod: u64,
    pub host_authority_mtime: u64,
    pub host_authority_fsync: u64,
    pub host_authority_detach: u64,

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
    pub output_pipeline_ns: u64,
    pub output_admission_ns: u64,
    pub output_blocked_ns: u64,
    pub output_consumer_idle_ns: u64,
    pub output_queue_peak_bytes: u64,
    pub namespace_ns: u64,
    /// Nested namespace subphases, excluded from attributed_ns summation.
    pub deletion_cursor_ns: u64,
    pub deletion_records_ns: u64,
    pub candidate_finish_ns: u64,
    pub local_admission_ns: u64,
    pub object_admission_ns: u64,
    pub object_admission_transactions: u64,
    pub max_admission_transaction_objects: u64,
    pub max_admission_transaction_bytes: u64,
    pub object_admission_begin_ns: u64,
    pub object_admission_insert_ns: u64,
    pub object_admission_commit_ns: u64,
    /// Nested shared-consumer validation/sort costs; excluded from phase summation.
    pub object_admission_authentication_ns: u64,
    /// Required selected-spill read authentication, including producer handoff.
    pub object_admission_storage_authentication_ns: u64,
    pub object_admission_sort_ns: u64,
    /// Selected canonical bytes moved from candidate memory into admission pages.
    pub object_admission_memory_owned_bytes: u64,
    /// Selected canonical bytes read from spill and moved into admission pages;
    /// excludes framing, read-ahead and physical filesystem I/O amplification.
    pub object_admission_spill_readback_bytes: u64,
    /// Additional borrowed-byte copies at this candidate-to-admission boundary.
    pub object_admission_borrowed_copy_bytes: u64,
    pub publication_ns: u64,
    pub publication_begin_ns: u64,
    pub publication_payload_ns: u64,
    pub publication_insert_ns: u64,
    pub publication_metadata_ns: u64,
    pub publication_commit_ns: u64,
    /// #144 D2: host-route regions previously inside unattributed_ns —
    /// pre-capture maintenance/metrics collection, the construction tail after
    /// the namespace phase, exact staging, and publication acknowledgment.
    /// `maintain_ns` is nested inside `pre_capture_ns` (excluded from the
    /// attributed summation, like the other nested sub-phase fields).
    pub pre_capture_ns: u64,
    pub maintain_ns: u64,
    pub construction_tail_ns: u64,
    pub stage_ns: u64,
    pub acknowledge_ns: u64,
    /// Includes spool_retirement_ns; retirement is a nested subphase.
    pub checkpoint_ns: u64,
    pub spool_retirement_ns: u64,
    /// Nested retain-predicate time; remaining retirement includes owner drop/close and map iteration.
    pub spool_retirement_scan_ns: u64,
    pub spool_retired_segments: u64,
    pub resume_ns: u64,
    pub unattributed_ns: u64,
    pub snapshot_database_calls: u64,
    pub snapshot_database_rows: u64,
    pub snapshot_database_bytes: u64,
    pub payload_bytes_read: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceCommitDiagnostics {
    pub cdc_bytes_scanned: u64,
    pub edit_count: u64,
    pub edit_piece_count: u64,
    pub edit_piece_height: u64,
    pub edit_piece_logical_charge: u64,
    pub edit_spool_allocated_bytes: u64,
    pub edit_spool_peak_bytes: u64,
    pub edit_spool_live_bytes: u64,
    pub edit_spool_superseded_bytes: u64,
    /// Aggregate owned spool inode st_blocks * 512 at observed mutation boundaries.
    /// None means an observation failed; these are not logical spool lengths.
    pub physical_spool_allocated_bytes: Option<u64>,
    /// Lifetime observed high-water, including failed writes before rollback.
    pub physical_spool_peak_bytes: Option<u64>,
    pub physical_spool_observation_errors: u64,
    pub physical_spool_observation_count: u64,
    pub edit_tree_visits: u64,
    pub edit_metric_nodes_scanned: u64,
    pub namespace_base_paths_visited: u64,
    pub namespace_final_paths_visited: u64,
    pub namespace_dirty_nodes_visited: u64,
    pub namespace_clean_nodes_visited: u64,
    pub namespace_candidate_probe_nodes: u64,
    /// #144 D2 bridge: host-route construction diagnostics from
    /// SnapshotCandidateDiagnostics, dropped before reaching this record
    /// before this note existed. Populated only on the host-authority route.
    pub construction_changed_keys: u64,
    pub construction_changed_inodes: u64,
    pub construction_binding_deltas: u64,
    pub construction_file_tasks: u64,
    pub construction_correspondence_visits: u64,
    pub construction_correspondence_fragments: u64,
    pub construction_replacement_bytes: u64,
    pub construction_reused_bytes: u64,
    pub construction_full_comparisons: u64,
    pub construction_full_builds: u64,
    pub construction_scratch_peak_reserved_bytes: u64,
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
            self.pre_capture_ns,
            self.construction_tail_ns,
            self.stage_ns,
            self.acknowledge_ns,
            self.checkpoint_ns,
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
    DeletionCursor,
    DeletionRecords,
    CandidateFinish,
    LocalAdmission,
    ObjectAdmission,
    Publication,
    PreCapture,
    Maintain,
    ConstructionTail,
    Stage,
    Acknowledge,
    Checkpoint,
    SpoolRetirement,
    Resume,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WorkspaceLifecycleKind {
    #[default]
    Attach,
    End,
}

/// #144 D2: host-side End phase split for the host-authority route. The
/// daemon-side teardown keeps its own `WorkspaceLifecycleReceipt`; this
/// record covers the regions of `end_workspace_session` that had no receipt:
/// correspondence settle (bounded maintenance drain), `after_detach`, and
/// per-Workspace state-directory removal.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostEndReceipt {
    pub settle_ns: u64,
    pub settle_steps: u64,
    pub after_detach_ns: u64,
    pub state_removal_ns: u64,
}

pub fn record_host_end(receipt: HostEndReceipt) -> Result<()> {
    RECEIPTS.with(|receipts| {
        receipts.borrow_mut().push(StorageReceipt::HostEnd(receipt));
    });
    Ok(())
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
#[allow(
    clippy::large_enum_variant,
    reason = "Keep telemetry receipts Copy without per-receipt heap allocation"
)]
pub enum StorageReceipt {
    Candidate(CandidateReceipt),
    WorkspaceCommit(WorkspaceCommitReceipt),
    WorkspaceLifecycle(WorkspaceLifecycleReceipt),
    FuseWrite(FuseWriteReceipt),
    WorkspaceRead(WorkspaceReadReceipt),
    PhysicalStorage(PhysicalStorageReceipt),
    HostEnd(HostEndReceipt),
}

thread_local! {
    static RECEIPTS: RefCell<Vec<StorageReceipt>> = const { RefCell::new(Vec::new()) };
    static LAYERSTACK_INITIALIZATIONS: RefCell<Vec<LayerStackInitializationReceipt>> = const { RefCell::new(Vec::new()) };
    static WORKSPACE_COMMIT: RefCell<Option<WorkspaceCommitReceipt>> = const { RefCell::new(None) };
    static WORKSPACE_COMMIT_DIAGNOSTIC: RefCell<Option<WorkspaceCommitDiagnostics>> = const { RefCell::new(None) };
    static WORKSPACE_COMMIT_DIAGNOSTICS: RefCell<Vec<WorkspaceCommitDiagnostics>> = const { RefCell::new(Vec::new()) };
    static WORKSPACE_COMMIT_DIAGNOSTICS_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub struct WorkspaceCommitTimer(Instant);
/// Diagnostics capture is thread-affine because its state is thread-local.
///
/// ```compile_fail
/// fn require_send(_: impl Send) {}
/// require_send(layerfs_layerstack_store::capture_workspace_commit_diagnostics().unwrap());
/// ```
pub struct WorkspaceCommitDiagnosticsGuard(std::marker::PhantomData<std::rc::Rc<()>>);

impl Drop for WorkspaceCommitDiagnosticsGuard {
    fn drop(&mut self) {
        WORKSPACE_COMMIT_DIAGNOSTICS_ENABLED.with(|enabled| enabled.set(false));
        WORKSPACE_COMMIT_DIAGNOSTIC.with(|diagnostic| diagnostic.borrow_mut().take());
        WORKSPACE_COMMIT_DIAGNOSTICS.with(|diagnostics| diagnostics.borrow_mut().clear());
    }
}

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
            WORKSPACE_COMMIT_DIAGNOSTIC.with(|current| {
                if let Some(diagnostic) = current.borrow_mut().take() {
                    WORKSPACE_COMMIT_DIAGNOSTICS.with(|diagnostics| {
                        diagnostics.borrow_mut().push(diagnostic);
                    });
                }
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
        WORKSPACE_COMMIT_DIAGNOSTICS_ENABLED.with(|enabled| {
            WORKSPACE_COMMIT_DIAGNOSTIC.with(|diagnostic| {
                *diagnostic.borrow_mut() = enabled.get().then(WorkspaceCommitDiagnostics::default);
            });
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
    spool_peak_bytes: u64,
    spool_live_bytes: u64,
    spool_superseded_bytes: u64,
    metric_nodes_scanned: u64,
) {
    WORKSPACE_COMMIT_DIAGNOSTIC.with(|current| {
        if let Some(diagnostic) = current.borrow_mut().as_mut() {
            diagnostic.edit_count = edit_count;
            diagnostic.edit_piece_count = piece_count;
            diagnostic.edit_piece_height = piece_height;
            diagnostic.edit_piece_logical_charge = piece_logical_charge;
            diagnostic.edit_spool_allocated_bytes = spool_allocated_bytes;
            diagnostic.edit_spool_peak_bytes = spool_peak_bytes;
            diagnostic.edit_spool_live_bytes = spool_live_bytes;
            diagnostic.edit_spool_superseded_bytes = spool_superseded_bytes;
            diagnostic.edit_metric_nodes_scanned = metric_nodes_scanned;
        }
    });
}

/// #144 D2 bridge: surface the host-route construction diagnostics that
/// previously never reached this thread-local record. Values overwrite; the
/// host route builds one candidate per attempt.
pub fn note_workspace_commit_construction(
    changed_keys: u64,
    changed_inodes: u64,
    binding_deltas: u64,
    file_tasks: u64,
    correspondence_visits: u64,
    correspondence_fragments: u64,
    replacement_bytes: u64,
    reused_bytes: u64,
    full_comparisons: u64,
    full_builds: u64,
    scratch_peak_reserved_bytes: u64,
) {
    WORKSPACE_COMMIT_DIAGNOSTIC.with(|current| {
        if let Some(diagnostic) = current.borrow_mut().as_mut() {
            diagnostic.construction_changed_keys = changed_keys;
            diagnostic.construction_changed_inodes = changed_inodes;
            diagnostic.construction_binding_deltas = binding_deltas;
            diagnostic.construction_file_tasks = file_tasks;
            diagnostic.construction_correspondence_visits = correspondence_visits;
            diagnostic.construction_correspondence_fragments = correspondence_fragments;
            diagnostic.construction_replacement_bytes = replacement_bytes;
            diagnostic.construction_reused_bytes = reused_bytes;
            diagnostic.construction_full_comparisons = full_comparisons;
            diagnostic.construction_full_builds = full_builds;
            diagnostic.construction_scratch_peak_reserved_bytes = scratch_peak_reserved_bytes;
        }
    });
}

/// Adds passive allocation observations without changing legacy receipt fields.
pub fn note_workspace_physical_spool(
    current: Option<u64>,
    peak: Option<u64>,
    errors: u64,
    observations: u64,
) {
    WORKSPACE_COMMIT_DIAGNOSTIC.with(|current_diagnostic| {
        if let Some(diagnostic) = current_diagnostic.borrow_mut().as_mut() {
            diagnostic.physical_spool_allocated_bytes = current;
            diagnostic.physical_spool_peak_bytes = peak;
            diagnostic.physical_spool_observation_errors = errors;
            diagnostic.physical_spool_observation_count = observations;
        }
    });
}

/// Counts actual candidate traversal work only while diagnostic capture is active.
pub fn note_workspace_namespace_visits(
    base: u64,
    final_paths: u64,
    dirty: u64,
    clean: u64,
    probes: u64,
) {
    WORKSPACE_COMMIT_DIAGNOSTIC.with(|current| {
        if let Some(diagnostic) = current.borrow_mut().as_mut() {
            diagnostic.namespace_base_paths_visited =
                diagnostic.namespace_base_paths_visited.saturating_add(base);
            diagnostic.namespace_final_paths_visited = diagnostic
                .namespace_final_paths_visited
                .saturating_add(final_paths);
            diagnostic.namespace_dirty_nodes_visited = diagnostic
                .namespace_dirty_nodes_visited
                .saturating_add(dirty);
            diagnostic.namespace_clean_nodes_visited = diagnostic
                .namespace_clean_nodes_visited
                .saturating_add(clean);
            diagnostic.namespace_candidate_probe_nodes = diagnostic
                .namespace_candidate_probe_nodes
                .saturating_add(probes);
        }
    });
}

pub fn note_workspace_commit_tree_visits(visits: u64) {
    WORKSPACE_COMMIT_DIAGNOSTIC.with(|current| {
        if let Some(diagnostic) = current.borrow_mut().as_mut() {
            diagnostic.edit_tree_visits = diagnostic.edit_tree_visits.saturating_add(visits);
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
            WorkspaceCommitPhase::DeletionCursor => &mut receipt.deletion_cursor_ns,
            WorkspaceCommitPhase::DeletionRecords => &mut receipt.deletion_records_ns,
            WorkspaceCommitPhase::CandidateFinish => &mut receipt.candidate_finish_ns,
            WorkspaceCommitPhase::LocalAdmission => &mut receipt.local_admission_ns,
            WorkspaceCommitPhase::ObjectAdmission => &mut receipt.object_admission_ns,
            WorkspaceCommitPhase::Publication => &mut receipt.publication_ns,
            WorkspaceCommitPhase::PreCapture => &mut receipt.pre_capture_ns,
            WorkspaceCommitPhase::Maintain => &mut receipt.maintain_ns,
            WorkspaceCommitPhase::ConstructionTail => &mut receipt.construction_tail_ns,
            WorkspaceCommitPhase::Stage => &mut receipt.stage_ns,
            WorkspaceCommitPhase::Acknowledge => &mut receipt.acknowledge_ns,
            WorkspaceCommitPhase::Checkpoint => &mut receipt.checkpoint_ns,
            WorkspaceCommitPhase::SpoolRetirement => &mut receipt.spool_retirement_ns,
            WorkspaceCommitPhase::Resume => &mut receipt.resume_ns,
        };
        *target = target.saturating_add(elapsed_ns);
    });
}

pub fn note_workspace_spool_retirement(total_ns: u64, scan_ns: u64, segments: u64) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.spool_retirement_ns = receipt.spool_retirement_ns.saturating_add(total_ns);
            receipt.spool_retirement_scan_ns =
                receipt.spool_retirement_scan_ns.saturating_add(scan_ns);
            receipt.spool_retired_segments =
                receipt.spool_retired_segments.saturating_add(segments);
        }
    });
}

pub(crate) fn note_workspace_commit_cdc(bytes: u64) {
    WORKSPACE_COMMIT_DIAGNOSTIC.with(|current| {
        if let Some(diagnostic) = current.borrow_mut().as_mut() {
            diagnostic.cdc_bytes_scanned = bytes;
        }
    });
}

pub fn take_workspace_commit_diagnostics() -> Vec<WorkspaceCommitDiagnostics> {
    WORKSPACE_COMMIT_DIAGNOSTICS.with(|diagnostics| std::mem::take(&mut *diagnostics.borrow_mut()))
}

pub fn capture_workspace_commit_diagnostics() -> Result<WorkspaceCommitDiagnosticsGuard> {
    WORKSPACE_COMMIT_DIAGNOSTICS_ENABLED.with(|enabled| {
        if enabled.replace(true) {
            return Err(StoreError::Integrity("nested Workspace Commit diagnostics"));
        }
        WORKSPACE_COMMIT_DIAGNOSTICS.with(|diagnostics| diagnostics.borrow_mut().clear());
        Ok(WorkspaceCommitDiagnosticsGuard(std::marker::PhantomData))
    })
}

pub(crate) fn note_workspace_admission_sort(elapsed_ns: u64) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.object_admission_sort_ns =
                receipt.object_admission_sort_ns.saturating_add(elapsed_ns);
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

pub(crate) fn note_workspace_output_pipeline(
    wall: u64,
    admission: u64,
    blocked: u64,
    idle: u64,
    peak: u64,
) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.output_pipeline_ns = wall;
            receipt.output_admission_ns = admission;
            receipt.output_blocked_ns = blocked;
            receipt.output_consumer_idle_ns = idle;
            receipt.output_queue_peak_bytes = peak;
        }
    });
}

pub(crate) fn note_workspace_candidate_delivery(
    memory_owned_bytes: u64,
    spill_readback_bytes: u64,
    storage_authentication_ns: u64,
) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.object_admission_memory_owned_bytes = receipt
                .object_admission_memory_owned_bytes
                .saturating_add(memory_owned_bytes);
            receipt.object_admission_spill_readback_bytes = receipt
                .object_admission_spill_readback_bytes
                .saturating_add(spill_readback_bytes);
            receipt.object_admission_borrowed_copy_bytes = 0;
            receipt.object_admission_storage_authentication_ns = receipt
                .object_admission_storage_authentication_ns
                .saturating_add(storage_authentication_ns);
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
    record_validated_candidate(receipt);
    Ok(())
}

pub(crate) fn record_validated_candidate(receipt: CandidateReceipt) {
    RECEIPTS.with(|receipts| {
        receipts
            .borrow_mut()
            .push(StorageReceipt::Candidate(receipt));
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_diagnostics_are_separate_from_the_legacy_commit_receipt() {
        take_storage_receipts();
        take_workspace_commit_diagnostics();
        for _ in 0..100 {
            drop(begin_workspace_commit(CaptureMode::Materialized).unwrap());
        }
        assert!(take_workspace_commit_diagnostics().is_empty());
        take_storage_receipts();
        let _diagnostics = capture_workspace_commit_diagnostics().unwrap();
        {
            let _timer = begin_workspace_commit(CaptureMode::Materialized).unwrap();
            note_workspace_commit_edit_state(2, 3, 4, 5, 6, 7, 8, 9, 10);
            note_workspace_commit_tree_visits(11);
            note_workspace_commit_cdc(12);
            note_workspace_namespace_visits(13, 14, 15, 16, 17);
        }
        let receipts = take_storage_receipts();
        let [StorageReceipt::WorkspaceCommit(receipt)] = receipts.as_slice() else {
            panic!("one legacy commit receipt")
        };
        assert_eq!(receipt.capture_mode, Some(CaptureMode::Materialized));
        let diagnostics = take_workspace_commit_diagnostics();
        let [diagnostic] = diagnostics.as_slice() else {
            panic!("one edit diagnostic")
        };
        assert_eq!(
            *diagnostic,
            WorkspaceCommitDiagnostics {
                cdc_bytes_scanned: 12,
                edit_count: 2,
                edit_piece_count: 3,
                edit_piece_height: 4,
                edit_piece_logical_charge: 5,
                edit_spool_allocated_bytes: 6,
                edit_spool_peak_bytes: 7,
                edit_spool_live_bytes: 8,
                edit_spool_superseded_bytes: 9,
                physical_spool_allocated_bytes: None,
                physical_spool_peak_bytes: None,
                physical_spool_observation_errors: 0,
                physical_spool_observation_count: 0,
                edit_tree_visits: 11,
                edit_metric_nodes_scanned: 10,
                namespace_base_paths_visited: 13,
                namespace_final_paths_visited: 14,
                namespace_dirty_nodes_visited: 15,
                namespace_clean_nodes_visited: 16,
                namespace_candidate_probe_nodes: 17,
            }
        );
    }
}
