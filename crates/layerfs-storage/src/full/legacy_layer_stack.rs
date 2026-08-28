//! Transitional legacy_full Engine LayerStack bootstrap/finalization compatibility.
//! Remove in P2/P7 once Working uses its 14-table owner and Durable runs typed Full.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::branch_contains_exact_version;
use crate::full::closure::membership::authenticate_root;
use crate::full::layer_stack::read::{
    read_layer_stack_head, LayerStackHead, LayerStackMergeOutcome,
};
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_request, commit_product_state,
    rollback_product_transaction, Engine,
};
use crate::full::record_id::{derive_id, object_id, LayerId, LayerStackId, RequestId};
use crate::working::layer_candidate::LayerCandidate;
use layerfs_core::ObjectId;
use rusqlite::{params, Connection, OptionalExtension};

impl Engine {
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
                validate_replayed_layer_candidate(
                    &connection,
                    request_id,
                    candidate,
                    expected,
                    head,
                )?;
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
                            source_branch_generation, source_branch_head_operation_version_id,
                            root_id, prepared_request_id, source_branch_delta_id
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
                &[request_id.as_bytes(), candidate.layer_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_layer_stack_transitions
                 (transition_id, layer_stack_id, before_generation, after_generation,
                  before_layer_id, after_layer_id, action_kind, source_record_id, request_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'layer_stack_merge', ?7, ?8)",
                    params![
                        transition,
                        candidate.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes(),
                        candidate.layer_id.as_bytes(),
                        stored.7,
                        request_id.as_bytes()
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
                        candidate.layer_stack_id.as_bytes(),
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
                generation: next,
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
}

pub(crate) fn replay_layer_stack_request(
    connection: &Connection,
    request_id: RequestId,
    action: &'static str,
    expected: LayerStackHead,
    after_layer_id: LayerId,
) -> EngineResult<Option<LayerStackHead>> {
    let receipt = connection.query_row(
        "SELECT t.layer_stack_id, t.before_generation, t.after_generation,
                t.before_layer_id, t.after_layer_id, t.action_kind,
                before_layer.root_id, after_layer.root_id
         FROM layerfs_layer_stack_transitions t JOIN layerfs_layers before_layer
           ON before_layer.layer_stack_id = t.layer_stack_id AND before_layer.layer_id = t.before_layer_id
         JOIN layerfs_layers after_layer ON after_layer.layer_stack_id = t.layer_stack_id
          AND after_layer.layer_id = t.after_layer_id WHERE t.request_id = ?1",
        params![request_id.as_bytes()],
        |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?,
            row.get::<_, Vec<u8>>(3)?, row.get::<_, Vec<u8>>(4)?, row.get::<_, String>(5)?,
            row.get::<_, Vec<u8>>(6)?, row.get::<_, Vec<u8>>(7)?)),
    ).optional().map_err(map_sqlite_error)?;
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.0.as_slice() != expected.layer_stack_id.as_bytes()
        || u64::try_from(receipt.1).ok() != Some(expected.generation)
        || receipt.3.as_slice() != expected.layer_id.as_bytes()
        || receipt.4.as_slice() != after_layer_id.as_bytes()
        || receipt.5 != action
        || object_id(&receipt.6)? != expected.root
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
        root: object_id(&receipt.7)?,
    }))
}

fn validate_replayed_layer_candidate(
    connection: &Connection,
    request_id: RequestId,
    candidate: LayerCandidate,
    expected: LayerStackHead,
    accepted: LayerStackHead,
) -> EngineResult<()> {
    let stored = connection
        .query_row(
            "SELECT l.parent_layer_id, l.source_branch_id, l.source_branch_depth,
                l.source_branch_generation, l.source_branch_head_operation_version_id, l.root_id,
                l.prepared_request_id, l.accepted_generation, d.source_branch_id,
                d.source_branch_generation, d.source_branch_operation_version_id, d.source_root,
                d.destination_root, d.result_root
         FROM layerfs_layer_stack_transitions t JOIN layerfs_layers l
           ON l.layer_stack_id = t.layer_stack_id AND l.layer_id = t.after_layer_id
         JOIN layerfs_branch_deltas d ON d.branch_delta_id = t.source_record_id
          AND d.branch_delta_id = l.source_branch_delta_id AND d.purpose = 'layer_stack_merge'
         WHERE t.request_id = ?1 AND t.action_kind = 'layer_stack_merge' AND l.state = 'accepted'",
            params![request_id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Vec<u8>>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Vec<u8>>(10)?,
                    row.get::<_, Vec<u8>>(11)?,
                    row.get::<_, Vec<u8>>(12)?,
                    row.get::<_, Vec<u8>>(13)?,
                ))
            },
        )
        .map_err(map_sqlite_error)?;
    let source = candidate
        .source
        .operation_version_id
        .ok_or(EngineError::InvalidRecord("candidate source version"))?;
    if candidate.layer_stack_id != expected.layer_stack_id
        || candidate.parent_layer_id != expected.layer_id
        || candidate.root != accepted.root
        || stored.0.as_slice() != candidate.parent_layer_id.as_bytes()
        || stored.1.as_slice() != candidate.source.branch_id.as_bytes()
        || u64::try_from(stored.2).ok() != Some(candidate.source_depth)
        || u64::try_from(stored.3).ok() != Some(candidate.source.generation)
        || stored.4.as_slice() != source.as_bytes()
        || object_id(&stored.5)? != candidate.root
        || stored.6.as_slice() != candidate.request_id.as_bytes()
        || u64::try_from(stored.7).ok() != Some(accepted.generation)
        || stored.8.as_slice() != candidate.source.branch_id.as_bytes()
        || u64::try_from(stored.9).ok() != Some(candidate.source.generation)
        || stored.10.as_slice() != source.as_bytes()
        || object_id(&stored.11)? != candidate.source.root
        || object_id(&stored.12)? != expected.root
        || object_id(&stored.13)? != candidate.root
        || !branch_contains_exact_version(connection, candidate.source)?
    {
        return Err(EngineError::InvalidRecord(
            "LayerStack request candidate identity conflict",
        ));
    }
    Ok(())
}
