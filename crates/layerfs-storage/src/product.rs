use super::*;
use layerfs_core::namespace_codec::decode_namespace_root;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

macro_rules! record_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        pub struct $name(pub [u8; 32]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }
    };
}

record_id!(LayerStackId);
record_id!(LayerId);
record_id!(BranchId);
record_id!(OperationId);
record_id!(OperationVersionId);
record_id!(RequestId);
record_id!(LeaseId);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LayerStackHead {
    pub layer_stack_id: LayerStackId,
    pub generation: u64,
    pub layer_id: LayerId,
    pub root: ObjectId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchHead {
    pub branch_id: BranchId,
    pub generation: u64,
    pub operation_version_id: Option<OperationVersionId>,
    pub root: ObjectId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchAncestry {
    pub immediate_parent_branch_id: Option<BranchId>,
    pub fork_operation_id: Option<OperationId>,
    pub fork_operation_version_id: Option<OperationVersionId>,
    pub fork_root: ObjectId,
    pub origin_layer_stack_id: LayerStackId,
    pub origin_layer_id: LayerId,
    pub depth: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum VersionRef {
    Layer {
        layer_stack_id: LayerStackId,
        layer_id: LayerId,
        root: ObjectId,
    },
    OperationVersion {
        branch_id: BranchId,
        operation_version_id: OperationVersionId,
        root: ObjectId,
    },
}

impl VersionRef {
    pub const fn root(self) -> ObjectId {
        match self {
            Self::Layer { root, .. } | Self::OperationVersion { root, .. } => root,
        }
    }
}

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChildMergeCandidate {
    pub source: BranchHead,
    pub expected_parent: BranchHead,
    pub result_root: ObjectId,
    pub source_transition: Vec<u8>,
    pub applied_transition: Vec<u8>,
    pub request_id: RequestId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChildMergeOutcome {
    WorkingRecorded {
        parent_head: BranchHead,
        reconciled: bool,
    },
    Conflict {
        actual_parent: BranchHead,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChildMergePublication {
    pub candidate: ChildMergeCandidate,
    pub accepted_parent: BranchHead,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LayerCandidate {
    pub layer_stack_id: LayerStackId,
    pub layer_id: LayerId,
    pub parent_layer_id: LayerId,
    pub source: BranchHead,
    pub source_depth: u64,
    pub root: ObjectId,
    pub request_id: RequestId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerCandidateRequest {
    pub source: BranchHead,
    pub expected_stack: LayerStackHead,
    pub result_root: ObjectId,
    pub source_transition: Vec<u8>,
    pub applied_transition: Vec<u8>,
    pub request_id: RequestId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LayerStackMergeOutcome {
    DurablyAccepted {
        head: LayerStackHead,
        reconciled: bool,
    },
    Conflict {
        actual: LayerStackHead,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BranchRollbackOutcome {
    WorkingRecorded { head: BranchHead, reconciled: bool },
    Conflict { actual: BranchHead },
    Blocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchRollbackPublication {
    pub expected: BranchHead,
    pub target: OperationVersionId,
    pub request_id: RequestId,
    pub accepted: BranchHead,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LayerStackRollbackOutcome {
    DurablyAccepted {
        head: LayerStackHead,
        reconciled: bool,
    },
    Conflict {
        actual: LayerStackHead,
    },
    Blocked,
}

pub const MAX_PUSH_OPERATION_RECORDS: usize = 1024;
pub const MAX_HISTORY_PAGE_RECORDS: usize = 64;
pub const MAX_TRANSITION_PAYLOAD_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedRelease {
    pub generation: u64,
    pub request_id: RequestId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedOperation {
    pub operation_id: OperationId,
    pub operation_sequence: u64,
    pub expected_branch_generation: u64,
    pub base: VersionRef,
    pub operation_version_id: OperationVersionId,
    pub version_sequence: u64,
    pub parent_operation_version_id: Option<OperationVersionId>,
    pub root: ObjectId,
    pub release: Option<PushedRelease>,
    pub operation_delta_id: [u8; 32],
    pub transition_delta_id: [u8; 32],
    pub transition_payload: Vec<u8>,
    pub request_id: RequestId,
    pub before_generation: u64,
    pub after_generation: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedChildMerge {
    pub operation_version_id: OperationVersionId,
    pub version_sequence: u64,
    pub parent_operation_version_id: Option<OperationVersionId>,
    pub root: ObjectId,
    pub release: Option<PushedRelease>,
    pub source_branch_id: BranchId,
    pub source_branch_generation: u64,
    pub source_operation_version_id: OperationVersionId,
    pub branch_delta_id: [u8; 32],
    pub base_root: ObjectId,
    pub source_root: ObjectId,
    pub destination_root: ObjectId,
    pub source_delta_id: [u8; 32],
    pub source_transition_payload: Vec<u8>,
    pub applied_delta_id: [u8; 32],
    pub applied_transition_payload: Vec<u8>,
    pub request_id: RequestId,
    pub before_generation: u64,
    pub after_generation: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedBranchRollback {
    pub before_operation_version_id: OperationVersionId,
    pub target_operation_version_id: OperationVersionId,
    pub target_root: ObjectId,
    pub request_id: RequestId,
    pub before_generation: u64,
    pub after_generation: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedLayerMerge {
    pub source_branch_id: BranchId,
    pub source_branch_depth: u64,
    pub source_branch_generation: u64,
    pub source_operation_version_id: OperationVersionId,
    pub request_id: RequestId,
    pub branch_delta_id: [u8; 32],
    pub base_root: ObjectId,
    pub source_root: ObjectId,
    pub destination_root: ObjectId,
    pub source_delta_id: [u8; 32],
    pub source_transition_payload: Vec<u8>,
    pub applied_delta_id: [u8; 32],
    pub applied_transition_payload: Vec<u8>,
    pub layer_delta_id: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedLayer {
    pub layer_id: LayerId,
    pub parent_layer_id: Option<LayerId>,
    pub root: ObjectId,
    pub release: Option<PushedRelease>,
    pub accepted_generation: u64,
    pub merge: Option<PushedLayerMerge>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PushedLayerStackAction {
    Merge,
    Rollback,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedLayerStackTransition {
    pub before_generation: u64,
    pub after_generation: u64,
    pub before_layer_id: LayerId,
    pub after_layer_id: LayerId,
    pub action: PushedLayerStackAction,
    pub source_record_id: [u8; 32],
    pub request_id: RequestId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedLayerStack {
    pub name: String,
    pub base: Option<LayerStackHead>,
    pub head: LayerStackHead,
    pub complete: bool,
    pub layers: Vec<PushedLayer>,
    pub transitions: Vec<PushedLayerStackTransition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchPushBundle {
    pub name: Option<String>,
    pub ancestry: BranchAncestry,
    pub base: Option<BranchHead>,
    pub head: BranchHead,
    pub complete: bool,
    pub operations: Vec<PushedOperation>,
    pub child_merges: Vec<PushedChildMerge>,
    pub rollbacks: Vec<PushedBranchRollback>,
    pub origin_stack: PushedLayerStack,
    pub dependencies: Vec<BranchPushBundle>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BranchPushOutcome {
    DurablyAccepted { head: BranchHead, reconciled: bool },
    Conflict { actual: Option<BranchHead> },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SyncTransferCounters {
    pub unique_bytes: u64,
    pub resumed_bytes: u64,
    pub retransmitted_bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchPushRequest {
    pub request_id: RequestId,
    pub expected: Option<BranchHead>,
    pub counters: SyncTransferCounters,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedFetchRequest {
    pub request_id: RequestId,
    pub durable_storage_id: [u8; 32],
    pub counters: SyncTransferCounters,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredTransferState {
    pub batch_sequence: u64,
    pub cursor: Vec<u8>,
    pub complete: bool,
    pub counters: SyncTransferCounters,
}

impl Engine {
    pub fn product_layer_stack_head(
        &self,
        layer_stack_id: LayerStackId,
    ) -> EngineResult<Option<LayerStackHead>> {
        let connection = self.lock_connection()?;
        read_layer_stack_head(&connection, layer_stack_id)
    }

    pub fn product_fetch_resume_layer_stack_head(
        &self,
        layer_stack_id: LayerStackId,
    ) -> EngineResult<Option<LayerStackHead>> {
        let connection = self.lock_connection()?;
        read_fetch_stack_head(&connection, layer_stack_id)
    }

    pub fn product_layer_root(
        &self,
        layer_stack_id: LayerStackId,
        layer_id: LayerId,
    ) -> EngineResult<Option<ObjectId>> {
        let connection = self.lock_connection()?;
        read_layer_root(&connection, layer_stack_id, layer_id)
    }

    pub fn product_branch_head(&self, branch_id: BranchId) -> EngineResult<Option<BranchHead>> {
        let connection = self.lock_connection()?;
        read_branch_head(&connection, branch_id)
    }

    pub fn product_branch_has_special_history_after(
        &self,
        branch_id: BranchId,
        generation: u64,
    ) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_branch_transitions
                 WHERE branch_id = ?1 AND after_generation > ?2
                   AND action_kind != 'operation_commit')",
                params![
                    branch_id.as_bytes(),
                    i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)
    }

    pub fn product_fetch_resume_branch_head(
        &self,
        branch_id: BranchId,
    ) -> EngineResult<Option<BranchHead>> {
        let connection = self.lock_connection()?;
        read_fetch_branch_head(&connection, branch_id)
    }

    pub fn product_contains_branch_head(&self, head: BranchHead) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        if head.generation == 0 && head.operation_version_id.is_none() {
            return Ok(read_branch_ancestry(&connection, head.branch_id)?
                .is_some_and(|ancestry| ancestry.fork_root == head.root));
        }
        branch_contains_exact_version(&connection, head)
    }

    pub fn product_branch_contains_root(
        &self,
        branch_id: BranchId,
        root: ObjectId,
    ) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        let fork = read_branch_ancestry(&connection, branch_id)?
            .is_some_and(|ancestry| ancestry.fork_root == root);
        if fork {
            return Ok(true);
        }
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_operation_versions
                 WHERE branch_id = ?1 AND root_id = ?2
                   AND NOT EXISTS(
                       SELECT 1 FROM layerfs_released_versions r
                       WHERE r.target_kind = 'operation_version'
                         AND r.owner_id = layerfs_operation_versions.branch_id
                         AND r.version_id = layerfs_operation_versions.operation_version_id))",
                params![branch_id.as_bytes(), root.as_bytes()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)
    }

    pub fn product_branch_ancestry(
        &self,
        branch_id: BranchId,
    ) -> EngineResult<Option<BranchAncestry>> {
        let connection = self.lock_connection()?;
        read_branch_ancestry(&connection, branch_id)
    }

    pub fn product_pin_branch_version(&self, head: BranchHead) -> EngineResult<VersionRef> {
        let connection = self.lock_connection()?;
        let retained = if head.generation == 0 && head.operation_version_id.is_none() {
            read_branch_ancestry(&connection, head.branch_id)?
                .is_some_and(|ancestry| ancestry.fork_root == head.root)
        } else {
            branch_contains_exact_version(&connection, head)?
        };
        if !retained {
            return Err(EngineError::InvalidRecord("Branch version"));
        }
        effective_branch_base(&connection, head)
    }

    pub fn product_validate_version_ref(&self, version: VersionRef) -> EngineResult<()> {
        let connection = self.lock_connection()?;
        let present = match version {
            VersionRef::Layer {
                layer_stack_id,
                layer_id,
                root,
            } => connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_layers
                     WHERE layer_stack_id = ?1 AND layer_id = ?2 AND root_id = ?3
                       AND NOT EXISTS(
                           SELECT 1 FROM layerfs_released_versions r
                           WHERE r.target_kind = 'layer'
                             AND r.owner_id = layerfs_layers.layer_stack_id
                             AND r.version_id = layerfs_layers.layer_id))",
                    params![
                        layer_stack_id.as_bytes(),
                        layer_id.as_bytes(),
                        root.as_bytes()
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?,
            VersionRef::OperationVersion {
                branch_id,
                operation_version_id,
                root,
            } => connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2 AND root_id = ?3
                       AND NOT EXISTS(
                           SELECT 1 FROM layerfs_released_versions r
                           WHERE r.target_kind = 'operation_version'
                             AND r.owner_id = layerfs_operation_versions.branch_id
                             AND r.version_id = layerfs_operation_versions.operation_version_id))",
                    params![
                        branch_id.as_bytes(),
                        operation_version_id.as_bytes(),
                        root.as_bytes()
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?,
        };
        if !present {
            return Err(EngineError::InvalidRecord("VersionRef"));
        }
        Ok(())
    }

    pub fn product_has_verified_branch_tracking(
        &self,
        durable_storage_id: [u8; 32],
        head: BranchHead,
    ) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        connection
            .query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM layerfs_durable_tracking_refs
                     WHERE durable_storage_id = ?1 AND target_kind = 'branch'
                       AND target_id = ?2 AND generation = ?3 AND root_id = ?4
                       AND target_version_id IS ?5
                       AND status = 'verified_complete')",
                params![
                    durable_storage_id.as_slice(),
                    head.branch_id.as_bytes(),
                    i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?,
                    head.root.as_bytes(),
                    head.operation_version_id
                        .map(|id| id.as_bytes().as_slice().to_vec()),
                ],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn product_record_transfer_state(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        batch_sequence: u64,
        direction: &str,
        cursor: &[u8],
        complete: bool,
        counters: SyncTransferCounters,
    ) -> EngineResult<bool> {
        if !matches!(direction, "fetch" | "push") || !(40..=41_008).contains(&cursor.len()) {
            return Err(EngineError::InvalidRecord("transfer state"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let sequence =
                i64::try_from(batch_sequence).map_err(|_| EngineError::CounterOverflow)?;
            let incumbent = connection
                .query_row(
                    "SELECT owner_request_id, direction FROM layerfs_transfer_state
                     WHERE request_id = ?1 AND batch_sequence = ?2",
                    params![request_id.as_bytes(), sequence],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            if incumbent.as_ref().is_some_and(|value| {
                value.0.as_slice() != owner_request_id.as_bytes() || value.1 != direction
            }) {
                return Err(EngineError::InvalidRecord("transfer request direction"));
            }
            connection
                .execute(
                    "INSERT INTO layerfs_transfer_state
                     (owner_request_id, request_id, batch_sequence, direction, cursor, state,
                      unique_bytes, resumed_bytes, retransmitted_bytes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(request_id, batch_sequence) DO UPDATE SET
                       cursor = excluded.cursor,
                       state = excluded.state,
                       unique_bytes = excluded.unique_bytes,
                       resumed_bytes = excluded.resumed_bytes,
                       retransmitted_bytes = excluded.retransmitted_bytes",
                    params![
                        owner_request_id.as_bytes(),
                        request_id.as_bytes(),
                        sequence,
                        direction,
                        cursor,
                        if complete { "complete" } else { "transferring" },
                        i64::try_from(counters.unique_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(counters.resumed_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(counters.retransmitted_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            let reconciliation = format!(
                "SELECT EXISTS(SELECT 1 FROM layerfs_transfer_state \
                 WHERE request_id = ?1 AND batch_sequence = {sequence})"
            );
            commit_product_state(
                self,
                &mut connection,
                &reconciliation,
                request_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_latest_transfer_state(
        &self,
        request_id: RequestId,
        direction: &str,
    ) -> EngineResult<Option<StoredTransferState>> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("transfer direction"));
        }
        let connection = self.lock_connection()?;
        connection
            .query_row(
                "SELECT batch_sequence, cursor, state, unique_bytes, resumed_bytes,
                        retransmitted_bytes
                 FROM layerfs_transfer_state
                 WHERE request_id = ?1 AND direction = ?2
                 ORDER BY batch_sequence DESC LIMIT 1",
                params![request_id.as_bytes(), direction],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?
            .map(
                |(batch_sequence, cursor, state, unique, resumed, retransmitted)| {
                    Ok(StoredTransferState {
                        batch_sequence: u64::try_from(batch_sequence)
                            .map_err(|_| EngineError::InvalidRecord("transfer sequence"))?,
                        cursor,
                        complete: state == "complete",
                        counters: SyncTransferCounters {
                            unique_bytes: u64::try_from(unique)
                                .map_err(|_| EngineError::InvalidRecord("transfer bytes"))?,
                            resumed_bytes: u64::try_from(resumed)
                                .map_err(|_| EngineError::InvalidRecord("transfer bytes"))?,
                            retransmitted_bytes: u64::try_from(retransmitted)
                                .map_err(|_| EngineError::InvalidRecord("transfer bytes"))?,
                        },
                    })
                },
            )
            .transpose()
    }

    pub fn product_clear_transfer_state(
        &self,
        request_id: RequestId,
        direction: &str,
    ) -> EngineResult<bool> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("transfer direction"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            connection
                .execute(
                    "DELETE FROM layerfs_transfer_state
                     WHERE request_id = ?1 AND direction = ?2",
                    params![request_id.as_bytes(), direction],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT NOT EXISTS(SELECT 1 FROM layerfs_transfer_state
                 WHERE request_id = ?1)",
                request_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_clear_transfer_state_owner(
        &self,
        owner_request_id: RequestId,
        direction: &str,
    ) -> EngineResult<bool> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("transfer direction"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            connection
                .execute(
                    "DELETE FROM layerfs_transfer_state
                     WHERE owner_request_id = ?1 AND direction = ?2",
                    params![owner_request_id.as_bytes(), direction],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT NOT EXISTS(SELECT 1 FROM layerfs_transfer_state
                 WHERE owner_request_id = ?1)",
                owner_request_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_record_push_outbox(
        &self,
        request_id: RequestId,
        durable_storage_id: [u8; 32],
        head: BranchHead,
        expected_durable_generation: Option<u64>,
        state: &str,
    ) -> EngineResult<bool> {
        if !matches!(
            state,
            "selected" | "transferring" | "transferred" | "accepted" | "conflict" | "indeterminate"
        ) {
            return Err(EngineError::InvalidRecord("Push outbox state"));
        }
        {
            let connection = self.lock_connection()?;
            let retained = if head.generation == 0 && head.operation_version_id.is_none() {
                read_branch_ancestry(&connection, head.branch_id)?
                    .is_some_and(|ancestry| ancestry.fork_root == head.root)
            } else {
                branch_contains_exact_version(&connection, head)?
            };
            if !retained {
                return Err(EngineError::PublicationConflict);
            }
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            connection
                .execute(
                    "INSERT INTO layerfs_durable_storages
                     (durable_storage_id, authenticated_at) VALUES (?1, ?2)
                     ON CONFLICT(durable_storage_id) DO UPDATE
                     SET authenticated_at = excluded.authenticated_at",
                    params![durable_storage_id.as_slice(), unix_seconds()?],
                )
                .map_err(map_sqlite_error)?;
            let incumbent = connection
                .query_row(
                    "SELECT durable_storage_id, branch_id, operation_version_id,
                            accepted_generation, accepted_root_id,
                            expected_durable_generation, state
                     FROM layerfs_push_outbox WHERE request_id = ?1",
                    params![request_id.as_bytes()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Vec<u8>>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, Vec<u8>>(4)?,
                            row.get::<_, Option<i64>>(5)?,
                            row.get::<_, String>(6)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?;
            let version = head
                .operation_version_id
                .map(|id| id.as_bytes().as_slice().to_vec());
            let generation = expected_durable_generation
                .map(i64::try_from)
                .transpose()
                .map_err(|_| EngineError::CounterOverflow)?;
            if let Some(incumbent) = incumbent {
                if incumbent.0.as_slice() != durable_storage_id
                    || incumbent.1.as_slice() != head.branch_id.as_bytes()
                    || incumbent.2 != version
                    || u64::try_from(incumbent.3).ok() != Some(head.generation)
                    || incumbent.4.as_slice() != head.root.as_bytes()
                    || incumbent.5 != generation
                {
                    return Err(EngineError::InvalidRecord("Push outbox request conflict"));
                }
                if matches!(incumbent.6.as_str(), "accepted" | "conflict") {
                    if state == incumbent.6 || !matches!(state, "accepted" | "conflict") {
                        return Ok(true);
                    }
                    return Err(EngineError::InvalidRecord("Push outbox terminal conflict"));
                }
                connection
                    .execute(
                        "UPDATE layerfs_push_outbox SET state = ?1 WHERE request_id = ?2",
                        params![state, request_id.as_bytes()],
                    )
                    .map_err(map_sqlite_error)?;
            } else {
                connection
                    .execute(
                        "INSERT INTO layerfs_push_outbox
                         (request_id, durable_storage_id, branch_id,
                          operation_version_id, accepted_generation, accepted_root_id,
                          expected_durable_generation, state)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            request_id.as_bytes(),
                            durable_storage_id.as_slice(),
                            head.branch_id.as_bytes(),
                            version,
                            i64::try_from(head.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            head.root.as_bytes(),
                            generation,
                            state,
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            let reconciliation = format!(
                "SELECT EXISTS(SELECT 1 FROM layerfs_push_outbox \
                 WHERE request_id = ?1 AND state = '{state}')"
            );
            commit_product_state(
                self,
                &mut connection,
                &reconciliation,
                request_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_push_outbox_state(&self, request_id: RequestId) -> EngineResult<Option<String>> {
        self.lock_connection()?
            .query_row(
                "SELECT state FROM layerfs_push_outbox WHERE request_id = ?1",
                params![request_id.as_bytes()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(map_sqlite_error)
    }

    pub fn product_push_outbox_head(
        &self,
        request_id: RequestId,
    ) -> EngineResult<Option<(BranchHead, String)>> {
        let connection = self.lock_connection()?;
        let row = connection
            .query_row(
                "SELECT branch_id, operation_version_id, accepted_generation,
                        accepted_root_id, state
                 FROM layerfs_push_outbox WHERE request_id = ?1",
                params![request_id.as_bytes()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Option<Vec<u8>>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let Some((branch, version, generation, root, state)) = row else {
            return Ok(None);
        };
        let branch = BranchId(bytes32(&branch, "BranchId")?);
        let head = BranchHead {
            branch_id: branch,
            generation: u64::try_from(generation)
                .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
            operation_version_id: version
                .map(|version| bytes32(&version, "OperationVersionId").map(OperationVersionId))
                .transpose()?,
            root: object_id(&root)?,
        };
        Ok(Some((head, state)))
    }

    pub fn product_reconcile_branch_push(
        &self,
        request_id: RequestId,
        expected: Option<BranchHead>,
        accepted: BranchHead,
    ) -> EngineResult<BranchPushOutcome> {
        let connection = self.lock_connection()?;
        let row = connection
            .query_row(
                "SELECT direction, candidate_kind, candidate_id,
                        expected_head_id, expected_generation, result,
                        accepted_head_id, accepted_generation, accepted_root_id
                 FROM layerfs_sync_receipts WHERE request_id = ?1",
                params![request_id.as_bytes()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Option<Vec<u8>>>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<Vec<u8>>>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, Option<Vec<u8>>>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or(EngineError::InvalidRecord("Push receipt"))?;
        let expected_version = expected
            .and_then(|head| head.operation_version_id)
            .map(|id| id.as_bytes().as_slice().to_vec());
        let expected_generation = expected
            .map(|head| i64::try_from(head.generation))
            .transpose()
            .map_err(|_| EngineError::CounterOverflow)?;
        let retained = if accepted.generation == 0 && accepted.operation_version_id.is_none() {
            read_branch_ancestry(&connection, accepted.branch_id)?
                .is_some_and(|ancestry| ancestry.fork_root == accepted.root)
        } else {
            branch_contains_exact_version(&connection, accepted)?
        };
        if row.0 != "push"
            || row.1 != "branch"
            || row.2.as_slice() != accepted.branch_id.as_bytes()
            || row.3 != expected_version
            || row.4 != expected_generation
            || row.5 != "durably_accepted"
            || row.6
                != accepted
                    .operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec())
            || row.7
                != Some(
                    i64::try_from(accepted.generation).map_err(|_| EngineError::CounterOverflow)?,
                )
            || row.8.as_deref() != Some(accepted.root.as_bytes())
            || !retained
        {
            return Err(EngineError::InvalidRecord("Push reconciliation"));
        }
        Ok(BranchPushOutcome::DurablyAccepted {
            head: accepted,
            reconciled: true,
        })
    }

    pub fn product_export_branch_push(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
    ) -> EngineResult<BranchPushBundle> {
        let connection = self.lock_connection()?;
        let ancestry = read_branch_ancestry(&connection, branch_id)?
            .ok_or(EngineError::InvalidRecord("Branch ancestry"))?;
        let stack = read_layer_stack_head(&connection, ancestry.origin_layer_stack_id)?
            .ok_or(EngineError::InvalidRecord("LayerStack"))?;
        drop(connection);
        self.product_export_branch_push_one(branch_id, base, Some(stack))
    }

    pub fn product_export_branch_fetch(
        &self,
        branch_id: BranchId,
    ) -> EngineResult<BranchPushBundle> {
        self.product_export_branch_fetch_page(branch_id, None, None)
    }

    pub fn product_export_branch_fetch_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> EngineResult<BranchPushBundle> {
        self.product_export_branch_push_one(branch_id, base, origin_stack_base)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn product_branch_fetch_object_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
        after: Option<ObjectId>,
        limit: usize,
    ) -> EngineResult<Vec<ObjectId>> {
        let bundle = self.product_export_branch_fetch_page(branch_id, base, origin_stack_base)?;
        if bundle.head != expected_head || bundle.origin_stack.head != expected_stack_head {
            return Err(EngineError::PublicationConflict);
        }
        let mut roots = std::collections::BTreeSet::new();
        collect_fetch_roots(&bundle, &mut roots)?;
        if limit == 0 || limit > 1024 {
            return Err(EngineError::InvalidRecord("closure page limit"));
        }
        let base_generation = base.map_or(0, |head| head.generation).to_be_bytes();
        let base_version = base
            .and_then(|head| head.operation_version_id)
            .map_or([0; 32], |id| id.0);
        let base_root = base.map_or([0; 32], |head| head.root.to_bytes());
        let stack_generation = origin_stack_base
            .map_or(0, |head| head.generation)
            .to_be_bytes();
        let stack_layer = origin_stack_base.map_or([0; 32], |head| head.layer_id.0);
        let exported_generation = bundle.head.generation.to_be_bytes();
        let exported_version = bundle.head.operation_version_id.map_or([0; 32], |id| id.0);
        let exported_root = bundle.head.root.to_bytes();
        let exported_stack_generation = bundle.origin_stack.head.generation.to_be_bytes();
        let exported_stack_layer = bundle.origin_stack.head.layer_id.0;
        let exported_stack_root = bundle.origin_stack.head.root.to_bytes();
        let closure_id = derive_id(
            b"fetch-closure",
            &[
                branch_id.as_bytes(),
                &base_generation,
                &base_version,
                &base_root,
                &stack_generation,
                &stack_layer,
                &exported_generation,
                &exported_version,
                &exported_root,
                &exported_stack_generation,
                &exported_stack_layer,
                &exported_stack_root,
            ],
        );
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let present = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1)",
                    params![closure_id.as_slice()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if !present {
                self.bump(|counters| checked_add(&mut counters.fetch_closure_builds, 1))?;
                let created_at = unix_seconds()?;
                crate::integrity::authenticated_closure_for_each(
                    &connection,
                    &self.path,
                    self.store_id,
                    roots,
                    |id| {
                        connection
                            .execute(
                                "INSERT INTO layerfs_fetch_closure_items
                                 (closure_id, object_id, created_at) VALUES (?1, ?2, ?3)",
                                params![closure_id.as_slice(), id.as_bytes(), created_at],
                            )
                            .map_err(map_sqlite_error)?;
                        Ok(())
                    },
                )?;
            }
            let page = connection
                .prepare(
                    "SELECT object_id FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1 AND (?2 IS NULL OR object_id > ?2)
                     ORDER BY object_id LIMIT ?3",
                )
                .map_err(map_sqlite_error)?
                .query_map(
                    params![
                        closure_id.as_slice(),
                        after.map(|id| id.as_bytes().as_slice().to_vec()),
                        i64::try_from(limit).map_err(|_| EngineError::CounterOverflow)?,
                    ],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .map_err(map_sqlite_error)?
                .map(|row| {
                    ObjectId::from_bytes(&row.map_err(map_sqlite_error)?).map_err(Into::into)
                })
                .collect::<EngineResult<Vec<_>>>()?;
            self.bump(|counters| checked_add(&mut counters.fetch_closure_pages, 1))?;
            if page.is_empty() {
                connection
                    .execute(
                        "DELETE FROM layerfs_fetch_closure_items WHERE closure_id = ?1",
                        params![closure_id.as_slice()],
                    )
                    .map_err(map_sqlite_error)?;
                commit_product_state(
                    self,
                    &mut connection,
                    "SELECT NOT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1)",
                    &closure_id,
                )?;
            } else {
                commit_product_state(
                    self,
                    &mut connection,
                    "SELECT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1)",
                    &closure_id,
                )?;
            }
            Ok(page)
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    fn product_export_branch_push_one(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> EngineResult<BranchPushBundle> {
        let connection = self.lock_connection()?;
        let head = read_branch_head(&connection, branch_id)?
            .ok_or(EngineError::InvalidRecord("Branch"))?;
        let ancestry = read_branch_ancestry(&connection, branch_id)?
            .ok_or(EngineError::InvalidRecord("Branch ancestry"))?;
        if base.is_some_and(|base| base.branch_id != branch_id) {
            return Err(EngineError::InvalidRecord("Push base Branch"));
        }
        if let Some(base) = base {
            let retained = if base.generation == 0 && base.operation_version_id.is_none() {
                base.root == ancestry.fork_root
            } else {
                branch_contains_exact_historical_version(&connection, base)?
            };
            if !retained {
                return Err(EngineError::InvalidRecord("Push base history"));
            }
        }
        let name = connection
            .query_row(
                "SELECT name FROM layerfs_branches WHERE branch_id = ?1 AND state = 'active'",
                params![branch_id.as_bytes()],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(map_sqlite_error)?;
        if name
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.len() > 255)
        {
            return Err(EngineError::InvalidRecord("Branch name"));
        }
        let after_generation = base.map(|head| head.generation).unwrap_or(0);
        let mut page_generation = connection
            .query_row(
                "SELECT after_generation FROM layerfs_branch_transitions
                 WHERE branch_id = ?1 AND after_generation > ?2
                 ORDER BY after_generation LIMIT 1 OFFSET ?3",
                params![
                    branch_id.as_bytes(),
                    i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(MAX_HISTORY_PAGE_RECORDS - 1)
                        .map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .map(u64::try_from)
            .transpose()
            .map_err(|_| EngineError::InvalidRecord("Branch generation"))?
            .unwrap_or(head.generation);
        if let Some(dependency_generation) = connection
            .query_row(
                "SELECT MIN(after_generation) FROM layerfs_branch_transitions
                 WHERE branch_id = ?1 AND action_kind != 'operation_commit'
                   AND after_generation > ?2 AND after_generation <= ?3",
                params![
                    branch_id.as_bytes(),
                    i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(page_generation).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(map_sqlite_error)?
            .map(u64::try_from)
            .transpose()
            .map_err(|_| EngineError::InvalidRecord("Branch generation"))?
        {
            page_generation = if dependency_generation
                > after_generation
                    .checked_add(1)
                    .ok_or(EngineError::CounterOverflow)?
            {
                dependency_generation - 1
            } else {
                dependency_generation
            };
        }
        if page_generation < after_generation || page_generation > head.generation {
            return Err(EngineError::InvalidRecord("Branch history page"));
        }
        let page_head = branch_head_at_generation(&connection, branch_id, page_generation)?;
        let branch_complete = page_head == head;
        let version_count = connection
            .query_row(
                "SELECT COUNT(*)
                 FROM layerfs_operation_versions v
                 JOIN layerfs_branch_transitions bt
                   ON bt.branch_id = v.branch_id
                  AND bt.after_operation_version_id = v.operation_version_id
                  AND bt.action_kind = 'operation_commit'
                 WHERE v.branch_id = ?1 AND bt.after_generation > ?2
                   AND bt.after_generation <= ?3",
                params![
                    branch_id.as_bytes(),
                    i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(page_generation).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        let version_count =
            usize::try_from(version_count).map_err(|_| EngineError::CounterOverflow)?;
        let mut statement = connection
            .prepare(
                "SELECT o.operation_id, o.sequence, o.expected_branch_generation,
                        o.base_kind, o.base_layer_stack_id, o.base_layer_id,
                        o.base_operation_version_id, o.base_root_id,
                        basev.branch_id,
                        v.operation_version_id, v.sequence,
                        v.parent_operation_version_id, v.root_id,
                        od.operation_delta_id, od.transition_delta_id, d.payload,
                        bt.request_id, bt.before_generation, bt.after_generation,
                        (SELECT r.release_generation FROM layerfs_released_versions r
                            WHERE r.target_kind = 'operation_version'
                              AND r.owner_id = v.branch_id
                              AND r.version_id = v.operation_version_id),
                        (SELECT r.request_id FROM layerfs_released_versions r
                            WHERE r.target_kind = 'operation_version'
                              AND r.owner_id = v.branch_id
                              AND r.version_id = v.operation_version_id)
                 FROM layerfs_operation_versions v
                 JOIN layerfs_operations o
                   ON v.created_by_kind = 'operation'
                  AND v.created_by_operation_id = o.operation_id
                 LEFT JOIN layerfs_operation_versions basev
                   ON basev.operation_version_id = o.base_operation_version_id
                 JOIN layerfs_operation_deltas od
                   ON od.operation_id = o.operation_id
                  AND od.operation_version_id = v.operation_version_id
                 JOIN layerfs_deltas d
                   ON d.delta_id = od.transition_delta_id AND d.format_version = 1
                 JOIN layerfs_branch_transitions bt
                   ON bt.branch_id = v.branch_id
                  AND bt.after_operation_version_id = v.operation_version_id
                  AND bt.action_kind = 'operation_commit'
                 WHERE v.branch_id = ?1 AND bt.after_generation > ?2
                   AND bt.after_generation <= ?3
                 ORDER BY bt.after_generation",
            )
            .map_err(map_sqlite_error)?;
        let mut rows = statement
            .query(params![
                branch_id.as_bytes(),
                i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(page_generation).map_err(|_| EngineError::CounterOverflow)?,
            ])
            .map_err(map_sqlite_error)?;
        let mut operations = Vec::with_capacity(version_count);
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            let base_kind = row.get::<_, String>(3).map_err(map_sqlite_error)?;
            let base_root = object_id(&row.get::<_, Vec<u8>>(7).map_err(map_sqlite_error)?)?;
            let base = match base_kind.as_str() {
                "layer" => VersionRef::Layer {
                    layer_stack_id: LayerStackId(bytes32(
                        &row.get::<_, Vec<u8>>(4).map_err(map_sqlite_error)?,
                        "LayerStackId",
                    )?),
                    layer_id: LayerId(bytes32(
                        &row.get::<_, Vec<u8>>(5).map_err(map_sqlite_error)?,
                        "LayerId",
                    )?),
                    root: base_root,
                },
                "operation_version" => VersionRef::OperationVersion {
                    branch_id: BranchId(bytes32(
                        &row.get::<_, Vec<u8>>(8).map_err(map_sqlite_error)?,
                        "BranchId",
                    )?),
                    operation_version_id: OperationVersionId(bytes32(
                        &row.get::<_, Vec<u8>>(6).map_err(map_sqlite_error)?,
                        "OperationVersionId",
                    )?),
                    root: base_root,
                },
                _ => return Err(EngineError::InvalidRecord("Operation base")),
            };
            operations.push(PushedOperation {
                operation_id: OperationId(bytes32(
                    &row.get::<_, Vec<u8>>(0).map_err(map_sqlite_error)?,
                    "OperationId",
                )?),
                operation_sequence: u64::try_from(row.get::<_, i64>(1).map_err(map_sqlite_error)?)
                    .map_err(|_| EngineError::InvalidRecord("Operation sequence"))?,
                expected_branch_generation: u64::try_from(
                    row.get::<_, i64>(2).map_err(map_sqlite_error)?,
                )
                .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                base,
                operation_version_id: OperationVersionId(bytes32(
                    &row.get::<_, Vec<u8>>(9).map_err(map_sqlite_error)?,
                    "OperationVersionId",
                )?),
                version_sequence: u64::try_from(row.get::<_, i64>(10).map_err(map_sqlite_error)?)
                    .map_err(|_| {
                    EngineError::InvalidRecord("OperationVersion sequence")
                })?,
                parent_operation_version_id: row
                    .get::<_, Option<Vec<u8>>>(11)
                    .map_err(map_sqlite_error)?
                    .map(|bytes| bytes32(&bytes, "OperationVersionId").map(OperationVersionId))
                    .transpose()?,
                root: object_id(&row.get::<_, Vec<u8>>(12).map_err(map_sqlite_error)?)?,
                release: pushed_release(
                    row.get::<_, Option<i64>>(19).map_err(map_sqlite_error)?,
                    row.get::<_, Option<Vec<u8>>>(20)
                        .map_err(map_sqlite_error)?,
                )?,
                operation_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(13).map_err(map_sqlite_error)?,
                    "OperationDeltaId",
                )?,
                transition_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(14).map_err(map_sqlite_error)?,
                    "TransitionId",
                )?,
                transition_payload: row.get::<_, Vec<u8>>(15).map_err(map_sqlite_error)?,
                request_id: RequestId(bytes32(
                    &row.get::<_, Vec<u8>>(16).map_err(map_sqlite_error)?,
                    "RequestId",
                )?),
                before_generation: u64::try_from(row.get::<_, i64>(17).map_err(map_sqlite_error)?)
                    .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                after_generation: u64::try_from(row.get::<_, i64>(18).map_err(map_sqlite_error)?)
                    .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
            });
        }
        if operations.len() != version_count {
            return Err(EngineError::InvalidRecord("Push operation history"));
        }
        let mut statement = connection
            .prepare(
                "SELECT v.operation_version_id, v.sequence,
                        v.parent_operation_version_id, v.root_id,
                        v.created_by_child_branch_id, bd.branch_delta_id,
                        bd.source_branch_generation,
                        bd.source_branch_operation_version_id,
                        bd.base_root, bd.source_root, bd.destination_root,
                        bd.source_delta_id, sd.payload,
                        bd.applied_delta_id, ad.payload,
                        bt.request_id, bt.before_generation, bt.after_generation,
                        (SELECT r.release_generation FROM layerfs_released_versions r
                            WHERE r.target_kind = 'operation_version'
                              AND r.owner_id = v.branch_id
                              AND r.version_id = v.operation_version_id),
                        (SELECT r.request_id FROM layerfs_released_versions r
                            WHERE r.target_kind = 'operation_version'
                              AND r.owner_id = v.branch_id
                              AND r.version_id = v.operation_version_id)
                 FROM layerfs_branch_transitions bt
                 JOIN layerfs_operation_versions v
                   ON v.branch_id = bt.branch_id
                  AND v.operation_version_id = bt.after_operation_version_id
                  AND v.created_by_kind = 'child_merge'
                 JOIN layerfs_branch_deltas bd
                   ON bd.branch_delta_id = v.created_by_branch_delta_id
                  AND bd.purpose = 'child_merge'
                 JOIN layerfs_deltas sd
                   ON sd.delta_id = bd.source_delta_id AND sd.format_version = 1
                 JOIN layerfs_deltas ad
                   ON ad.delta_id = bd.applied_delta_id AND ad.format_version = 1
                 WHERE bt.branch_id = ?1 AND bt.action_kind = 'child_branch_merge'
                   AND bt.after_generation > ?2
                   AND bt.after_generation <= ?3
                 ORDER BY bt.after_generation",
            )
            .map_err(map_sqlite_error)?;
        let child_merges = statement
            .query_map(
                params![
                    branch_id.as_bytes(),
                    i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(page_generation).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<Vec<u8>>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Vec<u8>>(7)?,
                        row.get::<_, Vec<u8>>(8)?,
                        row.get::<_, Vec<u8>>(9)?,
                        row.get::<_, Vec<u8>>(10)?,
                        row.get::<_, Vec<u8>>(11)?,
                        row.get::<_, Vec<u8>>(12)?,
                        row.get::<_, Vec<u8>>(13)?,
                        row.get::<_, Vec<u8>>(14)?,
                        row.get::<_, Vec<u8>>(15)?,
                        row.get::<_, i64>(16)?,
                        row.get::<_, i64>(17)?,
                        row.get::<_, Option<i64>>(18)?,
                        row.get::<_, Option<Vec<u8>>>(19)?,
                    ))
                },
            )
            .map_err(map_sqlite_error)?
            .map(|row| {
                let row = row.map_err(map_sqlite_error)?;
                Ok(PushedChildMerge {
                    operation_version_id: OperationVersionId(bytes32(
                        &row.0,
                        "OperationVersionId",
                    )?),
                    version_sequence: u64::try_from(row.1)
                        .map_err(|_| EngineError::InvalidRecord("OperationVersion sequence"))?,
                    parent_operation_version_id: row
                        .2
                        .map(|id| bytes32(&id, "OperationVersionId").map(OperationVersionId))
                        .transpose()?,
                    root: object_id(&row.3)?,
                    release: pushed_release(row.18, row.19)?,
                    source_branch_id: BranchId(bytes32(&row.4, "BranchId")?),
                    source_branch_generation: u64::try_from(row.6)
                        .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                    source_operation_version_id: OperationVersionId(bytes32(
                        &row.7,
                        "OperationVersionId",
                    )?),
                    branch_delta_id: bytes32(&row.5, "BranchDeltaId")?,
                    base_root: object_id(&row.8)?,
                    source_root: object_id(&row.9)?,
                    destination_root: object_id(&row.10)?,
                    source_delta_id: bytes32(&row.11, "TransitionId")?,
                    source_transition_payload: row.12,
                    applied_delta_id: bytes32(&row.13, "TransitionId")?,
                    applied_transition_payload: row.14,
                    request_id: RequestId(bytes32(&row.15, "RequestId")?),
                    before_generation: u64::try_from(row.16)
                        .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                    after_generation: u64::try_from(row.17)
                        .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                })
            })
            .collect::<EngineResult<Vec<_>>>()?;
        let mut statement = connection
            .prepare(
                "SELECT bt.before_operation_version_id,
                        bt.after_operation_version_id, v.root_id,
                        bt.request_id, bt.before_generation, bt.after_generation
                 FROM layerfs_branch_transitions bt
                 JOIN layerfs_operation_versions v
                   ON v.branch_id = bt.branch_id
                  AND v.operation_version_id = bt.after_operation_version_id
                 WHERE bt.branch_id = ?1 AND bt.action_kind = 'branch_rollback'
                   AND bt.after_generation > ?2
                   AND bt.after_generation <= ?3
                 ORDER BY bt.after_generation",
            )
            .map_err(map_sqlite_error)?;
        let rollbacks = statement
            .query_map(
                params![
                    branch_id.as_bytes(),
                    i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(page_generation).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .map_err(map_sqlite_error)?
            .map(|row| {
                let row = row.map_err(map_sqlite_error)?;
                Ok(PushedBranchRollback {
                    before_operation_version_id: OperationVersionId(bytes32(
                        &row.0,
                        "OperationVersionId",
                    )?),
                    target_operation_version_id: OperationVersionId(bytes32(
                        &row.1,
                        "OperationVersionId",
                    )?),
                    target_root: object_id(&row.2)?,
                    request_id: RequestId(bytes32(&row.3, "RequestId")?),
                    before_generation: u64::try_from(row.4)
                        .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                    after_generation: u64::try_from(row.5)
                        .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                })
            })
            .collect::<EngineResult<Vec<_>>>()?;
        let history_len = operations
            .len()
            .checked_add(child_merges.len())
            .and_then(|count| count.checked_add(rollbacks.len()))
            .ok_or(EngineError::CounterOverflow)?;
        if history_len > MAX_PUSH_OPERATION_RECORDS
            || history_len == 0
                && !matches!(base, Some(base) if base == page_head)
                && !(base.is_none()
                    && page_head.generation == 0
                    && page_head.operation_version_id.is_none())
            || history_len
                != usize::try_from(page_head.generation.saturating_sub(after_generation))
                    .map_err(|_| EngineError::CounterOverflow)?
        {
            return Err(EngineError::InvalidRecord(
                "Push requires bounded complete Branch history",
            ));
        }
        let origin_stack = export_layer_stack_snapshot(
            &connection,
            ancestry.origin_layer_stack_id,
            origin_stack_base,
            branch_complete,
        )?;
        Ok(BranchPushBundle {
            name,
            ancestry,
            base,
            head: page_head,
            complete: branch_complete && origin_stack.complete,
            operations,
            child_merges,
            rollbacks,
            origin_stack,
            dependencies: Vec::new(),
        })
    }

    pub fn product_stage_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: SyncTransferCounters,
    ) -> EngineResult<()> {
        validate_staged_push_page(bundle)?;
        let encoded = serde_json::to_vec(bundle)
            .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
        if encoded.len() > 1024 * 1024 {
            return Err(EngineError::InvalidRecord("Push page resource bound"));
        }
        let page_id = derive_id(
            b"branch-push-page",
            &[transfer_id.as_bytes(), &page_sequence.to_be_bytes()],
        );
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let observed_unique_bytes = connection
                .query_row(
                    "SELECT COALESCE(SUM(o.canonical_length), 0)
                     FROM layerfs_sync_object_pins p
                     JOIN layerfs_objects o ON o.object_id = p.object_id
                     WHERE p.owner_request_id = ?1 AND p.request_id = ?2
                       AND p.direction = 'push'",
                    params![transfer_id.as_bytes(), data_request_id.as_bytes()],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(map_sqlite_error)?;
            if u64::try_from(observed_unique_bytes).ok() != Some(counters.unique_bytes) {
                return Err(EngineError::InvalidRecord(
                    "Push page receiver-observed bytes",
                ));
            }
            let incumbent = connection
                .query_row(
                    "SELECT transfer_id, page_sequence, data_request_id, branch_id, bundle,
                            unique_bytes, resumed_bytes, retransmitted_bytes
                     FROM layerfs_branch_push_pages WHERE page_id = ?1",
                    params![page_id.as_slice()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, Vec<u8>>(2)?,
                            row.get::<_, Vec<u8>>(3)?,
                            row.get::<_, Vec<u8>>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, i64>(7)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?;
            if let Some(incumbent) = incumbent {
                if incumbent.0.as_slice() != transfer_id.as_bytes()
                    || u64::try_from(incumbent.1).ok() != Some(page_sequence)
                    || incumbent.2.as_slice() != data_request_id.as_bytes()
                    || incumbent.3.as_slice() != bundle.head.branch_id.as_bytes()
                    || incumbent.4 != encoded
                    || u64::try_from(incumbent.5).ok() != Some(counters.unique_bytes)
                    || u64::try_from(incumbent.6).ok() != Some(counters.resumed_bytes)
                    || u64::try_from(incumbent.7).ok() != Some(counters.retransmitted_bytes)
                {
                    return Err(EngineError::InvalidRecord("Push page identity conflict"));
                }
            } else {
                connection
                    .execute(
                        "INSERT INTO layerfs_branch_push_pages
                         (page_id, transfer_id, page_sequence, data_request_id, branch_id, bundle,
                          unique_bytes, resumed_bytes, retransmitted_bytes, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        params![
                            page_id.as_slice(),
                            transfer_id.as_bytes(),
                            i64::try_from(page_sequence)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            data_request_id.as_bytes(),
                            bundle.head.branch_id.as_bytes(),
                            encoded,
                            i64::try_from(counters.unique_bytes)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            i64::try_from(counters.resumed_bytes)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            i64::try_from(counters.retransmitted_bytes)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            unix_seconds()?,
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_branch_push_pages WHERE page_id = ?1)",
                &page_id,
            )?;
            Ok(())
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_abort_sync_transfer(
        &self,
        owner_request_id: RequestId,
        direction: &str,
    ) -> EngineResult<u64> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("sync direction"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = delete_sync_custody(self, &mut connection, owner_request_id, direction);
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_reap_one_abandoned_sync(
        &self,
        older_than_unix_seconds: i64,
    ) -> EngineResult<Option<(RequestId, String, u64)>> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let owner = connection
                .query_row(
                    "SELECT owner_request_id, direction FROM (
                         SELECT owner_request_id, direction, created_at
                         FROM layerfs_sync_object_pins
                         UNION ALL
                         SELECT owner_request_id, direction, created_at
                         FROM layerfs_sync_batch_receipts
                         UNION ALL
                         SELECT transfer_id, 'push', created_at
                         FROM layerfs_branch_push_pages
                         UNION ALL
                         SELECT closure_id, 'closure', created_at
                         FROM layerfs_fetch_closure_items)
                     WHERE created_at < ?1
                     ORDER BY created_at, owner_request_id LIMIT 1",
                    params![older_than_unix_seconds],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            let Some((owner, direction)) = owner else {
                self.commit_dispatch
                    .rollback(&connection)
                    .map_err(map_sqlite_error)?;
                connection.transaction = false;
                self.bump(|counters| checked_add(&mut counters.transactions_rolled_back, 1))?;
                return Ok(None);
            };
            let owner = RequestId(bytes32(&owner, "RequestId")?);
            let rows = if direction == "closure" {
                let rows = connection
                    .execute(
                        "DELETE FROM layerfs_fetch_closure_items WHERE closure_id = ?1",
                        params![owner.as_bytes()],
                    )
                    .map_err(map_sqlite_error)?;
                commit_product_state(
                    self,
                    &mut connection,
                    "SELECT NOT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1)",
                    owner.as_bytes(),
                )?;
                u64::try_from(rows).map_err(|_| EngineError::CounterOverflow)?
            } else {
                delete_sync_custody(self, &mut connection, owner, &direction)?
            };
            Ok(Some((owner, direction, rows)))
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_sync_custody_rows(
        &self,
        owner_request_id: RequestId,
        direction: &str,
    ) -> EngineResult<u64> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("sync direction"));
        }
        let connection = self.lock_connection()?;
        let rows = connection
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM layerfs_sync_object_pins
                      WHERE owner_request_id = ?1 AND direction = ?2)
                   + (SELECT COUNT(*) FROM layerfs_sync_batch_receipts
                      WHERE owner_request_id = ?1 AND direction = ?2)
                   + (SELECT COUNT(*) FROM layerfs_branch_push_pages
                      WHERE transfer_id = ?1 AND ?2 = 'push')",
                params![owner_request_id.as_bytes(), direction],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(rows).map_err(|_| EngineError::CounterOverflow)
    }

    pub fn product_commit_staged_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> EngineResult<BranchPushOutcome> {
        verify_staged_child_merges(self, request.request_id, branch_id)?;
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if let Some(outcome) = read_exact_push_receipt(&connection, request, branch_id)? {
                return Ok(outcome);
            }
            let (page_count, maximum) = connection
                .query_row(
                    "SELECT COUNT(*), MAX(page_sequence)
                     FROM layerfs_branch_push_pages WHERE transfer_id = ?1 AND branch_id = ?2",
                    params![request.request_id.as_bytes(), branch_id.as_bytes()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .map_err(map_sqlite_error)?;
            let maximum = maximum.ok_or(EngineError::InvalidRecord("Push pages"))?;
            if page_count != maximum.checked_add(1).ok_or(EngineError::CounterOverflow)? {
                return Err(EngineError::InvalidRecord("Push page sequence"));
            }
            let observed = connection
                .query_row(
                    "SELECT COALESCE(SUM(unique_bytes), 0),
                            COALESCE(SUM(resumed_bytes), 0),
                            COALESCE(SUM(retransmitted_bytes), 0)
                     FROM layerfs_branch_push_pages
                     WHERE transfer_id = ?1 AND branch_id = ?2",
                    params![request.request_id.as_bytes(), branch_id.as_bytes()],
                    |row| {
                        Ok(SyncTransferCounters {
                            unique_bytes: u64::try_from(row.get::<_, i64>(0)?).map_err(|_| {
                                rusqlite::Error::IntegralValueOutOfRange(0, i64::MAX)
                            })?,
                            resumed_bytes: u64::try_from(row.get::<_, i64>(1)?).map_err(|_| {
                                rusqlite::Error::IntegralValueOutOfRange(1, i64::MAX)
                            })?,
                            retransmitted_bytes: u64::try_from(row.get::<_, i64>(2)?).map_err(
                                |_| rusqlite::Error::IntegralValueOutOfRange(2, i64::MAX),
                            )?,
                        })
                    },
                )
                .map_err(map_sqlite_error)?;
            if observed != request.counters {
                return Err(EngineError::InvalidRecord("Push receipt counters"));
            }
            let final_encoded = connection
                .query_row(
                    "SELECT bundle FROM layerfs_branch_push_pages
                     WHERE transfer_id = ?1 AND branch_id = ?2 AND page_sequence = ?3",
                    params![request.request_id.as_bytes(), branch_id.as_bytes(), maximum],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .map_err(map_sqlite_error)?;
            let final_bundle: BranchPushBundle = serde_json::from_slice(&final_encoded)
                .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
            if !final_bundle.complete {
                return Err(EngineError::InvalidRecord("Push pages incomplete"));
            }
            if let Some(outcome) = read_push_receipt(&connection, request, &final_bundle)? {
                release_staged_push_pins(&connection, request.request_id)?;
                connection
                    .execute(
                        "DELETE FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
                        params![request.request_id.as_bytes()],
                    )
                    .map_err(map_sqlite_error)?;
                let _ = commit_product_request(
                    self,
                    &mut connection,
                    "layerfs_sync_receipts",
                    request.request_id,
                )?;
                return Ok(outcome);
            }
            self.bump(|counters| checked_add(&mut counters.durable_head_transactions, 1))?;
            let actual = read_branch_head(&connection, branch_id)?;
            if actual != request.expected {
                insert_push_receipt(self, &connection, request, &final_bundle, "conflict")?;
                release_staged_push_pins(&connection, request.request_id)?;
                connection
                    .execute(
                        "DELETE FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
                        params![request.request_id.as_bytes()],
                    )
                    .map_err(map_sqlite_error)?;
                let _ = commit_product_request(
                    self,
                    &mut connection,
                    "layerfs_sync_receipts",
                    request.request_id,
                )?;
                return Ok(BranchPushOutcome::Conflict { actual });
            }

            let mut prior_version = actual.and_then(|head| head.operation_version_id);
            let mut prior_root = actual.map(|head| head.root);
            let mut prior_generation = actual.map_or(0, |head| head.generation);
            let mut ancestry = None;
            let mut branch_name = None;
            let mut final_head = None;
            for sequence in 0..=maximum {
                let encoded = connection
                    .query_row(
                        "SELECT bundle FROM layerfs_branch_push_pages
                         WHERE transfer_id = ?1 AND branch_id = ?2 AND page_sequence = ?3",
                        params![
                            request.request_id.as_bytes(),
                            branch_id.as_bytes(),
                            sequence
                        ],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                    .map_err(map_sqlite_error)?;
                let bundle: BranchPushBundle = serde_json::from_slice(&encoded)
                    .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
                validate_staged_push_page(&bundle)?;
                if bundle.head.branch_id != branch_id
                    || bundle.base != final_head.or(request.expected)
                    || ancestry.is_some_and(|value| value != bundle.ancestry)
                    || branch_name
                        .as_ref()
                        .is_some_and(|value| value != &bundle.name)
                    || bundle.complete != (sequence == maximum)
                {
                    return Err(EngineError::InvalidRecord("Push page chain"));
                }
                if ancestry.is_none() {
                    validate_push_ancestry(&connection, &bundle)?;
                    ancestry = Some(bundle.ancestry);
                    branch_name = Some(bundle.name.clone());
                    if actual.is_none() {
                        insert_branch_base(&connection, &bundle)?;
                        prior_root = Some(bundle.ancestry.fork_root);
                    } else if read_branch_ancestry(&connection, branch_id)? != Some(bundle.ancestry)
                    {
                        return Err(EngineError::InvalidRecord("Push Branch ancestry changed"));
                    }
                }
                let mut history = Vec::with_capacity(
                    bundle
                        .operations
                        .len()
                        .checked_add(bundle.child_merges.len())
                        .and_then(|count| count.checked_add(bundle.rollbacks.len()))
                        .ok_or(EngineError::CounterOverflow)?,
                );
                history.extend(
                    bundle
                        .operations
                        .iter()
                        .enumerate()
                        .map(|(index, record)| (record.after_generation, 0_u8, index)),
                );
                history.extend(
                    bundle
                        .child_merges
                        .iter()
                        .enumerate()
                        .map(|(index, record)| (record.after_generation, 1_u8, index)),
                );
                history.extend(
                    bundle
                        .rollbacks
                        .iter()
                        .enumerate()
                        .map(|(index, record)| (record.after_generation, 2_u8, index)),
                );
                history.sort_unstable();
                for (_, kind, index) in history {
                    let next = match kind {
                        0 => insert_pushed_operation(
                            self,
                            &connection,
                            &bundle,
                            &bundle.operations[index],
                            prior_version,
                            prior_root.ok_or(EngineError::InvalidRecord("Push Branch base"))?,
                            prior_generation,
                        )?,
                        1 => insert_pushed_child_merge(
                            self,
                            &connection,
                            branch_id,
                            &bundle.child_merges[index],
                            prior_version,
                            prior_root.ok_or(EngineError::InvalidRecord("Push Branch base"))?,
                            prior_generation,
                            None,
                        )?,
                        2 => insert_pushed_branch_rollback(
                            &connection,
                            branch_id,
                            &bundle.rollbacks[index],
                            prior_version,
                            prior_generation,
                        )?,
                        _ => unreachable!(),
                    };
                    prior_version = Some(next.0);
                    prior_root = Some(next.1);
                    prior_generation = next.2;
                }
                if prior_version != bundle.head.operation_version_id
                    || prior_root != Some(bundle.head.root)
                    || prior_generation != bundle.head.generation
                {
                    return Err(EngineError::InvalidRecord("Push page head"));
                }
                final_head = Some(bundle.head);
            }
            let final_head = final_head.ok_or(EngineError::InvalidRecord("Push pages"))?;
            let initial = actual.unwrap_or(BranchHead {
                branch_id,
                generation: 0,
                operation_version_id: None,
                root: ancestry
                    .ok_or(EngineError::InvalidRecord("Push ancestry"))?
                    .fork_root,
            });
            if final_head != initial {
                let changed = connection
                    .execute(
                        "UPDATE layerfs_branches
                         SET generation = ?1, head_operation_version_id = ?2
                         WHERE branch_id = ?3 AND generation = ?4
                           AND head_operation_version_id IS ?5 AND state = 'active'",
                        params![
                            i64::try_from(final_head.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            final_head
                                .operation_version_id
                                .map(|id| id.as_bytes().as_slice().to_vec()),
                            branch_id.as_bytes(),
                            i64::try_from(initial.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            initial
                                .operation_version_id
                                .map(|id| id.as_bytes().as_slice().to_vec()),
                        ],
                    )
                    .map_err(map_sqlite_error)?;
                if changed != 1 {
                    return Err(EngineError::PublicationConflict);
                }
            }
            if actual.is_none() {
                insert_branch_origin_lease(&connection, &final_bundle)?;
            }
            insert_push_receipt(
                self,
                &connection,
                request,
                &final_bundle,
                "durably_accepted",
            )?;
            release_staged_push_pins(&connection, request.request_id)?;
            connection
                .execute(
                    "DELETE FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
                    params![request.request_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            let reconciled = commit_product_request(
                self,
                &mut connection,
                "layerfs_sync_receipts",
                request.request_id,
            )?;
            Ok(BranchPushOutcome::DurablyAccepted {
                head: final_head,
                reconciled,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_import_verified_branch_fetch(
        &self,
        expected: Option<BranchHead>,
        bundle: &BranchPushBundle,
        fetch: VerifiedFetchRequest,
    ) -> EngineResult<BranchPushOutcome> {
        self.product_accept_branch_bundle(expected, bundle, None, Some(fetch))
    }

    fn product_accept_branch_bundle(
        &self,
        expected: Option<BranchHead>,
        bundle: &BranchPushBundle,
        push: Option<BranchPushRequest>,
        fetch: Option<VerifiedFetchRequest>,
    ) -> EngineResult<BranchPushOutcome> {
        let history_len = bundle
            .operations
            .len()
            .checked_add(bundle.child_merges.len())
            .and_then(|count| count.checked_add(bundle.rollbacks.len()))
            .ok_or(EngineError::CounterOverflow)?;
        if history_len > MAX_PUSH_OPERATION_RECORDS {
            return Err(EngineError::InvalidRecord("Push history page required"));
        }
        if bundle.origin_stack.head.layer_stack_id != bundle.ancestry.origin_layer_stack_id
            || bundle.origin_stack.name.is_empty()
            || bundle.origin_stack.name.len() > 255
            || bundle
                .name
                .as_ref()
                .is_some_and(|name| name.is_empty() || name.len() > 255)
            || bundle
                .operations
                .iter()
                .any(|operation| operation.transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES)
            || bundle.child_merges.iter().any(|merge| {
                merge.source_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                    || merge.applied_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
            })
            || bundle.origin_stack.layers.iter().any(|layer| {
                layer.merge.as_ref().is_some_and(|merge| {
                    merge.source_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                        || merge.applied_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                })
            })
        {
            return Err(EngineError::InvalidRecord("Push history resource bound"));
        }
        if push.is_some() && bundle.base != expected {
            return Err(EngineError::InvalidRecord("Push expected head"));
        }
        if push.is_some() && !bundle.dependencies.is_empty() {
            return Err(EngineError::InvalidRecord(
                "Push dependencies require explicit publication",
            ));
        }
        if push.is_some() && (!bundle.child_merges.is_empty() || !bundle.rollbacks.is_empty()) {
            return Err(EngineError::InvalidRecord(
                "Push merge/rollback requires dedicated publication",
            ));
        }
        if push.is_some() && fetch.is_some() {
            return Err(EngineError::InvalidRecord("Branch transfer action"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if let Some(request) = push {
                if let Some(outcome) = read_push_receipt(&connection, request, bundle)? {
                    return Ok(outcome);
                }
            }
            let published = read_branch_head(&connection, bundle.head.branch_id)?;
            let actual = if fetch.is_some() {
                read_fetch_branch_head(&connection, bundle.head.branch_id)?
            } else {
                published
            };
            if let (Some(fetch), false, Some(actual)) = (fetch, bundle.complete, actual) {
                stage_fetch_branch_head(&connection, fetch.durable_storage_id, published, actual)?;
            }
            if actual != expected {
                if let Some(request) = push {
                    insert_push_receipt(self, &connection, request, bundle, "conflict")?;
                    let reconciled = commit_product_request(
                        self,
                        &mut connection,
                        "layerfs_sync_receipts",
                        request.request_id,
                    )?;
                    let _ = reconciled;
                }
                return Ok(BranchPushOutcome::Conflict { actual });
            }
            let mut fetch_source_roots = None;
            let preinserted = fetch.is_some() && actual.is_none();
            if let Some(fetch) = fetch {
                import_layer_stack_snapshot(
                    self,
                    &connection,
                    &bundle.origin_stack,
                    Some((fetch.durable_storage_id, bundle.complete)),
                )?;
            }
            if push.is_some() {
                let stack =
                    read_layer_stack_head(&connection, bundle.ancestry.origin_layer_stack_id)?;
                if stack.is_none()
                    || bundle.origin_stack.base != Some(bundle.origin_stack.head)
                    || !bundle.origin_stack.complete
                    || !bundle.origin_stack.layers.is_empty()
                    || !bundle.origin_stack.transitions.is_empty()
                {
                    return Err(EngineError::InvalidRecord("Push origin LayerStack"));
                }
            }
            if fetch.is_some() {
                let mut roots = std::collections::BTreeSet::new();
                collect_fetch_branch_roots(bundle, &mut roots)?;
                fetch_source_roots = Some(roots);
                let mut proofs = std::collections::BTreeMap::new();
                collect_fetch_ancestry_proofs(bundle, &mut proofs)?;
                if preinserted {
                    insert_branch_snapshot(&connection, bundle)?;
                }
                for dependency in &bundle.dependencies {
                    import_fetch_dependency(
                        self,
                        &connection,
                        dependency,
                        fetch_source_roots.as_ref().expect("Fetch roots"),
                        &proofs,
                    )?;
                }
            }
            let origin_root = read_layer_root(
                &connection,
                bundle.ancestry.origin_layer_stack_id,
                bundle.ancestry.origin_layer_id,
            )?
            .ok_or(EngineError::InvalidRecord("Push origin Layer"))?;
            match (
                bundle.ancestry.immediate_parent_branch_id,
                bundle.ancestry.fork_operation_id,
                bundle.ancestry.fork_operation_version_id,
            ) {
                (None, None, None)
                    if bundle.ancestry.depth == 0 && bundle.ancestry.fork_root == origin_root => {}
                (Some(parent), Some(operation), Some(version)) if bundle.ancestry.depth > 0 => {
                    let parent_ancestry = read_branch_ancestry(&connection, parent)?
                        .ok_or(EngineError::InvalidRecord("Push parent Branch"))?;
                    let fork = connection
                        .query_row(
                            "SELECT root_id, created_by_kind, created_by_operation_id
                             FROM layerfs_operation_versions
                             WHERE branch_id = ?1 AND operation_version_id = ?2",
                            params![parent.as_bytes(), version.as_bytes()],
                            |row| {
                                Ok((
                                    row.get::<_, Vec<u8>>(0)?,
                                    row.get::<_, String>(1)?,
                                    row.get::<_, Option<Vec<u8>>>(2)?,
                                ))
                            },
                        )
                        .optional()
                        .map_err(map_sqlite_error)?
                        .ok_or(EngineError::InvalidRecord("Push child origin"))?;
                    if object_id(&fork.0)? != bundle.ancestry.fork_root
                        || fork.1 != "operation"
                        || fork.2.as_deref() != Some(operation.as_bytes())
                        || parent_ancestry.origin_layer_stack_id
                            != bundle.ancestry.origin_layer_stack_id
                        || parent_ancestry.origin_layer_id != bundle.ancestry.origin_layer_id
                        || parent_ancestry.depth.checked_add(1) != Some(bundle.ancestry.depth)
                    {
                        return Err(EngineError::InvalidRecord("Push child ancestry"));
                    }
                }
                _ => return Err(EngineError::InvalidRecord("Push Branch ancestry")),
            }
            if actual.is_some()
                && read_branch_ancestry(&connection, bundle.head.branch_id)?
                    != Some(bundle.ancestry)
            {
                return Err(EngineError::InvalidRecord("Push Branch ancestry changed"));
            }
            let mut history = Vec::with_capacity(history_len);
            history.extend(
                bundle
                    .operations
                    .iter()
                    .enumerate()
                    .map(|(index, record)| (record.after_generation, 0_u8, index)),
            );
            history.extend(
                bundle
                    .child_merges
                    .iter()
                    .enumerate()
                    .map(|(index, record)| (record.after_generation, 1_u8, index)),
            );
            history.extend(
                bundle
                    .rollbacks
                    .iter()
                    .enumerate()
                    .map(|(index, record)| (record.after_generation, 2_u8, index)),
            );
            history.sort_unstable();
            let last_head = history.last().map(|(_, kind, index)| match kind {
                0 => Some(bundle.operations[*index].operation_version_id),
                1 => Some(bundle.child_merges[*index].operation_version_id),
                2 => Some(bundle.rollbacks[*index].target_operation_version_id),
                _ => unreachable!(),
            });
            if last_head.is_some_and(|version| {
                version != bundle.head.operation_version_id
                    || history.last().map(|record| record.0) != Some(bundle.head.generation)
            }) || last_head.is_none()
                && !matches!(actual, Some(actual) if actual == bundle.head)
                && !(actual.is_none()
                    && bundle.head.generation == 0
                    && bundle.head.operation_version_id.is_none())
            {
                return Err(EngineError::InvalidRecord("Push Branch head"));
            }
            match actual {
                None => 0,
                Some(current) if current == bundle.head => {
                    if let Some(fetch) = fetch {
                        let reconciled = if bundle.complete {
                            finish_fetch_staging(&connection, current.branch_id)?;
                            commit_verified_fetch(
                                self,
                                &mut connection,
                                fetch,
                                current,
                                bundle.origin_stack.head,
                            )?
                        } else {
                            stage_fetch_branch_head(
                                &connection,
                                fetch.durable_storage_id,
                                published,
                                current,
                            )?;
                            commit_partial_fetch(self, &mut connection, fetch, current)?
                        };
                        return Ok(BranchPushOutcome::DurablyAccepted {
                            head: current,
                            reconciled,
                        });
                    }
                    if let Some(request) = push {
                        insert_push_receipt(
                            self,
                            &connection,
                            request,
                            bundle,
                            "durably_accepted",
                        )?;
                        let reconciled = commit_product_request(
                            self,
                            &mut connection,
                            "layerfs_sync_receipts",
                            request.request_id,
                        )?;
                        return Ok(BranchPushOutcome::DurablyAccepted {
                            head: current,
                            reconciled,
                        });
                    }
                    return Ok(BranchPushOutcome::DurablyAccepted {
                        head: current,
                        reconciled: true,
                    });
                }
                Some(_) => 0,
            };
            if actual.is_none() && !preinserted {
                insert_branch_snapshot(&connection, bundle)?;
            }
            let mut prior_version = actual.and_then(|head| head.operation_version_id);
            let mut prior_root = actual
                .map(|head| head.root)
                .unwrap_or(bundle.ancestry.fork_root);
            let mut prior_generation = actual.map(|head| head.generation).unwrap_or(0);
            for (_, kind, index) in &history {
                let next = match kind {
                    0 => insert_pushed_operation(
                        self,
                        &connection,
                        bundle,
                        &bundle.operations[*index],
                        prior_version,
                        prior_root,
                        prior_generation,
                    )?,
                    1 => insert_pushed_child_merge(
                        self,
                        &connection,
                        bundle.head.branch_id,
                        &bundle.child_merges[*index],
                        prior_version,
                        prior_root,
                        prior_generation,
                        fetch_source_roots.as_ref(),
                    )?,
                    2 => insert_pushed_branch_rollback(
                        &connection,
                        bundle.head.branch_id,
                        &bundle.rollbacks[*index],
                        prior_version,
                        prior_generation,
                    )?,
                    _ => unreachable!(),
                };
                prior_version = Some(next.0);
                prior_root = next.1;
                prior_generation = next.2;
            }
            if prior_version != bundle.head.operation_version_id
                || prior_root != bundle.head.root
                || prior_generation != bundle.head.generation
            {
                return Err(EngineError::InvalidRecord("Push Branch history head"));
            }
            if let Some(current) = actual {
                let changed = connection
                    .execute(
                        "UPDATE layerfs_branches
                         SET generation = ?1, head_operation_version_id = ?2
                         WHERE branch_id = ?3 AND generation = ?4
                           AND head_operation_version_id IS ?5 AND state = 'active'",
                        params![
                            i64::try_from(bundle.head.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            bundle
                                .head
                                .operation_version_id
                                .map(|id| id.as_bytes().as_slice().to_vec()),
                            bundle.head.branch_id.as_bytes(),
                            i64::try_from(current.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            current
                                .operation_version_id
                                .map(|id| id.as_bytes().as_slice().to_vec()),
                        ],
                    )
                    .map_err(map_sqlite_error)?;
                if changed != 1 {
                    return Err(EngineError::PublicationConflict);
                }
            }
            if actual.is_none() {
                let lease_id = derive_id(
                    if bundle.ancestry.depth == 0 {
                        b"top-level-branch-origin-lease"
                    } else {
                        b"child-branch-origin-lease"
                    },
                    &[
                        bundle.head.branch_id.as_bytes(),
                        bundle
                            .ancestry
                            .fork_operation_version_id
                            .map(|id| id.0)
                            .unwrap_or(bundle.ancestry.origin_layer_id.0)
                            .as_slice(),
                    ],
                );
                connection
                    .execute(
                        "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, ?2, ?3, 'branch', ?4, ?5)",
                        params![
                            lease_id.as_slice(),
                            if bundle.ancestry.depth == 0 {
                                "layer"
                            } else {
                                "operation_version"
                            },
                            bundle
                                .ancestry
                                .fork_operation_version_id
                                .map(|id| id.0)
                                .unwrap_or(bundle.ancestry.origin_layer_id.0)
                                .as_slice(),
                            bundle.head.branch_id.as_bytes(),
                            unix_seconds()?,
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            let reconciled = if let Some(fetch) = fetch {
                if bundle.complete {
                    finish_fetch_staging(&connection, bundle.head.branch_id)?;
                    commit_verified_fetch(
                        self,
                        &mut connection,
                        fetch,
                        bundle.head,
                        bundle.origin_stack.head,
                    )?
                } else {
                    stage_fetch_branch_head(
                        &connection,
                        fetch.durable_storage_id,
                        published,
                        bundle.head,
                    )?;
                    commit_partial_fetch(self, &mut connection, fetch, bundle.head)?
                }
            } else if let Some(request) = push {
                insert_push_receipt(self, &connection, request, bundle, "durably_accepted")?;
                commit_product_request(
                    self,
                    &mut connection,
                    "layerfs_sync_receipts",
                    request.request_id,
                )?
            } else {
                commit_product_state(
                        self,
                        &mut connection,
                        "SELECT EXISTS(SELECT 1 FROM layerfs_branches WHERE branch_id = ?1 AND state = 'active')",
                        bundle.head.branch_id.as_bytes(),
                    )?;
                false
            };
            Ok(BranchPushOutcome::DurablyAccepted {
                head: bundle.head,
                reconciled,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_create_layer_stack(
        &self,
        layer_stack_id: LayerStackId,
        layer_id: LayerId,
        name: &str,
        root: ObjectId,
    ) -> EngineResult<LayerStackHead> {
        if name.is_empty() || name.len() > 255 {
            return Err(EngineError::InvalidRecord("LayerStack name"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            authenticate_root(self, &connection, root)?;
            connection
                .execute(
                    "INSERT INTO layerfs_layer_stacks
                     (layer_stack_id, name, generation, head_layer_id)
                     VALUES (?1, ?2, 0, ?3)",
                    params![layer_stack_id.as_bytes(), name, layer_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT INTO layerfs_layers
                     (layer_id, layer_stack_id, root_id, creation_kind, state, accepted_generation)
                     VALUES (?1, ?2, ?3, 'genesis', 'accepted', 0)",
                    params![
                        layer_id.as_bytes(),
                        layer_stack_id.as_bytes(),
                        root.as_bytes()
                    ],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT INTO layerfs_retained_roots (root_id) VALUES (?1)
                     ON CONFLICT(root_id) DO NOTHING",
                    params![root.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_layer_stacks WHERE layer_stack_id = ?1)",
                layer_stack_id.as_bytes(),
            )?;
            Ok(LayerStackHead {
                layer_stack_id,
                generation: 0,
                layer_id,
                root,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_create_top_level_branch(
        &self,
        branch_id: BranchId,
        name: Option<&str>,
        origin: LayerStackHead,
    ) -> EngineResult<BranchHead> {
        if name.is_some_and(|name| name.is_empty() || name.len() > 255) {
            return Err(EngineError::InvalidRecord("Branch name"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if read_layer_stack_head(&connection, origin.layer_stack_id)? != Some(origin) {
                return Err(EngineError::PublicationConflict);
            }
            connection
                .execute(
                    "INSERT INTO layerfs_branches
                     (branch_id, name, fork_root_id, origin_layer_stack_id, origin_layer_id,
                      depth, generation, state)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 'active')",
                    params![
                        branch_id.as_bytes(),
                        name,
                        origin.root.as_bytes(),
                        origin.layer_stack_id.as_bytes(),
                        origin.layer_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let lease_id = derive_id(
                b"top-level-branch-origin-lease",
                &[branch_id.as_bytes(), origin.layer_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, 'layer', ?2, 'branch', ?3, ?4)",
                    params![
                        lease_id.as_slice(),
                        origin.layer_id.as_bytes(),
                        branch_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_branches WHERE branch_id = ?1 AND state = 'active')",
                branch_id.as_bytes(),
            )?;
            Ok(BranchHead {
                branch_id,
                generation: 0,
                operation_version_id: None,
                root: origin.root,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_create_child_branch(
        &self,
        branch_id: BranchId,
        name: Option<&str>,
        origin: OperationRecordRef,
    ) -> EngineResult<BranchHead> {
        if name.is_some_and(|name| name.is_empty() || name.len() > 255) {
            return Err(EngineError::InvalidRecord("Branch name"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let (root, created_by_kind, operation) = connection
                .query_row(
                    "SELECT root_id, created_by_kind, created_by_operation_id
                     FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2",
                    params![
                        origin.parent_branch_id.as_bytes(),
                        origin.operation_version_id.as_bytes()
                    ],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("child Branch origin"))?;
            if created_by_kind != "operation"
                || operation.as_deref() != Some(origin.operation_id.as_bytes())
                || object_id(&root)? != origin.root
            {
                return Err(EngineError::InvalidRecord("child Branch origin"));
            }
            let parent = read_branch_ancestry(&connection, origin.parent_branch_id)?
                .ok_or(EngineError::InvalidRecord("parent Branch"))?;
            let depth = parent
                .depth
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            connection
                .execute(
                    "INSERT INTO layerfs_branches
                     (branch_id, name, immediate_parent_branch_id, fork_operation_id,
                      fork_operation_version_id, fork_root_id, origin_layer_stack_id,
                      origin_layer_id, depth, generation, state)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 'active')",
                    params![
                        branch_id.as_bytes(),
                        name,
                        origin.parent_branch_id.as_bytes(),
                        origin.operation_id.as_bytes(),
                        origin.operation_version_id.as_bytes(),
                        origin.root.as_bytes(),
                        parent.origin_layer_stack_id.as_bytes(),
                        parent.origin_layer_id.as_bytes(),
                        i64::try_from(depth).map_err(|_| EngineError::CounterOverflow)?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            let lease_id = derive_id(
                b"child-branch-origin-lease",
                &[branch_id.as_bytes(), origin.operation_version_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, 'operation_version', ?2, 'branch', ?3, ?4)",
                    params![
                        lease_id.as_slice(),
                        origin.operation_version_id.as_bytes(),
                        branch_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_branches WHERE branch_id = ?1 AND state = 'active')",
                branch_id.as_bytes(),
            )?;
            Ok(BranchHead {
                branch_id,
                generation: 0,
                operation_version_id: None,
                root: origin.root,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

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

    pub fn product_discard_operation(&self, operation_id: OperationId) -> EngineResult<bool> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let operation = connection
                .query_row(
                    "SELECT candidate_root_id, state FROM layerfs_operations
                     WHERE operation_id = ?1",
                    params![operation_id.as_bytes()],
                    |row| Ok((row.get::<_, Option<Vec<u8>>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Operation"))?;
            if operation.1 == "discarded" {
                return Ok(true);
            }
            if !matches!(
                operation.1.as_str(),
                "running" | "candidate" | "failed" | "preserved" | "indeterminate"
            ) {
                return Err(EngineError::InvalidRecord("Operation discard state"));
            }
            connection
                .execute(
                    "UPDATE layerfs_operations
                     SET state = 'discarded', reconciliation_class = 'explicit_discard'
                     WHERE operation_id = ?1",
                    params![operation_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            release_operation_lease(&connection, operation_id)?;
            if let Some(root) = operation.0 {
                release_retained_root_if_unreferenced(&connection, &root)?;
            }
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_operations WHERE operation_id = ?1 AND state = 'discarded')",
                operation_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_recoverable_operations(
        &self,
        limit: usize,
    ) -> EngineResult<Vec<RecoverableOperation>> {
        self.product_recoverable_operations_after(None, limit)
    }

    pub fn product_recoverable_operations_after(
        &self,
        after: Option<OperationId>,
        limit: usize,
    ) -> EngineResult<Vec<RecoverableOperation>> {
        if limit == 0 || limit > 1024 {
            return Err(EngineError::InvalidRecord("Operation recovery page"));
        }
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT operation_id, branch_id, expected_branch_generation,
                        base_root_id, candidate_root_id,
                        result_operation_version_id, state
                 FROM layerfs_operations
                 WHERE (state IN ('running', 'candidate', 'failed', 'indeterminate')
                        OR (state = 'working_recorded'
                            AND reconciliation_class IS NULL)
                        OR (state = 'preserved'
                            AND reconciliation_class IN
                                ('conflict', 'conflict_receipt_delivered')))
                   AND (?1 IS NULL OR operation_id > ?1)
                 ORDER BY operation_id LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let operations = statement
            .query_map(
                params![
                    after.map(|id| id.as_bytes().as_slice().to_vec()),
                    i64::try_from(limit).map_err(|_| EngineError::CounterOverflow)?
                ],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Option<Vec<u8>>>(4)?,
                        row.get::<_, Option<Vec<u8>>>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .map_err(map_sqlite_error)?
            .map(|row| {
                let row = row.map_err(map_sqlite_error)?;
                Ok(RecoverableOperation {
                    operation_id: OperationId(bytes32(&row.0, "OperationId")?),
                    branch_id: BranchId(bytes32(&row.1, "BranchId")?),
                    expected_branch_generation: u64::try_from(row.2)
                        .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                    base_root: object_id(&row.3)?,
                    candidate_root: row.4.map(|root| object_id(&root)).transpose()?,
                    result_operation_version_id: row
                        .5
                        .map(|id| bytes32(&id, "OperationVersionId").map(OperationVersionId))
                        .transpose()?,
                    state: match row.6.as_str() {
                        "running" => RecoverableOperationState::Running,
                        "candidate" => RecoverableOperationState::Candidate,
                        "failed" => RecoverableOperationState::Failed,
                        "indeterminate" => RecoverableOperationState::Indeterminate,
                        "working_recorded" => RecoverableOperationState::WorkingRecorded,
                        "preserved" => RecoverableOperationState::Preserved,
                        _ => return Err(EngineError::InvalidRecord("Operation recovery state")),
                    },
                })
            })
            .collect();
        operations
    }

    pub fn product_acknowledge_operation(
        &self,
        operation_id: OperationId,
        version_id: OperationVersionId,
    ) -> EngineResult<bool> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let exact = connection
                .query_row(
                    "SELECT state = 'working_recorded'
                            AND result_operation_version_id = ?2
                     FROM layerfs_operations WHERE operation_id = ?1",
                    params![operation_id.as_bytes(), version_id.as_bytes()],
                    |row| row.get::<_, bool>(0),
                )
                .optional()
                .map_err(map_sqlite_error)?
                .unwrap_or(false);
            if !exact {
                return Err(EngineError::InvalidRecord("Operation acknowledgement"));
            }
            connection
                .execute(
                    "UPDATE layerfs_operations
                     SET reconciliation_class = 'working_receipt_delivered'
                     WHERE operation_id = ?1 AND result_operation_version_id = ?2",
                    params![operation_id.as_bytes(), version_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state_pair(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_operations
                 WHERE operation_id = ?1 AND result_operation_version_id = ?2
                   AND reconciliation_class = 'working_receipt_delivered')",
                operation_id.as_bytes(),
                version_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_acknowledge_conflict(
        &self,
        operation_id: OperationId,
        root: ObjectId,
    ) -> EngineResult<bool> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let changed = connection
                .execute(
                    "UPDATE layerfs_operations
                     SET reconciliation_class = 'conflict_receipt_delivered'
                     WHERE operation_id = ?1 AND candidate_root_id = ?2
                       AND state = 'preserved' AND reconciliation_class = 'conflict'",
                    params![operation_id.as_bytes(), root.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::InvalidRecord("Conflict acknowledgement"));
            }
            commit_product_state_pair(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_operations
                 WHERE operation_id = ?1 AND candidate_root_id = ?2
                   AND state = 'preserved'
                   AND reconciliation_class = 'conflict_receipt_delivered')",
                operation_id.as_bytes(),
                root.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_operation_commit(
        &self,
        candidate: OperationCandidate,
    ) -> EngineResult<OperationCommitOutcome> {
        if candidate.normalized_transition.len() > MAX_TRANSITION_PAYLOAD_BYTES {
            return Err(EngineError::InvalidRecord(
                "Operation transition resource bound",
            ));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if let Some(outcome) = replay_operation_commit(self, &connection, &candidate)? {
                return Ok(outcome);
            }
            let operation = load_operation(&connection, candidate.operation_id)?;
            if operation.branch_id != candidate.expected.branch_id
                || operation.expected_generation != candidate.expected.generation
                || operation.base_root != candidate.expected.root
                || !matches!(
                    operation.state.as_str(),
                    "running" | "candidate" | "preserved"
                )
            {
                return Err(EngineError::InvalidRecord("Operation candidate binding"));
            }
            if operation.candidate_root == Some(candidate.candidate_root)
                && matches!(operation.state.as_str(), "candidate" | "preserved")
            {
                authenticate_root_shallow(self, &connection, candidate.candidate_root)?;
            } else {
                authenticate_root(self, &connection, candidate.candidate_root)?;
            }
            let actual = read_branch_head(&connection, candidate.expected.branch_id)?
                .ok_or(EngineError::InvalidRecord("Branch"))?;
            let transition_id = transition_identity(
                candidate.expected.root,
                candidate.candidate_root,
                &candidate.normalized_transition,
            );
            insert_transition(
                &connection,
                transition_id,
                candidate.expected.root,
                candidate.candidate_root,
                &candidate.normalized_transition,
            )?;
            connection
                .execute(
                    "INSERT INTO layerfs_retained_roots (root_id) VALUES (?1)
                     ON CONFLICT(root_id) DO NOTHING",
                    params![candidate.candidate_root.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            if actual != candidate.expected {
                connection
                    .execute(
                        "UPDATE layerfs_operations
                         SET candidate_root_id = ?1, state = 'preserved',
                             reconciliation_class = 'conflict'
                         WHERE operation_id = ?2",
                        params![
                            candidate.candidate_root.as_bytes(),
                            candidate.operation_id.as_bytes()
                        ],
                    )
                    .map_err(map_sqlite_error)?;
                release_operation_lease(&connection, candidate.operation_id)?;
                commit_product_state(
                    self,
                    &mut connection,
                    "SELECT EXISTS(SELECT 1 FROM layerfs_operations WHERE operation_id = ?1 AND state = 'preserved')",
                    candidate.operation_id.as_bytes(),
                )?;
                return Ok(OperationCommitOutcome::Conflict {
                    actual,
                    candidate: PreservedOperationCandidate {
                        operation_id: candidate.operation_id,
                        root: candidate.candidate_root,
                    },
                });
            }

            let version_id = OperationVersionId(derive_id(
                b"operation-version",
                &[
                    candidate.operation_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    candidate.candidate_root.as_bytes(),
                ],
            ));
            let operation_delta_id = derive_id(
                b"operation-delta",
                &[
                    candidate.operation_id.as_bytes(),
                    version_id.as_bytes(),
                    &transition_id,
                ],
            );
            let next_generation = actual
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            let version_sequence = next_operation_version_sequence(&connection, actual.branch_id)?;
            connection
                .execute(
                    "INSERT INTO layerfs_operation_versions
                     (operation_version_id, branch_id, sequence,
                      parent_operation_version_id, root_id, created_by_kind,
                      created_by_operation_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'operation', ?6)",
                    params![
                        version_id.as_bytes(),
                        actual.branch_id.as_bytes(),
                        version_sequence,
                        actual
                            .operation_version_id
                            .map(|id| id.as_bytes().as_slice().to_vec()),
                        candidate.candidate_root.as_bytes(),
                        candidate.operation_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "UPDATE layerfs_operations
                     SET candidate_root_id = ?1, result_operation_version_id = ?2,
                         state = 'working_recorded', reconciliation_class = NULL
                     WHERE operation_id = ?3",
                    params![
                        candidate.candidate_root.as_bytes(),
                        version_id.as_bytes(),
                        candidate.operation_id.as_bytes()
                    ],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT INTO layerfs_operation_deltas
                     (operation_delta_id, operation_id, operation_version_id,
                      transition_delta_id, base_root, result_root)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        operation_delta_id.as_slice(),
                        candidate.operation_id.as_bytes(),
                        version_id.as_bytes(),
                        transition_id.as_slice(),
                        candidate.expected.root.as_bytes(),
                        candidate.candidate_root.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let receipt_id = derive_id(
                b"branch-transition",
                &[candidate.request_id.as_bytes(), version_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_branch_transitions
                     (transition_id, branch_id, before_generation, after_generation,
                      before_operation_version_id, after_operation_version_id,
                      action_kind, source_record_id, request_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             'operation_commit', ?7, ?8)",
                    params![
                        receipt_id.as_slice(),
                        actual.branch_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        actual
                            .operation_version_id
                            .map(|id| id.as_bytes().as_slice().to_vec()),
                        version_id.as_bytes(),
                        candidate.operation_id.as_bytes(),
                        candidate.request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let changed = connection
                .execute(
                    "UPDATE layerfs_branches
                     SET generation = ?1, head_operation_version_id = ?2
                     WHERE branch_id = ?3 AND generation = ?4
                       AND head_operation_version_id IS ?5 AND state = 'active'",
                    params![
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        version_id.as_bytes(),
                        actual.branch_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        actual
                            .operation_version_id
                            .map(|id| id.as_bytes().as_slice().to_vec()),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            release_operation_lease(&connection, candidate.operation_id)?;
            let head = BranchHead {
                branch_id: actual.branch_id,
                generation: next_generation,
                operation_version_id: Some(version_id),
                root: candidate.candidate_root,
            };
            let record = OperationRecordRef {
                parent_branch_id: actual.branch_id,
                operation_id: candidate.operation_id,
                operation_version_id: version_id,
                root: candidate.candidate_root,
            };
            let reconciled = commit_product_request(
                self,
                &mut connection,
                "layerfs_branch_transitions",
                candidate.request_id,
            )?;
            Ok(OperationCommitOutcome::WorkingRecorded {
                head,
                record,
                reconciled,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_child_branch_merge(
        &self,
        candidate: ChildMergeCandidate,
    ) -> EngineResult<ChildMergeOutcome> {
        if candidate.source_transition.len() > MAX_TRANSITION_PAYLOAD_BYTES
            || candidate.applied_transition.len() > MAX_TRANSITION_PAYLOAD_BYTES
        {
            return Err(EngineError::InvalidRecord(
                "Merge transition resource bound",
            ));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let prior = connection
                .query_row(
                    "SELECT branch_id, after_generation, after_operation_version_id
                     FROM layerfs_branch_transitions
                     WHERE request_id = ?1 AND action_kind = 'child_branch_merge'",
                    params![candidate.request_id.as_bytes()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, Vec<u8>>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?;
            if let Some((branch, generation, version)) = prior {
                let version = OperationVersionId(bytes32(&version, "OperationVersionId")?);
                let expected_version = OperationVersionId(derive_id(
                    b"child-merge-operation-version",
                    &[
                        candidate.expected_parent.branch_id.as_bytes(),
                        candidate.request_id.as_bytes(),
                        candidate.result_root.as_bytes(),
                    ],
                ));
                let generation = u64::try_from(generation)
                    .map_err(|_| EngineError::InvalidRecord("Branch generation"))?;
                if branch.as_slice() != candidate.expected_parent.branch_id.as_bytes()
                    || version != expected_version
                    || generation
                        != candidate
                            .expected_parent
                            .generation
                            .checked_add(1)
                            .ok_or(EngineError::CounterOverflow)?
                {
                    return Err(EngineError::InvalidRecord(
                        "ChildBranchMerge request identity conflict",
                    ));
                }
                let root = connection
                    .query_row(
                        "SELECT root_id FROM layerfs_operation_versions
                         WHERE branch_id = ?1 AND operation_version_id = ?2",
                        params![branch, version.as_bytes()],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                    .map_err(map_sqlite_error)?;
                let parent_head = BranchHead {
                    branch_id: candidate.expected_parent.branch_id,
                    generation,
                    operation_version_id: Some(version),
                    root: object_id(&root)?,
                };
                if parent_head.root != candidate.result_root {
                    return Err(EngineError::InvalidRecord(
                        "ChildBranchMerge request result",
                    ));
                }
                return Ok(ChildMergeOutcome::WorkingRecorded {
                    parent_head,
                    reconciled: true,
                });
            }
            let ancestry = read_branch_ancestry(&connection, candidate.source.branch_id)?
                .ok_or(EngineError::InvalidRecord("source Branch"))?;
            let parent_id =
                ancestry
                    .immediate_parent_branch_id
                    .ok_or(EngineError::InvalidRecord(
                        "top-level Branch cannot child-merge",
                    ))?;
            if parent_id != candidate.expected_parent.branch_id
                || ancestry.fork_operation_id.is_none()
                || ancestry.fork_operation_version_id.is_none()
            {
                return Err(EngineError::InvalidRecord("non-parent Branch merge"));
            }
            if read_branch_head(&connection, candidate.source.branch_id)? != Some(candidate.source)
            {
                return Err(EngineError::PublicationConflict);
            }
            let source_version = candidate
                .source
                .operation_version_id
                .ok_or(EngineError::InvalidRecord("Child merge source head"))?;
            let actual_parent = read_branch_head(&connection, parent_id)?
                .ok_or(EngineError::InvalidRecord("parent Branch"))?;
            if actual_parent != candidate.expected_parent {
                return Ok(ChildMergeOutcome::Conflict { actual_parent });
            }
            authenticate_root(self, &connection, candidate.result_root)?;
            let source_delta_id = transition_identity(
                ancestry.fork_root,
                candidate.source.root,
                &candidate.source_transition,
            );
            insert_transition(
                &connection,
                source_delta_id,
                ancestry.fork_root,
                candidate.source.root,
                &candidate.source_transition,
            )?;
            let applied_delta_id = transition_identity(
                candidate.expected_parent.root,
                candidate.result_root,
                &candidate.applied_transition,
            );
            insert_transition(
                &connection,
                applied_delta_id,
                candidate.expected_parent.root,
                candidate.result_root,
                &candidate.applied_transition,
            )?;
            let branch_delta_id = derive_id(
                b"child-branch-delta",
                &[
                    candidate.source.branch_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    &source_delta_id,
                    &applied_delta_id,
                ],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_branch_deltas
                     (branch_delta_id, purpose, source_branch_id,
                      source_branch_generation, source_branch_operation_version_id, base_root,
                      source_root, destination_root, result_root,
                      source_delta_id, applied_delta_id)
                     VALUES (?1, 'child_merge', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        branch_delta_id.as_slice(),
                        candidate.source.branch_id.as_bytes(),
                        i64::try_from(candidate.source.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        source_version.as_bytes(),
                        ancestry.fork_root.as_bytes(),
                        candidate.source.root.as_bytes(),
                        candidate.expected_parent.root.as_bytes(),
                        candidate.result_root.as_bytes(),
                        source_delta_id.as_slice(),
                        applied_delta_id.as_slice(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let next_generation = actual_parent
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            let version_sequence = next_operation_version_sequence(&connection, parent_id)?;
            let version_id = OperationVersionId(derive_id(
                b"child-merge-operation-version",
                &[
                    parent_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    candidate.result_root.as_bytes(),
                ],
            ));
            connection
                .execute(
                    "INSERT INTO layerfs_operation_versions
                     (operation_version_id, branch_id, sequence,
                      parent_operation_version_id, root_id, created_by_kind,
                      created_by_child_branch_id, created_by_branch_delta_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'child_merge', ?6, ?7)",
                    params![
                        version_id.as_bytes(),
                        parent_id.as_bytes(),
                        version_sequence,
                        actual_parent
                            .operation_version_id
                            .map(|id| id.as_bytes().as_slice().to_vec()),
                        candidate.result_root.as_bytes(),
                        candidate.source.branch_id.as_bytes(),
                        branch_delta_id.as_slice(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let receipt_id = derive_id(
                b"child-branch-merge-receipt",
                &[candidate.request_id.as_bytes(), version_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_branch_transitions
                     (transition_id, branch_id, before_generation, after_generation,
                      before_operation_version_id, after_operation_version_id,
                      action_kind, source_record_id, request_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             'child_branch_merge', ?7, ?8)",
                    params![
                        receipt_id.as_slice(),
                        parent_id.as_bytes(),
                        i64::try_from(actual_parent.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        actual_parent
                            .operation_version_id
                            .map(|id| id.as_bytes().as_slice().to_vec()),
                        version_id.as_bytes(),
                        branch_delta_id.as_slice(),
                        candidate.request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let changed = connection
                .execute(
                    "UPDATE layerfs_branches
                     SET generation = ?1, head_operation_version_id = ?2
                     WHERE branch_id = ?3 AND generation = ?4
                       AND head_operation_version_id IS ?5 AND state = 'active'",
                    params![
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        version_id.as_bytes(),
                        parent_id.as_bytes(),
                        i64::try_from(actual_parent.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        actual_parent
                            .operation_version_id
                            .map(|id| id.as_bytes().as_slice().to_vec()),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            let parent_head = BranchHead {
                branch_id: parent_id,
                generation: next_generation,
                operation_version_id: Some(version_id),
                root: candidate.result_root,
            };
            let reconciled = commit_product_request(
                self,
                &mut connection,
                "layerfs_branch_transitions",
                candidate.request_id,
            )?;
            Ok(ChildMergeOutcome::WorkingRecorded {
                parent_head,
                reconciled,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_prepare_layer_candidate(
        &self,
        candidate: LayerCandidateRequest,
    ) -> EngineResult<LayerCandidate> {
        if candidate.source_transition.len() > MAX_TRANSITION_PAYLOAD_BYTES
            || candidate.applied_transition.len() > MAX_TRANSITION_PAYLOAD_BYTES
        {
            return Err(EngineError::InvalidRecord(
                "Merge transition resource bound",
            ));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if read_layer_stack_head(&connection, candidate.expected_stack.layer_stack_id)?
                != Some(candidate.expected_stack)
                || read_branch_head(&connection, candidate.source.branch_id)?
                    != Some(candidate.source)
            {
                return Err(EngineError::PublicationConflict);
            }
            let ancestry = read_branch_ancestry(&connection, candidate.source.branch_id)?
                .ok_or(EngineError::InvalidRecord("source Branch"))?;
            if ancestry.origin_layer_stack_id != candidate.expected_stack.layer_stack_id {
                return Err(EngineError::InvalidRecord("cross-tree LayerStack merge"));
            }
            let origin_root = read_layer_root(
                &connection,
                ancestry.origin_layer_stack_id,
                ancestry.origin_layer_id,
            )?
            .ok_or(EngineError::InvalidRecord("origin Layer"))?;
            let source_version = candidate
                .source
                .operation_version_id
                .ok_or(EngineError::InvalidRecord("Layer candidate source head"))?;
            authenticate_root(self, &connection, candidate.result_root)?;
            let source_delta_id = transition_identity(
                origin_root,
                candidate.source.root,
                &candidate.source_transition,
            );
            insert_transition(
                &connection,
                source_delta_id,
                origin_root,
                candidate.source.root,
                &candidate.source_transition,
            )?;
            let applied_delta_id = transition_identity(
                candidate.expected_stack.root,
                candidate.result_root,
                &candidate.applied_transition,
            );
            insert_transition(
                &connection,
                applied_delta_id,
                candidate.expected_stack.root,
                candidate.result_root,
                &candidate.applied_transition,
            )?;
            let branch_delta_id = derive_id(
                b"layer-stack-branch-delta",
                &[
                    candidate.source.branch_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    &source_delta_id,
                    &applied_delta_id,
                ],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_branch_deltas
                     (branch_delta_id, purpose, source_branch_id,
                      source_branch_generation, source_branch_operation_version_id, base_root,
                      source_root, destination_root, result_root,
                      source_delta_id, applied_delta_id)
                     VALUES (?1, 'layer_stack_merge', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        branch_delta_id.as_slice(),
                        candidate.source.branch_id.as_bytes(),
                        i64::try_from(candidate.source.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        source_version.as_bytes(),
                        origin_root.as_bytes(),
                        candidate.source.root.as_bytes(),
                        candidate.expected_stack.root.as_bytes(),
                        candidate.result_root.as_bytes(),
                        source_delta_id.as_slice(),
                        applied_delta_id.as_slice(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let layer_id = LayerId(derive_id(
                b"candidate-layer",
                &[
                    candidate.expected_stack.layer_stack_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    candidate.result_root.as_bytes(),
                ],
            ));
            connection
                .execute(
                    "INSERT INTO layerfs_layers
                     (layer_id, layer_stack_id, parent_layer_id, root_id,
                      creation_kind, source_branch_id, source_branch_depth,
                      source_branch_generation,
                      source_branch_head_operation_version_id,
                      source_branch_delta_id, state, prepared_request_id)
                     VALUES (?1, ?2, ?3, ?4, 'candidate', ?5, ?6, ?7, ?8, ?9,
                             'candidate', ?10)",
                    params![
                        layer_id.as_bytes(),
                        candidate.expected_stack.layer_stack_id.as_bytes(),
                        candidate.expected_stack.layer_id.as_bytes(),
                        candidate.result_root.as_bytes(),
                        candidate.source.branch_id.as_bytes(),
                        i64::try_from(ancestry.depth).map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(candidate.source.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        source_version.as_bytes(),
                        branch_delta_id.as_slice(),
                        candidate.request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let layer_delta_id = derive_id(
                b"layer-delta",
                &[
                    candidate.expected_stack.layer_id.as_bytes(),
                    layer_id.as_bytes(),
                    &applied_delta_id,
                ],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_layer_deltas
                     (layer_delta_id, parent_layer_id, candidate_layer_id,
                      transition_delta_id, parent_root, result_root)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        layer_delta_id.as_slice(),
                        candidate.expected_stack.layer_id.as_bytes(),
                        layer_id.as_bytes(),
                        applied_delta_id.as_slice(),
                        candidate.expected_stack.root.as_bytes(),
                        candidate.result_root.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let source_lease_id = derive_id(
                b"layer-candidate-source-lease",
                &[source_version.as_bytes(), candidate.request_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, 'operation_version', ?2, 'layer_candidate', ?3, ?4)",
                    params![
                        source_lease_id.as_slice(),
                        source_version.as_bytes(),
                        candidate.request_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            let lease_id = derive_id(
                b"layer-candidate-lease",
                &[layer_id.as_bytes(), candidate.request_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, 'layer', ?2, 'layer_candidate', ?3, ?4)",
                    params![
                        lease_id.as_slice(),
                        layer_id.as_bytes(),
                        candidate.request_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT INTO layerfs_retained_roots (root_id) VALUES (?1)
                     ON CONFLICT(root_id) DO NOTHING",
                    params![candidate.result_root.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_layers WHERE layer_id = ?1 AND state = 'candidate')",
                layer_id.as_bytes(),
            )?;
            Ok(LayerCandidate {
                layer_stack_id: candidate.expected_stack.layer_stack_id,
                layer_id,
                parent_layer_id: candidate.expected_stack.layer_id,
                source: candidate.source,
                source_depth: ancestry.depth,
                root: candidate.result_root,
                request_id: candidate.request_id,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_layer_candidates_after(
        &self,
        after: Option<LayerId>,
        limit: usize,
    ) -> EngineResult<Vec<LayerCandidate>> {
        if limit == 0 || limit > 1024 {
            return Err(EngineError::InvalidRecord("Layer candidate page limit"));
        }
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT l.layer_stack_id, l.layer_id, l.parent_layer_id,
                        l.source_branch_id, l.source_branch_generation,
                        l.source_branch_head_operation_version_id,
                        l.source_branch_depth, v.root_id, l.root_id,
                        l.prepared_request_id
                 FROM layerfs_layers l
                 JOIN layerfs_operation_versions v
                   ON v.branch_id = l.source_branch_id
                  AND v.operation_version_id = l.source_branch_head_operation_version_id
                 WHERE l.state = 'candidate' AND (?1 IS NULL OR l.layer_id > ?1)
                 ORDER BY l.layer_id LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map(
                params![
                    after.map(|id| id.as_bytes().as_slice().to_vec()),
                    i64::try_from(limit).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Vec<u8>>(7)?,
                        row.get::<_, Vec<u8>>(8)?,
                        row.get::<_, Vec<u8>>(9)?,
                    ))
                },
            )
            .map_err(map_sqlite_error)?;
        rows.map(|row| {
            let row = row.map_err(map_sqlite_error)?;
            Ok(LayerCandidate {
                layer_stack_id: LayerStackId(bytes32(&row.0, "LayerStackId")?),
                layer_id: LayerId(bytes32(&row.1, "LayerId")?),
                parent_layer_id: LayerId(bytes32(&row.2, "LayerId")?),
                source: BranchHead {
                    branch_id: BranchId(bytes32(&row.3, "BranchId")?),
                    generation: u64::try_from(row.4)
                        .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                    operation_version_id: Some(OperationVersionId(bytes32(
                        &row.5,
                        "OperationVersionId",
                    )?)),
                    root: object_id(&row.7)?,
                },
                source_depth: u64::try_from(row.6)
                    .map_err(|_| EngineError::InvalidRecord("Branch depth"))?,
                root: object_id(&row.8)?,
                request_id: RequestId(bytes32(&row.9, "RequestId")?),
            })
        })
        .collect()
    }

    pub fn product_drop_layer_candidate(&self, layer_id: LayerId) -> EngineResult<bool> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let candidate = connection
                .query_row(
                    "SELECT root_id, prepared_request_id, state FROM layerfs_layers
                     WHERE layer_id = ?1 AND creation_kind = 'candidate'",
                    params![layer_id.as_bytes()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Vec<u8>>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Layer candidate"))?;
            if candidate.2 == "dropped" {
                commit_product_state(
                    self,
                    &mut connection,
                    "SELECT EXISTS(SELECT 1 FROM layerfs_layers
                     WHERE layer_id = ?1 AND state = 'dropped')",
                    layer_id.as_bytes(),
                )?;
                return Ok(true);
            }
            if candidate.2 != "candidate" {
                return Err(EngineError::InvalidRecord("accepted Layer candidate"));
            }
            let changed = connection
                .execute(
                    "UPDATE layerfs_layers SET state = 'dropped'
                     WHERE layer_id = ?1 AND state = 'candidate'",
                    params![layer_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            connection
                .execute(
                    "DELETE FROM layerfs_version_leases
                     WHERE owner_kind = 'layer_candidate' AND owner_id = ?1",
                    params![candidate.1],
                )
                .map_err(map_sqlite_error)?;
            release_retained_root_if_unreferenced(&connection, &candidate.0)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_layers
                 WHERE layer_id = ?1 AND state = 'dropped')",
                layer_id.as_bytes(),
            )?;
            Ok(false)
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_import_layer_candidate(
        &self,
        candidate: LayerCandidate,
        expected_stack: LayerStackHead,
    ) -> EngineResult<LayerCandidate> {
        let connection = self.lock_connection()?;
        let incumbent = connection
            .query_row(
                "SELECT layer_stack_id, parent_layer_id, root_id,
                        source_branch_id, source_branch_depth,
                        source_branch_generation,
                        source_branch_head_operation_version_id,
                        prepared_request_id, state
                 FROM layerfs_layers WHERE layer_id = ?1",
                params![candidate.layer_id.as_bytes()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Vec<u8>>(6)?,
                        row.get::<_, Vec<u8>>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;
        if let Some(incumbent) = incumbent {
            if incumbent.0.as_slice() != candidate.layer_stack_id.as_bytes()
                || incumbent.1.as_slice() != candidate.parent_layer_id.as_bytes()
                || incumbent.2.as_slice() != candidate.root.as_bytes()
                || incumbent.3.as_slice() != candidate.source.branch_id.as_bytes()
                || u64::try_from(incumbent.4).ok() != Some(candidate.source_depth)
                || u64::try_from(incumbent.5).ok() != Some(candidate.source.generation)
                || incumbent.6.as_slice()
                    != candidate
                        .source
                        .operation_version_id
                        .ok_or(EngineError::InvalidRecord("Layer candidate source"))?
                        .as_bytes()
                || incumbent.7.as_slice() != candidate.request_id.as_bytes()
                || !matches!(incumbent.8.as_str(), "candidate" | "accepted")
                || !branch_contains_exact_version(&connection, candidate.source)?
            {
                return Err(EngineError::InvalidRecord(
                    "Layer candidate identity conflict",
                ));
            }
            return Ok(candidate);
        }
        drop(connection);
        let prepared = self.product_prepare_layer_candidate(LayerCandidateRequest {
            source: candidate.source,
            expected_stack,
            result_root: candidate.root,
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id: candidate.request_id,
        })?;
        if prepared != candidate {
            return Err(EngineError::InvalidRecord("Layer candidate identity"));
        }
        Ok(prepared)
    }

    pub fn product_accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
        request_id: RequestId,
    ) -> EngineResult<LayerStackMergeOutcome> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if let Some(head) = replay_layer_stack_request(
                &connection,
                request_id,
                "layer_stack_merge",
                expected,
                candidate.layer_id,
            )? {
                return Ok(LayerStackMergeOutcome::DurablyAccepted {
                    head,
                    reconciled: true,
                });
            }
            let actual = read_layer_stack_head(&connection, expected.layer_stack_id)?
                .ok_or(EngineError::InvalidRecord("LayerStack"))?;
            if actual != expected {
                return Ok(LayerStackMergeOutcome::Conflict { actual });
            }
            if candidate.layer_stack_id != expected.layer_stack_id
                || candidate.parent_layer_id != expected.layer_id
            {
                return Err(EngineError::InvalidRecord("Layer candidate destination"));
            }
            let stored = connection
                .query_row(
                    "SELECT parent_layer_id, source_branch_id, source_branch_depth,
                            source_branch_generation,
                            source_branch_head_operation_version_id, root_id,
                            prepared_request_id, source_branch_delta_id
                     FROM layerfs_layers
                     WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state = 'candidate'",
                    params![
                        candidate.layer_stack_id.as_bytes(),
                        candidate.layer_id.as_bytes()
                    ],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Vec<u8>>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, Vec<u8>>(4)?,
                            row.get::<_, Vec<u8>>(5)?,
                            row.get::<_, Vec<u8>>(6)?,
                            row.get::<_, Vec<u8>>(7)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Layer candidate"))?;
            if stored.0.as_slice() != candidate.parent_layer_id.as_bytes()
                || stored.1.as_slice() != candidate.source.branch_id.as_bytes()
                || u64::try_from(stored.2).ok() != Some(candidate.source_depth)
                || u64::try_from(stored.3).ok() != Some(candidate.source.generation)
                || stored.4.as_slice()
                    != candidate
                        .source
                        .operation_version_id
                        .ok_or(EngineError::InvalidRecord("candidate source version"))?
                        .as_bytes()
                || object_id(&stored.5)? != candidate.root
                || stored.6.as_slice() != candidate.request_id.as_bytes()
                || !branch_contains_exact_version(&connection, candidate.source)?
            {
                return Err(EngineError::InvalidRecord("Layer candidate binding"));
            }
            authenticate_root(self, &connection, candidate.root)?;
            let next_generation = actual
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            let changed = connection
                .execute(
                    "UPDATE layerfs_layers
                     SET state = 'accepted', accepted_generation = ?1
                     WHERE layer_stack_id = ?2 AND layer_id = ?3 AND state = 'candidate'",
                    params![
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        candidate.layer_stack_id.as_bytes(),
                        candidate.layer_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            let transition_id = derive_id(
                b"layer-stack-merge-receipt",
                &[request_id.as_bytes(), candidate.layer_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_layer_stack_transitions
                     (transition_id, layer_stack_id, before_generation,
                      after_generation, before_layer_id, after_layer_id,
                      action_kind, source_record_id, request_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             'layer_stack_merge', ?7, ?8)",
                    params![
                        transition_id.as_slice(),
                        candidate.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes(),
                        candidate.layer_id.as_bytes(),
                        stored.7.as_slice(),
                        request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let changed = connection
                .execute(
                    "UPDATE layerfs_layer_stacks
                     SET generation = ?1, head_layer_id = ?2
                     WHERE layer_stack_id = ?3 AND generation = ?4 AND head_layer_id = ?5",
                    params![
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        candidate.layer_id.as_bytes(),
                        candidate.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            connection
                .execute(
                    "DELETE FROM layerfs_version_leases
                     WHERE (target_kind = 'layer' AND target_id = ?1
                            AND owner_kind IN ('layer_candidate', 'layer_stack_merge'))
                        OR (owner_kind = 'layer_candidate' AND owner_id = ?2)",
                    params![
                        candidate.layer_id.as_bytes(),
                        candidate.request_id.as_bytes()
                    ],
                )
                .map_err(map_sqlite_error)?;
            let head = LayerStackHead {
                layer_stack_id: actual.layer_stack_id,
                generation: next_generation,
                layer_id: candidate.layer_id,
                root: candidate.root,
            };
            let reconciled = commit_product_request(
                self,
                &mut connection,
                "layerfs_layer_stack_transitions",
                request_id,
            )?;
            Ok(LayerStackMergeOutcome::DurablyAccepted { head, reconciled })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_drop_branch(&self, branch_id: BranchId) -> EngineResult<()> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let children = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_branches
                     WHERE immediate_parent_branch_id = ?1 AND state = 'active')",
                    params![branch_id.as_bytes()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if children {
                return Err(EngineError::InvalidRecord("active child Branch"));
            }
            let changed = connection
                .execute(
                    "UPDATE layerfs_branches SET state = 'dropped'
                     WHERE branch_id = ?1 AND state = 'active'",
                    params![branch_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::InvalidRecord("Branch"));
            }
            connection
                .execute(
                    "DELETE FROM layerfs_version_leases
                     WHERE owner_kind = 'branch' AND owner_id = ?1",
                    params![branch_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            release_unreferenced_retained_roots(&connection, None)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_branches WHERE branch_id = ?1 AND state = 'dropped')",
                branch_id.as_bytes(),
            )
            .map(drop)
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_branch_rollback(
        &self,
        expected: BranchHead,
        target: OperationVersionId,
        request_id: RequestId,
    ) -> EngineResult<BranchRollbackOutcome> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let prior = connection
                .query_row(
                    "SELECT after_generation, after_operation_version_id
                     FROM layerfs_branch_transitions
                     WHERE request_id = ?1 AND branch_id = ?2
                       AND action_kind = 'branch_rollback'",
                    params![request_id.as_bytes(), expected.branch_id.as_bytes()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            if let Some((generation, version)) = prior {
                let generation = u64::try_from(generation)
                    .map_err(|_| EngineError::InvalidRecord("Branch generation"))?;
                if generation
                    != expected
                        .generation
                        .checked_add(1)
                        .ok_or(EngineError::CounterOverflow)?
                    || version.as_slice() != target.as_bytes()
                {
                    return Err(EngineError::InvalidRecord(
                        "BranchRollback request identity conflict",
                    ));
                }
                let root = connection
                    .query_row(
                        "SELECT root_id FROM layerfs_operation_versions
                         WHERE branch_id = ?1 AND operation_version_id = ?2",
                        params![expected.branch_id.as_bytes(), target.as_bytes()],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                    .map_err(map_sqlite_error)?;
                return Ok(BranchRollbackOutcome::WorkingRecorded {
                    head: BranchHead {
                        branch_id: expected.branch_id,
                        generation,
                        operation_version_id: Some(target),
                        root: object_id(&root)?,
                    },
                    reconciled: true,
                });
            }
            let actual = read_branch_head(&connection, expected.branch_id)?
                .ok_or(EngineError::InvalidRecord("Branch"))?;
            if actual != expected {
                return Ok(BranchRollbackOutcome::Conflict { actual });
            }
            let (target_sequence, target_root) = connection
                .query_row(
                    "SELECT sequence, root_id FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2
                       AND NOT EXISTS(
                           SELECT 1 FROM layerfs_released_versions r
                           WHERE r.target_kind = 'operation_version'
                             AND r.owner_id = layerfs_operation_versions.branch_id
                             AND r.version_id = layerfs_operation_versions.operation_version_id)",
                    params![expected.branch_id.as_bytes(), target.as_bytes()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("rollback target"))?;
            let current_sequence = connection
                .query_row(
                    "SELECT sequence FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2",
                    params![
                        expected.branch_id.as_bytes(),
                        expected
                            .operation_version_id
                            .ok_or(EngineError::InvalidRecord("Branch rollback head"))?
                            .as_bytes()
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(map_sqlite_error)?;
            if target_sequence >= current_sequence {
                return Err(EngineError::InvalidRecord("rollback target is not earlier"));
            }
            let blocked = connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM layerfs_version_leases l
                       JOIN layerfs_operation_versions v
                         ON l.target_kind = 'operation_version' AND l.target_id = v.operation_version_id
                       WHERE v.branch_id = ?1 AND v.sequence > ?2
                    )",
                    params![expected.branch_id.as_bytes(), target_sequence],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if blocked {
                return Ok(BranchRollbackOutcome::Blocked);
            }
            let next_generation = actual
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            let transition_id = derive_id(
                b"branch-rollback-receipt",
                &[request_id.as_bytes(), target.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_branch_transitions
                     (transition_id, branch_id, before_generation, after_generation,
                      before_operation_version_id, after_operation_version_id,
                      action_kind, source_record_id, request_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             'branch_rollback', ?6, ?7)",
                    params![
                        transition_id.as_slice(),
                        expected.branch_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        actual
                            .operation_version_id
                            .ok_or(EngineError::InvalidRecord("Branch rollback head"))?
                            .as_bytes(),
                        target.as_bytes(),
                        request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let changed = connection
                .execute(
                    "UPDATE layerfs_branches
                     SET generation = ?1, head_operation_version_id = ?2
                     WHERE branch_id = ?3 AND generation = ?4
                       AND head_operation_version_id = ?5 AND state = 'active'",
                    params![
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        target.as_bytes(),
                        expected.branch_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        actual
                            .operation_version_id
                            .ok_or(EngineError::InvalidRecord("Branch rollback head"))?
                            .as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            record_branch_suffix_release(
                &connection,
                expected.branch_id,
                target_sequence,
                current_sequence,
                next_generation,
                request_id,
            )?;
            let head = BranchHead {
                branch_id: actual.branch_id,
                generation: next_generation,
                operation_version_id: Some(target),
                root: object_id(&target_root)?,
            };
            let reconciled = commit_product_request(
                self,
                &mut connection,
                "layerfs_branch_transitions",
                request_id,
            )?;
            Ok(BranchRollbackOutcome::WorkingRecorded { head, reconciled })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
        request_id: RequestId,
    ) -> EngineResult<LayerStackRollbackOutcome> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if let Some(head) = replay_layer_stack_request(
                &connection,
                request_id,
                "layer_stack_rollback",
                expected,
                target,
            )? {
                return Ok(LayerStackRollbackOutcome::DurablyAccepted {
                    head,
                    reconciled: true,
                });
            }
            let actual = read_layer_stack_head(&connection, expected.layer_stack_id)?
                .ok_or(EngineError::InvalidRecord("LayerStack"))?;
            if actual != expected {
                return Ok(LayerStackRollbackOutcome::Conflict { actual });
            }
            let (target_generation, target_root) = connection
                .query_row(
                    "SELECT accepted_generation, root_id FROM layerfs_layers
                     WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state = 'accepted'
                       AND NOT EXISTS(
                           SELECT 1 FROM layerfs_released_versions r
                           WHERE r.target_kind = 'layer'
                             AND r.owner_id = layerfs_layers.layer_stack_id
                             AND r.version_id = layerfs_layers.layer_id)",
                    params![expected.layer_stack_id.as_bytes(), target.as_bytes()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Layer rollback target"))?;
            let current_generation = connection
                .query_row(
                    "SELECT accepted_generation FROM layerfs_layers
                     WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state = 'accepted'",
                    params![
                        expected.layer_stack_id.as_bytes(),
                        expected.layer_id.as_bytes()
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(map_sqlite_error)?;
            if target_generation >= current_generation {
                return Err(EngineError::InvalidRecord(
                    "Layer rollback target is not earlier",
                ));
            }
            let blocked = connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM layerfs_version_leases v
                       JOIN layerfs_layers l
                         ON v.target_kind = 'layer' AND v.target_id = l.layer_id
                       WHERE l.layer_stack_id = ?1
                         AND (l.accepted_generation > ?2 OR l.accepted_generation IS NULL)
                    )",
                    params![expected.layer_stack_id.as_bytes(), target_generation],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if blocked {
                return Ok(LayerStackRollbackOutcome::Blocked);
            }
            let next_generation = actual
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            let transition_id = derive_id(
                b"layer-stack-rollback-receipt",
                &[request_id.as_bytes(), target.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_layer_stack_transitions
                     (transition_id, layer_stack_id, before_generation,
                      after_generation, before_layer_id, after_layer_id,
                      action_kind, source_record_id, request_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             'layer_stack_rollback', ?6, ?7)",
                    params![
                        transition_id.as_slice(),
                        expected.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes(),
                        target.as_bytes(),
                        request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let changed = connection
                .execute(
                    "UPDATE layerfs_layer_stacks
                     SET generation = ?1, head_layer_id = ?2
                     WHERE layer_stack_id = ?3 AND generation = ?4 AND head_layer_id = ?5",
                    params![
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        target.as_bytes(),
                        expected.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            record_layer_suffix_release(
                &connection,
                expected.layer_stack_id,
                target_generation,
                current_generation,
                next_generation,
                request_id,
            )?;
            let head = LayerStackHead {
                layer_stack_id: actual.layer_stack_id,
                generation: next_generation,
                layer_id: target,
                root: object_id(&target_root)?,
            };
            let reconciled = commit_product_request(
                self,
                &mut connection,
                "layerfs_layer_stack_transitions",
                request_id,
            )?;
            Ok(LayerStackRollbackOutcome::DurablyAccepted { head, reconciled })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}

fn record_branch_suffix_release(
    connection: &Connection,
    branch_id: BranchId,
    target_sequence: i64,
    current_sequence: i64,
    release_generation: u64,
    request_id: RequestId,
) -> EngineResult<()> {
    connection
        .execute(
            "INSERT OR IGNORE INTO layerfs_released_versions
             (target_kind, owner_id, version_id, root_id, release_generation, request_id)
             SELECT 'operation_version', branch_id, operation_version_id, root_id, ?1, ?2
             FROM layerfs_operation_versions
             WHERE branch_id = ?3 AND sequence > ?4 AND sequence <= ?5",
            params![
                i64::try_from(release_generation).map_err(|_| EngineError::CounterOverflow)?,
                request_id.as_bytes(),
                branch_id.as_bytes(),
                target_sequence,
                current_sequence,
            ],
        )
        .map_err(map_sqlite_error)?;
    release_unreferenced_retained_roots(connection, None)
}

fn record_layer_suffix_release(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    target_generation: i64,
    current_generation: i64,
    release_generation: u64,
    request_id: RequestId,
) -> EngineResult<()> {
    connection
        .execute(
            "INSERT OR IGNORE INTO layerfs_released_versions
             (target_kind, owner_id, version_id, root_id, release_generation, request_id)
             SELECT 'layer', layer_stack_id, layer_id, root_id, ?1, ?2
             FROM layerfs_layers
             WHERE layer_stack_id = ?3 AND state = 'accepted'
               AND accepted_generation > ?4 AND accepted_generation <= ?5",
            params![
                i64::try_from(release_generation).map_err(|_| EngineError::CounterOverflow)?,
                request_id.as_bytes(),
                layer_stack_id.as_bytes(),
                target_generation,
                current_generation,
            ],
        )
        .map_err(map_sqlite_error)?;
    release_unreferenced_retained_roots(connection, None)
}

fn collect_fetch_roots(
    bundle: &BranchPushBundle,
    roots: &mut std::collections::BTreeSet<ObjectId>,
) -> EngineResult<()> {
    roots.insert(bundle.ancestry.fork_root);
    let head_released =
        bundle.head.operation_version_id.is_some_and(|head| {
            bundle.operations.iter().any(|operation| {
                operation.operation_version_id == head && operation.release.is_some()
            }) || bundle
                .child_merges
                .iter()
                .any(|merge| merge.operation_version_id == head && merge.release.is_some())
        });
    if !head_released {
        roots.insert(bundle.head.root);
    }
    for layer in &bundle.origin_stack.layers {
        if layer.release.is_none() {
            roots.insert(layer.root);
        }
    }
    for operation in &bundle.operations {
        if operation.release.is_none() {
            roots.insert(operation.root);
        }
    }
    for merge in &bundle.child_merges {
        if merge.release.is_none() {
            roots.insert(merge.root);
        }
    }
    for dependency in &bundle.dependencies {
        collect_fetch_roots(dependency, roots)?;
    }
    if roots.len() > 4096 {
        return Err(EngineError::InvalidRecord("Fetch root closure bound"));
    }
    Ok(())
}

fn pushed_release(
    generation: Option<i64>,
    request_id: Option<Vec<u8>>,
) -> EngineResult<Option<PushedRelease>> {
    match (generation, request_id) {
        (None, None) => Ok(None),
        (Some(generation), Some(request_id)) => Ok(Some(PushedRelease {
            generation: u64::try_from(generation)
                .map_err(|_| EngineError::InvalidRecord("release generation"))?,
            request_id: RequestId(bytes32(&request_id, "RequestId")?),
        })),
        _ => Err(EngineError::InvalidRecord("release record")),
    }
}

#[derive(Debug)]
struct StoredOperation {
    branch_id: BranchId,
    expected_generation: u64,
    base_root: ObjectId,
    candidate_root: Option<ObjectId>,
    state: String,
}

fn load_operation(
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

fn replay_operation_commit(
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

fn effective_branch_base(connection: &Connection, head: BranchHead) -> EngineResult<VersionRef> {
    if let Some(operation_version_id) = head.operation_version_id {
        return Ok(VersionRef::OperationVersion {
            branch_id: head.branch_id,
            operation_version_id,
            root: head.root,
        });
    }
    let ancestry = read_branch_ancestry(connection, head.branch_id)?
        .ok_or(EngineError::InvalidRecord("Branch ancestry"))?;
    if let Some(operation_version_id) = ancestry.fork_operation_version_id {
        Ok(VersionRef::OperationVersion {
            branch_id: ancestry
                .immediate_parent_branch_id
                .ok_or(EngineError::InvalidRecord("child Branch parent"))?,
            operation_version_id,
            root: ancestry.fork_root,
        })
    } else {
        Ok(VersionRef::Layer {
            layer_stack_id: ancestry.origin_layer_stack_id,
            layer_id: ancestry.origin_layer_id,
            root: ancestry.fork_root,
        })
    }
}

fn next_operation_version_sequence(
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

fn read_layer_stack_head(
    connection: &Connection,
    id: LayerStackId,
) -> EngineResult<Option<LayerStackHead>> {
    if let Some(published) = read_published_fetch_stack_head(connection, id)? {
        return Ok(published);
    }
    read_database_layer_stack_head(connection, id)
}

fn read_database_layer_stack_head(
    connection: &Connection,
    id: LayerStackId,
) -> EngineResult<Option<LayerStackHead>> {
    connection
        .query_row(
            "SELECT s.generation, s.head_layer_id, l.root_id
             FROM layerfs_layer_stacks s
             JOIN layerfs_layers l
               ON l.layer_stack_id = s.layer_stack_id AND l.layer_id = s.head_layer_id
             WHERE s.layer_stack_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(generation, layer, root)| {
            Ok(LayerStackHead {
                layer_stack_id: id,
                generation: u64::try_from(generation)
                    .map_err(|_| EngineError::InvalidRecord("LayerStack generation"))?,
                layer_id: LayerId(bytes32(&layer, "LayerId")?),
                root: object_id(&root)?,
            })
        })
        .transpose()
}

fn layer_stack_head_at_generation(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    generation: u64,
) -> EngineResult<LayerStackHead> {
    let layer = if generation == 0 {
        connection
            .query_row(
                "SELECT layer_id, root_id FROM layerfs_layers
                 WHERE layer_stack_id = ?1 AND creation_kind = 'genesis'
                   AND state = 'accepted' AND accepted_generation = 0",
                params![layer_stack_id.as_bytes()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?
    } else {
        connection
            .query_row(
                "SELECT t.after_layer_id, l.root_id
                 FROM layerfs_layer_stack_transitions t
                 JOIN layerfs_layers l
                   ON l.layer_stack_id = t.layer_stack_id
                  AND l.layer_id = t.after_layer_id
                 WHERE t.layer_stack_id = ?1 AND t.after_generation = ?2",
                params![
                    layer_stack_id.as_bytes(),
                    i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?
    }
    .ok_or(EngineError::InvalidRecord("LayerStack generation"))?;
    Ok(LayerStackHead {
        layer_stack_id,
        generation,
        layer_id: LayerId(bytes32(&layer.0, "LayerId")?),
        root: object_id(&layer.1)?,
    })
}

fn read_layer_root(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    layer_id: LayerId,
) -> EngineResult<Option<ObjectId>> {
    connection
        .query_row(
            "SELECT root_id FROM layerfs_layers
             WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state != 'dropped'
               AND NOT EXISTS(
                   SELECT 1 FROM layerfs_released_versions r
                   WHERE r.target_kind = 'layer'
                     AND r.owner_id = layerfs_layers.layer_stack_id
                     AND r.version_id = layerfs_layers.layer_id)",
            params![layer_stack_id.as_bytes(), layer_id.as_bytes()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|bytes| object_id(&bytes))
        .transpose()
}

fn read_historical_layer_root(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    layer_id: LayerId,
) -> EngineResult<Option<ObjectId>> {
    connection
        .query_row(
            "SELECT root_id FROM layerfs_layers
             WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state != 'dropped'",
            params![layer_stack_id.as_bytes(), layer_id.as_bytes()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|bytes| object_id(&bytes))
        .transpose()
}

fn read_branch_head(connection: &Connection, id: BranchId) -> EngineResult<Option<BranchHead>> {
    if let Some(published) = read_published_fetch_branch_head(connection, id)? {
        return Ok(published);
    }
    read_database_branch_head(connection, id)
}

fn read_database_branch_head(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<BranchHead>> {
    connection
        .query_row(
            "SELECT b.generation, b.head_operation_version_id,
                    COALESCE(v.root_id, b.fork_root_id)
             FROM layerfs_branches b
             LEFT JOIN layerfs_operation_versions v
               ON v.branch_id = b.branch_id
              AND v.operation_version_id = b.head_operation_version_id
             WHERE b.branch_id = ?1 AND b.state = 'active'",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(generation, version, root)| {
            Ok(BranchHead {
                branch_id: id,
                generation: u64::try_from(generation)
                    .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                operation_version_id: version
                    .map(|bytes| -> EngineResult<OperationVersionId> {
                        Ok(OperationVersionId(bytes32(&bytes, "OperationVersionId")?))
                    })
                    .transpose()?,
                root: object_id(&root)?,
            })
        })
        .transpose()
}

fn read_published_fetch_branch_head(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<Option<BranchHead>>> {
    connection
        .query_row(
            "SELECT published_generation, published_version_id, published_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = 'branch' AND target_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(
            |(generation, version, root)| match (generation, version, root) {
                (None, None, None) => Ok(None),
                (Some(generation), version, Some(root)) => Ok(Some(BranchHead {
                    branch_id: id,
                    generation: u64::try_from(generation)
                        .map_err(|_| EngineError::InvalidRecord("Fetch Branch generation"))?,
                    operation_version_id: version
                        .map(|bytes| bytes32(&bytes, "OperationVersionId").map(OperationVersionId))
                        .transpose()?,
                    root: object_id(&root)?,
                })),
                _ => Err(EngineError::InvalidRecord("Fetch published Branch head")),
            },
        )
        .transpose()
}

fn read_published_fetch_stack_head(
    connection: &Connection,
    id: LayerStackId,
) -> EngineResult<Option<Option<LayerStackHead>>> {
    connection
        .query_row(
            "SELECT published_generation, published_version_id, published_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = 'layer_stack' AND target_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(
            |(generation, layer, root)| match (generation, layer, root) {
                (None, None, None) => Ok(None),
                (Some(generation), Some(layer), Some(root)) => Ok(Some(LayerStackHead {
                    layer_stack_id: id,
                    generation: u64::try_from(generation)
                        .map_err(|_| EngineError::InvalidRecord("Fetch LayerStack generation"))?,
                    layer_id: LayerId(bytes32(&layer, "LayerId")?),
                    root: object_id(&root)?,
                })),
                _ => Err(EngineError::InvalidRecord(
                    "Fetch published LayerStack head",
                )),
            },
        )
        .transpose()
}

fn read_fetch_branch_head(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<BranchHead>> {
    let staged = connection
        .query_row(
            "SELECT staged_generation, staged_version_id, staged_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = 'branch' AND target_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some((generation, version, root)) = staged else {
        return read_database_branch_head(connection, id);
    };
    let head = BranchHead {
        branch_id: id,
        generation: u64::try_from(generation)
            .map_err(|_| EngineError::InvalidRecord("Fetch Branch generation"))?,
        operation_version_id: version
            .map(|bytes| bytes32(&bytes, "OperationVersionId").map(OperationVersionId))
            .transpose()?,
        root: object_id(&root)?,
    };
    if read_database_branch_head(connection, id)? != Some(head) {
        return Err(EngineError::InvalidRecord("Fetch staged Branch head"));
    }
    Ok(Some(head))
}

fn read_fetch_stack_head(
    connection: &Connection,
    id: LayerStackId,
) -> EngineResult<Option<LayerStackHead>> {
    let staged = connection
        .query_row(
            "SELECT staged_generation, staged_version_id, staged_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = 'layer_stack' AND target_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some((generation, layer, root)) = staged else {
        return read_database_layer_stack_head(connection, id);
    };
    let head = LayerStackHead {
        layer_stack_id: id,
        generation: u64::try_from(generation)
            .map_err(|_| EngineError::InvalidRecord("Fetch LayerStack generation"))?,
        layer_id: LayerId(bytes32(
            &layer.ok_or(EngineError::InvalidRecord("Fetch staged LayerStack head"))?,
            "LayerId",
        )?),
        root: object_id(&root)?,
    };
    if read_database_layer_stack_head(connection, id)? != Some(head) {
        return Err(EngineError::InvalidRecord("Fetch staged LayerStack head"));
    }
    Ok(Some(head))
}

#[allow(clippy::too_many_arguments)]
fn stage_fetch_head(
    connection: &Connection,
    target_kind: &str,
    target_id: &[u8; 32],
    durable_storage_id: [u8; 32],
    published_generation: Option<u64>,
    published_version_id: Option<Vec<u8>>,
    published_root: Option<ObjectId>,
    staged_generation: u64,
    staged_version_id: Option<Vec<u8>>,
    staged_root: ObjectId,
) -> EngineResult<()> {
    let published_generation = published_generation
        .map(i64::try_from)
        .transpose()
        .map_err(|_| EngineError::CounterOverflow)?;
    let staged_generation =
        i64::try_from(staged_generation).map_err(|_| EngineError::CounterOverflow)?;
    let incumbent = connection
        .query_row(
            "SELECT durable_storage_id, published_generation,
                    published_version_id, published_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = ?1 AND target_id = ?2",
            params![target_kind, target_id],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let published_root = published_root.map(|root| root.as_bytes().to_vec());
    if let Some(incumbent) = incumbent {
        if incumbent.0.as_slice() != durable_storage_id
            || incumbent.1 != published_generation
            || incumbent.2 != published_version_id
            || incumbent.3 != published_root
        {
            return Err(EngineError::InvalidRecord("Fetch staging identity"));
        }
        connection
            .execute(
                "UPDATE layerfs_fetch_staging_heads
                 SET staged_generation = ?1, staged_version_id = ?2,
                     staged_root_id = ?3, updated_at = ?4
                 WHERE target_kind = ?5 AND target_id = ?6",
                params![
                    staged_generation,
                    staged_version_id,
                    staged_root.as_bytes(),
                    unix_seconds()?,
                    target_kind,
                    target_id,
                ],
            )
            .map_err(map_sqlite_error)?;
    } else {
        connection
            .execute(
                "INSERT INTO layerfs_fetch_staging_heads
                 (target_kind, target_id, durable_storage_id,
                  published_generation, published_version_id, published_root_id,
                  staged_generation, staged_version_id, staged_root_id, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    target_kind,
                    target_id,
                    durable_storage_id.as_slice(),
                    published_generation,
                    published_version_id,
                    published_root,
                    staged_generation,
                    staged_version_id,
                    staged_root.as_bytes(),
                    unix_seconds()?,
                ],
            )
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

fn stage_fetch_branch_head(
    connection: &Connection,
    durable_storage_id: [u8; 32],
    published: Option<BranchHead>,
    staged: BranchHead,
) -> EngineResult<()> {
    stage_fetch_head(
        connection,
        "branch",
        staged.branch_id.as_bytes(),
        durable_storage_id,
        published.map(|head| head.generation),
        published
            .and_then(|head| head.operation_version_id)
            .map(|id| id.0.to_vec()),
        published.map(|head| head.root),
        staged.generation,
        staged.operation_version_id.map(|id| id.0.to_vec()),
        staged.root,
    )
}

fn stage_fetch_stack_head(
    connection: &Connection,
    durable_storage_id: [u8; 32],
    published: Option<LayerStackHead>,
    staged: LayerStackHead,
) -> EngineResult<()> {
    stage_fetch_head(
        connection,
        "layer_stack",
        staged.layer_stack_id.as_bytes(),
        durable_storage_id,
        published.map(|head| head.generation),
        published.map(|head| head.layer_id.0.to_vec()),
        published.map(|head| head.root),
        staged.generation,
        Some(staged.layer_id.0.to_vec()),
        staged.root,
    )
}

fn branch_head_at_generation(
    connection: &Connection,
    branch_id: BranchId,
    generation: u64,
) -> EngineResult<BranchHead> {
    if generation == 0 {
        let ancestry = read_branch_ancestry(connection, branch_id)?
            .ok_or(EngineError::InvalidRecord("Branch ancestry"))?;
        return Ok(BranchHead {
            branch_id,
            generation,
            operation_version_id: None,
            root: ancestry.fork_root,
        });
    }
    connection
        .query_row(
            "SELECT bt.after_operation_version_id, v.root_id
             FROM layerfs_branch_transitions bt
             JOIN layerfs_operation_versions v
               ON v.branch_id = bt.branch_id
              AND v.operation_version_id = bt.after_operation_version_id
             WHERE bt.branch_id = ?1 AND bt.after_generation = ?2",
            params![
                branch_id.as_bytes(),
                i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?
            ],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(version, root)| -> EngineResult<BranchHead> {
            Ok(BranchHead {
                branch_id,
                generation,
                operation_version_id: Some(OperationVersionId(bytes32(
                    &version,
                    "OperationVersionId",
                )?)),
                root: object_id(&root)?,
            })
        })
        .transpose()?
        .ok_or(EngineError::InvalidRecord("Branch generation"))
}

fn read_branch_ancestry(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<BranchAncestry>> {
    connection
        .query_row(
            "SELECT immediate_parent_branch_id, fork_operation_id,
                    fork_operation_version_id, fork_root_id,
                    origin_layer_stack_id, origin_layer_id, depth
             FROM layerfs_branches WHERE branch_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Option<Vec<u8>>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(parent, operation, version, root, stack, layer, depth)| {
            Ok(BranchAncestry {
                immediate_parent_branch_id: parent
                    .map(|bytes| -> EngineResult<BranchId> {
                        Ok(BranchId(bytes32(&bytes, "BranchId")?))
                    })
                    .transpose()?,
                fork_operation_id: operation
                    .map(|bytes| -> EngineResult<OperationId> {
                        Ok(OperationId(bytes32(&bytes, "OperationId")?))
                    })
                    .transpose()?,
                fork_operation_version_id: version
                    .map(|bytes| -> EngineResult<OperationVersionId> {
                        Ok(OperationVersionId(bytes32(&bytes, "OperationVersionId")?))
                    })
                    .transpose()?,
                fork_root: object_id(&root)?,
                origin_layer_stack_id: LayerStackId(bytes32(&stack, "LayerStackId")?),
                origin_layer_id: LayerId(bytes32(&layer, "LayerId")?),
                depth: u64::try_from(depth)
                    .map_err(|_| EngineError::InvalidRecord("Branch depth"))?,
            })
        })
        .transpose()
}

fn authenticate_root(engine: &Engine, connection: &Connection, root: ObjectId) -> EngineResult<()> {
    authenticate_root_object(engine, connection, root)?;
    let statements = Cell::new(0);
    let failed = Cell::new(integrity::VerificationObservation::default());
    let observation = integrity::verify_root(
        connection,
        &engine.path,
        engine.store_id,
        root,
        &statements,
        &failed,
    )?;
    engine.bump(|counters| {
        checked_add(&mut counters.candidate_full_scans, 1)?;
        checked_add(&mut counters.root_verifications, 1)?;
        checked_add(&mut counters.root_verification_objects, observation.objects)?;
        checked_add(&mut counters.root_verification_bytes, observation.bytes)?;
        super::add_verification_progress_counters(counters, observation)
    })
}

fn authenticate_root_shallow(
    engine: &Engine,
    connection: &Connection,
    root: ObjectId,
) -> EngineResult<()> {
    authenticate_root_object(engine, connection, root)?;
    engine.bump(|counters| checked_add(&mut counters.candidate_shallow_bindings, 1))
}

fn authenticate_root_object(
    engine: &Engine,
    connection: &Connection,
    root: ObjectId,
) -> EngineResult<()> {
    with_authenticated_canonical_on_connection(
        engine,
        connection,
        root,
        true,
        true,
        |_, canonical| {
            decode_namespace_root(canonical)
                .map(drop)
                .map_err(EngineError::Core)
        },
    )
}

fn insert_transition(
    connection: &Connection,
    id: [u8; 32],
    parent: ObjectId,
    child: ObjectId,
    payload: &[u8],
) -> EngineResult<()> {
    let incumbent = connection
        .query_row(
            "SELECT format_version, parent_root, child_root, payload
             FROM layerfs_deltas WHERE delta_id = ?1",
            params![id.as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    if let Some((format, incumbent_parent, incumbent_child, incumbent_payload)) = incumbent {
        if format != TRANSITION_FORMAT_VERSION
            || incumbent_parent.as_deref() != Some(parent.as_bytes())
            || incumbent_child.as_slice() != child.as_bytes()
            || incumbent_payload != payload
        {
            return Err(EngineError::InvalidRecord("transition identity conflict"));
        }
        return Ok(());
    }
    connection
        .execute(
            "INSERT INTO layerfs_deltas
             (delta_id, format_version, parent_root, child_root, payload)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.as_slice(),
                TRANSITION_FORMAT_VERSION,
                parent.as_bytes(),
                child.as_bytes(),
                payload,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn insert_pushed_operation(
    engine: &Engine,
    connection: &Connection,
    bundle: &BranchPushBundle,
    operation: &PushedOperation,
    prior_version: Option<OperationVersionId>,
    prior_root: ObjectId,
    prior_generation: u64,
) -> EngineResult<(OperationVersionId, ObjectId, u64)> {
    let expected_base = match prior_version {
        Some(operation_version_id) => VersionRef::OperationVersion {
            branch_id: bundle.head.branch_id,
            operation_version_id,
            root: prior_root,
        },
        None => match (
            bundle.ancestry.immediate_parent_branch_id,
            bundle.ancestry.fork_operation_version_id,
        ) {
            (Some(parent), Some(operation_version_id)) => VersionRef::OperationVersion {
                branch_id: parent,
                operation_version_id,
                root: prior_root,
            },
            (None, None) => VersionRef::Layer {
                layer_stack_id: bundle.ancestry.origin_layer_stack_id,
                layer_id: bundle.ancestry.origin_layer_id,
                root: prior_root,
            },
            _ => return Err(EngineError::InvalidRecord("Push Branch ancestry")),
        },
    };
    if operation.parent_operation_version_id != prior_version
        || operation.base != expected_base
        || operation.expected_branch_generation != prior_generation
        || operation.before_generation != prior_generation
        || operation.after_generation
            != prior_generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?
    {
        return Err(EngineError::InvalidRecord("Push operation chain"));
    }
    if operation.release.is_none() {
        authenticate_root(engine, connection, operation.root)?;
    }
    let transition_id = transition_identity(
        operation.base.root(),
        operation.root,
        &operation.transition_payload,
    );
    if transition_id != operation.transition_delta_id {
        return Err(EngineError::InvalidRecord("Push transition identity"));
    }
    insert_transition(
        connection,
        transition_id,
        operation.base.root(),
        operation.root,
        &operation.transition_payload,
    )?;
    let (base_kind, base_stack, base_layer, base_version) = match operation.base {
        VersionRef::Layer {
            layer_stack_id,
            layer_id,
            ..
        } => (
            "layer",
            Some(layer_stack_id.0.to_vec()),
            Some(layer_id.0.to_vec()),
            None,
        ),
        VersionRef::OperationVersion {
            operation_version_id,
            ..
        } => (
            "operation_version",
            None,
            None,
            Some(operation_version_id.0.to_vec()),
        ),
    };
    connection
        .execute(
            "INSERT INTO layerfs_operations
             (operation_id, branch_id, sequence, expected_branch_generation,
              base_kind, base_layer_stack_id, base_layer_id,
              base_operation_version_id, base_root_id, candidate_root_id,
              result_operation_version_id, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                     'durably_accepted')",
            params![
                operation.operation_id.as_bytes(),
                bundle.head.branch_id.as_bytes(),
                i64::try_from(operation.operation_sequence)
                    .map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(operation.expected_branch_generation)
                    .map_err(|_| EngineError::CounterOverflow)?,
                base_kind,
                base_stack,
                base_layer,
                base_version,
                operation.base.root().as_bytes(),
                operation.root.as_bytes(),
                operation.operation_version_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "INSERT INTO layerfs_operation_versions
             (operation_version_id, branch_id, sequence,
              parent_operation_version_id, root_id, created_by_kind,
              created_by_operation_id)
             VALUES (?1, ?2, ?3, ?4, ?5, 'operation', ?6)",
            params![
                operation.operation_version_id.as_bytes(),
                bundle.head.branch_id.as_bytes(),
                i64::try_from(operation.version_sequence)
                    .map_err(|_| EngineError::CounterOverflow)?,
                operation
                    .parent_operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                operation.root.as_bytes(),
                operation.operation_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "INSERT INTO layerfs_operation_deltas
             (operation_delta_id, operation_id, operation_version_id,
              transition_delta_id, base_root, result_root)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                operation.operation_delta_id.as_slice(),
                operation.operation_id.as_bytes(),
                operation.operation_version_id.as_bytes(),
                operation.transition_delta_id.as_slice(),
                operation.base.root().as_bytes(),
                operation.root.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    let receipt = derive_id(
        b"branch-transition",
        &[
            operation.request_id.as_bytes(),
            operation.operation_version_id.as_bytes(),
        ],
    );
    connection
        .execute(
            "INSERT INTO layerfs_branch_transitions
             (transition_id, branch_id, before_generation, after_generation,
              before_operation_version_id, after_operation_version_id,
              action_kind, source_record_id, request_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'operation_commit', ?7, ?8)",
            params![
                receipt.as_slice(),
                bundle.head.branch_id.as_bytes(),
                i64::try_from(operation.before_generation)
                    .map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(operation.after_generation)
                    .map_err(|_| EngineError::CounterOverflow)?,
                operation
                    .parent_operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                operation.operation_version_id.as_bytes(),
                operation.operation_id.as_bytes(),
                operation.request_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    match operation.release {
        Some(release) => record_pushed_release(
            connection,
            "operation_version",
            bundle.head.branch_id.as_bytes(),
            operation.operation_version_id.as_bytes(),
            operation.root,
            release,
        )?,
        None => retain_root(connection, operation.root)?,
    }
    Ok((
        operation.operation_version_id,
        operation.root,
        operation.after_generation,
    ))
}

fn export_layer_stack_snapshot(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    base: Option<LayerStackHead>,
    advance: bool,
) -> EngineResult<PushedLayerStack> {
    let durable_head = read_layer_stack_head(connection, layer_stack_id)?
        .ok_or(EngineError::InvalidRecord("Fetch LayerStack"))?;
    if base.is_some_and(|base| {
        base.layer_stack_id != layer_stack_id || base.generation > durable_head.generation
    }) {
        return Err(EngineError::InvalidRecord("Fetch LayerStack base"));
    }
    if let Some(base) = base {
        if layer_stack_head_at_generation(connection, layer_stack_id, base.generation)? != base {
            return Err(EngineError::InvalidRecord("Fetch LayerStack base"));
        }
    }
    let after_generation = base.map_or(0, |base| base.generation);
    let page_generation = if advance {
        after_generation
            .checked_add(
                u64::try_from(MAX_HISTORY_PAGE_RECORDS)
                    .map_err(|_| EngineError::CounterOverflow)?,
            )
            .ok_or(EngineError::CounterOverflow)?
            .min(durable_head.generation)
    } else {
        after_generation
    };
    let head = layer_stack_head_at_generation(connection, layer_stack_id, page_generation)?;
    let name = connection
        .query_row(
            "SELECT name FROM layerfs_layer_stacks WHERE layer_stack_id = ?1",
            params![layer_stack_id.as_bytes()],
            |row| row.get::<_, String>(0),
        )
        .map_err(map_sqlite_error)?;
    if name.is_empty() || name.len() > 255 {
        return Err(EngineError::InvalidRecord("LayerStack name"));
    }
    let mut statement = connection
        .prepare(
            "SELECT l.layer_id, l.parent_layer_id, l.root_id, l.creation_kind,
                    l.accepted_generation, l.source_branch_id,
                    l.source_branch_depth,
                    l.source_branch_head_operation_version_id,
                    l.prepared_request_id, bd.branch_delta_id,
                    bd.base_root, bd.source_root, bd.destination_root,
                    bd.source_delta_id, sd.payload,
                    bd.applied_delta_id, ad.payload, ld.layer_delta_id,
                    (SELECT r.release_generation FROM layerfs_released_versions r
                        WHERE r.target_kind = 'layer'
                          AND r.owner_id = l.layer_stack_id
                          AND r.version_id = l.layer_id),
                    (SELECT r.request_id FROM layerfs_released_versions r
                        WHERE r.target_kind = 'layer'
                          AND r.owner_id = l.layer_stack_id
                          AND r.version_id = l.layer_id),
                    l.source_branch_generation
             FROM layerfs_layers l
             LEFT JOIN layerfs_branch_deltas bd
               ON bd.branch_delta_id = l.source_branch_delta_id
             LEFT JOIN layerfs_deltas sd
               ON sd.delta_id = bd.source_delta_id AND sd.format_version = 1
             LEFT JOIN layerfs_deltas ad
               ON ad.delta_id = bd.applied_delta_id AND ad.format_version = 1
             LEFT JOIN layerfs_layer_deltas ld
               ON ld.candidate_layer_id = l.layer_id
             WHERE l.layer_stack_id = ?1 AND l.state = 'accepted'
               AND ((?2 = 1 AND l.accepted_generation = 0)
                    OR (l.accepted_generation > ?3 AND l.accepted_generation <= ?4))
             ORDER BY l.accepted_generation",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query(params![
            layer_stack_id.as_bytes(),
            i64::from(base.is_none()),
            i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
            i64::try_from(page_generation).map_err(|_| EngineError::CounterOverflow)?,
        ])
        .map_err(map_sqlite_error)?;
    let mut layers = Vec::new();
    while let Some(row) = rows.next().map_err(map_sqlite_error)? {
        if layers.len() == MAX_HISTORY_PAGE_RECORDS + 1 {
            return Err(EngineError::InvalidRecord("Fetch LayerStack history page"));
        }
        let creation = row.get::<_, String>(3).map_err(map_sqlite_error)?;
        let merge = match creation.as_str() {
            "genesis" => None,
            "candidate" => Some(PushedLayerMerge {
                source_branch_id: BranchId(bytes32(
                    &row.get::<_, Vec<u8>>(5).map_err(map_sqlite_error)?,
                    "BranchId",
                )?),
                source_branch_depth: u64::try_from(row.get::<_, i64>(6).map_err(map_sqlite_error)?)
                    .map_err(|_| EngineError::InvalidRecord("Branch depth"))?,
                source_branch_generation: u64::try_from(
                    row.get::<_, i64>(20).map_err(map_sqlite_error)?,
                )
                .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                source_operation_version_id: OperationVersionId(bytes32(
                    &row.get::<_, Vec<u8>>(7).map_err(map_sqlite_error)?,
                    "OperationVersionId",
                )?),
                request_id: RequestId(bytes32(
                    &row.get::<_, Vec<u8>>(8).map_err(map_sqlite_error)?,
                    "RequestId",
                )?),
                branch_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(9).map_err(map_sqlite_error)?,
                    "BranchDeltaId",
                )?,
                base_root: object_id(&row.get::<_, Vec<u8>>(10).map_err(map_sqlite_error)?)?,
                source_root: object_id(&row.get::<_, Vec<u8>>(11).map_err(map_sqlite_error)?)?,
                destination_root: object_id(&row.get::<_, Vec<u8>>(12).map_err(map_sqlite_error)?)?,
                source_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(13).map_err(map_sqlite_error)?,
                    "TransitionId",
                )?,
                source_transition_payload: row.get::<_, Vec<u8>>(14).map_err(map_sqlite_error)?,
                applied_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(15).map_err(map_sqlite_error)?,
                    "TransitionId",
                )?,
                applied_transition_payload: row.get::<_, Vec<u8>>(16).map_err(map_sqlite_error)?,
                layer_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(17).map_err(map_sqlite_error)?,
                    "LayerDeltaId",
                )?,
            }),
            _ => return Err(EngineError::InvalidRecord("Layer creation kind")),
        };
        layers.push(PushedLayer {
            layer_id: LayerId(bytes32(
                &row.get::<_, Vec<u8>>(0).map_err(map_sqlite_error)?,
                "LayerId",
            )?),
            parent_layer_id: row
                .get::<_, Option<Vec<u8>>>(1)
                .map_err(map_sqlite_error)?
                .map(|id| bytes32(&id, "LayerId").map(LayerId))
                .transpose()?,
            root: object_id(&row.get::<_, Vec<u8>>(2).map_err(map_sqlite_error)?)?,
            release: pushed_release(
                row.get::<_, Option<i64>>(18).map_err(map_sqlite_error)?,
                row.get::<_, Option<Vec<u8>>>(19)
                    .map_err(map_sqlite_error)?,
            )?,
            accepted_generation: u64::try_from(row.get::<_, i64>(4).map_err(map_sqlite_error)?)
                .map_err(|_| EngineError::InvalidRecord("Layer generation"))?,
            merge,
        });
    }
    let mut statement = connection
        .prepare(
            "SELECT before_generation, after_generation, before_layer_id,
                    after_layer_id, action_kind, source_record_id, request_id
             FROM layerfs_layer_stack_transitions
             WHERE layer_stack_id = ?1 AND after_generation > ?2
               AND after_generation <= ?3 ORDER BY after_generation",
        )
        .map_err(map_sqlite_error)?;
    let transitions = statement
        .query_map(
            params![
                layer_stack_id.as_bytes(),
                i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(page_generation).map_err(|_| EngineError::CounterOverflow)?,
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                ))
            },
        )
        .map_err(map_sqlite_error)?
        .map(|row| {
            let row = row.map_err(map_sqlite_error)?;
            Ok(PushedLayerStackTransition {
                before_generation: u64::try_from(row.0)
                    .map_err(|_| EngineError::InvalidRecord("LayerStack generation"))?,
                after_generation: u64::try_from(row.1)
                    .map_err(|_| EngineError::InvalidRecord("LayerStack generation"))?,
                before_layer_id: LayerId(bytes32(&row.2, "LayerId")?),
                after_layer_id: LayerId(bytes32(&row.3, "LayerId")?),
                action: match row.4.as_str() {
                    "layer_stack_merge" => PushedLayerStackAction::Merge,
                    "layer_stack_rollback" => PushedLayerStackAction::Rollback,
                    _ => return Err(EngineError::InvalidRecord("LayerStack action")),
                },
                source_record_id: bytes32(&row.5, "LayerStack source record")?,
                request_id: RequestId(bytes32(&row.6, "RequestId")?),
            })
        })
        .collect::<EngineResult<Vec<_>>>()?;
    if base.is_none() && !layers.iter().any(|layer| layer.accepted_generation == 0)
        || transitions.len() > MAX_HISTORY_PAGE_RECORDS
        || transitions.len()
            != usize::try_from(page_generation - after_generation)
                .map_err(|_| EngineError::CounterOverflow)?
    {
        return Err(EngineError::InvalidRecord("Fetch LayerStack history"));
    }
    Ok(PushedLayerStack {
        name,
        base,
        head,
        complete: head == durable_head,
        layers,
        transitions,
    })
}

fn import_layer_stack_snapshot(
    engine: &Engine,
    connection: &Connection,
    stack: &PushedLayerStack,
    fetch_staging: Option<([u8; 32], bool)>,
) -> EngineResult<()> {
    if stack
        .base
        .is_some_and(|base| base.layer_stack_id != stack.head.layer_stack_id)
        || stack.transitions.len() > MAX_HISTORY_PAGE_RECORDS
        || stack.layers.len() > MAX_HISTORY_PAGE_RECORDS + 1
    {
        return Err(EngineError::InvalidRecord("Fetch LayerStack page"));
    }
    let published = read_layer_stack_head(connection, stack.head.layer_stack_id)?;
    let incumbent = if fetch_staging.is_some() {
        read_fetch_stack_head(connection, stack.head.layer_stack_id)?
    } else {
        published
    };
    if let (Some((durable_storage_id, false)), Some(incumbent)) = (fetch_staging, incumbent) {
        stage_fetch_stack_head(connection, durable_storage_id, published, incumbent)?;
    }
    if incumbent != stack.base {
        return Err(EngineError::InvalidRecord("Fetch LayerStack conflict"));
    }
    if let Some(incumbent) = incumbent {
        let name = connection
            .query_row(
                "SELECT name FROM layerfs_layer_stacks WHERE layer_stack_id = ?1",
                params![stack.head.layer_stack_id.as_bytes()],
                |row| row.get::<_, String>(0),
            )
            .map_err(map_sqlite_error)?;
        if name != stack.name {
            return Err(EngineError::InvalidRecord("Fetch LayerStack name"));
        }
        if stack.head.generation < incumbent.generation {
            return Err(EngineError::InvalidRecord("Fetch LayerStack generation"));
        }
    }
    for layer in &stack.layers {
        if layer.release.is_none() {
            authenticate_root(engine, connection, layer.root)?;
        }
    }
    let mut current = stack.base.map(|base| base.layer_id);
    let mut current_root = stack.base.map(|base| base.root);
    let mut generation = stack.base.map_or(0, |base| base.generation);
    if stack.base.is_none() {
        let genesis = stack
            .layers
            .iter()
            .find(|layer| {
                layer.merge.is_none()
                    && layer.parent_layer_id.is_none()
                    && layer.accepted_generation == 0
                    && layer.release.is_none()
            })
            .ok_or(EngineError::InvalidRecord("Fetch genesis Layer"))?;
        if stack
            .layers
            .iter()
            .filter(|layer| layer.merge.is_none())
            .count()
            != 1
        {
            return Err(EngineError::InvalidRecord("Fetch genesis Layer"));
        }
        connection
            .execute(
                "INSERT INTO layerfs_layer_stacks
                 (layer_stack_id, name, generation, head_layer_id)
                 VALUES (?1, ?2, 0, ?3)",
                params![
                    stack.head.layer_stack_id.as_bytes(),
                    &stack.name,
                    genesis.layer_id.as_bytes(),
                ],
            )
            .map_err(map_sqlite_error)?;
        connection
            .execute(
                "INSERT INTO layerfs_layers
                 (layer_id, layer_stack_id, parent_layer_id, root_id,
                  creation_kind, state, accepted_generation)
                 VALUES (?1, ?2, NULL, ?3, 'genesis', 'accepted', 0)",
                params![
                    genesis.layer_id.as_bytes(),
                    stack.head.layer_stack_id.as_bytes(),
                    genesis.root.as_bytes(),
                ],
            )
            .map_err(map_sqlite_error)?;
        retain_root(connection, genesis.root)?;
        current = Some(genesis.layer_id);
        current_root = Some(genesis.root);
    }
    for layer in &stack.layers {
        match &layer.merge {
            None => continue,
            Some(merge) => {
                let parent = layer
                    .parent_layer_id
                    .ok_or(EngineError::InvalidRecord("Fetch Layer parent"))?;
                if merge.source_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                    || merge.applied_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                    || layer.accepted_generation <= generation
                    || layer.accepted_generation > stack.head.generation
                    || layer.layer_id
                        != LayerId(derive_id(
                            b"candidate-layer",
                            &[
                                stack.head.layer_stack_id.as_bytes(),
                                merge.request_id.as_bytes(),
                                layer.root.as_bytes(),
                            ],
                        ))
                    || transition_identity(
                        merge.base_root,
                        merge.source_root,
                        &merge.source_transition_payload,
                    ) != merge.source_delta_id
                    || transition_identity(
                        merge.destination_root,
                        layer.root,
                        &merge.applied_transition_payload,
                    ) != merge.applied_delta_id
                    || derive_id(
                        b"layer-stack-branch-delta",
                        &[
                            merge.source_branch_id.as_bytes(),
                            merge.request_id.as_bytes(),
                            &merge.source_delta_id,
                            &merge.applied_delta_id,
                        ],
                    ) != merge.branch_delta_id
                    || derive_id(
                        b"layer-delta",
                        &[
                            parent.as_bytes(),
                            layer.layer_id.as_bytes(),
                            &merge.applied_delta_id,
                        ],
                    ) != merge.layer_delta_id
                {
                    return Err(EngineError::InvalidRecord("Fetch Layer identity"));
                }
                insert_transition(
                    connection,
                    merge.source_delta_id,
                    merge.base_root,
                    merge.source_root,
                    &merge.source_transition_payload,
                )?;
                insert_transition(
                    connection,
                    merge.applied_delta_id,
                    merge.destination_root,
                    layer.root,
                    &merge.applied_transition_payload,
                )?;
                connection
                    .execute(
                        "INSERT INTO layerfs_branch_deltas
                         (branch_delta_id, purpose, source_branch_id,
                          source_branch_generation, source_branch_operation_version_id, base_root,
                          source_root, destination_root, result_root,
                          source_delta_id, applied_delta_id)
                         VALUES (?1, 'layer_stack_merge', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        params![
                            merge.branch_delta_id.as_slice(),
                            merge.source_branch_id.as_bytes(),
                            i64::try_from(merge.source_branch_generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            merge.source_operation_version_id.as_bytes(),
                            merge.base_root.as_bytes(),
                            merge.source_root.as_bytes(),
                            merge.destination_root.as_bytes(),
                            layer.root.as_bytes(),
                            merge.source_delta_id.as_slice(),
                            merge.applied_delta_id.as_slice(),
                        ],
                    )
                    .map_err(map_sqlite_error)?;
                connection
                    .execute(
                        "INSERT INTO layerfs_layers
                         (layer_id, layer_stack_id, parent_layer_id, root_id,
                          creation_kind, source_branch_id, source_branch_depth,
                          source_branch_generation,
                          source_branch_head_operation_version_id,
                          source_branch_delta_id, state, prepared_request_id,
                          accepted_generation)
                         VALUES (?1, ?2, ?3, ?4, 'candidate', ?5, ?6, ?7, ?8, ?9,
                                 'accepted', ?10, ?11)",
                        params![
                            layer.layer_id.as_bytes(),
                            stack.head.layer_stack_id.as_bytes(),
                            parent.as_bytes(),
                            layer.root.as_bytes(),
                            merge.source_branch_id.as_bytes(),
                            i64::try_from(merge.source_branch_depth)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            i64::try_from(merge.source_branch_generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            merge.source_operation_version_id.as_bytes(),
                            merge.branch_delta_id.as_slice(),
                            merge.request_id.as_bytes(),
                            i64::try_from(layer.accepted_generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                        ],
                    )
                    .map_err(map_sqlite_error)?;
                connection
                    .execute(
                        "INSERT INTO layerfs_layer_deltas
                         (layer_delta_id, parent_layer_id, candidate_layer_id,
                          transition_delta_id, parent_root, result_root)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            merge.layer_delta_id.as_slice(),
                            parent.as_bytes(),
                            layer.layer_id.as_bytes(),
                            merge.applied_delta_id.as_slice(),
                            merge.destination_root.as_bytes(),
                            layer.root.as_bytes(),
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
        }
        match layer.release {
            Some(release) => record_pushed_release(
                connection,
                "layer",
                stack.head.layer_stack_id.as_bytes(),
                layer.layer_id.as_bytes(),
                layer.root,
                release,
            )?,
            None => retain_root(connection, layer.root)?,
        }
    }
    let mut current = current.ok_or(EngineError::InvalidRecord("Fetch LayerStack base"))?;
    let mut current_root =
        current_root.ok_or(EngineError::InvalidRecord("Fetch LayerStack base"))?;
    for transition in &stack.transitions {
        if transition.before_generation != generation
            || transition.after_generation
                != generation
                    .checked_add(1)
                    .ok_or(EngineError::CounterOverflow)?
            || transition.before_layer_id != current
        {
            return Err(EngineError::InvalidRecord(
                "Fetch LayerStack transition chain",
            ));
        }
        let (action, expected_source, receipt, release_range) = match transition.action {
            PushedLayerStackAction::Merge => {
                let layer = stack
                    .layers
                    .iter()
                    .find(|layer| layer.layer_id == transition.after_layer_id)
                    .ok_or(EngineError::InvalidRecord("Fetch LayerStack merge"))?;
                let merge = layer
                    .merge
                    .as_ref()
                    .ok_or(EngineError::InvalidRecord("Fetch LayerStack merge"))?;
                if layer.parent_layer_id != Some(current)
                    || layer.accepted_generation != transition.after_generation
                    || merge.destination_root != current_root
                {
                    return Err(EngineError::InvalidRecord("Fetch LayerStack merge chain"));
                }
                (
                    "layer_stack_merge",
                    merge.branch_delta_id,
                    derive_id(
                        b"layer-stack-merge-receipt",
                        &[
                            transition.request_id.as_bytes(),
                            transition.after_layer_id.as_bytes(),
                        ],
                    ),
                    None,
                )
            }
            PushedLayerStackAction::Rollback => {
                if read_layer_root(
                    connection,
                    stack.head.layer_stack_id,
                    transition.after_layer_id,
                )?
                .is_none()
                {
                    return Err(EngineError::InvalidRecord(
                        "Fetch LayerStack rollback target",
                    ));
                }
                let target_generation = connection
                    .query_row(
                        "SELECT accepted_generation FROM layerfs_layers
                         WHERE layer_stack_id = ?1 AND layer_id = ?2",
                        params![
                            stack.head.layer_stack_id.as_bytes(),
                            transition.after_layer_id.as_bytes(),
                        ],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(map_sqlite_error)?;
                let current_generation = connection
                    .query_row(
                        "SELECT accepted_generation FROM layerfs_layers
                         WHERE layer_stack_id = ?1 AND layer_id = ?2",
                        params![
                            stack.head.layer_stack_id.as_bytes(),
                            transition.before_layer_id.as_bytes(),
                        ],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(map_sqlite_error)?;
                (
                    "layer_stack_rollback",
                    transition.after_layer_id.0,
                    derive_id(
                        b"layer-stack-rollback-receipt",
                        &[
                            transition.request_id.as_bytes(),
                            transition.after_layer_id.as_bytes(),
                        ],
                    ),
                    Some((target_generation, current_generation)),
                )
            }
        };
        if transition.source_record_id != expected_source {
            return Err(EngineError::InvalidRecord("Fetch LayerStack source record"));
        }
        connection
            .execute(
                "INSERT INTO layerfs_layer_stack_transitions
                 (transition_id, layer_stack_id, before_generation,
                  after_generation, before_layer_id, after_layer_id,
                  action_kind, source_record_id, request_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    receipt.as_slice(),
                    stack.head.layer_stack_id.as_bytes(),
                    i64::try_from(transition.before_generation)
                        .map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(transition.after_generation)
                        .map_err(|_| EngineError::CounterOverflow)?,
                    transition.before_layer_id.as_bytes(),
                    transition.after_layer_id.as_bytes(),
                    action,
                    transition.source_record_id.as_slice(),
                    transition.request_id.as_bytes(),
                ],
            )
            .map_err(map_sqlite_error)?;
        if let Some((target_generation, current_generation)) = release_range {
            record_layer_suffix_release(
                connection,
                stack.head.layer_stack_id,
                target_generation,
                current_generation,
                transition.after_generation,
                transition.request_id,
            )?;
        }
        current = transition.after_layer_id;
        current_root = read_historical_layer_root(connection, stack.head.layer_stack_id, current)?
            .ok_or(EngineError::InvalidRecord(
                "Fetch LayerStack transition Layer",
            ))?;
        generation = transition.after_generation;
    }
    if generation != stack.head.generation
        || current != stack.head.layer_id
        || current_root != stack.head.root
    {
        return Err(EngineError::InvalidRecord("Fetch LayerStack head"));
    }
    let changed = connection
        .execute(
            "UPDATE layerfs_layer_stacks SET generation = ?1, head_layer_id = ?2
             WHERE layer_stack_id = ?3 AND generation = ?4 AND head_layer_id = ?5",
            params![
                i64::try_from(stack.head.generation).map_err(|_| EngineError::CounterOverflow)?,
                stack.head.layer_id.as_bytes(),
                stack.head.layer_stack_id.as_bytes(),
                i64::try_from(stack.base.map_or(0, |base| base.generation))
                    .map_err(|_| EngineError::CounterOverflow)?,
                stack
                    .base
                    .map_or_else(
                        || stack
                            .layers
                            .iter()
                            .find(|layer| layer.accepted_generation == 0)
                            .map(|layer| layer.layer_id),
                        |base| Some(base.layer_id),
                    )
                    .ok_or(EngineError::InvalidRecord("Fetch LayerStack base"))?
                    .as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    if changed != 1 {
        return Err(EngineError::PublicationConflict);
    }
    if let Some((durable_storage_id, complete)) = fetch_staging {
        if complete {
            finish_fetch_target(
                connection,
                "layer_stack",
                stack.head.layer_stack_id.as_bytes(),
            )?;
        } else {
            stage_fetch_stack_head(connection, durable_storage_id, published, stack.head)?;
        }
    }
    Ok(())
}

fn collect_fetch_branch_roots(
    bundle: &BranchPushBundle,
    roots: &mut std::collections::BTreeSet<(BranchId, ObjectId)>,
) -> EngineResult<()> {
    fn collect(
        bundle: &BranchPushBundle,
        roots: &mut std::collections::BTreeSet<(BranchId, ObjectId)>,
        branches: &mut std::collections::BTreeSet<BranchId>,
        remaining: &mut usize,
    ) -> EngineResult<()> {
        if !branches.insert(bundle.head.branch_id) {
            return Err(EngineError::InvalidRecord("Fetch duplicate Branch bundle"));
        }
        let records = 1_usize
            .checked_add(bundle.operations.len())
            .and_then(|count| count.checked_add(bundle.child_merges.len()))
            .and_then(|count| count.checked_add(bundle.rollbacks.len()))
            .and_then(|count| count.checked_add(bundle.origin_stack.layers.len()))
            .and_then(|count| count.checked_add(bundle.origin_stack.transitions.len()))
            .ok_or(EngineError::CounterOverflow)?;
        *remaining = remaining
            .checked_sub(records)
            .ok_or(EngineError::InvalidRecord("Fetch history page required"))?;
        roots.insert((bundle.head.branch_id, bundle.ancestry.fork_root));
        let head_released = bundle.head.operation_version_id.is_some_and(|head| {
            bundle.operations.iter().any(|operation| {
                operation.operation_version_id == head && operation.release.is_some()
            }) || bundle
                .child_merges
                .iter()
                .any(|merge| merge.operation_version_id == head && merge.release.is_some())
        });
        if !head_released {
            roots.insert((bundle.head.branch_id, bundle.head.root));
        }
        roots.extend(
            bundle
                .operations
                .iter()
                .filter(|operation| operation.release.is_none())
                .map(|operation| (bundle.head.branch_id, operation.root)),
        );
        roots.extend(
            bundle
                .child_merges
                .iter()
                .filter(|merge| merge.release.is_none())
                .map(|merge| (bundle.head.branch_id, merge.root)),
        );
        for dependency in &bundle.dependencies {
            collect(dependency, roots, branches, remaining)?;
        }
        Ok(())
    }

    let mut remaining = MAX_PUSH_OPERATION_RECORDS;
    collect(
        bundle,
        roots,
        &mut std::collections::BTreeSet::new(),
        &mut remaining,
    )
}

fn verify_staged_child_merges(
    engine: &Engine,
    transfer_id: RequestId,
    branch_id: BranchId,
) -> EngineResult<()> {
    let maximum = engine
        .lock_connection()?
        .query_row(
            "SELECT MAX(page_sequence) FROM layerfs_branch_push_pages
             WHERE transfer_id = ?1 AND branch_id = ?2",
            params![transfer_id.as_bytes(), branch_id.as_bytes()],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map_err(map_sqlite_error)?
        .ok_or(EngineError::InvalidRecord("Push pages"))?;
    for sequence in 0..=maximum {
        let encoded = engine
            .lock_connection()?
            .query_row(
                "SELECT bundle FROM layerfs_branch_push_pages
                 WHERE transfer_id = ?1 AND branch_id = ?2 AND page_sequence = ?3",
                params![transfer_id.as_bytes(), branch_id.as_bytes(), sequence],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(map_sqlite_error)?;
        let bundle: BranchPushBundle = serde_json::from_slice(&encoded)
            .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
        for merge in &bundle.child_merges {
            let mut writer = engine.begin_candidate_write()?;
            let merged = layerfs_core::logical::merge_roots(
                &mut writer,
                merge.base_root,
                merge.source_root,
                merge.destination_root,
            )?
            .map_err(|_| EngineError::InvalidRecord("Push child merge conflict"))?;
            if merged.root() != merge.root {
                return Err(EngineError::InvalidRecord("Push child merge result"));
            }
            writer.commit_objects()?;
        }
    }
    Ok(())
}

fn validate_staged_push_page(bundle: &BranchPushBundle) -> EngineResult<()> {
    let history_len = bundle
        .operations
        .len()
        .checked_add(bundle.child_merges.len())
        .and_then(|count| count.checked_add(bundle.rollbacks.len()))
        .ok_or(EngineError::CounterOverflow)?;
    let base_generation = bundle.base.map_or(0, |head| head.generation);
    if history_len > MAX_HISTORY_PAGE_RECORDS
        || !bundle.dependencies.is_empty()
        || bundle.head.branch_id
            != bundle
                .base
                .map_or(bundle.head.branch_id, |head| head.branch_id)
        || bundle.head.generation < base_generation
        || usize::try_from(bundle.head.generation - base_generation)
            .map_err(|_| EngineError::CounterOverflow)?
            != history_len
        || bundle.origin_stack.head.layer_stack_id != bundle.ancestry.origin_layer_stack_id
        || bundle.origin_stack.base != Some(bundle.origin_stack.head)
        || !bundle.origin_stack.complete
        || !bundle.origin_stack.layers.is_empty()
        || !bundle.origin_stack.transitions.is_empty()
        || bundle.origin_stack.name.is_empty()
        || bundle.origin_stack.name.len() > 255
        || bundle
            .name
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.len() > 255)
        || bundle
            .operations
            .iter()
            .any(|operation| operation.transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES)
        || bundle.child_merges.iter().any(|merge| {
            merge.source_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                || merge.applied_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
        })
    {
        return Err(EngineError::InvalidRecord("Push page"));
    }
    Ok(())
}

fn validate_push_ancestry(connection: &Connection, bundle: &BranchPushBundle) -> EngineResult<()> {
    if read_layer_stack_head(connection, bundle.ancestry.origin_layer_stack_id)?.is_none() {
        return Err(EngineError::InvalidRecord("Push origin LayerStack"));
    }
    let origin_root = read_layer_root(
        connection,
        bundle.ancestry.origin_layer_stack_id,
        bundle.ancestry.origin_layer_id,
    )?
    .ok_or(EngineError::InvalidRecord("Push origin Layer"))?;
    match (
        bundle.ancestry.immediate_parent_branch_id,
        bundle.ancestry.fork_operation_id,
        bundle.ancestry.fork_operation_version_id,
    ) {
        (None, None, None)
            if bundle.ancestry.depth == 0 && bundle.ancestry.fork_root == origin_root =>
        {
            Ok(())
        }
        (Some(parent), Some(operation), Some(version)) if bundle.ancestry.depth > 0 => {
            let parent_ancestry = read_branch_ancestry(connection, parent)?
                .ok_or(EngineError::InvalidRecord("Push parent Branch"))?;
            let fork = connection
                .query_row(
                    "SELECT root_id, created_by_kind, created_by_operation_id
                     FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2",
                    params![parent.as_bytes(), version.as_bytes()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Push child origin"))?;
            if object_id(&fork.0)? != bundle.ancestry.fork_root
                || fork.1 != "operation"
                || fork.2.as_deref() != Some(operation.as_bytes())
                || parent_ancestry.origin_layer_stack_id != bundle.ancestry.origin_layer_stack_id
                || parent_ancestry.origin_layer_id != bundle.ancestry.origin_layer_id
                || parent_ancestry.depth.checked_add(1) != Some(bundle.ancestry.depth)
            {
                return Err(EngineError::InvalidRecord("Push child ancestry"));
            }
            Ok(())
        }
        _ => Err(EngineError::InvalidRecord("Push Branch ancestry")),
    }
}

fn insert_branch_base(connection: &Connection, bundle: &BranchPushBundle) -> EngineResult<()> {
    connection
        .execute(
            "INSERT INTO layerfs_branches
             (branch_id, name, immediate_parent_branch_id, fork_operation_id,
              fork_operation_version_id, fork_root_id, origin_layer_stack_id,
              origin_layer_id, depth, generation, head_operation_version_id, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, NULL, 'active')",
            params![
                bundle.head.branch_id.as_bytes(),
                bundle.name.as_deref(),
                bundle
                    .ancestry
                    .immediate_parent_branch_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle
                    .ancestry
                    .fork_operation_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle
                    .ancestry
                    .fork_operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle.ancestry.fork_root.as_bytes(),
                bundle.ancestry.origin_layer_stack_id.as_bytes(),
                bundle.ancestry.origin_layer_id.as_bytes(),
                i64::try_from(bundle.ancestry.depth).map_err(|_| EngineError::CounterOverflow)?,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn insert_branch_origin_lease(
    connection: &Connection,
    bundle: &BranchPushBundle,
) -> EngineResult<()> {
    let lease_id = derive_id(
        if bundle.ancestry.depth == 0 {
            b"top-level-branch-origin-lease"
        } else {
            b"child-branch-origin-lease"
        },
        &[
            bundle.head.branch_id.as_bytes(),
            bundle
                .ancestry
                .fork_operation_version_id
                .map(|id| id.0)
                .unwrap_or(bundle.ancestry.origin_layer_id.0)
                .as_slice(),
        ],
    );
    let (target_kind, target_id) = match bundle.ancestry.fork_operation_version_id {
        Some(version) => ("operation_version", version.0),
        None => ("layer", bundle.ancestry.origin_layer_id.0),
    };
    connection
        .execute(
            "INSERT INTO layerfs_version_leases
             (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
             VALUES (?1, ?2, ?3, 'branch', ?4, ?5)",
            params![
                lease_id.as_slice(),
                target_kind,
                target_id.as_slice(),
                bundle.head.branch_id.as_bytes(),
                unix_seconds()?,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn insert_branch_snapshot(connection: &Connection, bundle: &BranchPushBundle) -> EngineResult<()> {
    connection
        .execute(
            "INSERT INTO layerfs_branches
             (branch_id, name, immediate_parent_branch_id, fork_operation_id,
              fork_operation_version_id, fork_root_id, origin_layer_stack_id,
              origin_layer_id, depth, generation, head_operation_version_id, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'active')",
            params![
                bundle.head.branch_id.as_bytes(),
                bundle.name.as_deref(),
                bundle
                    .ancestry
                    .immediate_parent_branch_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle
                    .ancestry
                    .fork_operation_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle
                    .ancestry
                    .fork_operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle.ancestry.fork_root.as_bytes(),
                bundle.ancestry.origin_layer_stack_id.as_bytes(),
                bundle.ancestry.origin_layer_id.as_bytes(),
                i64::try_from(bundle.ancestry.depth).map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(bundle.head.generation).map_err(|_| EngineError::CounterOverflow)?,
                bundle
                    .head
                    .operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

#[derive(Clone, Copy)]
struct FetchAncestryProof {
    operation_id: OperationId,
    root: ObjectId,
    ancestry: BranchAncestry,
}

fn collect_fetch_ancestry_proofs(
    bundle: &BranchPushBundle,
    proofs: &mut std::collections::BTreeMap<(BranchId, OperationVersionId), FetchAncestryProof>,
) -> EngineResult<()> {
    for operation in &bundle.operations {
        let proof = FetchAncestryProof {
            operation_id: operation.operation_id,
            root: operation.root,
            ancestry: bundle.ancestry,
        };
        if proofs
            .insert(
                (bundle.head.branch_id, operation.operation_version_id),
                proof,
            )
            .is_some()
        {
            return Err(EngineError::InvalidRecord("Fetch ancestry proof conflict"));
        }
    }
    if proofs.len() > MAX_PUSH_OPERATION_RECORDS {
        return Err(EngineError::InvalidRecord("Fetch ancestry proof page"));
    }
    for dependency in &bundle.dependencies {
        collect_fetch_ancestry_proofs(dependency, proofs)?;
    }
    Ok(())
}

fn import_fetch_dependency(
    engine: &Engine,
    connection: &Connection,
    bundle: &BranchPushBundle,
    source_roots: &std::collections::BTreeSet<(BranchId, ObjectId)>,
    ancestry_proofs: &std::collections::BTreeMap<
        (BranchId, OperationVersionId),
        FetchAncestryProof,
    >,
) -> EngineResult<()> {
    import_layer_stack_snapshot(engine, connection, &bundle.origin_stack, None)?;
    for dependency in &bundle.dependencies {
        import_fetch_dependency(
            engine,
            connection,
            dependency,
            source_roots,
            ancestry_proofs,
        )?;
    }
    if let Some(incumbent) = read_branch_head(connection, bundle.head.branch_id)? {
        let retained = incumbent == bundle.head
            || if bundle.head.generation == 0 && bundle.head.operation_version_id.is_none() {
                bundle.head.root == bundle.ancestry.fork_root
            } else {
                branch_contains_exact_version(connection, bundle.head)?
            };
        if retained
            && read_branch_ancestry(connection, bundle.head.branch_id)? == Some(bundle.ancestry)
        {
            return Ok(());
        }
        return Err(EngineError::InvalidRecord(
            "Fetch dependency Branch conflict",
        ));
    }
    let origin_root = read_layer_root(
        connection,
        bundle.ancestry.origin_layer_stack_id,
        bundle.ancestry.origin_layer_id,
    )?
    .ok_or(EngineError::InvalidRecord("Fetch dependency origin Layer"))?;
    match (
        bundle.ancestry.immediate_parent_branch_id,
        bundle.ancestry.fork_operation_id,
        bundle.ancestry.fork_operation_version_id,
    ) {
        (None, None, None)
            if bundle.ancestry.depth == 0 && bundle.ancestry.fork_root == origin_root => {}
        (Some(parent), Some(operation), Some(version)) if bundle.ancestry.depth > 0 => {
            let parent_ancestry = read_branch_ancestry(connection, parent)?
                .ok_or(EngineError::InvalidRecord("Fetch dependency parent Branch"))?;
            let fork = connection
                .query_row(
                    "SELECT root_id, created_by_kind, created_by_operation_id
                     FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2",
                    params![parent.as_bytes(), version.as_bytes()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?;
            let exact_fork = match fork {
                Some(fork) => {
                    object_id(&fork.0)? == bundle.ancestry.fork_root
                        && fork.1 == "operation"
                        && fork.2.as_deref() == Some(operation.as_bytes())
                }
                None => ancestry_proofs
                    .get(&(parent, version))
                    .is_some_and(|proof| {
                        proof.operation_id == operation
                            && proof.root == bundle.ancestry.fork_root
                            && proof.ancestry == parent_ancestry
                    }),
            };
            if !exact_fork
                || parent_ancestry.origin_layer_stack_id != bundle.ancestry.origin_layer_stack_id
                || parent_ancestry.origin_layer_id != bundle.ancestry.origin_layer_id
                || parent_ancestry.depth.checked_add(1) != Some(bundle.ancestry.depth)
            {
                return Err(EngineError::InvalidRecord(
                    "Fetch dependency child ancestry",
                ));
            }
            authenticate_root(engine, connection, bundle.ancestry.fork_root)?;
        }
        _ => return Err(EngineError::InvalidRecord("Fetch dependency ancestry")),
    }
    insert_branch_snapshot(connection, bundle)?;
    let mut history = Vec::new();
    history.extend(
        bundle
            .operations
            .iter()
            .enumerate()
            .map(|(index, record)| (record.after_generation, 0_u8, index)),
    );
    history.extend(
        bundle
            .child_merges
            .iter()
            .enumerate()
            .map(|(index, record)| (record.after_generation, 1_u8, index)),
    );
    history.extend(
        bundle
            .rollbacks
            .iter()
            .enumerate()
            .map(|(index, record)| (record.after_generation, 2_u8, index)),
    );
    history.sort_unstable();
    let mut prior_version = None;
    let mut prior_root = bundle.ancestry.fork_root;
    let mut prior_generation = 0;
    for (_, kind, index) in history {
        let next = match kind {
            0 => insert_pushed_operation(
                engine,
                connection,
                bundle,
                &bundle.operations[index],
                prior_version,
                prior_root,
                prior_generation,
            )?,
            1 => insert_pushed_child_merge(
                engine,
                connection,
                bundle.head.branch_id,
                &bundle.child_merges[index],
                prior_version,
                prior_root,
                prior_generation,
                Some(source_roots),
            )?,
            2 => insert_pushed_branch_rollback(
                connection,
                bundle.head.branch_id,
                &bundle.rollbacks[index],
                prior_version,
                prior_generation,
            )?,
            _ => unreachable!(),
        };
        prior_version = Some(next.0);
        prior_root = next.1;
        prior_generation = next.2;
    }
    if prior_version != bundle.head.operation_version_id
        || prior_root != bundle.head.root
        || prior_generation != bundle.head.generation
    {
        return Err(EngineError::InvalidRecord("Fetch dependency history head"));
    }
    let lease_id = derive_id(
        if bundle.ancestry.depth == 0 {
            b"top-level-branch-origin-lease"
        } else {
            b"child-branch-origin-lease"
        },
        &[
            bundle.head.branch_id.as_bytes(),
            bundle
                .ancestry
                .fork_operation_version_id
                .map(|id| id.0)
                .unwrap_or(bundle.ancestry.origin_layer_id.0)
                .as_slice(),
        ],
    );
    connection
        .execute(
            "INSERT INTO layerfs_version_leases
             (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
             VALUES (?1, ?2, ?3, 'branch', ?4, ?5)",
            params![
                lease_id.as_slice(),
                if bundle.ancestry.depth == 0 {
                    "layer"
                } else {
                    "operation_version"
                },
                bundle
                    .ancestry
                    .fork_operation_version_id
                    .map(|id| id.0)
                    .unwrap_or(bundle.ancestry.origin_layer_id.0)
                    .as_slice(),
                bundle.head.branch_id.as_bytes(),
                unix_seconds()?,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_pushed_child_merge(
    engine: &Engine,
    connection: &Connection,
    branch_id: BranchId,
    merge: &PushedChildMerge,
    prior_version: Option<OperationVersionId>,
    prior_root: ObjectId,
    prior_generation: u64,
    _fetch_source_roots: Option<&std::collections::BTreeSet<(BranchId, ObjectId)>>,
) -> EngineResult<(OperationVersionId, ObjectId, u64)> {
    let source = read_branch_ancestry(connection, merge.source_branch_id)?
        .ok_or(EngineError::InvalidRecord("Fetch merge source Branch"))?;
    let source_root_retained = connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM layerfs_operation_versions v
                 JOIN layerfs_branch_transitions t
                   ON t.branch_id = v.branch_id
                  AND t.after_operation_version_id = v.operation_version_id
                 WHERE v.branch_id = ?1 AND v.operation_version_id = ?2
                   AND v.root_id = ?3 AND t.after_generation = ?4)",
            params![
                merge.source_branch_id.as_bytes(),
                merge.source_operation_version_id.as_bytes(),
                merge.source_root.as_bytes(),
                i64::try_from(merge.source_branch_generation)
                    .map_err(|_| EngineError::CounterOverflow)?,
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)?;
    let expected_version = OperationVersionId(derive_id(
        b"child-merge-operation-version",
        &[
            branch_id.as_bytes(),
            merge.request_id.as_bytes(),
            merge.root.as_bytes(),
        ],
    ));
    if merge.parent_operation_version_id != prior_version
        || merge.destination_root != prior_root
        || merge.before_generation != prior_generation
        || merge.after_generation
            != prior_generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?
        || merge.operation_version_id != expected_version
        || source.fork_root != merge.base_root
        || !source_root_retained
    {
        return Err(EngineError::InvalidRecord("Push child merge chain"));
    }
    if merge.release.is_none() {
        authenticate_root(engine, connection, merge.root)?;
    }
    if transition_identity(
        merge.base_root,
        merge.source_root,
        &merge.source_transition_payload,
    ) != merge.source_delta_id
        || transition_identity(
            merge.destination_root,
            merge.root,
            &merge.applied_transition_payload,
        ) != merge.applied_delta_id
        || derive_id(
            b"child-branch-delta",
            &[
                merge.source_branch_id.as_bytes(),
                merge.request_id.as_bytes(),
                &merge.source_delta_id,
                &merge.applied_delta_id,
            ],
        ) != merge.branch_delta_id
    {
        return Err(EngineError::InvalidRecord("Push child merge identity"));
    }
    insert_transition(
        connection,
        merge.source_delta_id,
        merge.base_root,
        merge.source_root,
        &merge.source_transition_payload,
    )?;
    insert_transition(
        connection,
        merge.applied_delta_id,
        merge.destination_root,
        merge.root,
        &merge.applied_transition_payload,
    )?;
    connection
        .execute(
            "INSERT INTO layerfs_branch_deltas
             (branch_delta_id, purpose, source_branch_id,
              source_branch_generation, source_branch_operation_version_id, base_root,
              source_root, destination_root, result_root,
              source_delta_id, applied_delta_id)
             VALUES (?1, 'child_merge', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                merge.branch_delta_id.as_slice(),
                merge.source_branch_id.as_bytes(),
                i64::try_from(merge.source_branch_generation)
                    .map_err(|_| EngineError::CounterOverflow)?,
                merge.source_operation_version_id.as_bytes(),
                merge.base_root.as_bytes(),
                merge.source_root.as_bytes(),
                merge.destination_root.as_bytes(),
                merge.root.as_bytes(),
                merge.source_delta_id.as_slice(),
                merge.applied_delta_id.as_slice(),
            ],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "INSERT INTO layerfs_operation_versions
             (operation_version_id, branch_id, sequence,
              parent_operation_version_id, root_id, created_by_kind,
              created_by_child_branch_id, created_by_branch_delta_id)
             VALUES (?1, ?2, ?3, ?4, ?5, 'child_merge', ?6, ?7)",
            params![
                merge.operation_version_id.as_bytes(),
                branch_id.as_bytes(),
                i64::try_from(merge.version_sequence).map_err(|_| EngineError::CounterOverflow)?,
                merge
                    .parent_operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                merge.root.as_bytes(),
                merge.source_branch_id.as_bytes(),
                merge.branch_delta_id.as_slice(),
            ],
        )
        .map_err(map_sqlite_error)?;
    let receipt = derive_id(
        b"child-branch-merge-receipt",
        &[
            merge.request_id.as_bytes(),
            merge.operation_version_id.as_bytes(),
        ],
    );
    connection
        .execute(
            "INSERT INTO layerfs_branch_transitions
             (transition_id, branch_id, before_generation, after_generation,
              before_operation_version_id, after_operation_version_id,
              action_kind, source_record_id, request_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'child_branch_merge', ?7, ?8)",
            params![
                receipt.as_slice(),
                branch_id.as_bytes(),
                i64::try_from(merge.before_generation).map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(merge.after_generation).map_err(|_| EngineError::CounterOverflow)?,
                merge
                    .parent_operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                merge.operation_version_id.as_bytes(),
                merge.branch_delta_id.as_slice(),
                merge.request_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    match merge.release {
        Some(release) => record_pushed_release(
            connection,
            "operation_version",
            branch_id.as_bytes(),
            merge.operation_version_id.as_bytes(),
            merge.root,
            release,
        )?,
        None => retain_root(connection, merge.root)?,
    }
    Ok((
        merge.operation_version_id,
        merge.root,
        merge.after_generation,
    ))
}

fn insert_pushed_branch_rollback(
    connection: &Connection,
    branch_id: BranchId,
    rollback: &PushedBranchRollback,
    prior_version: Option<OperationVersionId>,
    prior_generation: u64,
) -> EngineResult<(OperationVersionId, ObjectId, u64)> {
    if prior_version != Some(rollback.before_operation_version_id)
        || rollback.before_generation != prior_generation
        || rollback.after_generation
            != prior_generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?
    {
        return Err(EngineError::InvalidRecord("Push Branch rollback chain"));
    }
    let (target_sequence, target_root) = connection
        .query_row(
            "SELECT sequence, root_id FROM layerfs_operation_versions
             WHERE branch_id = ?1 AND operation_version_id = ?2",
            params![
                branch_id.as_bytes(),
                rollback.target_operation_version_id.as_bytes()
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or(EngineError::InvalidRecord("Push Branch rollback target"))?;
    if object_id(&target_root)? != rollback.target_root {
        return Err(EngineError::InvalidRecord("Push Branch rollback root"));
    }
    let current_sequence = connection
        .query_row(
            "SELECT sequence FROM layerfs_operation_versions
             WHERE branch_id = ?1 AND operation_version_id = ?2",
            params![
                branch_id.as_bytes(),
                rollback.before_operation_version_id.as_bytes()
            ],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_sqlite_error)?;
    let receipt = derive_id(
        b"branch-rollback-receipt",
        &[
            rollback.request_id.as_bytes(),
            rollback.target_operation_version_id.as_bytes(),
        ],
    );
    connection
        .execute(
            "INSERT INTO layerfs_branch_transitions
             (transition_id, branch_id, before_generation, after_generation,
              before_operation_version_id, after_operation_version_id,
              action_kind, source_record_id, request_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'branch_rollback', ?6, ?7)",
            params![
                receipt.as_slice(),
                branch_id.as_bytes(),
                i64::try_from(rollback.before_generation)
                    .map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(rollback.after_generation)
                    .map_err(|_| EngineError::CounterOverflow)?,
                rollback.before_operation_version_id.as_bytes(),
                rollback.target_operation_version_id.as_bytes(),
                rollback.request_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    record_branch_suffix_release(
        connection,
        branch_id,
        target_sequence,
        current_sequence,
        rollback.after_generation,
        rollback.request_id,
    )?;
    Ok((
        rollback.target_operation_version_id,
        rollback.target_root,
        rollback.after_generation,
    ))
}

fn retain_root(connection: &Connection, root: ObjectId) -> EngineResult<()> {
    connection
        .execute(
            "INSERT INTO layerfs_retained_roots (root_id) VALUES (?1)
             ON CONFLICT(root_id) DO NOTHING",
            params![root.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn record_pushed_release(
    connection: &Connection,
    target_kind: &'static str,
    owner_id: &[u8; 32],
    version_id: &[u8; 32],
    root: ObjectId,
    release: PushedRelease,
) -> EngineResult<()> {
    if !matches!(target_kind, "layer" | "operation_version") || release.generation == 0 {
        return Err(EngineError::InvalidRecord("release record"));
    }
    connection
        .execute(
            "INSERT INTO layerfs_released_versions
             (target_kind, owner_id, version_id, root_id, release_generation, request_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                target_kind,
                owner_id,
                version_id,
                root.as_bytes(),
                i64::try_from(release.generation).map_err(|_| EngineError::CounterOverflow)?,
                release.request_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    release_retained_root_if_unreferenced(connection, root.as_bytes())
}

fn branch_contains_exact_version(connection: &Connection, head: BranchHead) -> EngineResult<bool> {
    let Some(version) = head.operation_version_id else {
        return Ok(false);
    };
    connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM layerfs_operation_versions v
                 JOIN layerfs_branch_transitions t
                   ON t.branch_id = v.branch_id
                  AND t.after_operation_version_id = v.operation_version_id
                 WHERE v.branch_id = ?1 AND v.operation_version_id = ?2
                   AND v.root_id = ?3 AND t.after_generation = ?4
                   AND NOT EXISTS(
                       SELECT 1 FROM layerfs_released_versions r
                       WHERE r.target_kind = 'operation_version'
                         AND r.owner_id = v.branch_id
                         AND r.version_id = v.operation_version_id))",
            params![
                head.branch_id.as_bytes(),
                version.as_bytes(),
                head.root.as_bytes(),
                i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?,
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)
}

fn branch_contains_exact_historical_version(
    connection: &Connection,
    head: BranchHead,
) -> EngineResult<bool> {
    let Some(version) = head.operation_version_id else {
        return Ok(false);
    };
    connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM layerfs_operation_versions v
                 JOIN layerfs_branch_transitions t
                   ON t.branch_id = v.branch_id
                  AND t.after_operation_version_id = v.operation_version_id
                 WHERE v.branch_id = ?1 AND v.operation_version_id = ?2
                   AND v.root_id = ?3 AND t.after_generation = ?4)",
            params![
                head.branch_id.as_bytes(),
                version.as_bytes(),
                head.root.as_bytes(),
                i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?,
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)
}

fn commit_verified_fetch(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    request: VerifiedFetchRequest,
    head: BranchHead,
    stack_head: LayerStackHead,
) -> EngineResult<bool> {
    if engine.fetch_boundary_failure.swap(false, Ordering::AcqRel) {
        return Err(EngineError::InvalidRecord(
            "injected Fetch history/tracking boundary failure",
        ));
    }
    connection
        .execute(
            "DELETE FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = 'fetch'",
            params![request.request_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "DELETE FROM layerfs_transfer_state
             WHERE request_id = ?1 AND direction = 'fetch'",
            params![request.request_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    let incumbent = insert_verified_fetch_rows(engine, connection, request, head, stack_head)?;
    let reconciled = commit_product_request(
        engine,
        connection,
        "layerfs_sync_receipts",
        request.request_id,
    )?;
    Ok(incumbent || reconciled)
}

fn commit_partial_fetch(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    request: VerifiedFetchRequest,
    head: BranchHead,
) -> EngineResult<bool> {
    connection
        .execute(
            "DELETE FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = 'fetch'",
            params![request.request_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "DELETE FROM layerfs_transfer_state
             WHERE request_id = ?1 AND direction = 'fetch'",
            params![request.request_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    commit_product_state(
        engine,
        connection,
        "SELECT EXISTS(SELECT 1 FROM layerfs_fetch_staging_heads
         WHERE target_kind = 'branch' AND target_id = ?1)",
        head.branch_id.as_bytes(),
    )
}

fn finish_fetch_target(
    connection: &Connection,
    target_kind: &str,
    target_id: &[u8; 32],
) -> EngineResult<()> {
    connection
        .execute(
            "DELETE FROM layerfs_fetch_staging_heads
             WHERE target_kind = ?1 AND target_id = ?2",
            params![target_kind, target_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn finish_fetch_staging(connection: &Connection, branch_id: BranchId) -> EngineResult<()> {
    finish_fetch_target(connection, "branch", branch_id.as_bytes())
}

fn insert_verified_fetch_rows(
    engine: &Engine,
    connection: &Connection,
    request: VerifiedFetchRequest,
    head: BranchHead,
    stack_head: LayerStackHead,
) -> EngineResult<bool> {
    if read_branch_head(connection, head.branch_id)? != Some(head) {
        return Err(EngineError::PublicationConflict);
    }
    authenticate_root(engine, connection, head.root)?;
    connection
        .execute(
            "INSERT INTO layerfs_durable_storages
             (durable_storage_id, authenticated_at) VALUES (?1, ?2)
             ON CONFLICT(durable_storage_id) DO UPDATE
             SET authenticated_at = excluded.authenticated_at",
            params![request.durable_storage_id.as_slice(), unix_seconds()?],
        )
        .map_err(map_sqlite_error)?;
    let incumbent = connection
        .query_row(
            "SELECT durable_storage_id, direction, candidate_kind, candidate_id,
                    expected_head_id, expected_generation, result,
                    unique_bytes, resumed_bytes, retransmitted_bytes,
                    accepted_head_id, accepted_generation, accepted_root_id
             FROM layerfs_sync_receipts WHERE request_id = ?1",
            params![request.request_id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<Vec<u8>>>(10)?,
                    row.get::<_, Option<i64>>(11)?,
                    row.get::<_, Option<Vec<u8>>>(12)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let expected_version = head
        .operation_version_id
        .map(|id| id.as_bytes().as_slice().to_vec());
    if let Some(incumbent) = &incumbent {
        if incumbent.0.as_slice() != request.durable_storage_id
            || incumbent.1 != "fetch"
            || incumbent.2 != "branch"
            || incumbent.3.as_slice() != head.branch_id.as_bytes()
            || incumbent.4 != expected_version
            || incumbent.5
                != Some(i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?)
            || incumbent.6 != "fetched"
            || u64::try_from(incumbent.7).ok() != Some(request.counters.unique_bytes)
            || u64::try_from(incumbent.8).ok() != Some(request.counters.resumed_bytes)
            || u64::try_from(incumbent.9).ok() != Some(request.counters.retransmitted_bytes)
            || incumbent.10 != expected_version
            || incumbent.11
                != Some(i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?)
            || incumbent.12.as_deref() != Some(head.root.as_bytes())
        {
            return Err(EngineError::InvalidRecord(
                "Fetch request identity conflict",
            ));
        }
    } else {
        connection
            .execute(
                "INSERT INTO layerfs_sync_receipts
                 (request_id, durable_storage_id, direction, candidate_kind,
                  candidate_id, expected_head_id, expected_generation, result,
                  accepted_head_id, accepted_generation, accepted_root_id,
                  unique_bytes, resumed_bytes, retransmitted_bytes,
                  reconciliation_result)
                 VALUES (?1, ?2, 'fetch', 'branch', ?3, ?4, ?5, 'fetched',
                         ?4, ?5, ?6, ?7, ?8, ?9, 'verified_complete')",
                params![
                    request.request_id.as_bytes(),
                    request.durable_storage_id.as_slice(),
                    head.branch_id.as_bytes(),
                    expected_version,
                    i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?,
                    head.root.as_bytes(),
                    i64::try_from(request.counters.unique_bytes)
                        .map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(request.counters.resumed_bytes)
                        .map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(request.counters.retransmitted_bytes)
                        .map_err(|_| EngineError::CounterOverflow)?,
                ],
            )
            .map_err(map_sqlite_error)?;
    }
    insert_verified_tracking_ref(
        connection,
        request,
        "branch",
        head.branch_id.as_bytes(),
        head.operation_version_id,
        head.generation,
        head.root,
    )?;
    if read_layer_stack_head(connection, stack_head.layer_stack_id)? != Some(stack_head) {
        return Err(EngineError::PublicationConflict);
    }
    authenticate_root(engine, connection, stack_head.root)?;
    insert_verified_tracking_ref(
        connection,
        request,
        "layer",
        stack_head.layer_id.as_bytes(),
        None,
        stack_head.generation,
        stack_head.root,
    )?;
    Ok(incumbent.is_some())
}

fn insert_verified_tracking_ref(
    connection: &Connection,
    request: VerifiedFetchRequest,
    target_kind: &str,
    target_id: &[u8; 32],
    target_version_id: Option<OperationVersionId>,
    generation: u64,
    root: ObjectId,
) -> EngineResult<()> {
    if !matches!(target_kind, "branch" | "layer") {
        return Err(EngineError::InvalidRecord("DurableTrackingRef kind"));
    }
    let version_bytes = target_version_id.map_or([0; 32], |id| id.0);
    let tracking_ref_id = derive_id(
        b"durable-tracking-ref",
        &[
            request.durable_storage_id.as_slice(),
            target_kind.as_bytes(),
            target_id,
            &version_bytes,
            &generation.to_be_bytes(),
            root.as_bytes(),
        ],
    );
    let tracked = connection
        .query_row(
            "SELECT tracking_ref_id, target_version_id, root_id,
                    verification_receipt_id, status
             FROM layerfs_durable_tracking_refs
             WHERE durable_storage_id = ?1 AND target_kind = ?2
               AND target_id = ?3 AND generation = ?4",
            params![
                request.durable_storage_id.as_slice(),
                target_kind,
                target_id,
                i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    match tracked {
        Some((id, stored_version, stored_root, _receipt, status))
            if id.as_slice() == tracking_ref_id
                && stored_version
                    == target_version_id.map(|id| id.as_bytes().as_slice().to_vec())
                && stored_root.as_slice() == root.as_bytes()
                && status == "verified_complete" => {}
        Some(_) => return Err(EngineError::InvalidRecord("DurableTrackingRef conflict")),
        None => {
            connection
                .execute(
                    "INSERT INTO layerfs_durable_tracking_refs
                     (tracking_ref_id, durable_storage_id, target_kind, target_id,
                      target_version_id, generation, root_id, verification_receipt_id, status)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                             'verified_complete')",
                    params![
                        tracking_ref_id.as_slice(),
                        request.durable_storage_id.as_slice(),
                        target_kind,
                        target_id,
                        target_version_id.map(|id| id.as_bytes().as_slice().to_vec()),
                        i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
                        root.as_bytes(),
                        request.request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
    }
    Ok(())
}

fn read_push_receipt(
    connection: &Connection,
    request: BranchPushRequest,
    bundle: &BranchPushBundle,
) -> EngineResult<Option<BranchPushOutcome>> {
    let incumbent = connection
        .query_row(
            "SELECT durable_storage_id, direction, candidate_kind, candidate_id,
                    expected_head_id, expected_generation, result,
                    unique_bytes, resumed_bytes, retransmitted_bytes,
                    accepted_head_id, accepted_generation, accepted_root_id
             FROM layerfs_sync_receipts WHERE request_id = ?1",
            params![request.request_id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<Vec<u8>>>(10)?,
                    row.get::<_, Option<i64>>(11)?,
                    row.get::<_, Option<Vec<u8>>>(12)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some(incumbent) = incumbent else {
        return Ok(None);
    };
    let expected_version = request
        .expected
        .and_then(|head| head.operation_version_id)
        .map(|id| id.as_bytes().as_slice().to_vec());
    let expected_generation = request
        .expected
        .map(|head| i64::try_from(head.generation))
        .transpose()
        .map_err(|_| EngineError::CounterOverflow)?;
    if incumbent.0.as_slice() != connection_store_id(connection)?.as_slice()
        || incumbent.1 != "push"
        || incumbent.2 != "branch"
        || incumbent.3.as_slice() != bundle.head.branch_id.as_bytes()
        || incumbent.4 != expected_version
        || incumbent.5 != expected_generation
        || u64::try_from(incumbent.7).ok() != Some(request.counters.unique_bytes)
        || u64::try_from(incumbent.8).ok() != Some(request.counters.resumed_bytes)
        || u64::try_from(incumbent.9).ok() != Some(request.counters.retransmitted_bytes)
        || incumbent.6 == "durably_accepted"
            && (incumbent.10
                != bundle
                    .head
                    .operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec())
                || incumbent.11
                    != Some(
                        i64::try_from(bundle.head.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                    )
                || incumbent.12.as_deref() != Some(bundle.head.root.as_bytes()))
    {
        return Err(EngineError::InvalidRecord("Push request identity conflict"));
    }
    let actual = read_branch_head(connection, bundle.head.branch_id)?;
    let retained = if bundle.head.generation == 0 && bundle.head.operation_version_id.is_none() {
        read_branch_ancestry(connection, bundle.head.branch_id)?
            .is_some_and(|ancestry| ancestry.fork_root == bundle.head.root)
    } else {
        branch_contains_exact_version(connection, bundle.head)?
    };
    match incumbent.6.as_str() {
        "durably_accepted" if retained => Ok(Some(BranchPushOutcome::DurablyAccepted {
            head: bundle.head,
            reconciled: true,
        })),
        "conflict" => Ok(Some(BranchPushOutcome::Conflict { actual })),
        _ => Err(EngineError::InvalidRecord("Push receipt result")),
    }
}

fn read_exact_push_receipt(
    connection: &Connection,
    request: BranchPushRequest,
    branch_id: BranchId,
) -> EngineResult<Option<BranchPushOutcome>> {
    let row = connection
        .query_row(
            "SELECT durable_storage_id, direction, candidate_kind, candidate_id,
                    expected_head_id, expected_generation, result,
                    unique_bytes, resumed_bytes, retransmitted_bytes,
                    accepted_head_id, accepted_generation, accepted_root_id
             FROM layerfs_sync_receipts WHERE request_id = ?1",
            params![request.request_id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<Vec<u8>>>(10)?,
                    row.get::<_, Option<i64>>(11)?,
                    row.get::<_, Option<Vec<u8>>>(12)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let expected_version = request
        .expected
        .and_then(|head| head.operation_version_id)
        .map(|id| id.as_bytes().as_slice().to_vec());
    let expected_generation = request
        .expected
        .map(|head| i64::try_from(head.generation))
        .transpose()
        .map_err(|_| EngineError::CounterOverflow)?;
    if row.0.as_slice() != connection_store_id(connection)?.as_slice()
        || row.1 != "push"
        || row.2 != "branch"
        || row.3.as_slice() != branch_id.as_bytes()
        || row.4 != expected_version
        || row.5 != expected_generation
        || u64::try_from(row.7).ok() != Some(request.counters.unique_bytes)
        || u64::try_from(row.8).ok() != Some(request.counters.resumed_bytes)
        || u64::try_from(row.9).ok() != Some(request.counters.retransmitted_bytes)
    {
        return Err(EngineError::InvalidRecord("Push request identity conflict"));
    }
    match row.6.as_str() {
        "durably_accepted" => {
            let generation = row
                .11
                .ok_or(EngineError::InvalidRecord("Push accepted generation"))?;
            let root = row
                .12
                .ok_or(EngineError::InvalidRecord("Push accepted root"))?;
            let head = BranchHead {
                branch_id,
                generation: u64::try_from(generation)
                    .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                operation_version_id: row
                    .10
                    .map(|id| bytes32(&id, "OperationVersionId").map(OperationVersionId))
                    .transpose()?,
                root: object_id(&root)?,
            };
            if !branch_contains_exact_historical_version(connection, head)? {
                return Err(EngineError::InvalidRecord("Push accepted head"));
            }
            Ok(Some(BranchPushOutcome::DurablyAccepted {
                head,
                reconciled: true,
            }))
        }
        "conflict" => Ok(Some(BranchPushOutcome::Conflict {
            actual: read_branch_head(connection, branch_id)?,
        })),
        _ => Err(EngineError::InvalidRecord("Push receipt result")),
    }
}

fn insert_push_receipt(
    engine: &Engine,
    connection: &Connection,
    request: BranchPushRequest,
    bundle: &BranchPushBundle,
    result: &str,
) -> EngineResult<()> {
    if !matches!(result, "durably_accepted" | "conflict") {
        return Err(EngineError::InvalidRecord("Push result"));
    }
    connection
        .execute(
            "INSERT INTO layerfs_durable_storages
             (durable_storage_id, authenticated_at) VALUES (?1, ?2)
             ON CONFLICT(durable_storage_id) DO UPDATE
             SET authenticated_at = excluded.authenticated_at",
            params![engine.store_id_cached().as_slice(), unix_seconds()?],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "INSERT INTO layerfs_sync_receipts
             (request_id, durable_storage_id, direction, candidate_kind,
              candidate_id, expected_head_id, expected_generation, result,
              accepted_head_id, accepted_generation, accepted_root_id,
              unique_bytes, resumed_bytes, retransmitted_bytes,
              reconciliation_result)
             VALUES (?1, ?2, 'push', 'branch', ?3, ?4, ?5, ?6,
                     ?7, ?8, ?9, ?10, ?11, ?12, 'exact')",
            params![
                request.request_id.as_bytes(),
                engine.store_id_cached().as_slice(),
                bundle.head.branch_id.as_bytes(),
                request
                    .expected
                    .and_then(|head| head.operation_version_id)
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                request
                    .expected
                    .map(|head| i64::try_from(head.generation))
                    .transpose()
                    .map_err(|_| EngineError::CounterOverflow)?,
                result,
                if result == "durably_accepted" {
                    bundle
                        .head
                        .operation_version_id
                        .map(|id| id.as_bytes().as_slice().to_vec())
                } else {
                    None
                },
                if result == "durably_accepted" {
                    Some(
                        i64::try_from(bundle.head.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                    )
                } else {
                    None
                },
                if result == "durably_accepted" {
                    Some(bundle.head.root.as_bytes().as_slice().to_vec())
                } else {
                    None
                },
                i64::try_from(request.counters.unique_bytes)
                    .map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(request.counters.resumed_bytes)
                    .map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(request.counters.retransmitted_bytes)
                    .map_err(|_| EngineError::CounterOverflow)?,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn connection_store_id(connection: &Connection) -> EngineResult<[u8; 32]> {
    connection
        .query_row(
            "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(map_sqlite_error)
        .and_then(|bytes| bytes32(&bytes, "StorageId"))
}

fn release_staged_push_pins(connection: &Connection, transfer_id: RequestId) -> EngineResult<()> {
    connection
        .execute(
            "DELETE FROM layerfs_sync_object_pins
             WHERE direction = 'push' AND owner_request_id = ?1",
            params![transfer_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn delete_sync_custody(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    owner_request_id: RequestId,
    direction: &str,
) -> EngineResult<u64> {
    let pins = connection
        .execute(
            "DELETE FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = ?2",
            params![owner_request_id.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let batches = connection
        .execute(
            "DELETE FROM layerfs_sync_batch_receipts
             WHERE owner_request_id = ?1 AND direction = ?2",
            params![owner_request_id.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let pages = if direction == "push" {
        connection
            .execute(
                "DELETE FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
                params![owner_request_id.as_bytes()],
            )
            .map_err(map_sqlite_error)?
    } else {
        0
    };
    let progress = connection
        .execute(
            "DELETE FROM layerfs_transfer_state
             WHERE owner_request_id = ?1 AND direction = ?2",
            params![owner_request_id.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let sql = if direction == "push" {
        "SELECT NOT EXISTS(SELECT 1 FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = 'push')
         AND NOT EXISTS(SELECT 1 FROM layerfs_sync_batch_receipts
             WHERE owner_request_id = ?1 AND direction = 'push')
         AND NOT EXISTS(SELECT 1 FROM layerfs_branch_push_pages WHERE transfer_id = ?1)"
    } else {
        "SELECT NOT EXISTS(SELECT 1 FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = 'fetch')
         AND NOT EXISTS(SELECT 1 FROM layerfs_sync_batch_receipts
             WHERE owner_request_id = ?1 AND direction = 'fetch')"
    };
    commit_product_state(engine, connection, sql, owner_request_id.as_bytes())?;
    u64::try_from(pins)
        .ok()
        .and_then(|pins| {
            u64::try_from(batches)
                .ok()
                .and_then(|batches| pins.checked_add(batches))
        })
        .and_then(|rows| {
            u64::try_from(pages)
                .ok()
                .and_then(|pages| rows.checked_add(pages))
        })
        .and_then(|rows| {
            u64::try_from(progress)
                .ok()
                .and_then(|progress| rows.checked_add(progress))
        })
        .ok_or(EngineError::CounterOverflow)
}

pub(crate) fn verify_product_integrity(connection: &Connection) -> EngineResult<u64> {
    let mut statements = 0_u64;
    let foreign_key_failure = {
        statements = statements
            .checked_add(1)
            .ok_or(EngineError::CounterOverflow)?;
        let mut statement = connection
            .prepare("PRAGMA foreign_key_check")
            .map_err(map_sqlite_error)?;
        let mut rows = statement.query([]).map_err(map_sqlite_error)?;
        rows.next().map_err(map_sqlite_error)?.is_some()
    };
    if foreign_key_failure {
        return Err(EngineError::InvalidRecord("product foreign key integrity"));
    }
    let checks = [
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_branches b
            LEFT JOIN layerfs_branches p ON p.branch_id = b.immediate_parent_branch_id
            LEFT JOIN layerfs_operation_versions v
              ON v.branch_id = b.immediate_parent_branch_id
             AND v.operation_version_id = b.fork_operation_version_id
            WHERE (b.depth = 0 AND NOT EXISTS(
                       SELECT 1 FROM layerfs_layers l
                       WHERE l.layer_stack_id = b.origin_layer_stack_id
                         AND l.layer_id = b.origin_layer_id
                         AND l.root_id = b.fork_root_id))
               OR (b.depth > 0 AND (
                    p.branch_id IS NULL OR p.depth + 1 != b.depth
                    OR p.origin_layer_stack_id != b.origin_layer_stack_id
                    OR p.origin_layer_id != b.origin_layer_id
                    OR v.root_id != b.fork_root_id
                    OR v.created_by_kind != 'operation'
                    OR v.created_by_operation_id != b.fork_operation_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_branches b
            WHERE (b.generation = 0 AND b.head_operation_version_id IS NOT NULL)
               OR (b.generation > 0 AND NOT EXISTS(
                    SELECT 1 FROM layerfs_branch_transitions t
                    WHERE t.branch_id = b.branch_id
                      AND t.after_generation = b.generation
                      AND t.after_operation_version_id = b.head_operation_version_id))
               OR (SELECT COUNT(*) FROM layerfs_branch_transitions t
                   WHERE t.branch_id = b.branch_id) != b.generation
               OR (SELECT COUNT(DISTINCT after_generation)
                   FROM layerfs_branch_transitions t
                   WHERE t.branch_id = b.branch_id) != b.generation)",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_branch_transitions t
            LEFT JOIN layerfs_branch_transitions p
              ON p.branch_id = t.branch_id
             AND p.after_generation = t.before_generation
            WHERE (t.before_generation = 0 AND t.before_operation_version_id IS NOT NULL)
               OR (t.before_generation > 0 AND (
                    p.transition_id IS NULL
                    OR p.after_operation_version_id IS NOT t.before_operation_version_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_layer_stacks s
            WHERE NOT EXISTS(
                    SELECT 1 FROM layerfs_layers g
                    WHERE g.layer_stack_id = s.layer_stack_id
                      AND g.creation_kind = 'genesis'
                      AND g.accepted_generation = 0)
               OR (s.generation = 0 AND NOT EXISTS(
                    SELECT 1 FROM layerfs_layers g
                    WHERE g.layer_stack_id = s.layer_stack_id
                      AND g.layer_id = s.head_layer_id
                      AND g.creation_kind = 'genesis'))
               OR (s.generation > 0 AND NOT EXISTS(
                    SELECT 1 FROM layerfs_layer_stack_transitions t
                    WHERE t.layer_stack_id = s.layer_stack_id
                      AND t.after_generation = s.generation
                      AND t.after_layer_id = s.head_layer_id))
               OR (SELECT COUNT(*) FROM layerfs_layer_stack_transitions t
                   WHERE t.layer_stack_id = s.layer_stack_id) != s.generation
               OR (SELECT COUNT(DISTINCT after_generation)
                   FROM layerfs_layer_stack_transitions t
                   WHERE t.layer_stack_id = s.layer_stack_id) != s.generation)",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_layer_stack_transitions t
            LEFT JOIN layerfs_layer_stack_transitions p
              ON p.layer_stack_id = t.layer_stack_id
             AND p.after_generation = t.before_generation
            LEFT JOIN layerfs_layers g
              ON g.layer_stack_id = t.layer_stack_id
             AND g.creation_kind = 'genesis'
            WHERE (t.before_generation = 0 AND t.before_layer_id != g.layer_id)
               OR (t.before_generation > 0 AND (
                    p.transition_id IS NULL OR p.after_layer_id != t.before_layer_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_layers l
            LEFT JOIN layerfs_branches b ON b.branch_id = l.source_branch_id
            LEFT JOIN layerfs_branch_transitions t
              ON t.branch_id = l.source_branch_id
             AND t.after_generation = l.source_branch_generation
             AND t.after_operation_version_id = l.source_branch_head_operation_version_id
            WHERE l.creation_kind = 'candidate' AND (
                b.branch_id IS NULL OR b.depth != l.source_branch_depth
                OR b.origin_layer_stack_id != l.layer_stack_id
                OR t.transition_id IS NULL))
            OR EXISTS(
                SELECT 1 FROM layerfs_branch_deltas d
                LEFT JOIN layerfs_branch_transitions t
                  ON t.branch_id = d.source_branch_id
                 AND t.after_generation = d.source_branch_generation
                 AND t.after_operation_version_id = d.source_branch_operation_version_id
                LEFT JOIN layerfs_operation_versions v
                  ON v.branch_id = d.source_branch_id
                 AND v.operation_version_id = d.source_branch_operation_version_id
                WHERE t.transition_id IS NULL OR v.root_id != d.source_root)",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_version_leases x
            WHERE (x.target_kind = 'layer' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_layers l WHERE l.layer_id = x.target_id))
               OR (x.target_kind = 'operation_version' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_operation_versions v
                    WHERE v.operation_version_id = x.target_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_released_versions r
            LEFT JOIN layerfs_operation_versions v
              ON r.target_kind = 'operation_version'
             AND v.branch_id = r.owner_id AND v.operation_version_id = r.version_id
            LEFT JOIN layerfs_branch_transitions t
              ON t.branch_id = r.owner_id AND t.request_id = r.request_id
             AND t.action_kind = 'branch_rollback'
            LEFT JOIN layerfs_operation_versions target
              ON target.branch_id = t.branch_id
             AND target.operation_version_id = t.after_operation_version_id
            LEFT JOIN layerfs_operation_versions before_v
              ON before_v.branch_id = t.branch_id
             AND before_v.operation_version_id = t.before_operation_version_id
            LEFT JOIN layerfs_branches b ON b.branch_id = r.owner_id
            WHERE r.target_kind = 'operation_version' AND (
                v.operation_version_id IS NULL OR v.root_id != r.root_id
                OR (t.transition_id IS NULL AND NOT EXISTS(
                    SELECT 1 FROM layerfs_fetch_staging_heads f
                    WHERE f.target_kind = 'branch' AND f.target_id = r.owner_id
                      AND r.release_generation > f.staged_generation))
                OR t.after_generation != r.release_generation
                OR v.sequence <= target.sequence OR v.sequence > before_v.sequence
                OR (b.head_operation_version_id = r.version_id AND NOT EXISTS(
                    SELECT 1 FROM layerfs_fetch_staging_heads f
                    WHERE f.target_kind = 'branch' AND f.target_id = b.branch_id))
                OR EXISTS(SELECT 1 FROM layerfs_version_leases x
                    WHERE x.target_kind = 'operation_version'
                      AND x.target_id = r.version_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_released_versions r
            LEFT JOIN layerfs_layers l
              ON r.target_kind = 'layer'
             AND l.layer_stack_id = r.owner_id AND l.layer_id = r.version_id
            LEFT JOIN layerfs_layer_stack_transitions t
              ON t.layer_stack_id = r.owner_id AND t.request_id = r.request_id
             AND t.action_kind = 'layer_stack_rollback'
            LEFT JOIN layerfs_layers target
              ON target.layer_stack_id = t.layer_stack_id
             AND target.layer_id = t.after_layer_id
            LEFT JOIN layerfs_layers before_l
              ON before_l.layer_stack_id = t.layer_stack_id
             AND before_l.layer_id = t.before_layer_id
            LEFT JOIN layerfs_layer_stacks s ON s.layer_stack_id = r.owner_id
            WHERE r.target_kind = 'layer' AND (
                l.layer_id IS NULL OR l.root_id != r.root_id
                OR (t.transition_id IS NULL AND NOT EXISTS(
                    SELECT 1 FROM layerfs_fetch_staging_heads f
                    WHERE f.target_kind = 'layer_stack' AND f.target_id = r.owner_id
                      AND r.release_generation > f.staged_generation))
                OR t.after_generation != r.release_generation
                OR l.accepted_generation <= target.accepted_generation
                OR l.accepted_generation > before_l.accepted_generation
                OR (s.head_layer_id = r.version_id AND NOT EXISTS(
                    SELECT 1 FROM layerfs_fetch_staging_heads f
                    WHERE f.target_kind = 'layer_stack'
                      AND f.target_id = s.layer_stack_id))
                OR EXISTS(SELECT 1 FROM layerfs_version_leases x
                    WHERE x.target_kind = 'layer' AND x.target_id = r.version_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_durable_tracking_refs r
            WHERE (r.target_kind = 'branch' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_branches b
                    WHERE b.branch_id = r.target_id AND (
                        (r.generation = 0 AND b.fork_root_id = r.root_id)
                        OR EXISTS(
                            SELECT 1 FROM layerfs_branch_transitions t
                            JOIN layerfs_operation_versions v
                              ON v.branch_id = t.branch_id
                             AND v.operation_version_id = t.after_operation_version_id
                            WHERE t.branch_id = b.branch_id
                              AND t.after_generation = r.generation
                              AND t.after_operation_version_id = r.target_version_id
                              AND v.root_id = r.root_id))))
               OR (r.target_kind = 'layer' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_layers l
                    WHERE l.layer_id = r.target_id AND l.root_id = r.root_id))
               OR (r.target_kind = 'operation_version' AND NOT EXISTS(
                    SELECT 1 FROM layerfs_operation_versions v
                    WHERE v.operation_version_id = r.target_id
                      AND v.root_id = r.root_id)))",
        "SELECT EXISTS(
            SELECT 1 FROM layerfs_push_outbox o
            LEFT JOIN layerfs_branches b ON b.branch_id = o.branch_id
            WHERE (o.accepted_generation = 0 AND (
                       o.operation_version_id IS NOT NULL
                       OR b.fork_root_id != o.accepted_root_id))
               OR (o.accepted_generation > 0 AND NOT EXISTS(
                    SELECT 1 FROM layerfs_branch_transitions t
                    JOIN layerfs_operation_versions v
                      ON v.branch_id = t.branch_id
                     AND v.operation_version_id = t.after_operation_version_id
                    WHERE t.branch_id = o.branch_id
                      AND t.after_generation = o.accepted_generation
                      AND t.after_operation_version_id = o.operation_version_id
                      AND v.root_id = o.accepted_root_id)))",
    ];
    for (index, sql) in checks.into_iter().enumerate() {
        statements = statements
            .checked_add(1)
            .ok_or(EngineError::CounterOverflow)?;
        let invalid = connection
            .query_row(sql, [], |row| row.get::<_, bool>(0))
            .map_err(map_sqlite_error)?;
        if invalid {
            return Err(EngineError::InvalidRecord(match index {
                0 => "product Branch ancestry",
                1 => "product Branch head",
                2 => "product Branch transition chain",
                3 => "product LayerStack head",
                4 => "product LayerStack transition chain",
                5 => "product Layer source ancestry",
                6 => "product lease target",
                7 => "product released OperationVersion",
                8 => "product released Layer",
                9 => "product tracking target",
                _ => "product outbox target",
            }));
        }
    }
    Ok(statements)
}

fn release_operation_lease(connection: &Connection, operation_id: OperationId) -> EngineResult<()> {
    connection
        .execute(
            "DELETE FROM layerfs_version_leases
             WHERE owner_kind = 'operation_workspace' AND owner_id = ?1",
            params![operation_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn replay_layer_stack_request(
    connection: &Connection,
    request_id: RequestId,
    action: &'static str,
    expected: LayerStackHead,
    after_layer_id: LayerId,
) -> EngineResult<Option<LayerStackHead>> {
    let receipt = connection
        .query_row(
            "SELECT t.layer_stack_id, t.before_generation, t.after_generation,
                    t.before_layer_id, t.after_layer_id, t.action_kind, l.root_id
             FROM layerfs_layer_stack_transitions t
             JOIN layerfs_layers l
               ON l.layer_stack_id = t.layer_stack_id
              AND l.layer_id = t.after_layer_id
             WHERE t.request_id = ?1",
            params![request_id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.0.as_slice() != expected.layer_stack_id.as_bytes()
        || u64::try_from(receipt.1).ok() != Some(expected.generation)
        || receipt.3.as_slice() != expected.layer_id.as_bytes()
        || receipt.4.as_slice() != after_layer_id.as_bytes()
        || receipt.5 != action
    {
        return Err(EngineError::InvalidRecord(
            "LayerStack request identity conflict",
        ));
    }
    Ok(Some(LayerStackHead {
        layer_stack_id: expected.layer_stack_id,
        generation: u64::try_from(receipt.2)
            .map_err(|_| EngineError::InvalidRecord("LayerStack generation"))?,
        layer_id: after_layer_id,
        root: object_id(&receipt.6)?,
    }))
}

pub(crate) fn release_retained_root_if_unreferenced(
    connection: &Connection,
    root: &[u8],
) -> EngineResult<()> {
    release_unreferenced_retained_roots(connection, Some(root))
}

fn release_unreferenced_retained_roots(
    connection: &Connection,
    root: Option<&[u8]>,
) -> EngineResult<()> {
    connection
        .execute(
            "DELETE FROM layerfs_retained_roots
             WHERE (?1 IS NULL OR root_id = ?1) AND NOT EXISTS (
                 SELECT 1 FROM (
                     SELECT root_id AS referenced_root FROM layerfs_refs
                     UNION ALL SELECT root_id FROM layerfs_roots
                     UNION ALL SELECT l.root_id FROM layerfs_layers l
                         WHERE l.state != 'dropped' AND NOT EXISTS(
                             SELECT 1 FROM layerfs_released_versions r
                             WHERE r.target_kind = 'layer'
                               AND r.owner_id = l.layer_stack_id
                               AND r.version_id = l.layer_id)
                     UNION ALL SELECT fork_root_id FROM layerfs_branches WHERE state = 'active'
                     UNION ALL SELECT v.root_id FROM layerfs_operation_versions v
                         WHERE NOT EXISTS(
                             SELECT 1 FROM layerfs_released_versions r
                             WHERE r.target_kind = 'operation_version'
                               AND r.owner_id = v.branch_id
                               AND r.version_id = v.operation_version_id)
                     UNION ALL SELECT candidate_root_id FROM layerfs_operations
                         WHERE candidate_root_id IS NOT NULL
                           AND state IN ('running', 'candidate', 'preserved', 'indeterminate')
                     UNION ALL SELECT published_root_id FROM layerfs_fetch_staging_heads
                         WHERE published_root_id IS NOT NULL
                     UNION ALL SELECT root_id FROM layerfs_durable_tracking_refs
                 ) WHERE referenced_root = layerfs_retained_roots.root_id
             )",
            params![root],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn begin_product_transaction(engine: &Engine) -> EngineResult<ConnectionGuard<'_>> {
    let mut connection = engine.lock_write_connection()?;
    if !connection.transaction {
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(map_sqlite_error)?;
        connection.transaction = true;
        engine.bump(|counters| checked_add(&mut counters.transactions_started, 1))?;
    }
    Ok(connection)
}

pub(crate) fn commit_product_state(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    reconciliation_sql: &str,
    key: &[u8],
) -> EngineResult<bool> {
    match engine.commit_dispatch.commit(connection) {
        Ok(()) => {
            connection.transaction = false;
            engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
            Ok(false)
        }
        Err(error) => {
            let _ = engine.note_sqlite_error(&error);
            let _ = engine.commit_dispatch.rollback(connection);
            connection.transaction = false;
            connection.guard.take();
            let fresh = Connection::open_with_flags(
                &engine.path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(map_sqlite_error)?;
            preflight_schema(&fresh)?;
            if fresh
                .query_row(reconciliation_sql, params![key], |row| {
                    row.get::<_, bool>(0)
                })
                .map_err(map_sqlite_error)?
            {
                restore_product_primary(engine, connection)?;
                engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
                Ok(true)
            } else {
                Err(EngineError::AmbiguousDurability)
            }
        }
    }
}

pub(crate) fn commit_product_state_pair(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    reconciliation_sql: &str,
    first: &[u8],
    second: &[u8],
) -> EngineResult<bool> {
    match engine.commit_dispatch.commit(connection) {
        Ok(()) => {
            connection.transaction = false;
            engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
            Ok(false)
        }
        Err(error) => {
            let _ = engine.note_sqlite_error(&error);
            let _ = engine.commit_dispatch.rollback(connection);
            connection.transaction = false;
            connection.guard.take();
            let fresh = Connection::open_with_flags(
                &engine.path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(map_sqlite_error)?;
            preflight_schema(&fresh)?;
            if fresh
                .query_row(reconciliation_sql, params![first, second], |row| {
                    row.get::<_, bool>(0)
                })
                .map_err(map_sqlite_error)?
            {
                restore_product_primary(engine, connection)?;
                engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
                Ok(true)
            } else {
                Err(EngineError::AmbiguousDurability)
            }
        }
    }
}

fn commit_product_request(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    table: &str,
    request_id: RequestId,
) -> EngineResult<bool> {
    match engine.commit_dispatch.commit(connection) {
        Ok(()) => {
            connection.transaction = false;
            engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
            Ok(false)
        }
        Err(error) => {
            let _ = engine.note_sqlite_error(&error);
            let _ = engine.commit_dispatch.rollback(connection);
            connection.transaction = false;
            connection.guard.take();
            let fresh = Connection::open_with_flags(
                &engine.path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(map_sqlite_error)?;
            preflight_schema(&fresh)?;
            let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE request_id = ?1)");
            if fresh
                .query_row(&sql, params![request_id.as_bytes()], |row| {
                    row.get::<_, bool>(0)
                })
                .map_err(map_sqlite_error)?
            {
                restore_product_primary(engine, connection)?;
                engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
                Ok(true)
            } else {
                Err(EngineError::AmbiguousDurability)
            }
        }
    }
}

fn restore_product_primary(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
) -> EngineResult<()> {
    let reopened = Connection::open_with_flags(&engine.path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(map_sqlite_error)?;
    reopened
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(map_sqlite_error)?;
    preflight_schema(&reopened)?;
    let mut statements = 0;
    let profile = configure_profile_counted(&reopened, &mut statements)?;
    if profile != engine.profile {
        return Err(EngineError::ProfileMismatch);
    }
    if admitted_store_id_counted(&reopened, &mut statements)? != engine.store_id {
        return Err(EngineError::InvalidRecord("reconciliation StorageId"));
    }
    *connection.guard = Some(reopened);
    Ok(())
}

fn rollback_product_transaction<T>(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    _result: &EngineResult<T>,
) {
    if connection.transaction {
        if engine.commit_dispatch.rollback(connection).is_ok() {
            let _ = engine.bump(|counters| checked_add(&mut counters.transactions_rolled_back, 1));
        }
        connection.transaction = false;
    }
}

fn transition_identity(parent: ObjectId, child: ObjectId, payload: &[u8]) -> [u8; 32] {
    derive_id(
        b"root-transition-v1",
        &[parent.as_bytes(), child.as_bytes(), payload],
    )
}

pub fn derive_id(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs-product-id-v1\0");
    hasher.update(&(domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update(&(part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

fn bytes32(bytes: &[u8], field: &'static str) -> EngineResult<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|_| EngineError::InvalidRecord(field))
}

fn object_id(bytes: &[u8]) -> EngineResult<ObjectId> {
    ObjectId::from_bytes(bytes).map_err(EngineError::Core)
}

pub(crate) fn unix_seconds() -> EngineResult<i64> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| EngineError::InvalidRecord("system clock"))?
        .as_secs();
    i64::try_from(seconds).map_err(|_| EngineError::CounterOverflow)
}
