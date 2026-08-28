use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::BranchHead;
use crate::full::record_id::{derive_id, transition_identity, BranchId, OperationVersionId};
use crate::full::transfer::batch::PushedOperation;
use crate::schema::TRANSITION_FORMAT_VERSION;
use layerfs_core::ObjectId;
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn sql_u64(value: u64) -> EngineResult<i64> {
    i64::try_from(value).map_err(|_| EngineError::CounterOverflow)
}

pub(crate) fn version_blob(value: Option<OperationVersionId>) -> Option<Vec<u8>> {
    value.map(|id| id.0.to_vec())
}

pub(crate) fn insert_full_ordinary_operation(
    connection: &Connection,
    branch_id: BranchId,
    prior: BranchHead,
    operation: &PushedOperation,
) -> EngineResult<BranchHead> {
    let after_generation = prior
        .generation
        .checked_add(1)
        .ok_or(EngineError::CounterOverflow)?;
    let version = OperationVersionId(derive_id(
        b"operation-version",
        &[
            operation.operation_id.as_bytes(),
            operation.request_id.as_bytes(),
            operation.root.as_bytes(),
        ],
    ));
    let delta = transition_identity(
        operation.base.root(),
        operation.root,
        &operation.transition_payload,
    );
    let operation_delta = derive_id(
        b"operation-delta",
        &[
            operation.operation_id.as_bytes(),
            version.as_bytes(),
            &delta,
        ],
    );
    if operation.parent_operation_version_id != prior.operation_version_id
        || operation.base.root() != prior.root
        || operation.expected_branch_generation != prior.generation
        || operation.before_generation != prior.generation
        || operation.after_generation != after_generation
        || operation.operation_version_id != version
        || operation.transition_delta_id != delta
        || operation.operation_delta_id != operation_delta
        || operation.release.is_some()
    {
        return Err(EngineError::InvalidRecord("Full operation identity"));
    }
    connection
        .execute(
            "INSERT INTO layerfs_deltas
             (delta_id, format_version, parent_root_id, result_root_id, payload)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(delta_id) DO NOTHING",
            params![
                delta.as_slice(),
                TRANSITION_FORMAT_VERSION,
                prior.root.as_bytes(),
                operation.root.as_bytes(),
                operation.transition_payload,
            ],
        )
        .map_err(map_sqlite_error)?;
    if !connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM layerfs_deltas
             WHERE delta_id = ?1 AND format_version = ?2 AND parent_root_id = ?3
               AND result_root_id = ?4 AND payload = ?5)",
            params![
                delta.as_slice(),
                TRANSITION_FORMAT_VERSION,
                prior.root.as_bytes(),
                operation.root.as_bytes(),
                operation.transition_payload,
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)?
    {
        return Err(EngineError::InvalidRecord("Full transition conflict"));
    }
    let (base_kind, stack, layer, base_version) = match operation.base {
        crate::full::branch::read::VersionRef::Layer {
            layer_stack_id,
            layer_id,
            ..
        } => ("layer", Some(layer_stack_id.0), Some(layer_id.0), None),
        crate::full::branch::read::VersionRef::OperationVersion {
            operation_version_id,
            ..
        } => (
            "operation_version",
            None,
            None,
            Some(operation_version_id.0),
        ),
    };
    connection
        .execute(
            "INSERT INTO layerfs_operations
             (operation_id, branch_id, sequence, expected_branch_generation, base_kind,
              base_layer_stack_id, base_layer_id, base_operation_version_id, base_root_id,
              candidate_root_id, result_operation_version_id, state, reconciliation_class)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                     'durably_accepted', 'exact')",
            params![
                operation.operation_id.as_bytes(),
                branch_id.as_bytes(),
                sql_u64(operation.operation_sequence)?,
                sql_u64(operation.expected_branch_generation)?,
                base_kind,
                stack,
                layer,
                base_version,
                prior.root.as_bytes(),
                operation.root.as_bytes(),
                version.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "INSERT INTO layerfs_operation_versions
             (operation_version_id, branch_id, sequence, parent_operation_version_id,
              created_by_kind, operation_id, child_branch_id, branch_delta_id,
              transition_delta_id, base_root_id, result_root_id)
             VALUES (?1, ?2, ?3, ?4, 'operation', ?5, NULL, NULL, ?6, ?7, ?8)",
            params![
                version.as_bytes(),
                branch_id.as_bytes(),
                sql_u64(operation.version_sequence)?,
                version_blob(prior.operation_version_id),
                operation.operation_id.as_bytes(),
                delta.as_slice(),
                prior.root.as_bytes(),
                operation.root.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    let transition = derive_id(
        b"branch-transition",
        &[operation.request_id.as_bytes(), version.as_bytes()],
    );
    connection
        .execute(
            "INSERT INTO layerfs_branch_transitions
             (transition_id, branch_id, before_generation, after_generation,
              before_operation_version_id, after_operation_version_id,
              action_kind, source_record_id, request_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'operation_commit', ?7, ?8)",
            params![
                transition.as_slice(),
                branch_id.as_bytes(),
                sql_u64(prior.generation)?,
                sql_u64(after_generation)?,
                version_blob(prior.operation_version_id),
                version.as_bytes(),
                operation.operation_id.as_bytes(),
                operation.request_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO layerfs_retained_roots (root_id) VALUES (?1)",
            params![operation.root.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    Ok(BranchHead {
        branch_id,
        generation: after_generation,
        operation_version_id: Some(version),
        root: operation.root,
    })
}

pub(crate) fn insert_transition(
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

pub(crate) use crate::full::legacy_branch_transition::{
    insert_pushed_branch_rollback, insert_pushed_child_merge,
};
