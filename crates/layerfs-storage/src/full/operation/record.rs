//! Accepted Operation and OperationVersion admission.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::VersionRef;
use crate::full::branch::transition::insert_transition;
use crate::full::closure::membership::authenticate_root;
use crate::full::compaction::reachability::{record_pushed_release, retain_root};
use crate::full::legacy_store::Engine;
use crate::full::record_id::{derive_id, transition_identity, OperationVersionId};
use crate::full::transfer::batch::{BranchPushBundle, PushedOperation};
use layerfs_core::ObjectId;
use rusqlite::params;
use rusqlite::Connection;

pub(crate) fn insert_pushed_operation(
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
