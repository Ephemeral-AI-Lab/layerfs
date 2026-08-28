//! Transitional legacy_full child-merge and Branch-rollback row writers.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::read_branch_ancestry;
use crate::full::branch::transition::{insert_transition, sql_u64, version_blob};
use crate::full::closure::membership::authenticate_root;
use crate::full::compaction::reachability;
use crate::full::legacy_store::Engine;
use crate::full::record_id::{
    derive_id, object_id, transition_identity, BranchId, OperationVersionId,
};
use crate::full::transfer::batch::{PushedBranchRollback, PushedChildMerge};
use layerfs_core::ObjectId;
use rusqlite::{params, Connection, OptionalExtension};

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_pushed_child_merge(
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
                sql_u64(merge.source_branch_generation)?,
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
                sql_u64(merge.source_branch_generation)?,
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
                sql_u64(merge.version_sequence)?,
                version_blob(merge.parent_operation_version_id),
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
                sql_u64(merge.before_generation)?,
                sql_u64(merge.after_generation)?,
                version_blob(merge.parent_operation_version_id),
                merge.operation_version_id.as_bytes(),
                merge.branch_delta_id.as_slice(),
                merge.request_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    match merge.release {
        Some(release) => reachability::record_pushed_release(
            connection,
            "operation_version",
            branch_id.as_bytes(),
            merge.operation_version_id.as_bytes(),
            merge.root,
            release,
        )?,
        None => reachability::retain_root(connection, merge.root)?,
    }
    Ok((
        merge.operation_version_id,
        merge.root,
        merge.after_generation,
    ))
}

pub(crate) fn insert_pushed_branch_rollback(
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
                sql_u64(rollback.before_generation)?,
                sql_u64(rollback.after_generation)?,
                rollback.before_operation_version_id.as_bytes(),
                rollback.target_operation_version_id.as_bytes(),
                rollback.request_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    reachability::record_branch_suffix_release(
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
