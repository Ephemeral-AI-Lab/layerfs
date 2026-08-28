//! Exact-head LayerStack rollback rows.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::compaction::reachability::record_layer_suffix_release;
use crate::full::layer_stack::read::{
    full_layer_stack_head, read_layer_stack_head, LayerStackHead, LayerStackRollbackOutcome,
};
use crate::full::layer_stack::transition::{full_transaction, replay_full_request};
use crate::full::legacy_layer_stack::replay_layer_stack_request;
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_request, rollback_product_transaction, Engine,
};
use crate::full::record_id::{derive_id, full_release_id, object_id, LayerId, RequestId};
use crate::FullStorage;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
        request_id: RequestId,
    ) -> EngineResult<LayerStackRollbackOutcome> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if let Some(head) = replay_layer_stack_request(
                &connection,
                request_id,
                "layer_stack_rollback",
                expected,
                target,
            )? {
                return Ok(LayerStackRollbackOutcome::DurablyAccepted {
                    head,
                    reconciled: true,
                });
            }
            let actual = read_layer_stack_head(&connection, expected.layer_stack_id)?
                .ok_or(EngineError::InvalidRecord("LayerStack"))?;
            if actual != expected {
                return Ok(LayerStackRollbackOutcome::Conflict { actual });
            }
            let (target_generation, target_root) = connection
                .query_row(
                    "SELECT accepted_generation, root_id FROM layerfs_layers
                     WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state = 'accepted'
                       AND NOT EXISTS(
                           SELECT 1 FROM layerfs_released_versions r
                           WHERE r.target_kind = 'layer'
                             AND r.owner_id = layerfs_layers.layer_stack_id
                             AND r.version_id = layerfs_layers.layer_id)",
                    params![expected.layer_stack_id.as_bytes(), target.as_bytes()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Layer rollback target"))?;
            let current_generation = connection
                .query_row(
                    "SELECT accepted_generation FROM layerfs_layers
                     WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state = 'accepted'",
                    params![
                        expected.layer_stack_id.as_bytes(),
                        expected.layer_id.as_bytes()
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(map_sqlite_error)?;
            if target_generation >= current_generation {
                return Err(EngineError::InvalidRecord(
                    "Layer rollback target is not earlier",
                ));
            }
            let blocked = connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM layerfs_version_leases v
                       JOIN layerfs_layers l
                         ON v.target_kind = 'layer' AND v.target_id = l.layer_id
                       WHERE l.layer_stack_id = ?1
                         AND (l.accepted_generation > ?2 OR l.accepted_generation IS NULL)
                    )",
                    params![expected.layer_stack_id.as_bytes(), target_generation],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if blocked {
                return Ok(LayerStackRollbackOutcome::Blocked);
            }
            let next_generation = actual
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            let transition_id = derive_id(
                b"layer-stack-rollback-receipt",
                &[request_id.as_bytes(), target.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_layer_stack_transitions
                     (transition_id, layer_stack_id, before_generation,
                      after_generation, before_layer_id, after_layer_id,
                      action_kind, source_record_id, request_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             'layer_stack_rollback', ?6, ?7)",
                    params![
                        transition_id.as_slice(),
                        expected.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes(),
                        target.as_bytes(),
                        request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let changed = connection
                .execute(
                    "UPDATE layerfs_layer_stacks
                     SET generation = ?1, head_layer_id = ?2
                     WHERE layer_stack_id = ?3 AND generation = ?4 AND head_layer_id = ?5",
                    params![
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        target.as_bytes(),
                        expected.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            record_layer_suffix_release(
                &connection,
                expected.layer_stack_id,
                target_generation,
                current_generation,
                next_generation,
                request_id,
            )?;
            let head = LayerStackHead {
                layer_stack_id: actual.layer_stack_id,
                generation: next_generation,
                layer_id: target,
                root: object_id(&target_root)?,
            };
            let reconciled = commit_product_request(
                self,
                &mut connection,
                "layerfs_layer_stack_transitions",
                request_id,
            )?;
            Ok(LayerStackRollbackOutcome::DurablyAccepted { head, reconciled })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}

impl FullStorage {
    pub fn rollback_layer_stack(
        &self,
        expected: LayerStackHead,
        target: LayerId,
        request: RequestId,
    ) -> EngineResult<LayerStackRollbackOutcome> {
        self.require_authority()?;
        let connection = self.lock_connection()?;
        full_transaction(&connection, |connection| {
            if let Some(head) = replay_full_request(
                connection,
                request,
                "layer_stack_rollback",
                expected,
                target,
            )? {
                return Ok(LayerStackRollbackOutcome::DurablyAccepted {
                    head,
                    reconciled: true,
                });
            }
            let actual = full_layer_stack_head(connection, expected.layer_stack_id)?
                .ok_or(EngineError::InvalidRecord("LayerStack"))?;
            if actual != expected {
                return Ok(LayerStackRollbackOutcome::Conflict { actual });
            }
            let (target_generation, target_root) = connection
                .query_row(
                    "SELECT accepted_generation, result_root_id FROM layerfs_layers l
                     WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state = 'accepted'
                       AND NOT EXISTS(SELECT 1 FROM layerfs_released_versions r
                         WHERE r.target_kind = 'layer' AND r.layer_stack_id = l.layer_stack_id
                           AND r.layer_id = l.layer_id)",
                    params![expected.layer_stack_id.as_bytes(), target.as_bytes()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Layer rollback target"))?;
            let current_generation = connection
                .query_row(
                    "SELECT accepted_generation FROM layerfs_layers
                     WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state = 'accepted'",
                    params![
                        expected.layer_stack_id.as_bytes(),
                        expected.layer_id.as_bytes()
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(map_sqlite_error)?;
            if target_generation >= current_generation {
                return Err(EngineError::InvalidRecord(
                    "Layer rollback target is not earlier",
                ));
            }
            if connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_version_leases v
                     JOIN layerfs_layers l ON v.target_kind = 'layer'
                       AND v.layer_stack_id = l.layer_stack_id AND v.layer_id = l.layer_id
                     WHERE l.layer_stack_id = ?1
                       AND (l.accepted_generation > ?2 OR l.accepted_generation IS NULL))",
                    params![expected.layer_stack_id.as_bytes(), target_generation],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?
            {
                return Ok(LayerStackRollbackOutcome::Blocked);
            }
            let next = actual
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            let transition = derive_id(
                b"layer-stack-rollback-receipt",
                &[request.as_bytes(), target.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_layer_stack_transitions
                 (transition_id, layer_stack_id, before_generation, after_generation,
                  before_layer_id, after_layer_id, action_kind, source_record_id, request_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'layer_stack_rollback', ?6, ?7)",
                    params![
                        transition,
                        expected.layer_stack_id.as_bytes(),
                        i64::try_from(actual.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?,
                        actual.layer_id.as_bytes(),
                        target.as_bytes(),
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
                        target.as_bytes(),
                        expected.layer_stack_id.as_bytes(),
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
            let mut releases = connection
                .prepare(
                    "SELECT layer_id, result_root_id FROM layerfs_layers
                 WHERE layer_stack_id = ?1 AND accepted_generation > ?2
                   AND accepted_generation <= ?3 ORDER BY accepted_generation",
                )
                .map_err(map_sqlite_error)?;
            let releases = releases
                .query_map(
                    params![
                        expected.layer_stack_id.as_bytes(),
                        target_generation,
                        current_generation
                    ],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .map_err(map_sqlite_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(map_sqlite_error)?;
            for (layer, root) in releases {
                let layer: [u8; 32] = layer.try_into().map_err(|_| EngineError::SchemaMismatch)?;
                connection
                    .execute(
                        "INSERT OR IGNORE INTO layerfs_released_versions
                     (release_id, target_kind, layer_stack_id, layer_id, root_id,
                      release_generation, request_id)
                     VALUES (?1, 'layer', ?2, ?3, ?4, ?5, ?6)",
                        params![
                            full_release_id("layer", expected.layer_stack_id.as_bytes(), &layer)?,
                            expected.layer_stack_id.as_bytes(),
                            layer,
                            root,
                            i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?,
                            request.as_bytes()
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            Ok(LayerStackRollbackOutcome::DurablyAccepted {
                head: LayerStackHead {
                    layer_stack_id: actual.layer_stack_id,
                    generation: next,
                    layer_id: target,
                    root: object_id(&target_root)?,
                },
                reconciled: false,
            })
        })
    }
}
