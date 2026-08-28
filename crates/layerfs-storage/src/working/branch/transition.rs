//! Private Branch drop and rollback transitions.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{read_branch_head, BranchHead, BranchRollbackOutcome};
use crate::full::compaction::reachability::record_branch_suffix_release;
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_request, commit_product_state,
    rollback_product_transaction, Engine,
};
use crate::full::record_id::{derive_id, object_id, BranchId, OperationVersionId, RequestId};
use crate::working::compaction::reachability::release_unreferenced_retained_roots;
use rusqlite::{params, OptionalExtension};

impl Engine {
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
                    "SELECT t.branch_id, t.before_generation,
                            t.before_operation_version_id,
                            COALESCE(before_version.root_id, b.fork_root_id),
                            t.after_generation, t.after_operation_version_id,
                            after_version.root_id, t.action_kind, t.source_record_id
                     FROM layerfs_branch_transitions t
                     JOIN layerfs_branches b ON b.branch_id = t.branch_id
                     LEFT JOIN layerfs_operation_versions before_version
                       ON before_version.branch_id = t.branch_id
                      AND before_version.operation_version_id = t.before_operation_version_id
                     JOIN layerfs_operation_versions after_version
                       ON after_version.branch_id = t.branch_id
                      AND after_version.operation_version_id = t.after_operation_version_id
                     WHERE t.request_id = ?1",
                    params![request_id.as_bytes()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                            row.get::<_, Vec<u8>>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, Option<Vec<u8>>>(5)?,
                            row.get::<_, Vec<u8>>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, Vec<u8>>(8)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?;
            if let Some(prior) = prior {
                let generation = u64::try_from(prior.4)
                    .map_err(|_| EngineError::InvalidRecord("Branch generation"))?;
                if prior.0.as_slice() != expected.branch_id.as_bytes()
                    || u64::try_from(prior.1).ok() != Some(expected.generation)
                    || prior.2
                        != expected
                            .operation_version_id
                            .map(|version| version.as_bytes().to_vec())
                    || object_id(&prior.3)? != expected.root
                    || generation
                        != expected
                            .generation
                            .checked_add(1)
                            .ok_or(EngineError::CounterOverflow)?
                    || prior.5.as_deref() != Some(target.as_bytes())
                    || prior.7 != "branch_rollback"
                    || prior.8.as_slice() != target.as_bytes()
                {
                    return Err(EngineError::InvalidRecord(
                        "BranchRollback request identity conflict",
                    ));
                }
                return Ok(BranchRollbackOutcome::WorkingRecorded {
                    head: BranchHead {
                        branch_id: expected.branch_id,
                        generation,
                        operation_version_id: Some(target),
                        root: object_id(&prior.6)?,
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
}
