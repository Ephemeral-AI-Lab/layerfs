//! Accepted LayerStack snapshot admission.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::import::{read_fetch_stack_head, stage_fetch_stack_head};
use crate::full::branch::transition::insert_transition;
use crate::full::closure::membership::authenticate_root;
use crate::full::compaction::reachability::{
    record_layer_suffix_release, record_pushed_release, retain_root,
};
use crate::full::layer_stack::read::{
    read_historical_layer_root, read_layer_root, read_layer_stack_head,
};
use crate::full::legacy_store::Engine;
use crate::full::receipt::finish_fetch_target;
use crate::full::record_id::{derive_id, transition_identity, LayerId};
use crate::full::transfer::batch::{
    PushedLayerStack, PushedLayerStackAction, MAX_HISTORY_PAGE_RECORDS,
    MAX_TRANSITION_PAYLOAD_BYTES,
};
use rusqlite::params;
use rusqlite::Connection;

pub(crate) fn import_layer_stack_snapshot(
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
