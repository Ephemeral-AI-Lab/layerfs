//! Working retained-root release after local-owner checks.

use crate::error::{map_sqlite_error, EngineResult};
use rusqlite::params;
use rusqlite::Connection;

pub(crate) fn release_retained_root_if_unreferenced(
    connection: &Connection,
    root: &[u8],
) -> EngineResult<()> {
    release_unreferenced_retained_roots(connection, Some(root))
}

pub(crate) fn release_unreferenced_retained_roots(
    connection: &Connection,
    root: Option<&[u8]>,
) -> EngineResult<()> {
    connection
        .execute(
            "DELETE FROM layerfs_retained_roots
             WHERE (?1 IS NULL OR root_id = ?1) AND NOT EXISTS (
                 SELECT 1 FROM (
                     SELECT root_id AS referenced_root FROM layerfs_refs
                     UNION ALL SELECT root_id FROM layerfs_roots
                     UNION ALL SELECT l.root_id FROM layerfs_layers l
                         WHERE l.state != 'dropped' AND NOT EXISTS(
                             SELECT 1 FROM layerfs_released_versions r
                             WHERE r.target_kind = 'layer'
                               AND r.owner_id = l.layer_stack_id
                               AND r.version_id = l.layer_id)
                     UNION ALL SELECT fork_root_id FROM layerfs_branches WHERE state = 'active'
                     UNION ALL SELECT v.root_id FROM layerfs_operation_versions v
                         WHERE NOT EXISTS(
                             SELECT 1 FROM layerfs_released_versions r
                             WHERE r.target_kind = 'operation_version'
                               AND r.owner_id = v.branch_id
                               AND r.version_id = v.operation_version_id)
                     UNION ALL SELECT candidate_root_id FROM layerfs_operations
                         WHERE candidate_root_id IS NOT NULL
                           AND state IN ('running', 'candidate', 'preserved', 'indeterminate')
                     UNION ALL SELECT published_root_id FROM layerfs_fetch_staging_heads
                         WHERE published_root_id IS NOT NULL
                     UNION ALL SELECT root_id FROM layerfs_durable_tracking_refs
                 ) WHERE referenced_root = layerfs_retained_roots.root_id
             )",
            params![root],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}
