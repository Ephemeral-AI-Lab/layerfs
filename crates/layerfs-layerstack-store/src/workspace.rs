use crate::objects::{
    admit_planned_objects, apply_reconcile_choices_bounded, combine_candidates,
    insert_object_batch, reconcile_candidate, BuildCounters, BuiltRoot, CanonicalObject,
    DeferredObjectStore, ObjectSource,
};
use crate::records::{
    decode_branch, decode_commit, decode_layer_stack_at, decode_object_id, optional_id,
};
use crate::{
    BranchId, BranchRecord, CommitId, CommitRecord, LayerId, LayerStackStore, Result, StoreError,
    WorkspaceReadReceipt,
};
use layerfs_content::ObjectId;
use rusqlite::{OptionalExtension, TransactionBehavior};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct WorkspaceLease {
    _lease: crate::schema::BranchLease,
}

#[derive(Clone)]
pub struct SnapshotReader {
    db: crate::schema::StoreDb,
    root: ObjectId,
    overlays: Vec<Arc<Mutex<DeferredObjectStore>>>,
    read_metrics: Arc<Mutex<WorkspaceReadReceipt>>,
    cache: Arc<Mutex<SnapshotCache>>,
}

const SNAPSHOT_CACHE_BYTES: usize = 8 * 1024 * 1024;
const SNAPSHOT_CACHE_OBJECT_BYTES: usize = 1024;

#[derive(Default)]
struct SnapshotCache {
    rows: HashMap<ObjectId, Vec<u8>>,
    bytes: usize,
}

pub struct PinnedSnapshot {
    pub branch: BranchRecord,
    pub layer_stack: crate::LayerStackRecord,
    pub root: ObjectId,
    pub reader: SnapshotReader,
}

pub struct PreparedReconciliation {
    pub branch_id: BranchId,
    pub expected_head: CommitId,
    pub old_base_layer_id: LayerId,
    pub current_layer_id: LayerId,
    pub old_base_root: ObjectId,
    pub branch_root: ObjectId,
    pub layer_root: ObjectId,
    pub root_id: ObjectId,
    pub conflicts: Vec<layerfs_content::filesystem::ReconcileConflict>,
    objects: Arc<Mutex<DeferredObjectStore>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    Committed {
        commit_id: CommitId,
        root_id: ObjectId,
        counters: BuildCounters,
        candidate_objects: u64,
        candidate_bytes: u64,
        inserted_objects: u64,
        inserted_bytes: u64,
        reused_objects: u64,
        reused_bytes: u64,
    },
    UpToDate {
        root_id: ObjectId,
    },
}

impl LayerStackStore {
    pub fn acquire_workspace_lease(&self, branch_id: BranchId) -> Result<Option<WorkspaceLease>> {
        Ok(self
            .db
            .acquire_workspace_lease(branch_id)?
            .map(|lease| WorkspaceLease { _lease: lease }))
    }

    pub fn pin_branch(&self, branch_id: BranchId) -> Result<PinnedSnapshot> {
        let (branch, layer_stack, root) = self.load_workspace_snapshot(branch_id)?;
        Ok(PinnedSnapshot {
            branch,
            layer_stack,
            root,
            reader: SnapshotReader {
                db: self.db.clone(),
                root,
                overlays: Vec::new(),
                read_metrics: Arc::new(Mutex::new(WorkspaceReadReceipt::default())),
                cache: Arc::new(Mutex::new(SnapshotCache::default())),
            },
        })
    }

    pub fn snapshot_reader(&self, root: ObjectId) -> SnapshotReader {
        SnapshotReader {
            db: self.db.clone(),
            root,
            overlays: Vec::new(),
            read_metrics: Arc::new(Mutex::new(WorkspaceReadReceipt::default())),
            cache: Arc::new(Mutex::new(SnapshotCache::default())),
        }
    }

    pub fn reconciliation_reader(&self, prepared: &PreparedReconciliation) -> SnapshotReader {
        SnapshotReader {
            db: self.db.clone(),
            root: prepared.root_id,
            overlays: vec![prepared.objects.clone()],
            read_metrics: Arc::new(Mutex::new(WorkspaceReadReceipt::default())),
            cache: Arc::new(Mutex::new(SnapshotCache::default())),
        }
    }

    pub fn prepare_reconciliation(
        &self,
        branch_id: BranchId,
        current_layer_id: LayerId,
    ) -> Result<PreparedReconciliation> {
        let branch = self
            .branch(branch_id)?
            .ok_or(StoreError::NotFound("Branch"))?;
        let expected_head = branch
            .head_commit_id
            .ok_or(StoreError::InvalidInput("Branch without Commit"))?;
        let commit = self
            .commit(expected_head)?
            .ok_or(StoreError::Integrity("Branch head Commit"))?;
        let old_base = self
            .layer(branch.base_layer_id)?
            .ok_or(StoreError::Integrity("old base Layer"))?;
        let current = self
            .layer(current_layer_id)?
            .ok_or(StoreError::NotFound("current Layer"))?;
        if old_base.layer_stack_id != branch.layer_stack_id
            || current.layer_stack_id != branch.layer_stack_id
        {
            return Err(StoreError::InvalidInput("LayerStack mismatch"));
        }
        let reader = self.snapshot_reader(commit.root_id);
        let reconciled =
            reconcile_candidate(&reader, old_base.root_id, commit.root_id, current.root_id)?;
        Ok(PreparedReconciliation {
            branch_id,
            expected_head,
            old_base_layer_id: old_base.id,
            current_layer_id,
            old_base_root: old_base.root_id,
            branch_root: commit.root_id,
            layer_root: current.root_id,
            root_id: reconciled.root_id,
            conflicts: reconciled.conflicts,
            objects: Arc::new(Mutex::new(reconciled.objects)),
        })
    }

    /// Dispose a prepared source when a Workspace ends without committing it.
    #[doc(hidden)]
    pub fn cleanup_reconciliation(&self, prepared: &PreparedReconciliation) -> Result<()> {
        let _operation = self.db.enter_operation()?;
        prepared
            .objects
            .lock()
            .map_err(|_| StoreError::Integrity("reconciliation candidate"))?
            .unlink_private(&self.db)
    }

    pub fn commit_reconciliation(
        &self,
        prepared: &PreparedReconciliation,
        working: BuiltRoot,
        choices: &[layerfs_content::filesystem::ReconcileChoice],
    ) -> Result<CommitOutcome> {
        self.commit_reconciliation_checked(prepared, working, choices, |_, _, _| Ok(()))
    }

    /// Inspect the actual selected candidate before any publication. The reader
    /// includes the working and selected private objects for authenticated diff.
    pub fn commit_reconciliation_checked(
        &self,
        prepared: &PreparedReconciliation,
        working: BuiltRoot,
        choices: &[layerfs_content::filesystem::ReconcileChoice],
        inspect: impl FnOnce(&SnapshotReader, ObjectId, ObjectId) -> Result<()>,
    ) -> Result<CommitOutcome> {
        self.commit_reconciliation_checked_bounded(
            prepared,
            working,
            choices,
            layerfs_content::filesystem::ReconcileBudget {
                scratch_dir: &std::env::temp_dir(),
                memory_bytes: 8 * 1024 * 1024,
                spool_bytes: 1024 * 1024 * 1024,
            },
            inspect,
        )
    }

    pub fn commit_reconciliation_checked_bounded(
        &self,
        prepared: &PreparedReconciliation,
        working: BuiltRoot,
        choices: &[layerfs_content::filesystem::ReconcileChoice],
        budget: layerfs_content::filesystem::ReconcileBudget<'_>,
        inspect: impl FnOnce(&SnapshotReader, ObjectId, ObjectId) -> Result<()>,
    ) -> Result<CommitOutcome> {
        // Own all source cleanup through publication under the normal Store
        // gate. Each source is unlinked before another disposable source exists,
        // so the five-path failure owner never needs an unbounded list.
        let _operation = self.db.enter_operation()?;
        let mut working = working;
        working.objects.unlink_private(&self.db)?;
        prepared
            .objects
            .lock()
            .map_err(|_| StoreError::Integrity("reconciliation candidate"))?
            .unlink_private(&self.db)?;
        let branch = self
            .branch(prepared.branch_id)?
            .ok_or(StoreError::NotFound("Branch"))?;
        if branch.head_commit_id != Some(prepared.expected_head)
            || branch.base_layer_id != prepared.old_base_layer_id
        {
            return Err(StoreError::CommitHeadMoved {
                expected: Some(prepared.expected_head),
                actual: branch.head_commit_id,
            });
        }
        let working_root = working.root_id;
        let working = Arc::new(Mutex::new(working.objects));
        let reader = SnapshotReader {
            db: self.db.clone(),
            root: working_root,
            overlays: vec![working.clone(), prepared.objects.clone()],
            read_metrics: Arc::new(Mutex::new(WorkspaceReadReceipt::default())),
            cache: Arc::new(Mutex::new(SnapshotCache::default())),
        };
        let selected_cpu_started = crate::workspace_process_cpu_ns();
        let selected_started = Instant::now();
        layerfs_content::filesystem::delta_spool::reset_metrics();
        let selected = apply_reconcile_choices_bounded(
            &reader,
            working_root,
            prepared.branch_root,
            prepared.layer_root,
            &prepared.conflicts,
            choices,
            budget,
        );
        crate::telemetry::note_workspace_commit_phase(
            crate::WorkspaceCommitPhase::Namespace,
            elapsed_ns(selected_started),
        );
        crate::note_workspace_commit_phase_cpu(
            crate::WorkspaceCommitPhase::Namespace,
            selected_cpu_started
                .zip(crate::workspace_process_cpu_ns())
                .and_then(|(start, end)| end.checked_sub(start)),
        );
        if let Some(metrics) = layerfs_content::filesystem::delta_spool::take_metrics() {
            crate::telemetry::note_workspace_commit_sort(
                metrics.sequential_read_calls,
                metrics.sequential_read_bytes,
                metrics.positional_read_calls,
                metrics.positional_read_bytes,
                metrics.write_calls,
                metrics.write_bytes,
                metrics.merge_passes,
            );
        }
        let mut selected = selected?;
        selected.objects.unlink_private(&self.db)?;
        let selected_root = selected.root_id;
        let selected_counters = selected.counters;
        let selected = Arc::new(Mutex::new(selected.objects));
        let mut selected_reader = reader.clone();
        selected_reader.root = selected_root;
        selected_reader.overlays.insert(0, selected.clone());
        let inspect_cpu_started = crate::workspace_process_cpu_ns();
        let inspect_started = Instant::now();
        let inspected = inspect(&selected_reader, working_root, selected_root);
        crate::telemetry::note_workspace_commit_phase(
            crate::WorkspaceCommitPhase::CandidateFinish,
            elapsed_ns(inspect_started),
        );
        crate::note_workspace_commit_phase_cpu(
            crate::WorkspaceCommitPhase::CandidateFinish,
            inspect_cpu_started
                .zip(crate::workspace_process_cpu_ns())
                .and_then(|(start, end)| end.checked_sub(start)),
        );
        inspected?;
        let selected = selected
            .lock()
            .map_err(|_| StoreError::Integrity("reconciliation candidate"))?;
        let working = working
            .lock()
            .map_err(|_| StoreError::Integrity("reconciliation candidate"))?;
        let prepared_objects = prepared
            .objects
            .lock()
            .map_err(|_| StoreError::Integrity("reconciliation candidate"))?;
        let objects = combine_candidates(selected_root, &[&working, &prepared_objects, &selected])?;
        self.commit_candidate_under_permit(
            &branch,
            prepared.branch_root,
            prepared.current_layer_id,
            BuiltRoot {
                root_id: selected_root,
                objects,
                counters: selected_counters,
            },
        )
    }

    pub fn commit_candidate(
        &self,
        expected: &BranchRecord,
        expected_root: ObjectId,
        new_base_layer_id: LayerId,
        built: BuiltRoot,
    ) -> Result<CommitOutcome> {
        let _operation = self.db.enter_operation()?;
        self.commit_candidate_under_permit(expected, expected_root, new_base_layer_id, built)
    }

    fn commit_candidate_under_permit(
        &self,
        expected: &BranchRecord,
        expected_root: ObjectId,
        new_base_layer_id: LayerId,
        built: BuiltRoot,
    ) -> Result<CommitOutcome> {
        #[cfg(feature = "test-instrumentation")]
        crate::schema::verification_candidate(expected.id, built.counters.spill_count);
        crate::telemetry::note_workspace_commit_cdc(built.counters.cdc_bytes_scanned);
        if built.root_id == expected_root && new_base_layer_id == expected.base_layer_id {
            let pinned = self.pin_branch(expected.id)?;
            if pinned.branch.head_commit_id != expected.head_commit_id
                || pinned.branch.base_layer_id != expected.base_layer_id
            {
                return Err(StoreError::CommitHeadMoved {
                    expected: expected.head_commit_id,
                    actual: pinned.branch.head_commit_id,
                });
            }
            if pinned.root != expected_root {
                return Err(StoreError::Integrity("Workspace expected root"));
            }
            built.objects.cleanup(&self.db)?;
            return Ok(CommitOutcome::UpToDate {
                root_id: expected_root,
            });
        }
        let new_base = self
            .layer(new_base_layer_id)?
            .ok_or(StoreError::NotFound("base Layer"))?;
        if new_base.layer_stack_id != expected.layer_stack_id {
            return Err(StoreError::Integrity("Branch LayerStack ownership"));
        }
        let commit = CommitRecord {
            id: CommitId::derive(built.root_id, expected.head_commit_id, new_base_layer_id),
            root_id: built.root_id,
            parent_commit_id: expected.head_commit_id,
            base_layer_id: new_base_layer_id,
        };
        let started = Instant::now();
        let mut plan = self.db.plan_candidate(&built.objects)?;
        crate::telemetry::note_workspace_commit_phase(
            crate::WorkspaceCommitPhase::LocalAdmission,
            elapsed_ns(started),
        );
        let started = Instant::now();
        let mut statement_number = 0;
        let admission =
            admit_planned_objects(&self.db, built.objects, &mut plan, &mut statement_number)?;
        crate::telemetry::note_workspace_admission(
            admission.transactions,
            admission.max_transaction_objects,
            admission.max_transaction_bytes,
            admission.begin_ns,
            admission.insert_ns,
            admission.commit_ns,
        );
        crate::telemetry::note_workspace_commit_phase(
            crate::WorkspaceCommitPhase::ObjectAdmission,
            elapsed_ns(started),
        );
        let started = Instant::now();
        let begin_started = Instant::now();
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let begin_ns = elapsed_ns(begin_started);
        let insert_metrics =
            insert_object_batch(&transaction, &admission.final_batch, &mut statement_number)?;
        let metadata_started = Instant::now();
        statement_number += 1;
        crate::schema::fail_transaction_statement(statement_number)?;
        if transaction.execute(
            crate::statements::workspace::INSERT_COMMIT,
            rusqlite::params![
                commit.id.as_slice(),
                commit.root_id.as_bytes().as_slice(),
                commit.parent_commit_id.map(|id| id.to_bytes().to_vec()),
                commit.base_layer_id.as_slice(),
            ],
        )? == 0
        {
            let existing = transaction
                .query_row(
                    crate::statements::branch::GET_COMMIT,
                    [commit.id.as_slice()],
                    decode_commit,
                )
                .optional()?
                .ok_or(StoreError::Integrity("Commit conflict"))?;
            if existing != commit {
                return Err(StoreError::Integrity("Commit collision"));
            }
        }
        statement_number += 1;
        crate::schema::fail_transaction_statement(statement_number)?;
        #[cfg(feature = "test-instrumentation")]
        crate::schema::verification_store_checkpoint(
            crate::schema::VerificationStoreFault::FinalPublication,
        )?;
        if transaction.execute(
            crate::statements::workspace::ADVANCE_BRANCH,
            rusqlite::params![
                expected.id.as_slice(),
                commit.id.as_slice(),
                expected.head_commit_id.map(|id| id.to_bytes().to_vec()),
                new_base_layer_id.as_slice(),
                expected.base_layer_id.as_slice(),
            ],
        )? == 0
        {
            let actual = transaction
                .query_row(
                    crate::statements::workspace::CURRENT_BRANCH,
                    [expected.id.as_slice()],
                    |row| Ok((row.get::<_, Option<Vec<u8>>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()?;
            drop(transaction);
            let actual = actual
                .map(|(head, _)| optional_id::<CommitId>(head))
                .transpose()?
                .flatten();
            return Err(StoreError::CommitHeadMoved {
                expected: expected.head_commit_id,
                actual,
            });
        }
        let receipt = crate::CandidateReceipt {
            candidate_objects: plan.candidate_objects,
            candidate_bytes: plan.candidate_bytes,
            inserted_objects: plan.inserted_objects,
            inserted_bytes: plan.inserted_bytes,
            reused_objects: plan.reused_objects,
            reused_bytes: plan.reused_bytes,
            batch_inserted_objects: admission.batch_inserted_objects,
            batch_inserted_bytes: admission.batch_inserted_bytes,
            final_inserted_objects: insert_metrics.objects,
            final_inserted_bytes: insert_metrics.bytes,
            preexisting_reused_objects: plan.reused_objects,
            preexisting_reused_bytes: plan.reused_bytes,
            admission_transactions: admission.transactions,
            max_transaction_objects: admission.max_transaction_objects,
            max_transaction_bytes: admission.max_transaction_bytes,
        };
        receipt.validate()?;
        let metadata_ns = elapsed_ns(metadata_started);
        let commit_started = Instant::now();
        transaction.commit()?;
        let commit_ns = elapsed_ns(commit_started);
        crate::telemetry::note_workspace_publication(
            begin_ns,
            insert_metrics.payload_ns,
            insert_metrics.insert_ns,
            metadata_ns,
            commit_ns,
        );
        crate::telemetry::note_workspace_commit_phase(
            crate::WorkspaceCommitPhase::Publication,
            elapsed_ns(started),
        );
        crate::telemetry::record_candidate(receipt);
        Ok(CommitOutcome::Committed {
            commit_id: commit.id,
            root_id: commit.root_id,
            counters: built.counters,
            candidate_objects: plan.candidate_objects,
            candidate_bytes: plan.candidate_bytes,
            inserted_objects: plan.inserted_objects,
            inserted_bytes: plan.inserted_bytes,
            reused_objects: plan.reused_objects,
            reused_bytes: plan.reused_bytes,
        })
    }

    fn load_workspace_snapshot(
        &self,
        branch_id: BranchId,
    ) -> Result<(BranchRecord, crate::LayerStackRecord, ObjectId)> {
        self.db
            .reader()?
            .query_row(
                crate::statements::workspace::LOAD_SNAPSHOT,
                [branch_id.as_slice()],
                |row| {
                    Ok((
                        decode_branch(row),
                        decode_layer_stack_at(row, 5),
                        row.get::<_, Vec<u8>>(8),
                    ))
                },
            )
            .optional()?
            .ok_or(StoreError::NotFound("Branch"))
            .and_then(|(branch, layer_stack, root)| {
                Ok((branch?, layer_stack?, decode_object_id(root?)?))
            })
    }
}

// Restricted to pure inode-page inspection: a callback may not reenter the
// reader while its cache/SQLite row borrow is live. Published roots need no
// candidate overlay lookup. Preserve Store errors outside the content adapter.
struct PublishedInodeReader<'a> {
    reader: &'a SnapshotReader,
    error: std::cell::RefCell<Option<StoreError>>,
}
impl layerfs_content::object::access::ObjectRead for PublishedInodeReader<'_> {
    fn get(&self, _: ObjectId) -> layerfs_content::CoreResult<Vec<u8>> {
        Err(layerfs_content::CoreError::InvalidRecord(
            "borrowed inode reader",
        ))
    }
    fn with_authenticated_canonical<T, F>(
        &self,
        id: ObjectId,
        callback: F,
    ) -> layerfs_content::CoreResult<T>
    where
        F: FnOnce(&[u8]) -> layerfs_content::CoreResult<T>,
    {
        let result = (|| -> Result<T> {
            let started = Instant::now();
            {
                let cache = self
                    .reader
                    .cache
                    .lock()
                    .map_err(|_| StoreError::Integrity("snapshot object cache"))?;
                if let Some(bytes) = cache.rows.get(&id) {
                    // Cache insertion authenticated immutable identity already.
                    let result = callback(bytes);
                    self.reader.note_snapshot_cache(1, bytes.len() as u64)?;
                    self.reader
                        .note_local_read(1, 1, bytes.len() as u64, elapsed_ns(started))?;
                    return Ok(result?);
                }
            }
            let connection = self.reader.db.reader()?;
            let mut statement = connection.prepare_cached(crate::statements::objects::GET)?;
            let mut rows = statement.query([id.as_bytes().as_slice()])?;
            let row = rows
                .next()?
                .ok_or(StoreError::Integrity("visible object missing"))?;
            let bytes = row
                .get_ref(0)?
                .as_blob()
                .map_err(|_| StoreError::Integrity("object bytes"))?;
            layerfs_content::authenticate_identity(bytes, id)?;
            let result = callback(bytes);
            self.reader.note_snapshot_database(1, bytes.len() as u64)?;
            self.reader
                .note_local_read(1, 1, bytes.len() as u64, elapsed_ns(started))?;
            Ok(result?)
        })();
        result.map_err(|error| {
            *self.error.borrow_mut() = Some(error);
            layerfs_content::CoreError::InvalidRecord("published inode read")
        })
    }
}

impl SnapshotReader {
    /// Heap allocation made by Clone for the overlay handle vector. The Store,
    /// cache, metric state and candidate payloads remain shared Arc ownership.
    #[doc(hidden)]
    pub fn clone_owned_bytes(&self) -> usize {
        self.overlays
            .len()
            .saturating_mul(std::mem::size_of::<Arc<Mutex<DeferredObjectStore>>>())
    }
    #[doc(hidden)]
    pub fn owned_capacity_bytes(&self) -> usize {
        self.overlays
            .capacity()
            .saturating_mul(std::mem::size_of::<Arc<Mutex<DeferredObjectStore>>>())
    }

    /// Inspect an inode table using this snapshot's source ownership. Published
    /// snapshots borrow cache/SQLite bytes; reconciliation snapshots retain the
    /// existing overlay lookup order and reserve one owned canonical page. Both
    /// routes share the same checked traversal and caller scratch allowance.
    pub fn lookup_inode_with_budget(
        &self,
        root: layerfs_content::tree::inode::InodeTableRoot,
        key: layerfs_content::tree::inode::InodeId,
        maximum: usize,
        counters: &mut layerfs_content::tree::inode::InodeTableCounters,
    ) -> Result<(Option<ObjectId>, usize)> {
        if !self.overlays.is_empty() {
            return Ok(
                layerfs_content::tree::inode::inode_table_lookup_with_budget(
                    &crate::objects::CoreReader(self),
                    root,
                    key,
                    maximum,
                    counters,
                )?,
            );
        }
        let reader = PublishedInodeReader {
            reader: self,
            error: std::cell::RefCell::new(None),
        };
        let result = layerfs_content::tree::inode::inode_table_lookup_with_source_budget(
            &reader, root, key, maximum, 0, counters,
        );
        if let Some(error) = reader.error.into_inner() {
            return Err(error);
        }
        Ok(result?)
    }

    pub fn root(&self) -> ObjectId {
        self.root
    }

    pub fn reset_read_metrics(&self) -> Result<()> {
        *self
            .read_metrics
            .lock()
            .map_err(|_| StoreError::Integrity("read metrics"))? = WorkspaceReadReceipt::default();
        Ok(())
    }

    pub fn take_read_metrics(&self) -> Result<WorkspaceReadReceipt> {
        Ok(std::mem::take(
            &mut *self
                .read_metrics
                .lock()
                .map_err(|_| StoreError::Integrity("read metrics"))?,
        ))
    }

    pub fn read_metrics_snapshot(&self) -> Result<WorkspaceReadReceipt> {
        Ok(*self
            .read_metrics
            .lock()
            .map_err(|_| StoreError::Integrity("read metrics"))?)
    }

    pub fn take_create_metrics(&self) -> Result<(WorkspaceReadReceipt, u64, u64)> {
        let read = self.take_read_metrics()?;
        let cache = self
            .cache
            .lock()
            .map_err(|_| StoreError::Integrity("snapshot object cache"))?;
        Ok((read, cache.rows.len() as u64, cache.bytes as u64))
    }

    pub fn with_read_metrics_from(mut self, previous: &Self) -> Self {
        self.read_metrics = previous.read_metrics.clone();
        self.cache = previous.cache.clone();
        self
    }

    pub fn note_workspace_read(
        &self,
        requested_bytes: u64,
        output_bytes: u64,
        elapsed_ns: u64,
    ) -> Result<()> {
        let mut metrics = self
            .read_metrics
            .lock()
            .map_err(|_| StoreError::Integrity("read metrics"))?;
        metrics.workspace_read_calls = metrics.workspace_read_calls.saturating_add(1);
        metrics.workspace_requested_bytes = metrics
            .workspace_requested_bytes
            .saturating_add(requested_bytes);
        metrics.workspace_output_bytes =
            metrics.workspace_output_bytes.saturating_add(output_bytes);
        metrics.workspace_read_ns = metrics.workspace_read_ns.saturating_add(elapsed_ns);
        Ok(())
    }

    pub fn note_rope_read(
        &self,
        counters: layerfs_content::file::rope::RopeCounters,
    ) -> Result<()> {
        let mut metrics = self
            .read_metrics
            .lock()
            .map_err(|_| StoreError::Integrity("read metrics"))?;
        metrics.read_plan_builds = metrics.read_plan_builds.saturating_add(1);
        metrics.rope_nodes_read = metrics.rope_nodes_read.saturating_add(counters.nodes_read);
        metrics.payload_ids = metrics
            .payload_ids
            .saturating_add(counters.payload_ids_read);
        metrics.payload_batches = metrics
            .payload_batches
            .saturating_add(counters.payload_batches_read);
        metrics.max_payload_batch = metrics.max_payload_batch.max(counters.max_payload_batch);
        metrics.payload_bytes_read = metrics
            .payload_bytes_read
            .saturating_add(counters.payload_bytes_read);
        Ok(())
    }

    fn note_local_read(&self, ids: usize, rows: usize, bytes: u64, elapsed_ns: u64) -> Result<()> {
        let mut metrics = self
            .read_metrics
            .lock()
            .map_err(|_| StoreError::Integrity("read metrics"))?;
        metrics.local_calls = metrics.local_calls.saturating_add(1);
        metrics.local_ids = metrics.local_ids.saturating_add(ids as u64);
        metrics.local_rows = metrics.local_rows.saturating_add(rows as u64);
        metrics.local_bytes = metrics.local_bytes.saturating_add(bytes);
        metrics.local_read_auth_ns = metrics.local_read_auth_ns.saturating_add(elapsed_ns);
        Ok(())
    }

    fn note_snapshot_database(&self, rows: usize, bytes: u64) -> Result<()> {
        let mut metrics = self
            .read_metrics
            .lock()
            .map_err(|_| StoreError::Integrity("read metrics"))?;
        metrics.snapshot_database_calls = metrics.snapshot_database_calls.saturating_add(1);
        metrics.snapshot_database_rows = metrics.snapshot_database_rows.saturating_add(rows as u64);
        metrics.snapshot_database_bytes = metrics.snapshot_database_bytes.saturating_add(bytes);
        Ok(())
    }

    fn note_snapshot_cache(&self, rows: usize, bytes: u64) -> Result<()> {
        let mut metrics = self
            .read_metrics
            .lock()
            .map_err(|_| StoreError::Integrity("read metrics"))?;
        metrics.snapshot_cache_hits = metrics.snapshot_cache_hits.saturating_add(1);
        metrics.snapshot_cache_rows = metrics.snapshot_cache_rows.saturating_add(rows as u64);
        metrics.snapshot_cache_bytes = metrics.snapshot_cache_bytes.saturating_add(bytes);
        Ok(())
    }

    fn cached_object(&self, id: ObjectId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .cache
            .lock()
            .map_err(|_| StoreError::Integrity("snapshot object cache"))?
            .rows
            .get(&id)
            .cloned())
    }

    fn cache_object(&self, object: &CanonicalObject) -> Result<()> {
        self.cache_objects(std::slice::from_ref(object))
    }

    fn cache_objects(&self, objects: &[CanonicalObject]) -> Result<()> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| StoreError::Integrity("snapshot object cache"))?;
        for object in objects {
            if object.bytes.len() > SNAPSHOT_CACHE_OBJECT_BYTES
                || cache.rows.contains_key(&object.id)
            {
                continue;
            }
            let charge = object.bytes.len().saturating_add(64);
            // ponytail: requested immutable objects share one fixed 8 MiB cap;
            // add eviction only if measured reads need it.
            if cache.bytes.saturating_add(charge) > SNAPSHOT_CACHE_BYTES {
                continue;
            }
            cache.bytes += charge;
            cache.rows.insert(object.id, object.bytes.clone());
        }
        Ok(())
    }
}

impl ObjectSource for SnapshotReader {
    fn read_object_bounded(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>> {
        let started = Instant::now();
        {
            let cache = self
                .cache
                .lock()
                .map_err(|_| StoreError::Integrity("snapshot object cache"))?;
            if let Some(bytes) = cache.rows.get(&id) {
                if bytes.len() > maximum {
                    return Err(StoreError::Core(
                        layerfs_content::CoreError::ObjectLimitExceeded,
                    ));
                }
                self.note_snapshot_cache(1, bytes.len() as u64)?;
                self.note_local_read(1, 1, bytes.len() as u64, elapsed_ns(started))?;
                return Ok(bytes.clone());
            }
        }
        for overlay in &self.overlays {
            let result = overlay
                .lock()
                .map_err(|_| StoreError::Integrity("candidate overlay"))?
                .read_object_bounded(id, maximum);
            match result {
                Ok(bytes) => {
                    layerfs_content::authenticate_identity(&bytes, id)?;
                    self.note_local_read(1, 1, bytes.len() as u64, elapsed_ns(started))?;
                    return Ok(bytes);
                }
                Err(StoreError::MissingObject(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let bytes = self.db.read_object_bounded(id, maximum)?;
        self.note_snapshot_database(1, bytes.len() as u64)?;
        self.note_local_read(1, 1, bytes.len() as u64, elapsed_ns(started))?;
        Ok(bytes)
    }
    fn candidate_cleanup(&self) -> Option<crate::objects::CandidateCleanup> {
        Some(crate::objects::CandidateCleanup(self.db.clone()))
    }
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        let started = Instant::now();
        if let Some(bytes) = self.cached_object(id)? {
            self.note_snapshot_cache(1, bytes.len() as u64)?;
            self.note_local_read(1, 1, bytes.len() as u64, elapsed_ns(started))?;
            return Ok(bytes);
        }
        let mut candidate = None;
        for overlay in &self.overlays {
            match overlay
                .lock()
                .map_err(|_| StoreError::Integrity("candidate overlay"))?
                .read_object(id)
            {
                Ok(bytes) => {
                    candidate = Some(bytes);
                    break;
                }
                Err(StoreError::MissingObject(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let bytes = match candidate {
            Some(bytes) => {
                layerfs_content::authenticate_identity(&bytes, id)?;
                bytes
            }
            None => {
                let bytes = self.db.read_object_row(id)?;
                self.note_snapshot_database(1, bytes.len() as u64)?;
                bytes
            }
        };
        self.cache_object(&CanonicalObject {
            id,
            bytes: bytes.clone(),
        })?;
        self.note_local_read(1, 1, bytes.len() as u64, elapsed_ns(started))?;
        Ok(bytes)
    }

    fn read_authenticated_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        if ids.len() > crate::OBJECT_PAGE_COUNT {
            return Err(StoreError::InvalidInput("object read page"));
        }
        let started = Instant::now();
        let mut objects = vec![None; ids.len()];
        let mut database_ids = Vec::new();
        let mut database_slots = Vec::new();
        let mut missing = Vec::new();
        {
            let cache = self
                .cache
                .lock()
                .map_err(|_| StoreError::Integrity("snapshot object cache"))?;
            for (slot, id) in ids.iter().copied().enumerate() {
                match cache.rows.get(&id) {
                    Some(bytes) => {
                        self.note_snapshot_cache(1, bytes.len() as u64)?;
                        objects[slot] = Some(CanonicalObject {
                            id,
                            bytes: bytes.clone(),
                        });
                    }
                    None => missing.push((slot, id)),
                }
            }
        }
        for (slot, id) in missing {
            let mut candidate = None;
            for overlay in &self.overlays {
                match overlay
                    .lock()
                    .map_err(|_| StoreError::Integrity("candidate overlay"))?
                    .read_object(id)
                {
                    Ok(bytes) => {
                        layerfs_content::authenticate_identity(&bytes, id)?;
                        candidate = Some(CanonicalObject { id, bytes });
                        break;
                    }
                    Err(StoreError::MissingObject(_)) => {}
                    Err(error) => return Err(error),
                }
            }
            if let Some(object) = candidate {
                objects[slot] = Some(object);
            } else {
                database_ids.push(id);
                database_slots.push(slot);
            }
        }
        let database_objects = self.db.read_object_rows(&database_ids)?;
        if !database_ids.is_empty() {
            self.note_snapshot_database(
                database_objects.len(),
                database_objects
                    .iter()
                    .map(|object| object.bytes.len() as u64)
                    .sum(),
            )?;
        }
        for (slot, object) in database_slots.into_iter().zip(database_objects) {
            objects[slot] = Some(object);
        }
        let objects = objects
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(StoreError::Integrity("object read batch"))?;
        self.cache_objects(&objects)?;
        self.note_local_read(
            ids.len(),
            objects.len(),
            objects.iter().map(|object| object.bytes.len() as u64).sum(),
            elapsed_ns(started),
        )?;
        Ok(objects)
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inode_lookup_preserves_private_overlay_source_and_owned_budget() {
        use layerfs_content::object::access::ObjectStore;
        use layerfs_content::tree::inode::codec::{encode_inode_table_node, InodeTableNodeV1};
        use layerfs_content::tree::inode::{InodeId, InodeTableCounters, InodeTableRoot};
        let root = std::env::temp_dir().join(format!(
            "layerfs-overlay-inode-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let inode = InodeId::allocate([85; 32], 1);
        let record = ObjectId::for_bytes(b"private inode record");
        let canonical =
            encode_inode_table_node(&InodeTableNodeV1::Leaf(vec![(inode, record)])).unwrap();
        let mut buffer = crate::objects::ObjectBuffer::empty().unwrap();
        let id = buffer.put(&canonical).unwrap();
        assert!(
            store.db.read_object_row(id).is_err(),
            "private page is absent from SQLite"
        );
        let mut reader = store.snapshot_reader(id);
        reader
            .overlays
            .push(Arc::new(Mutex::new(buffer.into_resumable())));
        let mut counters = InodeTableCounters::default();
        assert!(matches!(
            reader.lookup_inode_with_budget(InodeTableRoot(id), inode, 8703, &mut counters),
            Err(StoreError::Core(
                layerfs_content::CoreError::ObjectLimitExceeded
            ))
        ));
        assert_eq!(
            counters.nodes_read, 0,
            "owned source rejected before reading or allocating"
        );
        assert_eq!(reader.read_metrics_snapshot().unwrap().local_calls, 0);
        assert_eq!(
            reader
                .lookup_inode_with_budget(InodeTableRoot(id), inode, 8704, &mut counters)
                .unwrap(),
            (Some(record), 8704)
        );
        assert_eq!(
            reader
                .read_metrics_snapshot()
                .unwrap()
                .snapshot_database_rows,
            0
        );
        assert!(
            store.db.read_object_row(id).is_err(),
            "lookup does not publish private preparation"
        );
        drop(reader);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn published_inode_lookup_borrows_sql_and_cache_with_tiny_budget() {
        use layerfs_content::tree::inode::codec::{encode_inode_table_node, InodeTableNodeV1};
        use layerfs_content::tree::inode::{InodeId, InodeTableCounters, InodeTableRoot};
        let root = std::env::temp_dir().join(format!(
            "layerfs-published-inode-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let inode = InodeId::allocate([83; 32], 1);
        let record = ObjectId::for_bytes(b"inode record identity");
        let canonical =
            encode_inode_table_node(&InodeTableNodeV1::Leaf(vec![(inode, record)])).unwrap();
        let id = ObjectId::for_bytes(&canonical);
        let corrupt_id = ObjectId::for_bytes(b"wrong inode page identity");
        {
            let connection = store.db.writer().unwrap();
            for key in [id, corrupt_id] {
                connection
                    .execute(
                        crate::statements::objects::INSERT,
                        rusqlite::params![key.as_bytes().as_slice(), &canonical],
                    )
                    .unwrap();
            }
        }
        let reader = store.snapshot_reader(id);
        let mut counters = InodeTableCounters::default();
        assert!(matches!(
            reader.lookup_inode_with_budget(InodeTableRoot(id), inode, 511, &mut counters),
            Err(StoreError::Core(
                layerfs_content::CoreError::ObjectLimitExceeded
            ))
        ));
        assert_eq!(counters.nodes_read, 0, "reject before borrowing a page");
        assert_eq!(
            reader
                .lookup_inode_with_budget(InodeTableRoot(id), inode, 512, &mut counters)
                .unwrap(),
            (Some(record), 512)
        );
        assert_eq!(
            reader
                .read_metrics_snapshot()
                .unwrap()
                .snapshot_database_rows,
            1
        );
        assert!(
            reader.cache.lock().unwrap().rows.is_empty(),
            "borrowed SQL inspection does not allocate a cache copy"
        );
        // The ordinary authenticated route populates the small immutable cache.
        assert_eq!(reader.read_object(id).unwrap(), canonical);
        reader.reset_read_metrics().unwrap();
        assert_eq!(
            reader
                .lookup_inode_with_budget(InodeTableRoot(id), inode, 512, &mut counters)
                .unwrap(),
            (Some(record), 512)
        );
        let metrics = reader.read_metrics_snapshot().unwrap();
        assert_eq!(metrics.snapshot_cache_rows, 1);
        assert_eq!(metrics.snapshot_database_rows, 0);
        assert!(matches!(
            reader.lookup_inode_with_budget(InodeTableRoot(corrupt_id), inode, 512, &mut counters),
            Err(StoreError::Integrity("object identity"))
        ));
        // A full canonical leaf uses the same copied-state allowance: its SQL
        // bytes stay in the already accounted Store page instead of a new Vec.
        let mut entries = (0..127)
            .map(|index| (InodeId::allocate([84; 32], index), record))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.0);
        let selected = entries[83].0;
        let large = encode_inode_table_node(&InodeTableNodeV1::Leaf(entries)).unwrap();
        let large_id = ObjectId::for_bytes(&large);
        store
            .db
            .writer()
            .unwrap()
            .execute(
                crate::statements::objects::INSERT,
                rusqlite::params![large_id.as_bytes().as_slice(), large],
            )
            .unwrap();
        assert_eq!(
            reader
                .lookup_inode_with_budget(InodeTableRoot(large_id), selected, 512, &mut counters)
                .unwrap(),
            (Some(record), 512)
        );
        drop(reader);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snapshot_cache_reads_only_requested_authenticated_objects_and_reuses_them() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-demand-cache-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let first = layerfs_content::encode_bytes_object(b"first").unwrap();
        let second = layerfs_content::encode_bytes_object(b"second").unwrap();
        let unrelated = layerfs_content::encode_bytes_object(b"unrelated").unwrap();
        let corrupt = layerfs_content::encode_bytes_object(b"corrupt").unwrap();
        let first_id = ObjectId::for_bytes(&first);
        let second_id = ObjectId::for_bytes(&second);
        let unrelated_id = ObjectId::for_bytes(&unrelated);
        let corrupt_id = ObjectId::for_bytes(b"different bytes");
        {
            let connection = store.db.writer().unwrap();
            for (id, bytes) in [
                (first_id, first.as_slice()),
                (second_id, second.as_slice()),
                (unrelated_id, unrelated.as_slice()),
                (corrupt_id, corrupt.as_slice()),
            ] {
                connection
                    .execute(
                        crate::statements::objects::INSERT,
                        rusqlite::params![id.as_bytes().as_slice(), bytes],
                    )
                    .unwrap();
            }
        }

        let reader = store.snapshot_reader(first_id);
        #[cfg(feature = "test-instrumentation")]
        {
            crate::schema::reset_sql_trace();
            crate::objects::reset_read_batch_counters();
        }
        let ids = [first_id, second_id, first_id];
        let objects = reader.read_authenticated_objects(&ids).unwrap();
        assert_eq!(
            objects.iter().map(|object| object.id).collect::<Vec<_>>(),
            ids
        );
        assert_eq!(objects[0].bytes, first);
        assert_eq!(objects[1].bytes, second);
        assert_eq!(objects[2].bytes, first);
        let metrics = reader.read_metrics_snapshot().unwrap();
        assert_eq!(metrics.snapshot_database_calls, 1);
        assert_eq!(metrics.snapshot_database_rows, 3);
        assert_eq!(
            metrics.snapshot_database_bytes,
            (first.len() * 2 + second.len()) as u64
        );
        assert_eq!(metrics.snapshot_cache_rows, 0);
        {
            let cache = reader.cache.lock().unwrap();
            assert_eq!(cache.rows.len(), 2);
            assert!(!cache.rows.contains_key(&unrelated_id));
            assert!(!cache.rows.contains_key(&corrupt_id));
        }
        #[cfg(feature = "test-instrumentation")]
        {
            let trace = crate::schema::sql_trace();
            assert!(trace.iter().all(|sql| !sql.contains("length(bytes)")));
            assert_eq!(
                trace
                    .iter()
                    .filter(|sql| sql.contains("WHERE object_id IN"))
                    .count(),
                1
            );
            assert_eq!(
                crate::objects::read_batch_counters(),
                crate::objects::ReadBatchCounters {
                    unique_hashes: 2,
                    cloned_bytes: first.len() as u64,
                }
            );
        }

        store
            .db
            .writer()
            .unwrap()
            .execute(
                "DELETE FROM objects WHERE object_id IN (?1, ?2)",
                rusqlite::params![
                    first_id.as_bytes().as_slice(),
                    second_id.as_bytes().as_slice()
                ],
            )
            .unwrap();
        #[cfg(feature = "test-instrumentation")]
        {
            crate::schema::reset_sql_trace();
            crate::objects::reset_read_batch_counters();
        }
        assert_eq!(reader.read_authenticated_objects(&ids).unwrap(), objects);
        assert_eq!(reader.read_object(first_id).unwrap(), first);
        let metrics = reader.read_metrics_snapshot().unwrap();
        assert_eq!(metrics.snapshot_database_calls, 1);
        assert_eq!(metrics.snapshot_cache_hits, 4);
        assert_eq!(metrics.snapshot_cache_rows, 4);
        #[cfg(feature = "test-instrumentation")]
        {
            assert!(crate::schema::sql_trace().is_empty());
            assert_eq!(
                crate::objects::read_batch_counters(),
                crate::objects::ReadBatchCounters::default()
            );
        }

        drop(reader);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
