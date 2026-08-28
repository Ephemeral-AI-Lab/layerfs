//! LayerStack heads and accepted-history reads.

use crate::full::record_id::{LayerId, LayerStackId};
use layerfs_core::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LayerStackHead {
    pub layer_stack_id: LayerStackId,
    pub generation: u64,
    pub layer_id: LayerId,
    pub root: ObjectId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LayerStackMergeOutcome {
    DurablyAccepted {
        head: LayerStackHead,
        reconciled: bool,
    },
    Conflict {
        actual: LayerStackHead,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LayerStackRollbackOutcome {
    DurablyAccepted {
        head: LayerStackHead,
        reconciled: bool,
    },
    Conflict {
        actual: LayerStackHead,
    },
    Blocked,
}

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::export::pushed_release;
use crate::full::branch::import::{read_fetch_stack_head, read_published_fetch_stack_head};
use crate::full::legacy_store::Engine;
use crate::full::record_id::{bytes32, object_id, BranchId, OperationVersionId, RequestId};
use crate::full::transfer::batch::{
    PushedLayer, PushedLayerMerge, PushedLayerStack, PushedLayerStackAction,
    PushedLayerStackTransition, MAX_HISTORY_PAGE_RECORDS,
};
use crate::FullStorage;
use rusqlite::Connection;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_layer_stack_head(
        &self,
        layer_stack_id: LayerStackId,
    ) -> EngineResult<Option<LayerStackHead>> {
        let connection = self.lock_connection()?;
        read_layer_stack_head(&connection, layer_stack_id)
    }

    pub fn product_fetch_resume_layer_stack_head(
        &self,
        layer_stack_id: LayerStackId,
    ) -> EngineResult<Option<LayerStackHead>> {
        let connection = self.lock_connection()?;
        read_fetch_stack_head(&connection, layer_stack_id)
    }

    pub fn product_layer_root(
        &self,
        layer_stack_id: LayerStackId,
        layer_id: LayerId,
    ) -> EngineResult<Option<ObjectId>> {
        let connection = self.lock_connection()?;
        read_layer_root(&connection, layer_stack_id, layer_id)
    }
}

impl FullStorage {
    pub fn layer_stack_head(
        &self,
        layer_stack_id: LayerStackId,
    ) -> EngineResult<Option<LayerStackHead>> {
        self.require_authority()?;
        let connection = self.lock_connection()?;
        full_layer_stack_head(&connection, layer_stack_id)
    }

    pub fn layer_root(
        &self,
        layer_stack_id: LayerStackId,
        layer_id: LayerId,
    ) -> EngineResult<Option<ObjectId>> {
        self.require_authority()?;
        self.lock_connection()?
            .query_row(
                "SELECT result_root_id FROM layerfs_layers l
                 WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state != 'dropped'
                   AND NOT EXISTS(SELECT 1 FROM layerfs_released_versions r
                       WHERE r.target_kind = 'layer'
                         AND r.layer_stack_id = l.layer_stack_id AND r.layer_id = l.layer_id)",
                params![layer_stack_id.as_bytes(), layer_id.as_bytes()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .map(|root| object_id(&root))
            .transpose()
    }
}

pub(crate) fn full_layer_stack_head(
    connection: &Connection,
    id: LayerStackId,
) -> EngineResult<Option<LayerStackHead>> {
    connection
        .query_row(
            "SELECT s.generation, s.head_layer_id, l.result_root_id
             FROM layerfs_layer_stacks s JOIN layerfs_layers l
               ON l.layer_stack_id = s.layer_stack_id AND l.layer_id = s.head_layer_id
             WHERE s.layer_stack_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|row| {
            Ok(LayerStackHead {
                layer_stack_id: id,
                generation: u64::try_from(row.0)
                    .map_err(|_| EngineError::InvalidRecord("LayerStack generation"))?,
                layer_id: LayerId(bytes32(&row.1, "LayerId")?),
                root: object_id(&row.2)?,
            })
        })
        .transpose()
}
pub(crate) fn read_layer_stack_head(
    connection: &Connection,
    id: LayerStackId,
) -> EngineResult<Option<LayerStackHead>> {
    if let Some(published) = read_published_fetch_stack_head(connection, id)? {
        return Ok(published);
    }
    read_database_layer_stack_head(connection, id)
}

pub(crate) fn read_database_layer_stack_head(
    connection: &Connection,
    id: LayerStackId,
) -> EngineResult<Option<LayerStackHead>> {
    connection
        .query_row(
            "SELECT s.generation, s.head_layer_id, l.root_id
             FROM layerfs_layer_stacks s
             JOIN layerfs_layers l
               ON l.layer_stack_id = s.layer_stack_id AND l.layer_id = s.head_layer_id
             WHERE s.layer_stack_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(generation, layer, root)| {
            Ok(LayerStackHead {
                layer_stack_id: id,
                generation: u64::try_from(generation)
                    .map_err(|_| EngineError::InvalidRecord("LayerStack generation"))?,
                layer_id: LayerId(bytes32(&layer, "LayerId")?),
                root: object_id(&root)?,
            })
        })
        .transpose()
}

pub(crate) fn layer_stack_head_at_generation(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    generation: u64,
) -> EngineResult<LayerStackHead> {
    let layer = if generation == 0 {
        connection
            .query_row(
                "SELECT layer_id, root_id FROM layerfs_layers
                 WHERE layer_stack_id = ?1 AND creation_kind = 'genesis'
                   AND state = 'accepted' AND accepted_generation = 0",
                params![layer_stack_id.as_bytes()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?
    } else {
        connection
            .query_row(
                "SELECT t.after_layer_id, l.root_id
                 FROM layerfs_layer_stack_transitions t
                 JOIN layerfs_layers l
                   ON l.layer_stack_id = t.layer_stack_id
                  AND l.layer_id = t.after_layer_id
                 WHERE t.layer_stack_id = ?1 AND t.after_generation = ?2",
                params![
                    layer_stack_id.as_bytes(),
                    i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?
    }
    .ok_or(EngineError::InvalidRecord("LayerStack generation"))?;
    Ok(LayerStackHead {
        layer_stack_id,
        generation,
        layer_id: LayerId(bytes32(&layer.0, "LayerId")?),
        root: object_id(&layer.1)?,
    })
}

pub(crate) fn read_layer_root(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    layer_id: LayerId,
) -> EngineResult<Option<ObjectId>> {
    connection
        .query_row(
            "SELECT root_id FROM layerfs_layers
             WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state != 'dropped'
               AND NOT EXISTS(
                   SELECT 1 FROM layerfs_released_versions r
                   WHERE r.target_kind = 'layer'
                     AND r.owner_id = layerfs_layers.layer_stack_id
                     AND r.version_id = layerfs_layers.layer_id)",
            params![layer_stack_id.as_bytes(), layer_id.as_bytes()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|bytes| object_id(&bytes))
        .transpose()
}

pub(crate) fn read_historical_layer_root(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    layer_id: LayerId,
) -> EngineResult<Option<ObjectId>> {
    connection
        .query_row(
            "SELECT root_id FROM layerfs_layers
             WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state != 'dropped'",
            params![layer_stack_id.as_bytes(), layer_id.as_bytes()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|bytes| object_id(&bytes))
        .transpose()
}

pub(crate) fn export_layer_stack_snapshot(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    base: Option<LayerStackHead>,
    advance: bool,
) -> EngineResult<PushedLayerStack> {
    let durable_head = read_layer_stack_head(connection, layer_stack_id)?
        .ok_or(EngineError::InvalidRecord("Fetch LayerStack"))?;
    if base.is_some_and(|base| {
        base.layer_stack_id != layer_stack_id || base.generation > durable_head.generation
    }) {
        return Err(EngineError::InvalidRecord("Fetch LayerStack base"));
    }
    if let Some(base) = base {
        if layer_stack_head_at_generation(connection, layer_stack_id, base.generation)? != base {
            return Err(EngineError::InvalidRecord("Fetch LayerStack base"));
        }
    }
    let after_generation = base.map_or(0, |base| base.generation);
    let page_generation = if advance {
        after_generation
            .checked_add(
                u64::try_from(MAX_HISTORY_PAGE_RECORDS)
                    .map_err(|_| EngineError::CounterOverflow)?,
            )
            .ok_or(EngineError::CounterOverflow)?
            .min(durable_head.generation)
    } else {
        after_generation
    };
    let head = layer_stack_head_at_generation(connection, layer_stack_id, page_generation)?;
    let name = connection
        .query_row(
            "SELECT name FROM layerfs_layer_stacks WHERE layer_stack_id = ?1",
            params![layer_stack_id.as_bytes()],
            |row| row.get::<_, String>(0),
        )
        .map_err(map_sqlite_error)?;
    if name.is_empty() || name.len() > 255 {
        return Err(EngineError::InvalidRecord("LayerStack name"));
    }
    let mut statement = connection
        .prepare(
            "SELECT l.layer_id, l.parent_layer_id, l.root_id, l.creation_kind,
                    l.accepted_generation, l.source_branch_id,
                    l.source_branch_depth,
                    l.source_branch_head_operation_version_id,
                    l.prepared_request_id, bd.branch_delta_id,
                    bd.base_root, bd.source_root, bd.destination_root,
                    bd.source_delta_id, sd.payload,
                    bd.applied_delta_id, ad.payload, ld.layer_delta_id,
                    (SELECT r.release_generation FROM layerfs_released_versions r
                        WHERE r.target_kind = 'layer'
                          AND r.owner_id = l.layer_stack_id
                          AND r.version_id = l.layer_id),
                    (SELECT r.request_id FROM layerfs_released_versions r
                        WHERE r.target_kind = 'layer'
                          AND r.owner_id = l.layer_stack_id
                          AND r.version_id = l.layer_id),
                    l.source_branch_generation
             FROM layerfs_layers l
             LEFT JOIN layerfs_branch_deltas bd
               ON bd.branch_delta_id = l.source_branch_delta_id
             LEFT JOIN layerfs_deltas sd
               ON sd.delta_id = bd.source_delta_id AND sd.format_version = 1
             LEFT JOIN layerfs_deltas ad
               ON ad.delta_id = bd.applied_delta_id AND ad.format_version = 1
             LEFT JOIN layerfs_layer_deltas ld
               ON ld.candidate_layer_id = l.layer_id
             WHERE l.layer_stack_id = ?1 AND l.state = 'accepted'
               AND ((?2 = 1 AND l.accepted_generation = 0)
                    OR (l.accepted_generation > ?3 AND l.accepted_generation <= ?4))
             ORDER BY l.accepted_generation",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query(params![
            layer_stack_id.as_bytes(),
            i64::from(base.is_none()),
            i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
            i64::try_from(page_generation).map_err(|_| EngineError::CounterOverflow)?,
        ])
        .map_err(map_sqlite_error)?;
    let mut layers = Vec::new();
    while let Some(row) = rows.next().map_err(map_sqlite_error)? {
        if layers.len() == MAX_HISTORY_PAGE_RECORDS + 1 {
            return Err(EngineError::InvalidRecord("Fetch LayerStack history page"));
        }
        let creation = row.get::<_, String>(3).map_err(map_sqlite_error)?;
        let merge = match creation.as_str() {
            "genesis" => None,
            "candidate" => Some(PushedLayerMerge {
                source_branch_id: BranchId(bytes32(
                    &row.get::<_, Vec<u8>>(5).map_err(map_sqlite_error)?,
                    "BranchId",
                )?),
                source_branch_depth: u64::try_from(row.get::<_, i64>(6).map_err(map_sqlite_error)?)
                    .map_err(|_| EngineError::InvalidRecord("Branch depth"))?,
                source_branch_generation: u64::try_from(
                    row.get::<_, i64>(20).map_err(map_sqlite_error)?,
                )
                .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                source_operation_version_id: OperationVersionId(bytes32(
                    &row.get::<_, Vec<u8>>(7).map_err(map_sqlite_error)?,
                    "OperationVersionId",
                )?),
                request_id: RequestId(bytes32(
                    &row.get::<_, Vec<u8>>(8).map_err(map_sqlite_error)?,
                    "RequestId",
                )?),
                branch_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(9).map_err(map_sqlite_error)?,
                    "BranchDeltaId",
                )?,
                base_root: object_id(&row.get::<_, Vec<u8>>(10).map_err(map_sqlite_error)?)?,
                source_root: object_id(&row.get::<_, Vec<u8>>(11).map_err(map_sqlite_error)?)?,
                destination_root: object_id(&row.get::<_, Vec<u8>>(12).map_err(map_sqlite_error)?)?,
                source_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(13).map_err(map_sqlite_error)?,
                    "TransitionId",
                )?,
                source_transition_payload: row.get::<_, Vec<u8>>(14).map_err(map_sqlite_error)?,
                applied_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(15).map_err(map_sqlite_error)?,
                    "TransitionId",
                )?,
                applied_transition_payload: row.get::<_, Vec<u8>>(16).map_err(map_sqlite_error)?,
                layer_delta_id: bytes32(
                    &row.get::<_, Vec<u8>>(17).map_err(map_sqlite_error)?,
                    "LayerDeltaId",
                )?,
            }),
            _ => return Err(EngineError::InvalidRecord("Layer creation kind")),
        };
        layers.push(PushedLayer {
            layer_id: LayerId(bytes32(
                &row.get::<_, Vec<u8>>(0).map_err(map_sqlite_error)?,
                "LayerId",
            )?),
            parent_layer_id: row
                .get::<_, Option<Vec<u8>>>(1)
                .map_err(map_sqlite_error)?
                .map(|id| bytes32(&id, "LayerId").map(LayerId))
                .transpose()?,
            root: object_id(&row.get::<_, Vec<u8>>(2).map_err(map_sqlite_error)?)?,
            release: pushed_release(
                row.get::<_, Option<i64>>(18).map_err(map_sqlite_error)?,
                row.get::<_, Option<Vec<u8>>>(19)
                    .map_err(map_sqlite_error)?,
            )?,
            accepted_generation: u64::try_from(row.get::<_, i64>(4).map_err(map_sqlite_error)?)
                .map_err(|_| EngineError::InvalidRecord("Layer generation"))?,
            merge,
        });
    }
    let mut statement = connection
        .prepare(
            "SELECT before_generation, after_generation, before_layer_id,
                    after_layer_id, action_kind, source_record_id, request_id
             FROM layerfs_layer_stack_transitions
             WHERE layer_stack_id = ?1 AND after_generation > ?2
               AND after_generation <= ?3 ORDER BY after_generation",
        )
        .map_err(map_sqlite_error)?;
    let transitions = statement
        .query_map(
            params![
                layer_stack_id.as_bytes(),
                i64::try_from(after_generation).map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(page_generation).map_err(|_| EngineError::CounterOverflow)?,
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                ))
            },
        )
        .map_err(map_sqlite_error)?
        .map(|row| {
            let row = row.map_err(map_sqlite_error)?;
            Ok(PushedLayerStackTransition {
                before_generation: u64::try_from(row.0)
                    .map_err(|_| EngineError::InvalidRecord("LayerStack generation"))?,
                after_generation: u64::try_from(row.1)
                    .map_err(|_| EngineError::InvalidRecord("LayerStack generation"))?,
                before_layer_id: LayerId(bytes32(&row.2, "LayerId")?),
                after_layer_id: LayerId(bytes32(&row.3, "LayerId")?),
                action: match row.4.as_str() {
                    "layer_stack_merge" => PushedLayerStackAction::Merge,
                    "layer_stack_rollback" => PushedLayerStackAction::Rollback,
                    _ => return Err(EngineError::InvalidRecord("LayerStack action")),
                },
                source_record_id: bytes32(&row.5, "LayerStack source record")?,
                request_id: RequestId(bytes32(&row.6, "RequestId")?),
            })
        })
        .collect::<EngineResult<Vec<_>>>()?;
    if base.is_none() && !layers.iter().any(|layer| layer.accepted_generation == 0)
        || transitions.len() > MAX_HISTORY_PAGE_RECORDS
        || transitions.len()
            != usize::try_from(page_generation - after_generation)
                .map_err(|_| EngineError::CounterOverflow)?
    {
        return Err(EngineError::InvalidRecord("Fetch LayerStack history"));
    }
    Ok(PushedLayerStack {
        name,
        base,
        head,
        complete: head == durable_head,
        layers,
        transitions,
    })
}
