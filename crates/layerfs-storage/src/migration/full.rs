//! Quiescent legacy_full Durable row transformation into a side-by-side Full file.

use super::verify::{
    open_legacy_source, verify_full_candidate, verify_legacy_accepted_state, LegacySource,
};
use crate::full::record_id::full_release_id;
use crate::generation::{NativeGenerationDriver, StoreGenerationDriver};
use crate::{map_sqlite_error, EngineError, EngineResult, FullStorage};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub fn migrate_legacy_durable_file(
    source_path: impl AsRef<Path>,
    candidate_path: impl AsRef<Path>,
    expected_durable_storage_id: [u8; 32],
) -> EngineResult<FullStorage> {
    migrate(
        source_path.as_ref(),
        candidate_path.as_ref(),
        expected_durable_storage_id,
        &mut |_| Ok(()),
    )
}

pub(super) fn verify_existing_legacy_durable_file(
    source_path: &Path,
    candidate_path: &Path,
    expected_durable_storage_id: [u8; 32],
) -> EngineResult<FullStorage> {
    let source = open_legacy_source(source_path, expected_durable_storage_id)?;
    preflight_legacy_durable(&source)?;
    verify_legacy_accepted_state(&source)?;
    verify_full_candidate(&source, candidate_path)
}

fn migrate(
    source_path: &Path,
    candidate_path: &Path,
    expected_durable_storage_id: [u8; 32],
    inject: &mut dyn FnMut(&'static str) -> EngineResult<()>,
) -> EngineResult<FullStorage> {
    if candidate_path.exists() {
        return Err(EngineError::InvalidRecord(
            "Full migration candidate exists",
        ));
    }
    let source = open_legacy_source(source_path, expected_durable_storage_id)?;
    preflight_legacy_durable(&source)?;
    verify_legacy_accepted_state(&source)?;
    inject("source_verified")?;
    let candidate = FullStorage::create_durable_with_id(candidate_path, source.storage_id)?;
    let transformed = inject("candidate_created")
        .and_then(|_| transform(&source, &candidate, inject))
        .and_then(|_| inject("candidate_committed"));
    let candidate_identity = candidate.owned_file_identity();
    drop(candidate);
    if let Err(error) = transformed {
        if let Ok(identity) = candidate_identity {
            remove_owned_candidate(candidate_path, &identity);
        }
        return Err(error);
    }
    let candidate_identity = candidate_identity?;
    match verify_full_candidate(&source, candidate_path) {
        Ok(candidate) => match inject("candidate_verified") {
            Ok(()) => Ok(candidate),
            Err(error) => {
                drop(candidate);
                remove_owned_candidate(candidate_path, &candidate_identity);
                Err(error)
            }
        },
        Err(error) => {
            remove_owned_candidate(candidate_path, &candidate_identity);
            Err(error)
        }
    }
}

#[cfg(test)]
pub(crate) fn migrate_legacy_durable_file_fault(
    source_path: &Path,
    candidate_path: &Path,
    expected_durable_storage_id: [u8; 32],
    point: &'static str,
) -> EngineResult<FullStorage> {
    migrate(
        source_path,
        candidate_path,
        expected_durable_storage_id,
        &mut |observed| {
            if observed == point {
                Err(EngineError::InjectedFailure(point))
            } else {
                Ok(())
            }
        },
    )
}

#[cfg(test)]
pub(crate) fn migrate_legacy_durable_file_with_injector(
    source_path: &Path,
    candidate_path: &Path,
    expected_durable_storage_id: [u8; 32],
    inject: &mut dyn FnMut(&'static str) -> EngineResult<()>,
) -> EngineResult<FullStorage> {
    migrate(
        source_path,
        candidate_path,
        expected_durable_storage_id,
        inject,
    )
}

fn preflight_legacy_durable(source: &LegacySource) -> EngineResult<()> {
    let connection = &source.connection;
    for (table, predicate) in [
        ("layerfs_refs", "1"),
        ("layerfs_roots", "1"),
        ("layerfs_push_outbox", "1"),
        ("layerfs_fetch_staging_heads", "1"),
        ("layerfs_fetch_closure_items", "1"),
        ("layerfs_branch_push_pages", "1"),
        ("layerfs_deltas", "format_version = 0"),
        ("layerfs_operations", "state != 'durably_accepted'"),
        ("layerfs_layers", "state != 'accepted'"),
        ("layerfs_transfer_state", "state != 'complete'"),
        (
            "layerfs_durable_tracking_refs",
            "status != 'verified_complete'",
        ),
    ] {
        let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {predicate})");
        if connection
            .query_row(&sql, [], |row| row.get::<_, bool>(0))
            .map_err(map_sqlite_error)?
        {
            return Err(EngineError::InvalidRecord(
                "unquiesced legacy Durable state",
            ));
        }
    }
    let visible = connection
        .query_row(
            "SELECT visible_root IS NOT NULL FROM layerfs_store_meta WHERE store_id = 1",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)?;
    if visible {
        return Err(EngineError::InvalidRecord("legacy visible root"));
    }
    for sql in [
        "SELECT EXISTS(SELECT 1 FROM layerfs_durable_storages
         WHERE durable_storage_id != ?1)",
        "SELECT EXISTS(SELECT 1 FROM layerfs_durable_tracking_refs
         WHERE durable_storage_id != ?1)",
        "SELECT EXISTS(SELECT 1 FROM layerfs_sync_receipts
         WHERE durable_storage_id != ?1)",
    ] {
        if connection
            .query_row(sql, params![source.storage_id], |row| row.get::<_, bool>(0))
            .map_err(map_sqlite_error)?
        {
            return Err(EngineError::InvalidRecord("foreign Durable identity"));
        }
    }
    require_exact_fold_sources(connection)
}

fn require_exact_fold_sources(connection: &Connection) -> EngineResult<()> {
    let invalid_operation_fold = connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM layerfs_operation_versions AS v
                 WHERE (v.created_by_kind = 'operation' AND
                           (SELECT count(*) FROM layerfs_operation_deltas AS d
                            WHERE d.operation_version_id = v.operation_version_id
                              AND d.operation_id = v.created_by_operation_id) != 1)
                    OR (v.created_by_kind = 'child_merge' AND
                           (EXISTS(SELECT 1 FROM layerfs_operation_deltas AS d
                                   WHERE d.operation_version_id = v.operation_version_id)
                            OR NOT EXISTS(
                                SELECT 1 FROM layerfs_branch_deltas AS b
                                WHERE b.branch_delta_id = v.created_by_branch_delta_id
                                  AND b.purpose = 'child_merge'
                                  AND b.source_branch_id = v.created_by_child_branch_id
                                  AND b.result_root = v.root_id)))
             ) OR EXISTS(
                 SELECT 1 FROM layerfs_operation_deltas AS d
                 WHERE NOT EXISTS(
                     SELECT 1 FROM layerfs_operation_versions AS v
                     WHERE v.operation_version_id = d.operation_version_id
                       AND v.created_by_kind = 'operation'
                       AND v.created_by_operation_id = d.operation_id))",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)?;
    let invalid_layer_fold = connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM layerfs_layers AS l
                 WHERE (l.creation_kind = 'genesis' AND EXISTS(
                           SELECT 1 FROM layerfs_layer_deltas AS d
                           WHERE d.candidate_layer_id = l.layer_id))
                    OR (l.creation_kind = 'candidate' AND
                           (SELECT count(*) FROM layerfs_layer_deltas AS d
                            WHERE d.candidate_layer_id = l.layer_id
                              AND d.parent_layer_id = l.parent_layer_id
                              AND d.result_root = l.root_id) != 1)
             ) OR EXISTS(
                 SELECT 1 FROM layerfs_layer_deltas AS d
                 WHERE NOT EXISTS(SELECT 1 FROM layerfs_layers AS l
                                  WHERE l.layer_id = d.candidate_layer_id
                                    AND l.creation_kind = 'candidate'))",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)?;
    if invalid_operation_fold || invalid_layer_fold {
        Err(EngineError::InvalidRecord("legacy Full fold source"))
    } else {
        Ok(())
    }
}

fn transform(
    source: &LegacySource,
    candidate: &FullStorage,
    inject: &mut dyn FnMut(&'static str) -> EngineResult<()>,
) -> EngineResult<()> {
    let connection = candidate.lock_connection()?;
    connection
        .execute(
            "ATTACH DATABASE ?1 AS legacy",
            params![source
                .path
                .to_str()
                .ok_or(EngineError::InvalidRecord("legacy migration path"))?],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute_batch("BEGIN")
        .map_err(map_sqlite_error)?;
    let result = (|| {
        copy_kernel_and_history(&connection)?;
        inject("history_copied")?;
        copy_releases(&connection)?;
        inject("releases_copied")?;
        copy_sync(&connection)?;
        inject("sync_copied")?;
        rebuild_tracking_membership(source, &connection)?;
        inject("membership_rebuilt")?;
        let violation = connection
            .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
            .optional()
            .map_err(|_| EngineError::SchemaMismatch)?;
        if violation.is_some() {
            return Err(EngineError::SchemaMismatch);
        }
        inject("foreign_keys_verified")?;
        connection.execute_batch("COMMIT").map_err(map_sqlite_error)
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK");
    }
    let detached = connection
        .execute_batch("DETACH DATABASE legacy")
        .map_err(map_sqlite_error);
    result.and(detached)
}

fn copy_kernel_and_history(connection: &Connection) -> EngineResult<()> {
    connection
        .execute_batch(
            "UPDATE main.layerfs_store_meta
             SET next_inode_serial = (SELECT next_inode_serial FROM legacy.layerfs_authority
                                      WHERE authority_id = 1),
                 trusted_history = 0;
             INSERT INTO main.layerfs_objects
             SELECT rowid, object_id, kind, canonical_length, canonical_bytes
             FROM legacy.layerfs_objects;
             INSERT INTO main.layerfs_deltas
             SELECT delta_id, format_version, parent_root, child_root, payload
             FROM legacy.layerfs_deltas;
             INSERT INTO main.layerfs_retained_roots SELECT * FROM legacy.layerfs_retained_roots;
             INSERT INTO main.layerfs_layer_stacks SELECT * FROM legacy.layerfs_layer_stacks;
             INSERT INTO main.layerfs_branch_deltas
             SELECT branch_delta_id, purpose, source_branch_id, source_branch_generation,
                    source_branch_operation_version_id, base_root, source_root,
                    destination_root, result_root, source_delta_id, applied_delta_id
             FROM legacy.layerfs_branch_deltas;
             INSERT INTO main.layerfs_branches SELECT * FROM legacy.layerfs_branches;
             INSERT INTO main.layerfs_operations SELECT * FROM legacy.layerfs_operations;
             INSERT INTO main.layerfs_operation_versions
             SELECT v.operation_version_id, v.branch_id, v.sequence,
                    v.parent_operation_version_id, v.created_by_kind,
                    v.created_by_operation_id, v.created_by_child_branch_id,
                    v.created_by_branch_delta_id,
                    CASE WHEN v.created_by_kind = 'operation' THEN d.transition_delta_id
                         ELSE b.applied_delta_id END,
                    CASE WHEN v.created_by_kind = 'operation' THEN d.base_root
                         ELSE b.destination_root END,
                    CASE WHEN v.created_by_kind = 'operation' THEN d.result_root
                         ELSE b.result_root END
             FROM legacy.layerfs_operation_versions AS v
             LEFT JOIN legacy.layerfs_operation_deltas AS d
               ON d.operation_version_id = v.operation_version_id
              AND d.operation_id = v.created_by_operation_id
             LEFT JOIN legacy.layerfs_branch_deltas AS b
               ON b.branch_delta_id = v.created_by_branch_delta_id;
             INSERT INTO main.layerfs_layers
             SELECT l.layer_id, l.layer_stack_id, l.parent_layer_id, l.root_id,
                    l.creation_kind, l.source_branch_id, l.source_branch_depth,
                    l.source_branch_generation, l.source_branch_head_operation_version_id,
                    l.source_branch_delta_id, d.transition_delta_id, d.parent_root,
                    l.state, l.prepared_request_id, l.accepted_generation
             FROM legacy.layerfs_layers AS l
             LEFT JOIN legacy.layerfs_layer_deltas AS d
               ON d.candidate_layer_id = l.layer_id;
             INSERT INTO main.layerfs_branch_transitions
             SELECT * FROM legacy.layerfs_branch_transitions;
             INSERT INTO main.layerfs_layer_stack_transitions
             SELECT * FROM legacy.layerfs_layer_stack_transitions;
             INSERT INTO main.layerfs_version_leases
             SELECT v.lease_id, v.target_kind,
                    CASE WHEN v.target_kind = 'layer' THEN l.layer_stack_id END,
                    CASE WHEN v.target_kind = 'layer' THEN v.target_id END,
                    CASE WHEN v.target_kind = 'operation_version' THEN o.branch_id END,
                    CASE WHEN v.target_kind = 'operation_version' THEN v.target_id END,
                    v.owner_kind, v.owner_id, v.created_at, v.expires_at
             FROM legacy.layerfs_version_leases AS v
             LEFT JOIN legacy.layerfs_layers AS l
               ON v.target_kind = 'layer' AND l.layer_id = v.target_id
             LEFT JOIN legacy.layerfs_operation_versions AS o
               ON v.target_kind = 'operation_version'
              AND o.operation_version_id = v.target_id;",
        )
        .map_err(map_sqlite_error)
}

fn copy_releases(connection: &Connection) -> EngineResult<()> {
    let mut after: Option<(String, Vec<u8>, Vec<u8>)> = None;
    loop {
        let row = connection
            .query_row(
                "SELECT target_kind, owner_id, version_id, root_id,
                        release_generation, request_id
                 FROM legacy.layerfs_released_versions
                 WHERE ?1 IS NULL OR target_kind > ?1
                    OR (target_kind = ?1 AND owner_id > ?2)
                    OR (target_kind = ?1 AND owner_id = ?2 AND version_id > ?3)
                 ORDER BY target_kind, owner_id, version_id LIMIT 1",
                params![
                    after.as_ref().map(|value| value.0.as_str()),
                    after.as_ref().map(|value| value.1.as_slice()),
                    after.as_ref().map(|value| value.2.as_slice()),
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let Some(row) = row else { return Ok(()) };
        let owner: [u8; 32] = row
            .1
            .as_slice()
            .try_into()
            .map_err(|_| EngineError::SchemaMismatch)?;
        let version: [u8; 32] = row
            .2
            .as_slice()
            .try_into()
            .map_err(|_| EngineError::SchemaMismatch)?;
        let release_id = full_release_id(&row.0, &owner, &version)?;
        let (stack, layer, branch, operation) = if row.0 == "layer" {
            (Some(&row.1), Some(&row.2), None, None)
        } else {
            (None, None, Some(&row.1), Some(&row.2))
        };
        connection
            .execute(
                "INSERT INTO main.layerfs_released_versions
                 (release_id, target_kind, layer_stack_id, layer_id, branch_id,
                  operation_version_id, root_id, release_generation, request_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![release_id, row.0, stack, layer, branch, operation, row.3, row.4, row.5],
            )
            .map_err(map_sqlite_error)?;
        after = Some((row.0, row.1, row.2));
    }
}

fn copy_sync(connection: &Connection) -> EngineResult<()> {
    connection
        .execute_batch(
            "INSERT INTO main.layerfs_transfer_state SELECT * FROM legacy.layerfs_transfer_state;
             INSERT INTO main.layerfs_sync_object_pins SELECT * FROM legacy.layerfs_sync_object_pins;
             INSERT INTO main.layerfs_sync_batch_receipts
             SELECT * FROM legacy.layerfs_sync_batch_receipts;
             INSERT INTO main.layerfs_sync_receipts
             SELECT request_id, durable_storage_id, direction, candidate_kind, candidate_id,
                    identity_version, transfer_id, candidate_digest, expected_head_id,
                    expected_generation, expected_root_id,
                    CASE WHEN result IN ('fetched', 'durably_accepted') THEN 1 ELSE 0 END,
                    accepted_head_id, accepted_generation, accepted_root_id, result,
                    unique_bytes, resumed_bytes, retransmitted_bytes, reconciliation_result
             FROM legacy.layerfs_sync_receipts;
             INSERT INTO main.layerfs_durable_tracking_refs
             (tracking_ref_id, store_id, target_kind, target_id, target_version_id,
              generation, root_id, verification_receipt_id, status)
             SELECT tracking_ref_id, 1, target_kind, target_id, target_version_id,
                    generation, root_id, verification_receipt_id, status
             FROM legacy.layerfs_durable_tracking_refs
             WHERE status = 'verified_complete';",
        )
        .map_err(map_sqlite_error)
}

fn rebuild_tracking_membership(source: &LegacySource, candidate: &Connection) -> EngineResult<()> {
    let mut after: Option<Vec<u8>> = None;
    loop {
        let row = source
            .connection
            .query_row(
                "SELECT tracking_ref_id, root_id FROM layerfs_durable_tracking_refs
                 WHERE status = 'verified_complete'
                   AND (?1 IS NULL OR tracking_ref_id > ?1)
                 ORDER BY tracking_ref_id LIMIT 1",
                params![after.as_deref()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let Some((tracking_ref, root)) = row else {
            return Ok(());
        };
        let root = layerfs_core::ObjectId::from_bytes(&root).map_err(EngineError::Core)?;
        crate::integrity::authenticated_closure_for_each(
            &source.connection,
            &source.path,
            source.storage_id,
            [root],
            |object| {
                candidate
                    .execute(
                        "INSERT INTO main.layerfs_fetch_closure_items
                         (tracking_ref_id, object_id, created_at) VALUES (?1, ?2, 0)",
                        params![tracking_ref, object.as_bytes()],
                    )
                    .map_err(map_sqlite_error)?;
                Ok(())
            },
        )?;
        after = Some(tracking_ref);
    }
}

fn remove_owned_candidate(path: &Path, identity: &[u8]) {
    let _ = NativeGenerationDriver.remove_file_if_identity(path, identity);
}
