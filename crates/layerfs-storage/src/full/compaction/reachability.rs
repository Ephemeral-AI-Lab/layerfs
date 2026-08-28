//! Full retained-root reachability rows.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::record_id::{BranchId, LayerStackId, RequestId};
use crate::full::transfer::batch::PushedRelease;
use crate::working::compaction::reachability::{
    release_retained_root_if_unreferenced, release_unreferenced_retained_roots,
};
use layerfs_core::ObjectId;
use rusqlite::params;
use rusqlite::Connection;

pub(crate) fn retain_root(connection: &Connection, root: ObjectId) -> EngineResult<()> {
    connection
        .execute(
            "INSERT INTO layerfs_retained_roots (root_id) VALUES (?1)
             ON CONFLICT(root_id) DO NOTHING",
            params![root.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    release_unreferenced_retained_roots(connection, None)
}

pub(crate) fn record_pushed_release(
    connection: &Connection,
    target_kind: &'static str,
    owner_id: &[u8; 32],
    version_id: &[u8; 32],
    root: ObjectId,
    release: PushedRelease,
) -> EngineResult<()> {
    if !matches!(target_kind, "layer" | "operation_version") || release.generation == 0 {
        return Err(EngineError::InvalidRecord("release record"));
    }
    connection
        .execute(
            "INSERT INTO layerfs_released_versions
             (target_kind, owner_id, version_id, root_id, release_generation, request_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                target_kind,
                owner_id,
                version_id,
                root.as_bytes(),
                i64::try_from(release.generation).map_err(|_| EngineError::CounterOverflow)?,
                release.request_id.as_bytes(),
            ],
        )
        .map_err(map_sqlite_error)?;
    release_retained_root_if_unreferenced(connection, root.as_bytes())
}

pub(crate) fn record_branch_suffix_release(
    connection: &Connection,
    branch_id: BranchId,
    target_sequence: i64,
    current_sequence: i64,
    release_generation: u64,
    request_id: RequestId,
) -> EngineResult<()> {
    connection
        .execute(
            "INSERT OR IGNORE INTO layerfs_released_versions
             (target_kind, owner_id, version_id, root_id, release_generation, request_id)
             SELECT 'operation_version', branch_id, operation_version_id, root_id, ?1, ?2
             FROM layerfs_operation_versions
             WHERE branch_id = ?3 AND sequence > ?4 AND sequence <= ?5",
            params![
                i64::try_from(release_generation).map_err(|_| EngineError::CounterOverflow)?,
                request_id.as_bytes(),
                branch_id.as_bytes(),
                target_sequence,
                current_sequence,
            ],
        )
        .map_err(map_sqlite_error)?;
    release_unreferenced_retained_roots(connection, None)
}

pub(crate) fn record_layer_suffix_release(
    connection: &Connection,
    layer_stack_id: LayerStackId,
    target_generation: i64,
    current_generation: i64,
    release_generation: u64,
    request_id: RequestId,
) -> EngineResult<()> {
    connection
        .execute(
            "INSERT OR IGNORE INTO layerfs_released_versions
             (target_kind, owner_id, version_id, root_id, release_generation, request_id)
             SELECT 'layer', layer_stack_id, layer_id, root_id, ?1, ?2
             FROM layerfs_layers
             WHERE layer_stack_id = ?3
               AND accepted_generation > ?4 AND accepted_generation <= ?5",
            params![
                i64::try_from(release_generation).map_err(|_| EngineError::CounterOverflow)?,
                request_id.as_bytes(),
                layer_stack_id.as_bytes(),
                target_generation,
                current_generation,
            ],
        )
        .map_err(map_sqlite_error)?;
    release_unreferenced_retained_roots(connection, None)
}
