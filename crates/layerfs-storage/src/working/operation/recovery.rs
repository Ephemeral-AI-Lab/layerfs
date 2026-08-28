//! Recoverable private Operation state.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_state, commit_product_state_pair,
    rollback_product_transaction, Engine,
};
use crate::full::record_id::{bytes32, object_id, BranchId, OperationId, OperationVersionId};
use crate::working::compaction::reachability::release_retained_root_if_unreferenced;
use crate::working::lease::release_operation_lease;
use crate::working::operation::record::{RecoverableOperation, RecoverableOperationState};
use layerfs_core::ObjectId;
use rusqlite::{params, OptionalExtension};

impl Engine {
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
}
