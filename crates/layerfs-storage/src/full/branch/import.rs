//! Accepted Branch snapshot and fetched-row admission.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{read_branch_head, read_database_branch_head, BranchHead};
use crate::full::closure::membership::authenticate_root;
use crate::full::closure::tracking::insert_verified_tracking_ref;
use crate::full::layer_stack::read::{
    read_database_layer_stack_head, read_layer_stack_head, LayerStackHead,
};
use crate::full::legacy_store::Engine;
use crate::full::record_id::{
    bytes32, object_id, BranchId, LayerId, LayerStackId, OperationVersionId, RequestId,
};
use crate::full::transfer::batch::{BranchPushBundle, VerifiedFetchRequest};
use crate::working::lease::unix_seconds;
use layerfs_core::ObjectId;
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn read_published_fetch_branch_head(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<Option<BranchHead>>> {
    connection
        .query_row(
            "SELECT published_generation, published_version_id, published_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = 'branch' AND target_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(
            |(generation, version, root)| match (generation, version, root) {
                (None, None, None) => Ok(None),
                (Some(generation), version, Some(root)) => Ok(Some(BranchHead {
                    branch_id: id,
                    generation: u64::try_from(generation)
                        .map_err(|_| EngineError::InvalidRecord("Fetch Branch generation"))?,
                    operation_version_id: version
                        .map(|bytes| bytes32(&bytes, "OperationVersionId").map(OperationVersionId))
                        .transpose()?,
                    root: object_id(&root)?,
                })),
                _ => Err(EngineError::InvalidRecord("Fetch published Branch head")),
            },
        )
        .transpose()
}

pub(crate) fn read_published_fetch_stack_head(
    connection: &Connection,
    id: LayerStackId,
) -> EngineResult<Option<Option<LayerStackHead>>> {
    connection
        .query_row(
            "SELECT published_generation, published_version_id, published_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = 'layer_stack' AND target_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(
            |(generation, layer, root)| match (generation, layer, root) {
                (None, None, None) => Ok(None),
                (Some(generation), Some(layer), Some(root)) => Ok(Some(LayerStackHead {
                    layer_stack_id: id,
                    generation: u64::try_from(generation)
                        .map_err(|_| EngineError::InvalidRecord("Fetch LayerStack generation"))?,
                    layer_id: LayerId(bytes32(&layer, "LayerId")?),
                    root: object_id(&root)?,
                })),
                _ => Err(EngineError::InvalidRecord(
                    "Fetch published LayerStack head",
                )),
            },
        )
        .transpose()
}

pub(crate) fn read_fetch_branch_head(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<BranchHead>> {
    let staged = connection
        .query_row(
            "SELECT staged_generation, staged_version_id, staged_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = 'branch' AND target_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some((generation, version, root)) = staged else {
        return read_database_branch_head(connection, id);
    };
    let head = BranchHead {
        branch_id: id,
        generation: u64::try_from(generation)
            .map_err(|_| EngineError::InvalidRecord("Fetch Branch generation"))?,
        operation_version_id: version
            .map(|bytes| bytes32(&bytes, "OperationVersionId").map(OperationVersionId))
            .transpose()?,
        root: object_id(&root)?,
    };
    if read_database_branch_head(connection, id)? != Some(head) {
        return Err(EngineError::InvalidRecord("Fetch staged Branch head"));
    }
    Ok(Some(head))
}

pub(crate) fn read_fetch_stack_head(
    connection: &Connection,
    id: LayerStackId,
) -> EngineResult<Option<LayerStackHead>> {
    let staged = connection
        .query_row(
            "SELECT staged_generation, staged_version_id, staged_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = 'layer_stack' AND target_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some((generation, layer, root)) = staged else {
        return read_database_layer_stack_head(connection, id);
    };
    let head = LayerStackHead {
        layer_stack_id: id,
        generation: u64::try_from(generation)
            .map_err(|_| EngineError::InvalidRecord("Fetch LayerStack generation"))?,
        layer_id: LayerId(bytes32(
            &layer.ok_or(EngineError::InvalidRecord("Fetch staged LayerStack head"))?,
            "LayerId",
        )?),
        root: object_id(&root)?,
    };
    if read_database_layer_stack_head(connection, id)? != Some(head) {
        return Err(EngineError::InvalidRecord("Fetch staged LayerStack head"));
    }
    Ok(Some(head))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn stage_fetch_head(
    connection: &Connection,
    target_kind: &str,
    target_id: &[u8; 32],
    durable_storage_id: [u8; 32],
    published_generation: Option<u64>,
    published_version_id: Option<Vec<u8>>,
    published_root: Option<ObjectId>,
    staged_generation: u64,
    staged_version_id: Option<Vec<u8>>,
    staged_root: ObjectId,
) -> EngineResult<()> {
    let published_generation = published_generation
        .map(i64::try_from)
        .transpose()
        .map_err(|_| EngineError::CounterOverflow)?;
    let staged_generation =
        i64::try_from(staged_generation).map_err(|_| EngineError::CounterOverflow)?;
    let incumbent = connection
        .query_row(
            "SELECT durable_storage_id, published_generation,
                    published_version_id, published_root_id
             FROM layerfs_fetch_staging_heads
             WHERE target_kind = ?1 AND target_id = ?2",
            params![target_kind, target_id],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let published_root = published_root.map(|root| root.as_bytes().to_vec());
    if let Some(incumbent) = incumbent {
        if incumbent.0.as_slice() != durable_storage_id
            || incumbent.1 != published_generation
            || incumbent.2 != published_version_id
            || incumbent.3 != published_root
        {
            return Err(EngineError::InvalidRecord("Fetch staging identity"));
        }
        connection
            .execute(
                "UPDATE layerfs_fetch_staging_heads
                 SET staged_generation = ?1, staged_version_id = ?2,
                     staged_root_id = ?3, updated_at = ?4
                 WHERE target_kind = ?5 AND target_id = ?6",
                params![
                    staged_generation,
                    staged_version_id,
                    staged_root.as_bytes(),
                    unix_seconds()?,
                    target_kind,
                    target_id,
                ],
            )
            .map_err(map_sqlite_error)?;
    } else {
        connection
            .execute(
                "INSERT INTO layerfs_fetch_staging_heads
                 (target_kind, target_id, durable_storage_id,
                  published_generation, published_version_id, published_root_id,
                  staged_generation, staged_version_id, staged_root_id, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    target_kind,
                    target_id,
                    durable_storage_id.as_slice(),
                    published_generation,
                    published_version_id,
                    published_root,
                    staged_generation,
                    staged_version_id,
                    staged_root.as_bytes(),
                    unix_seconds()?,
                ],
            )
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

pub(crate) fn stage_fetch_branch_head(
    connection: &Connection,
    durable_storage_id: [u8; 32],
    published: Option<BranchHead>,
    staged: BranchHead,
) -> EngineResult<()> {
    stage_fetch_head(
        connection,
        "branch",
        staged.branch_id.as_bytes(),
        durable_storage_id,
        published.map(|head| head.generation),
        published
            .and_then(|head| head.operation_version_id)
            .map(|id| id.0.to_vec()),
        published.map(|head| head.root),
        staged.generation,
        staged.operation_version_id.map(|id| id.0.to_vec()),
        staged.root,
    )
}

pub(crate) fn stage_fetch_stack_head(
    connection: &Connection,
    durable_storage_id: [u8; 32],
    published: Option<LayerStackHead>,
    staged: LayerStackHead,
) -> EngineResult<()> {
    stage_fetch_head(
        connection,
        "layer_stack",
        staged.layer_stack_id.as_bytes(),
        durable_storage_id,
        published.map(|head| head.generation),
        published.map(|head| head.layer_id.0.to_vec()),
        published.map(|head| head.root),
        staged.generation,
        Some(staged.layer_id.0.to_vec()),
        staged.root,
    )
}

pub(crate) fn verify_staged_child_merges(
    engine: &Engine,
    transfer_id: RequestId,
    branch_id: BranchId,
) -> EngineResult<()> {
    let maximum = engine
        .lock_connection()?
        .query_row(
            "SELECT MAX(page_sequence) FROM layerfs_branch_push_pages
             WHERE transfer_id = ?1 AND branch_id = ?2",
            params![transfer_id.as_bytes(), branch_id.as_bytes()],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map_err(map_sqlite_error)?
        .ok_or(EngineError::InvalidRecord("Push pages"))?;
    for sequence in 0..=maximum {
        let encoded = engine
            .lock_connection()?
            .query_row(
                "SELECT bundle FROM layerfs_branch_push_pages
                 WHERE transfer_id = ?1 AND branch_id = ?2 AND page_sequence = ?3",
                params![transfer_id.as_bytes(), branch_id.as_bytes(), sequence],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(map_sqlite_error)?;
        let bundle: BranchPushBundle = serde_json::from_slice(&encoded)
            .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
        for merge in &bundle.child_merges {
            let mut writer = engine.begin_candidate_write()?;
            let merged = layerfs_core::logical::merge_roots(
                &mut writer,
                merge.base_root,
                merge.source_root,
                merge.destination_root,
            )?
            .map_err(|_| EngineError::InvalidRecord("Push child merge conflict"))?;
            if merged.root() != merge.root {
                return Err(EngineError::InvalidRecord("Push child merge result"));
            }
            writer.commit_objects()?;
        }
    }
    Ok(())
}

pub(crate) fn insert_verified_fetch_rows(
    engine: &Engine,
    connection: &Connection,
    request: VerifiedFetchRequest,
    head: BranchHead,
    stack_head: LayerStackHead,
) -> EngineResult<bool> {
    if read_branch_head(connection, head.branch_id)? != Some(head) {
        return Err(EngineError::PublicationConflict);
    }
    authenticate_root(engine, connection, head.root)?;
    connection
        .execute(
            "INSERT INTO layerfs_durable_storages
             (durable_storage_id, authenticated_at) VALUES (?1, ?2)
             ON CONFLICT(durable_storage_id) DO UPDATE
             SET authenticated_at = excluded.authenticated_at",
            params![request.durable_storage_id.as_slice(), unix_seconds()?],
        )
        .map_err(map_sqlite_error)?;
    let incumbent = connection
        .query_row(
            "SELECT durable_storage_id, direction, candidate_kind, candidate_id,
                    expected_head_id, expected_generation, expected_root_id, result,
                    unique_bytes, resumed_bytes, retransmitted_bytes,
                    accepted_head_id, accepted_generation, accepted_root_id
             FROM layerfs_sync_receipts WHERE request_id = ?1",
            params![request.request_id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<Vec<u8>>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, Option<Vec<u8>>>(11)?,
                    row.get::<_, Option<i64>>(12)?,
                    row.get::<_, Option<Vec<u8>>>(13)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let expected_version = head
        .operation_version_id
        .map(|id| id.as_bytes().as_slice().to_vec());
    if let Some(incumbent) = &incumbent {
        if incumbent.0.as_slice() != request.durable_storage_id
            || incumbent.1 != "fetch"
            || incumbent.2 != "branch"
            || incumbent.3.as_slice() != head.branch_id.as_bytes()
            || incumbent.4 != expected_version
            || incumbent.5
                != Some(i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?)
            || incumbent.6.as_deref() != Some(head.root.as_bytes())
            || incumbent.7 != "fetched"
            || u64::try_from(incumbent.8).ok() != Some(request.counters.unique_bytes)
            || u64::try_from(incumbent.9).ok() != Some(request.counters.resumed_bytes)
            || u64::try_from(incumbent.10).ok() != Some(request.counters.retransmitted_bytes)
            || incumbent.11 != expected_version
            || incumbent.12
                != Some(i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?)
            || incumbent.13.as_deref() != Some(head.root.as_bytes())
        {
            return Err(EngineError::InvalidRecord(
                "Fetch request identity conflict",
            ));
        }
    } else {
        connection
            .execute(
                "INSERT INTO layerfs_sync_receipts
                 (request_id, durable_storage_id, direction, candidate_kind,
                  candidate_id, expected_head_id, expected_generation, expected_root_id, result,
                  accepted_head_id, accepted_generation, accepted_root_id,
                  unique_bytes, resumed_bytes, retransmitted_bytes,
                  reconciliation_result)
                 VALUES (?1, ?2, 'fetch', 'branch', ?3, ?4, ?5, ?6, 'fetched',
                         ?4, ?5, ?6, ?7, ?8, ?9, 'verified_complete')",
                params![
                    request.request_id.as_bytes(),
                    request.durable_storage_id.as_slice(),
                    head.branch_id.as_bytes(),
                    expected_version,
                    i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?,
                    head.root.as_bytes(),
                    i64::try_from(request.counters.unique_bytes)
                        .map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(request.counters.resumed_bytes)
                        .map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(request.counters.retransmitted_bytes)
                        .map_err(|_| EngineError::CounterOverflow)?,
                ],
            )
            .map_err(map_sqlite_error)?;
    }
    insert_verified_tracking_ref(
        connection,
        request,
        "branch",
        head.branch_id.as_bytes(),
        head.operation_version_id,
        head.generation,
        head.root,
    )?;
    if read_layer_stack_head(connection, stack_head.layer_stack_id)? != Some(stack_head) {
        return Err(EngineError::PublicationConflict);
    }
    authenticate_root(engine, connection, stack_head.root)?;
    insert_verified_tracking_ref(
        connection,
        request,
        "layer",
        stack_head.layer_id.as_bytes(),
        None,
        stack_head.generation,
        stack_head.root,
    )?;
    Ok(incumbent.is_some())
}
