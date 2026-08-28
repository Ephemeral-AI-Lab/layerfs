//! Bounded Branch Push page assembly.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::export::pushed_release;
use crate::full::branch::read::{
    branch_contains_exact_historical_version, branch_head_at_generation, read_branch_ancestry,
    read_branch_head, BranchHead, VersionRef,
};
use crate::full::layer_stack::read::{export_layer_stack_snapshot, LayerStackHead};
use crate::full::legacy_store::Engine;
use crate::full::record_id::{
    bytes32, object_id, BranchId, LayerId, LayerStackId, OperationId, OperationVersionId, RequestId,
};
use crate::full::transfer::batch::{
    BranchPushBundle, PushedBranchRollback, PushedChildMerge, PushedOperation,
    MAX_HISTORY_PAGE_RECORDS, MAX_PUSH_OPERATION_RECORDS,
};
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub(crate) fn product_export_branch_push_one(
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
}
