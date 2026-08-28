//! Authoritative Full LayerStack bootstrap, candidate import, and finalization CAS.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::layer_stack::read::{
    full_layer_stack_head, LayerStackHead, LayerStackMergeOutcome,
};
use crate::full::record_id::{
    derive_id, object_id, transition_identity, LayerId, LayerStackId, RequestId,
};
use crate::working::layer_candidate::LayerCandidate;
use crate::FullStorage;
use layerfs_core::ObjectId;
use rusqlite::{params, Connection, OptionalExtension};

impl FullStorage {
    pub fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> EngineResult<LayerStackHead> {
        self.require_authority()?;
        if name.is_empty() || name.len() > 255 {
            return Err(EngineError::InvalidRecord("LayerStack name"));
        }
        let connection = self.lock_connection()?;
        full_transaction(&connection, |connection| {
            if let Some(head) = full_layer_stack_head(connection, stack)? {
                let stored_name = connection
                    .query_row(
                        "SELECT name FROM layerfs_layer_stacks WHERE layer_stack_id = ?1",
                        params![stack.as_bytes()],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(map_sqlite_error)?;
                return if stored_name == name
                    && head
                        == (LayerStackHead {
                            layer_stack_id: stack,
                            generation: 0,
                            layer_id: layer,
                            root,
                        })
                {
                    Ok(head)
                } else {
                    Err(EngineError::InvalidRecord("LayerStack bootstrap identity"))
                };
            }
            connection
                .execute(
                    "INSERT INTO layerfs_layer_stacks
                 (layer_stack_id, name, generation, head_layer_id) VALUES (?1, ?2, 0, ?3)",
                    params![stack.as_bytes(), name, layer.as_bytes()],
                )
                .and_then(|_| {
                    connection.execute(
                        "INSERT INTO layerfs_layers
                 (layer_id, layer_stack_id, result_root_id, creation_kind, state,
                  accepted_generation) VALUES (?1, ?2, ?3, 'genesis', 'accepted', 0)",
                        params![layer.as_bytes(), stack.as_bytes(), root.as_bytes()],
                    )
                })
                .and_then(|_| {
                    connection.execute(
                        "INSERT OR IGNORE INTO layerfs_retained_roots (root_id) VALUES (?1)",
                        params![root.as_bytes()],
                    )
                })
                .map_err(map_sqlite_error)?;
            Ok(LayerStackHead {
                layer_stack_id: stack,
                generation: 0,
                layer_id: layer,
                root,
            })
        })
    }

    pub fn import_layer_candidate(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> EngineResult<LayerCandidate> {
        self.require_authority()?;
        let connection = self.lock_connection()?;
        full_transaction(&connection, |connection| {
            if candidate.layer_stack_id != expected.layer_stack_id
                || candidate.parent_layer_id != expected.layer_id
                || candidate.layer_id
                    != LayerId(derive_id(
                        b"candidate-layer",
                        &[
                            expected.layer_stack_id.as_bytes(),
                            candidate.request_id.as_bytes(),
                            candidate.root.as_bytes(),
                        ],
                    ))
            {
                return Err(EngineError::InvalidRecord(
                    "Full Layer candidate destination",
                ));
            }
            if let Some(matches) = full_candidate_matches(connection, candidate, expected)? {
                return matches
                    .then_some(candidate)
                    .ok_or(EngineError::InvalidRecord("Full Layer candidate identity"));
            }
            if full_layer_stack_head(connection, expected.layer_stack_id)? != Some(expected) {
                return Err(EngineError::PublicationConflict);
            }
            let version = candidate
                .source
                .operation_version_id
                .ok_or(EngineError::InvalidRecord("Full Layer candidate source"))?;
            let source = connection.query_row(
                "SELECT b.depth, b.origin_layer_stack_id, origin.result_root_id, v.result_root_id
                 FROM layerfs_branches b JOIN layerfs_layers origin
                   ON origin.layer_stack_id = b.origin_layer_stack_id AND origin.layer_id = b.origin_layer_id
                 JOIN layerfs_operation_versions v
                   ON v.branch_id = b.branch_id AND v.operation_version_id = ?2
                 JOIN layerfs_branch_transitions t ON t.branch_id = b.branch_id
                  AND t.after_generation = ?3 AND t.after_operation_version_id = v.operation_version_id
                 WHERE b.branch_id = ?1",
                params![candidate.source.branch_id.as_bytes(), version.as_bytes(),
                    i64::try_from(candidate.source.generation).map_err(|_| EngineError::CounterOverflow)?],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?, row.get::<_, Vec<u8>>(3)?)),
            ).map_err(map_sqlite_error)?;
            if u64::try_from(source.0).ok() != Some(candidate.source_depth)
                || source.1.as_slice() != expected.layer_stack_id.as_bytes()
                || object_id(&source.3)? != candidate.source.root
            {
                return Err(EngineError::InvalidRecord("Full Layer candidate source"));
            }
            let origin = object_id(&source.2)?;
            let source_delta = transition_identity(origin, candidate.source.root, &[]);
            let applied_delta = transition_identity(expected.root, candidate.root, &[]);
            for (id, parent, result) in [
                (source_delta, origin, candidate.source.root),
                (applied_delta, expected.root, candidate.root),
            ] {
                connection
                    .execute(
                        "INSERT OR IGNORE INTO layerfs_deltas
                     (delta_id, format_version, parent_root_id, result_root_id, payload)
                     VALUES (?1, 1, ?2, ?3, X'')",
                        params![id, parent.as_bytes(), result.as_bytes()],
                    )
                    .map_err(map_sqlite_error)?;
            }
            let branch_delta = derive_id(
                b"layer-stack-branch-delta",
                &[
                    candidate.source.branch_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    &source_delta,
                    &applied_delta,
                ],
            );
            connection
                .execute(
                    "INSERT OR IGNORE INTO layerfs_branch_deltas
                 (branch_delta_id, purpose, source_branch_id, source_branch_generation,
                  source_operation_version_id, base_root_id, source_root_id,
                  destination_root_id, result_root_id, source_delta_id, applied_delta_id)
                 VALUES (?1, 'layer_stack_merge', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        branch_delta,
                        candidate.source.branch_id.as_bytes(),
                        i64::try_from(candidate.source.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        version.as_bytes(),
                        origin.as_bytes(),
                        candidate.source.root.as_bytes(),
                        expected.root.as_bytes(),
                        candidate.root.as_bytes(),
                        source_delta,
                        applied_delta
                    ],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT INTO layerfs_layers
                 (layer_id, layer_stack_id, parent_layer_id, result_root_id, creation_kind,
                  source_branch_id, source_branch_depth, source_branch_generation,
                  source_operation_version_id, source_branch_delta_id, transition_delta_id,
                  parent_root_id, state, prepared_request_id)
                 VALUES (?1, ?2, ?3, ?4, 'candidate', ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                         'candidate', ?12)",
                    params![
                        candidate.layer_id.as_bytes(),
                        candidate.layer_stack_id.as_bytes(),
                        candidate.parent_layer_id.as_bytes(),
                        candidate.root.as_bytes(),
                        candidate.source.branch_id.as_bytes(),
                        i64::try_from(candidate.source_depth)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(candidate.source.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        version.as_bytes(),
                        branch_delta,
                        applied_delta,
                        expected.root.as_bytes(),
                        candidate.request_id.as_bytes()
                    ],
                )
                .map_err(map_sqlite_error)?;
            for (lease, kind, stack_id, layer_id, branch_id, version_id) in [
                (
                    derive_id(
                        b"layer-candidate-source-lease",
                        &[version.as_bytes(), candidate.request_id.as_bytes()],
                    ),
                    "operation_version",
                    None,
                    None,
                    Some(candidate.source.branch_id),
                    Some(version),
                ),
                (
                    derive_id(
                        b"layer-candidate-lease",
                        &[
                            candidate.layer_id.as_bytes(),
                            candidate.request_id.as_bytes(),
                        ],
                    ),
                    "layer",
                    Some(candidate.layer_stack_id),
                    Some(candidate.layer_id),
                    None,
                    None,
                ),
            ] {
                connection
                    .execute(
                        "INSERT OR IGNORE INTO layerfs_version_leases
                     (lease_id, target_kind, layer_stack_id, layer_id, branch_id,
                      operation_version_id, owner_kind, owner_id, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'layer_candidate', ?7, 0)",
                        params![
                            lease,
                            kind,
                            stack_id.map(|id| id.0),
                            layer_id.map(|id| id.0),
                            branch_id.map(|id| id.0),
                            version_id.map(|id| id.0),
                            candidate.request_id.as_bytes()
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            connection
                .execute(
                    "INSERT OR IGNORE INTO layerfs_retained_roots (root_id) VALUES (?1)",
                    params![candidate.root.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            Ok(candidate)
        })
    }

    pub fn finalize_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
        request: RequestId,
    ) -> EngineResult<LayerStackMergeOutcome> {
        self.require_authority()?;
        let connection = self.lock_connection()?;
        full_transaction(&connection, |connection| {
            if let Some(head) = replay_full_request(
                connection,
                request,
                "layer_stack_merge",
                expected,
                candidate.layer_id,
            )? {
                if full_candidate_matches(connection, candidate, expected)? != Some(true) {
                    return Err(EngineError::InvalidRecord("Full Layer candidate identity"));
                }
                return Ok(LayerStackMergeOutcome::DurablyAccepted {
                    head,
                    reconciled: true,
                });
            }
            let actual = full_layer_stack_head(connection, expected.layer_stack_id)?
                .ok_or(EngineError::InvalidRecord("LayerStack"))?;
            if actual != expected {
                return Ok(LayerStackMergeOutcome::Conflict { actual });
            }
            if full_candidate_matches(connection, candidate, expected)? != Some(true) {
                return Err(EngineError::InvalidRecord("Full Layer candidate identity"));
            }
            let source_record = connection
                .query_row(
                    "SELECT source_branch_delta_id FROM layerfs_layers WHERE layer_id = ?1",
                    params![candidate.layer_id.as_bytes()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .map_err(map_sqlite_error)?;
            let next = actual
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            if connection
                .execute(
                    "UPDATE layerfs_layers SET state = 'accepted', accepted_generation = ?1
                WHERE layer_stack_id = ?2 AND layer_id = ?3 AND state = 'candidate'",
                    params![
                        i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?,
                        candidate.layer_stack_id.as_bytes(),
                        candidate.layer_id.as_bytes()
                    ],
                )
                .map_err(map_sqlite_error)?
                != 1
            {
                return Err(EngineError::PublicationConflict);
            }
            let transition = derive_id(
                b"layer-stack-merge-receipt",
                &[request.as_bytes(), candidate.layer_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_layer_stack_transitions
                 (transition_id, layer_stack_id, before_generation, after_generation,
                  before_layer_id, after_layer_id, action_kind, source_record_id, request_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'layer_stack_merge', ?7, ?8)",
                    params![
                        transition,
                        actual.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes(),
                        candidate.layer_id.as_bytes(),
                        source_record,
                        request.as_bytes()
                    ],
                )
                .map_err(map_sqlite_error)?;
            if connection
                .execute(
                    "UPDATE layerfs_layer_stacks SET generation = ?1, head_layer_id = ?2
                WHERE layer_stack_id = ?3 AND generation = ?4 AND head_layer_id = ?5",
                    params![
                        i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?,
                        candidate.layer_id.as_bytes(),
                        actual.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes()
                    ],
                )
                .map_err(map_sqlite_error)?
                != 1
            {
                return Err(EngineError::PublicationConflict);
            }
            connection
                .execute(
                    "DELETE FROM layerfs_version_leases
                WHERE owner_kind = 'layer_candidate' AND owner_id = ?1",
                    params![candidate.request_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            Ok(LayerStackMergeOutcome::DurablyAccepted {
                head: LayerStackHead {
                    layer_stack_id: actual.layer_stack_id,
                    generation: next,
                    layer_id: candidate.layer_id,
                    root: candidate.root,
                },
                reconciled: false,
            })
        })
    }
}

fn full_candidate_matches(
    connection: &Connection,
    candidate: LayerCandidate,
    expected: LayerStackHead,
) -> EngineResult<Option<bool>> {
    connection
        .query_row(
            "SELECT parent_layer_id = ?2 AND result_root_id = ?3 AND source_branch_id = ?4
             AND source_branch_depth = ?5 AND source_branch_generation = ?6
             AND source_operation_version_id = ?7 AND prepared_request_id = ?8
             AND parent_root_id = ?9 AND state IN ('candidate', 'accepted')
             AND EXISTS(SELECT 1 FROM layerfs_operation_versions v
               WHERE v.branch_id = ?4 AND v.operation_version_id = ?7 AND v.result_root_id = ?11)
         FROM layerfs_layers WHERE layer_id = ?1 AND layer_stack_id = ?10",
            params![
                candidate.layer_id.as_bytes(),
                candidate.parent_layer_id.as_bytes(),
                candidate.root.as_bytes(),
                candidate.source.branch_id.as_bytes(),
                i64::try_from(candidate.source_depth).map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(candidate.source.generation)
                    .map_err(|_| EngineError::CounterOverflow)?,
                candidate.source.operation_version_id.map(|id| id.0),
                candidate.request_id.as_bytes(),
                expected.root.as_bytes(),
                candidate.layer_stack_id.as_bytes(),
                candidate.source.root.as_bytes()
            ],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map_err(map_sqlite_error)
}

pub(crate) fn full_transaction<T>(
    connection: &Connection,
    action: impl FnOnce(&Connection) -> EngineResult<T>,
) -> EngineResult<T> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(map_sqlite_error)?;
    let result = action(connection);
    if result.is_ok() {
        connection
            .execute_batch("COMMIT")
            .map_err(map_sqlite_error)?;
    } else {
        let _ = connection.execute_batch("ROLLBACK");
    }
    result
}

pub(crate) fn replay_full_request(
    connection: &Connection,
    request: RequestId,
    action: &str,
    expected: LayerStackHead,
    after: LayerId,
) -> EngineResult<Option<LayerStackHead>> {
    connection.query_row(
        "SELECT t.before_generation, t.after_generation, t.before_layer_id, t.after_layer_id,
                t.action_kind, b.result_root_id, a.result_root_id
         FROM layerfs_layer_stack_transitions t JOIN layerfs_layers b
           ON b.layer_stack_id = t.layer_stack_id AND b.layer_id = t.before_layer_id
         JOIN layerfs_layers a ON a.layer_stack_id = t.layer_stack_id AND a.layer_id = t.after_layer_id
         WHERE t.request_id = ?1 AND t.layer_stack_id = ?2",
        params![request.as_bytes(), expected.layer_stack_id.as_bytes()],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, Vec<u8>>(2)?,
            row.get::<_, Vec<u8>>(3)?, row.get::<_, String>(4)?, row.get::<_, Vec<u8>>(5)?,
            row.get::<_, Vec<u8>>(6)?)),
    ).optional().map_err(map_sqlite_error)?.map(|row| {
        if u64::try_from(row.0).ok() != Some(expected.generation)
            || row.2.as_slice() != expected.layer_id.as_bytes() || row.3.as_slice() != after.as_bytes()
            || row.4 != action || object_id(&row.5)? != expected.root {
            return Err(EngineError::InvalidRecord("Full LayerStack request identity"));
        }
        Ok(LayerStackHead { layer_stack_id: expected.layer_stack_id,
            generation: u64::try_from(row.1).map_err(|_| EngineError::InvalidRecord("LayerStack generation"))?,
            layer_id: after, root: object_id(&row.6)? })
    }).transpose()
}
