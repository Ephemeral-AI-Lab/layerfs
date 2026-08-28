//! Private Operation and OperationVersion records.

use crate::full::branch::read::{BranchHead, VersionRef};
use crate::full::record_id::{BranchId, LeaseId, OperationId, OperationVersionId, RequestId};
use layerfs_core::ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationRecordRef {
    pub parent_branch_id: BranchId,
    pub operation_id: OperationId,
    pub operation_version_id: OperationVersionId,
    pub root: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationAdmission {
    pub operation_id: OperationId,
    pub branch_head: BranchHead,
    pub base: VersionRef,
    pub lease_id: LeaseId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationCandidate {
    pub operation_id: OperationId,
    pub expected: BranchHead,
    pub candidate_root: ObjectId,
    pub normalized_transition: Vec<u8>,
    pub request_id: RequestId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationCommitOutcome {
    WorkingRecorded {
        head: BranchHead,
        record: OperationRecordRef,
        reconciled: bool,
    },
    Conflict {
        actual: BranchHead,
        candidate: PreservedOperationCandidate,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreservedOperationCandidate {
    pub operation_id: OperationId,
    pub root: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoverableOperation {
    pub operation_id: OperationId,
    pub branch_id: BranchId,
    pub expected_branch_generation: u64,
    pub base_root: ObjectId,
    pub candidate_root: Option<ObjectId>,
    pub result_operation_version_id: Option<OperationVersionId>,
    pub state: RecoverableOperationState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoverableOperationState {
    Running,
    Candidate,
    Failed,
    Indeterminate,
    WorkingRecorded,
    Preserved,
}

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::read_branch_head;
use crate::full::closure::membership::{authenticate_root, authenticate_root_shallow};
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_state, commit_product_state_pair,
    rollback_product_transaction, Engine,
};
use crate::full::record_id::{bytes32, derive_id, object_id, transition_identity};
use crate::working::binding::effective_branch_base;
use crate::working::compaction::reachability::release_retained_root_if_unreferenced;
use crate::working::lease::unix_seconds;
use rusqlite::Connection;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_begin_operation(
        &self,
        operation_id: OperationId,
        expected: BranchHead,
        lease_id: LeaseId,
    ) -> EngineResult<OperationAdmission> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let actual = read_branch_head(&connection, expected.branch_id)?
                .ok_or(EngineError::InvalidRecord("Branch"))?;
            if actual != expected {
                return Err(EngineError::PublicationConflict);
            }
            let base = effective_branch_base(&connection, expected)?;
            let sequence = connection
                .query_row(
                    "SELECT COALESCE(MAX(sequence), -1) + 1
                     FROM layerfs_operations WHERE branch_id = ?1",
                    params![expected.branch_id.as_bytes()],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(map_sqlite_error)?;
            let (base_kind, base_stack, base_layer, base_version, target_kind, target_id) =
                match base {
                    VersionRef::Layer {
                        layer_stack_id,
                        layer_id,
                        ..
                    } => (
                        "layer",
                        Some(layer_stack_id.0),
                        Some(layer_id.0),
                        None,
                        "layer",
                        layer_id.0,
                    ),
                    VersionRef::OperationVersion {
                        operation_version_id,
                        ..
                    } => (
                        "operation_version",
                        None,
                        None,
                        Some(operation_version_id.0),
                        "operation_version",
                        operation_version_id.0,
                    ),
                };
            connection
                .execute(
                    "INSERT INTO layerfs_operations
                     (operation_id, branch_id, sequence, expected_branch_generation,
                      base_kind, base_layer_stack_id, base_layer_id,
                      base_operation_version_id, base_root_id, state)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'running')",
                    params![
                        operation_id.as_bytes(),
                        expected.branch_id.as_bytes(),
                        sequence,
                        i64::try_from(expected.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        base_kind,
                        base_stack.map(|id| id.to_vec()),
                        base_layer.map(|id| id.to_vec()),
                        base_version.map(|id| id.to_vec()),
                        base.root().as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, ?2, ?3, 'operation_workspace', ?4, ?5)",
                    params![
                        lease_id.as_bytes(),
                        target_kind,
                        target_id.as_slice(),
                        operation_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_operations WHERE operation_id = ?1 AND state = 'running')",
                operation_id.as_bytes(),
            )?;
            Ok(OperationAdmission {
                operation_id,
                branch_head: expected,
                base,
                lease_id,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_record_operation_candidate(
        &self,
        operation_id: OperationId,
        candidate_root: ObjectId,
    ) -> EngineResult<bool> {
        self.product_record_operation_candidate_inner(operation_id, candidate_root, false)
    }

    pub fn product_record_version_operation_candidate(
        &self,
        operation_id: OperationId,
        version: VersionRef,
    ) -> EngineResult<bool> {
        self.product_validate_version_ref(version)?;
        self.product_record_operation_candidate_inner(operation_id, version.root(), true)
    }

    fn product_record_operation_candidate_inner(
        &self,
        operation_id: OperationId,
        candidate_root: ObjectId,
        trusted_local: bool,
    ) -> EngineResult<bool> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if trusted_local {
                authenticate_root_shallow(self, &connection, candidate_root)?;
            } else {
                authenticate_root(self, &connection, candidate_root)?;
            }
            let incumbent = connection
                .query_row(
                    "SELECT candidate_root_id, state FROM layerfs_operations
                     WHERE operation_id = ?1",
                    params![operation_id.as_bytes()],
                    |row| Ok((row.get::<_, Option<Vec<u8>>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Operation"))?;
            if incumbent.1 == "candidate"
                && incumbent.0.as_deref() == Some(candidate_root.as_bytes())
            {
                return Ok(true);
            }
            if !matches!(incumbent.1.as_str(), "running" | "candidate") {
                return Err(EngineError::InvalidRecord("Operation candidate state"));
            }
            connection
                .execute(
                    "UPDATE layerfs_operations
                     SET candidate_root_id = ?1, state = 'candidate'
                     WHERE operation_id = ?2 AND state IN ('running', 'candidate')",
                    params![candidate_root.as_bytes(), operation_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT INTO layerfs_retained_roots (root_id) VALUES (?1)
                     ON CONFLICT(root_id) DO NOTHING",
                    params![candidate_root.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            if let Some(previous) = incumbent.0 {
                release_retained_root_if_unreferenced(&connection, &previous)?;
            }
            commit_product_state_pair(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_operations
                 WHERE operation_id = ?1 AND candidate_root_id = ?2 AND state = 'candidate')",
                operation_id.as_bytes(),
                candidate_root.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}
#[derive(Debug)]
pub(crate) struct StoredOperation {
    pub(crate) branch_id: BranchId,
    pub(crate) expected_generation: u64,
    pub(crate) base_root: ObjectId,
    pub(crate) candidate_root: Option<ObjectId>,
    pub(crate) state: String,
}

pub(crate) fn load_operation(
    connection: &Connection,
    operation_id: OperationId,
) -> EngineResult<StoredOperation> {
    connection
        .query_row(
            "SELECT branch_id, expected_branch_generation, base_root_id,
                    candidate_root_id, state
             FROM layerfs_operations WHERE operation_id = ?1",
            params![operation_id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(
            |(branch, generation, root, candidate_root, state)| -> EngineResult<StoredOperation> {
                Ok(StoredOperation {
                    branch_id: BranchId(bytes32(&branch, "BranchId")?),
                    expected_generation: u64::try_from(generation)
                        .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                    base_root: object_id(&root)?,
                    candidate_root: candidate_root.as_deref().map(object_id).transpose()?,
                    state,
                })
            },
        )
        .transpose()?
        .ok_or(EngineError::InvalidRecord("Operation"))
}

pub(crate) fn replay_operation_commit(
    engine: &Engine,
    connection: &Connection,
    candidate: &OperationCandidate,
) -> EngineResult<Option<OperationCommitOutcome>> {
    let row = connection
        .query_row(
            "SELECT o.branch_id, o.expected_branch_generation, o.base_root_id,
                    o.candidate_root_id, o.result_operation_version_id, o.state,
                    v.parent_operation_version_id, v.root_id,
                    bt.before_generation, bt.after_generation,
                    d.parent_root, d.child_root, d.payload, d.delta_id
             FROM layerfs_operations o
             JOIN layerfs_operation_versions v
               ON v.operation_version_id = o.result_operation_version_id
              AND v.branch_id = o.branch_id
             JOIN layerfs_operation_deltas od
               ON od.operation_id = o.operation_id
              AND od.operation_version_id = v.operation_version_id
             JOIN layerfs_deltas d
               ON d.delta_id = od.transition_delta_id AND d.format_version = 1
             JOIN layerfs_branch_transitions bt
               ON bt.branch_id = o.branch_id
              AND bt.after_operation_version_id = v.operation_version_id
              AND bt.action_kind = 'operation_commit'
              AND bt.request_id = ?2
             WHERE o.operation_id = ?1",
            params![
                candidate.operation_id.as_bytes(),
                candidate.request_id.as_bytes()
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<Vec<u8>>>(6)?,
                    row.get::<_, Vec<u8>>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<Vec<u8>>>(10)?,
                    row.get::<_, Vec<u8>>(11)?,
                    row.get::<_, Vec<u8>>(12)?,
                    row.get::<_, Vec<u8>>(13)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let version = OperationVersionId(derive_id(
        b"operation-version",
        &[
            candidate.operation_id.as_bytes(),
            candidate.request_id.as_bytes(),
            candidate.candidate_root.as_bytes(),
        ],
    ));
    let branch = BranchId(bytes32(&row.0, "BranchId")?);
    let expected_generation =
        u64::try_from(row.1).map_err(|_| EngineError::InvalidRecord("Branch generation"))?;
    let base_root = object_id(&row.2)?;
    let candidate_root = row.3.as_deref().map(object_id).transpose()?;
    let result_version = row
        .4
        .as_deref()
        .map(|id| bytes32(id, "OperationVersionId").map(OperationVersionId))
        .transpose()?;
    let parent_version = row
        .6
        .as_deref()
        .map(|id| bytes32(id, "OperationVersionId").map(OperationVersionId))
        .transpose()?;
    let result_root = object_id(&row.7)?;
    let before_generation =
        u64::try_from(row.8).map_err(|_| EngineError::InvalidRecord("Branch generation"))?;
    let after_generation =
        u64::try_from(row.9).map_err(|_| EngineError::InvalidRecord("Branch generation"))?;
    let transition_parent = row.10.as_deref().map(object_id).transpose()?;
    let transition_child = object_id(&row.11)?;
    let expected_transition = transition_identity(
        candidate.expected.root,
        candidate.candidate_root,
        &candidate.normalized_transition,
    );
    if row.5 != "working_recorded"
        || branch != candidate.expected.branch_id
        || expected_generation != candidate.expected.generation
        || base_root != candidate.expected.root
        || candidate_root != Some(candidate.candidate_root)
        || result_version != Some(version)
        || parent_version != candidate.expected.operation_version_id
        || result_root != candidate.candidate_root
        || before_generation != candidate.expected.generation
        || after_generation
            != before_generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?
        || transition_parent != Some(candidate.expected.root)
        || transition_child != candidate.candidate_root
        || row.12 != candidate.normalized_transition
        || bytes32(&row.13, "TransitionId")? != expected_transition
    {
        return Err(EngineError::InvalidRecord("Operation request replay"));
    }
    authenticate_root_shallow(engine, connection, candidate.candidate_root)?;
    Ok(Some(OperationCommitOutcome::WorkingRecorded {
        head: BranchHead {
            branch_id: branch,
            generation: after_generation,
            operation_version_id: Some(version),
            root: candidate.candidate_root,
        },
        record: OperationRecordRef {
            parent_branch_id: branch,
            operation_id: candidate.operation_id,
            operation_version_id: version,
            root: candidate.candidate_root,
        },
        reconciled: true,
    }))
}

pub(crate) fn next_operation_version_sequence(
    connection: &Connection,
    branch_id: BranchId,
) -> EngineResult<i64> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(sequence), -1) + 1
             FROM layerfs_operation_versions WHERE branch_id = ?1",
            params![branch_id.as_bytes()],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_sqlite_error)
}
