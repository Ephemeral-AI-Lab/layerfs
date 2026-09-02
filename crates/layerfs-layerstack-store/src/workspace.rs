use crate::objects::{
    admit_planned_objects, apply_reconcile_choices, combine_candidates, insert_object_batch,
    reconcile_candidate, BuildCounters, BuiltRoot, CanonicalObject, DeferredObjectStore,
    ObjectSource,
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

const SNAPSHOT_CACHE_BYTES: usize = 64 * 1024 * 1024;
const SNAPSHOT_CACHE_OBJECT_BYTES: usize = 1024;

#[derive(Default)]
struct SnapshotCache {
    rows: HashMap<ObjectId, Vec<u8>>,
    bytes: usize,
    loaded: bool,
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

    pub fn commit_reconciliation(
        &self,
        prepared: &PreparedReconciliation,
        working: BuiltRoot,
        choices: &[layerfs_content::filesystem::ReconcileChoice],
    ) -> Result<CommitOutcome> {
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
        let selected = apply_reconcile_choices(
            &reader,
            working_root,
            prepared.branch_root,
            prepared.layer_root,
            &prepared.conflicts,
            choices,
        )?;
        let working = working
            .lock()
            .map_err(|_| StoreError::Integrity("reconciliation candidate"))?;
        let prepared_objects = prepared
            .objects
            .lock()
            .map_err(|_| StoreError::Integrity("reconciliation candidate"))?;
        let objects = combine_candidates(
            selected.root_id,
            &[&working, &prepared_objects, &selected.objects],
        )?;
        self.commit_candidate(
            &branch,
            prepared.branch_root,
            prepared.current_layer_id,
            BuiltRoot {
                root_id: selected.root_id,
                objects,
                counters: selected.counters,
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
        if built.root_id == expected_root && new_base_layer_id == expected.base_layer_id {
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
        let plan = self.db.plan_candidate(&built.objects)?;
        crate::telemetry::note_workspace_commit_phase(
            crate::WorkspaceCommitPhase::LocalAdmission,
            elapsed_ns(started),
        );
        let started = Instant::now();
        let mut statement_number = 0;
        let admission =
            admit_planned_objects(&self.db, &built.objects, &plan, &mut statement_number)?;
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
        crate::telemetry::record_candidate(crate::CandidateReceipt {
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
        })?;
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

impl SnapshotReader {
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

    fn cached_object(&self, id: ObjectId) -> Result<Option<Vec<u8>>> {
        self.load_snapshot_cache()?;
        Ok(self
            .cache
            .lock()
            .map_err(|_| StoreError::Integrity("snapshot object cache"))?
            .rows
            .get(&id)
            .cloned())
    }

    fn load_snapshot_cache(&self) -> Result<()> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| StoreError::Integrity("snapshot object cache"))?;
        if cache.loaded {
            return Ok(());
        }
        let started = Instant::now();
        let objects = self
            .db
            .read_small_object_rows(SNAPSHOT_CACHE_OBJECT_BYTES, SNAPSHOT_CACHE_BYTES)?;
        let rows = objects.len();
        let bytes = objects.iter().map(|object| object.bytes.len() as u64).sum();
        for object in objects {
            let charge = object.bytes.len().saturating_add(64);
            cache.bytes += charge;
            cache.rows.insert(object.id, object.bytes);
        }
        cache.loaded = true;
        drop(cache);
        self.note_local_read(rows, rows, bytes, elapsed_ns(started))
    }

    fn cache_object(&self, object: &CanonicalObject) -> Result<()> {
        if object.bytes.len() > SNAPSHOT_CACHE_OBJECT_BYTES {
            return Ok(());
        }
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| StoreError::Integrity("snapshot object cache"))?;
        if cache.rows.contains_key(&object.id) {
            return Ok(());
        }
        let charge = object.bytes.len().saturating_add(64);
        // ponytail: retain the first 64 MiB of small immutable structure; add
        // eviction only if a larger namespace proves this bounded cache misses.
        if cache.bytes.saturating_add(charge) > SNAPSHOT_CACHE_BYTES {
            return Ok(());
        }
        cache.bytes += charge;
        cache.rows.insert(object.id, object.bytes.clone());
        Ok(())
    }
}

impl ObjectSource for SnapshotReader {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        let started = Instant::now();
        if let Some(bytes) = self.cached_object(id)? {
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
            None => self.db.read_object_row(id)?,
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
        for (slot, id) in ids.iter().copied().enumerate() {
            if let Some(bytes) = self.cached_object(id)? {
                objects[slot] = Some(CanonicalObject { id, bytes });
                continue;
            }
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
                self.cache_object(&object)?;
                objects[slot] = Some(object);
            } else {
                database_ids.push(id);
                database_slots.push(slot);
            }
        }
        for (slot, object) in database_slots
            .into_iter()
            .zip(self.db.read_object_rows(&database_ids)?)
        {
            self.cache_object(&object)?;
            objects[slot] = Some(object);
        }
        let objects = objects
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(StoreError::Integrity("object read batch"))?;
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
