//! Private Operation candidate commit transaction.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{read_branch_head, BranchHead};
use crate::full::branch::transition::insert_transition;
use crate::full::closure::membership::{authenticate_root, authenticate_root_shallow};
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_request, commit_product_state,
    rollback_product_transaction, Engine,
};
use crate::full::record_id::{derive_id, transition_identity, OperationVersionId};
use crate::full::transfer::batch::MAX_TRANSITION_PAYLOAD_BYTES;
use crate::working::lease::release_operation_lease;
use crate::working::operation::record::{
    load_operation, next_operation_version_sequence, replay_operation_commit, OperationCandidate,
    OperationCommitOutcome, OperationRecordRef, PreservedOperationCandidate,
};
use rusqlite::params;

impl Engine {
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
}
