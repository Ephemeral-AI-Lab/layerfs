//! Independent legacy-source and typed Full candidate qualification.

use crate::generation::selector::{
    generation_filename, install_exact_selector, open_current_full_durable, open_legacy_generation,
    open_selected_full_durable_verified, read_selector,
};
use crate::generation::switch::{acquire_maintenance, install_verified_full_candidate_locked};
use crate::generation::{StoreGenerationDriver, StoreSelector};
use crate::integrity::IntegrityMode;
use crate::sqlite::admission::admit_legacy_full_migration_source;
use crate::{
    configure_profile_counted, map_sqlite_error, EngineError, EngineResult, FullStorage, StoreRole,
    BUSY_TIMEOUT,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct VerifiedFullCandidate {
    storage: FullStorage,
    generation: u64,
    prior: StoreSelector,
}

impl VerifiedFullCandidate {
    pub(crate) fn storage(&self) -> &FullStorage {
        &self.storage
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn prior(&self) -> &StoreSelector {
        &self.prior
    }
}

pub(super) struct LegacySource {
    pub connection: Connection,
    pub path: PathBuf,
    pub storage_id: [u8; 32],
}

pub fn migrate_selected_legacy_durable_generation(
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<FullStorage> {
    let maintenance = acquire_maintenance(directory)?;
    let prior = read_selector(&directory.join("CURRENT"))?;
    if prior.schema_version == crate::FULL_SCHEMA.schema_version as u32 {
        let retained = read_selector(&directory.join("ROLLBACK"))?;
        if retained.schema_version != crate::SCHEMA_VERSION as u32
            || retained.generation.checked_add(1) != Some(prior.generation)
            || retained.store_id != prior.store_id
            || retained.profile_id != prior.profile_id
        {
            return Err(EngineError::InvalidRecord("Full rollback selector"));
        }
        let source = directory.join(generation_filename(retained.generation));
        let selected = directory.join(generation_filename(prior.generation));
        drop(super::full::verify_existing_legacy_durable_file(
            &source,
            &selected,
            prior.store_id,
        )?);
        drop(maintenance);
        return open_current_full_durable(directory);
    }
    if prior.schema_version != crate::SCHEMA_VERSION as u32 {
        return Err(EngineError::SchemaMismatch);
    }
    let source = directory.join(generation_filename(prior.generation));
    let selected = open_legacy_generation(directory, &prior, IntegrityMode::Verified)?;
    if selected.path() != source || selected.store_id()? != prior.store_id {
        return Err(EngineError::InvalidRecord("migration source selector"));
    }
    drop(selected);
    let generation = prior
        .generation
        .checked_add(1)
        .ok_or(EngineError::CounterOverflow)?;
    let final_path = directory.join(generation_filename(generation));
    let staging = directory.join(format!(".layerfs-full-migration-{generation:016x}.sqlite"));
    if final_path.exists() && staging.exists() {
        return Err(EngineError::UnresolvedGenerationResidue { generation });
    }
    if staging.exists() {
        match super::full::verify_existing_legacy_durable_file(&source, &staging, prior.store_id) {
            Ok(storage) => drop(storage),
            Err(error) => {
                let storage = FullStorage::open_durable(&staging).map_err(|_| error)?;
                if storage.storage_id() != prior.store_id {
                    return Err(EngineError::InvalidRecord("migration staging StoreId"));
                }
                let identity = storage.owned_file_identity()?;
                drop(storage);
                driver
                    .remove_file_if_identity(&staging, &identity)
                    .map_err(crate::io_engine_error)?;
                driver
                    .sync_directory(directory)
                    .map_err(crate::io_engine_error)?;
            }
        }
    }
    if !final_path.exists() {
        if !staging.exists() {
            drop(super::full::migrate_legacy_durable_file(
                &source,
                &staging,
                prior.store_id,
            )?);
        }
        let storage =
            super::full::verify_existing_legacy_durable_file(&source, &staging, prior.store_id)?;
        let identity = storage.owned_file_identity()?;
        drop(storage);
        fs::File::open(&staging)
            .and_then(|file| file.sync_all())
            .map_err(crate::io_engine_error)?;
        if driver
            .file_identity(&staging)
            .map_err(crate::io_engine_error)?
            != identity
        {
            return Err(EngineError::InvalidRecord("migration staging identity"));
        }
        fs::rename(&staging, &final_path).map_err(crate::io_engine_error)?;
        driver
            .sync_directory(directory)
            .map_err(crate::io_engine_error)?;
    }
    let storage =
        super::full::verify_existing_legacy_durable_file(&source, &final_path, prior.store_id)?;
    let sealed = VerifiedFullCandidate {
        storage,
        generation,
        prior,
    };
    drop(install_verified_full_candidate_locked(
        directory,
        &sealed,
        driver,
        &maintenance,
    )?);
    drop(sealed);
    drop(maintenance);
    open_current_full_durable(directory)
}

pub fn rollback_selected_full_generation(
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<crate::Engine> {
    let maintenance = acquire_maintenance(directory)?;
    let current = read_selector(&directory.join("CURRENT"))?;
    let retained = read_selector(&directory.join("ROLLBACK"))?;
    let full_generation = if current.schema_version == crate::FULL_SCHEMA.schema_version as u32 {
        current.generation
    } else if current == retained {
        retained
            .generation
            .checked_add(1)
            .ok_or(EngineError::CounterOverflow)?
    } else {
        return Err(EngineError::InvalidRecord("Full rollback CURRENT"));
    };
    if retained.schema_version != crate::SCHEMA_VERSION as u32
        || retained.generation.checked_add(1) != Some(full_generation)
        || retained.store_id != current.store_id
        || retained.profile_id != current.profile_id
    {
        return Err(EngineError::InvalidRecord("Full rollback selector"));
    }
    let legacy_path = directory.join(generation_filename(retained.generation));
    let full_path = directory.join(generation_filename(full_generation));
    let full = if current.schema_version == crate::FULL_SCHEMA.schema_version as u32 {
        let full = open_selected_full_durable_verified(directory)?;
        drop(super::full::verify_existing_legacy_durable_file(
            &legacy_path,
            &full_path,
            current.store_id,
        )?);
        install_exact_selector(directory, "CURRENT", &retained, Some(&current), driver)?;
        full
    } else if current == retained && full_path.exists() {
        super::full::verify_existing_legacy_durable_file(
            &legacy_path,
            &full_path,
            current.store_id,
        )?
    } else {
        return Err(EngineError::InvalidRecord("Full rollback candidate"));
    };
    drop(open_legacy_generation(
        directory,
        &retained,
        IntegrityMode::Verified,
    )?);
    let full_identity = full.owned_file_identity()?;
    drop(full);
    driver
        .remove_file_if_identity(&full_path, &full_identity)
        .map_err(crate::io_engine_error)?;
    let rollback_path = directory.join("ROLLBACK");
    let rollback_identity = driver
        .file_identity(&rollback_path)
        .map_err(crate::io_engine_error)?;
    driver
        .remove_file_if_identity(&rollback_path, &rollback_identity)
        .map_err(crate::io_engine_error)?;
    driver
        .sync_directory(directory)
        .map_err(crate::io_engine_error)?;
    drop(maintenance);
    crate::generation::open_current(directory, IntegrityMode::Verified)
}

pub(super) fn open_legacy_source(
    path: &Path,
    expected_storage_id: [u8; 32],
) -> EngineResult<LegacySource> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(map_sqlite_error)?;
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(map_sqlite_error)?;
    let storage_id = admit_legacy_full_migration_source(&connection)?;
    let mut statements = 0;
    configure_profile_counted(&connection, &mut statements)?;
    if storage_id != expected_storage_id {
        return Err(EngineError::SchemaMismatch);
    }
    connection
        .execute_batch("BEGIN")
        .map_err(map_sqlite_error)?;
    connection
        .query_row(
            "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
            [],
            |_| Ok(()),
        )
        .map_err(map_sqlite_error)?;
    require_integrity_check(&connection)?;
    require_foreign_keys(&connection)?;
    crate::integrity::full::object::authenticate_object_table(&connection)?;
    Ok(LegacySource {
        connection,
        path: path.to_owned(),
        storage_id,
    })
}

pub(super) fn verify_legacy_accepted_state(source: &LegacySource) -> EngineResult<()> {
    crate::full::compaction::verify::verify_product_integrity(&source.connection)?;
    let statements = Cell::new(0);
    let failed = Cell::new(crate::integrity::VerificationObservation::default());
    crate::integrity::verify_retained_union_observed_counted(
        &source.connection,
        &source.path,
        source.storage_id,
        &statements,
        &failed,
    )?;
    Ok(())
}

pub(super) fn verify_full_candidate(
    source: &LegacySource,
    candidate_path: &Path,
) -> EngineResult<FullStorage> {
    let candidate = FullStorage::open_durable_verified(candidate_path)?;
    if candidate.role() != StoreRole::Durable
        || candidate.storage_id() != source.storage_id
        || candidate.durable_storage_id() != source.storage_id
    {
        return Err(EngineError::SchemaMismatch);
    }
    let connection = candidate.lock_connection()?;
    compare_source_candidate(&connection, &source.path)?;
    drop(connection);
    Ok(candidate)
}

fn require_integrity_check(connection: &Connection) -> EngineResult<()> {
    let result = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(map_sqlite_error)?;
    if result == "ok" {
        Ok(())
    } else {
        Err(EngineError::SchemaMismatch)
    }
}

fn require_foreign_keys(connection: &Connection) -> EngineResult<()> {
    if connection
        .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
        .optional()
        .map_err(map_sqlite_error)?
        .is_some()
    {
        Err(EngineError::SchemaMismatch)
    } else {
        Ok(())
    }
}

fn compare_source_candidate(connection: &Connection, source: &Path) -> EngineResult<()> {
    connection
        .execute(
            "ATTACH DATABASE ?1 AS verification_source",
            params![source
                .to_str()
                .ok_or(EngineError::InvalidRecord("legacy verification path"))?],
        )
        .map_err(map_sqlite_error)?;
    let result = require_equivalent_rows(connection);
    connection
        .execute_batch("DETACH DATABASE verification_source")
        .map_err(map_sqlite_error)?;
    result
}

fn require_equivalent_rows(connection: &Connection) -> EngineResult<()> {
    for (source, candidate) in [
        (
            "SELECT next_inode_serial FROM verification_source.layerfs_authority",
            "SELECT next_inode_serial FROM main.layerfs_store_meta",
        ),
        (
            "SELECT rowid, object_id, kind, canonical_length, canonical_bytes
             FROM verification_source.layerfs_objects",
            "SELECT rowid, object_id, kind, canonical_length, canonical_bytes
             FROM main.layerfs_objects",
        ),
        (
            "SELECT delta_id, format_version, parent_root, child_root, payload
             FROM verification_source.layerfs_deltas",
            "SELECT delta_id, format_version, parent_root_id, result_root_id, payload
             FROM main.layerfs_deltas",
        ),
        (
            "SELECT * FROM verification_source.layerfs_retained_roots",
            "SELECT * FROM main.layerfs_retained_roots",
        ),
        (
            "SELECT * FROM verification_source.layerfs_layer_stacks",
            "SELECT * FROM main.layerfs_layer_stacks",
        ),
        (
            "SELECT branch_delta_id, purpose, source_branch_id, source_branch_generation,
                    source_branch_operation_version_id, base_root, source_root,
                    destination_root, result_root, source_delta_id, applied_delta_id
             FROM verification_source.layerfs_branch_deltas",
            "SELECT branch_delta_id, purpose, source_branch_id, source_branch_generation,
                    source_operation_version_id, base_root_id, source_root_id,
                    destination_root_id, result_root_id, source_delta_id, applied_delta_id
             FROM main.layerfs_branch_deltas",
        ),
        (
            "SELECT * FROM verification_source.layerfs_branches",
            "SELECT * FROM main.layerfs_branches",
        ),
        (
            "SELECT * FROM verification_source.layerfs_operations",
            "SELECT * FROM main.layerfs_operations",
        ),
        (
            "SELECT v.operation_version_id, v.branch_id, v.sequence,
                    v.parent_operation_version_id, v.created_by_kind,
                    v.created_by_operation_id, v.created_by_child_branch_id,
                    v.created_by_branch_delta_id,
                    CASE WHEN v.created_by_kind = 'operation' THEN d.transition_delta_id
                         ELSE b.applied_delta_id END,
                    CASE WHEN v.created_by_kind = 'operation' THEN d.base_root
                         ELSE b.destination_root END,
                    CASE WHEN v.created_by_kind = 'operation' THEN d.result_root
                         ELSE b.result_root END
             FROM verification_source.layerfs_operation_versions AS v
             LEFT JOIN verification_source.layerfs_operation_deltas AS d
               ON d.operation_version_id = v.operation_version_id
              AND d.operation_id = v.created_by_operation_id
             LEFT JOIN verification_source.layerfs_branch_deltas AS b
               ON b.branch_delta_id = v.created_by_branch_delta_id",
            "SELECT * FROM main.layerfs_operation_versions",
        ),
        (
            "SELECT l.layer_id, l.layer_stack_id, l.parent_layer_id, l.root_id,
                    l.creation_kind, l.source_branch_id, l.source_branch_depth,
                    l.source_branch_generation, l.source_branch_head_operation_version_id,
                    l.source_branch_delta_id, d.transition_delta_id, d.parent_root,
                    l.state, l.prepared_request_id, l.accepted_generation
             FROM verification_source.layerfs_layers AS l
             LEFT JOIN verification_source.layerfs_layer_deltas AS d
               ON d.candidate_layer_id = l.layer_id",
            "SELECT * FROM main.layerfs_layers",
        ),
        (
            "SELECT * FROM verification_source.layerfs_branch_transitions",
            "SELECT * FROM main.layerfs_branch_transitions",
        ),
        (
            "SELECT * FROM verification_source.layerfs_layer_stack_transitions",
            "SELECT * FROM main.layerfs_layer_stack_transitions",
        ),
        (
            "SELECT v.lease_id, v.target_kind,
                    CASE WHEN v.target_kind = 'layer' THEN l.layer_stack_id END,
                    CASE WHEN v.target_kind = 'layer' THEN v.target_id END,
                    CASE WHEN v.target_kind = 'operation_version' THEN o.branch_id END,
                    CASE WHEN v.target_kind = 'operation_version' THEN v.target_id END,
                    v.owner_kind, v.owner_id, v.created_at, v.expires_at
             FROM verification_source.layerfs_version_leases AS v
             LEFT JOIN verification_source.layerfs_layers AS l
               ON v.target_kind = 'layer' AND l.layer_id = v.target_id
             LEFT JOIN verification_source.layerfs_operation_versions AS o
               ON v.target_kind = 'operation_version'
              AND o.operation_version_id = v.target_id",
            "SELECT * FROM main.layerfs_version_leases",
        ),
        (
            "SELECT target_kind, owner_id, version_id, root_id,
                    release_generation, request_id
             FROM verification_source.layerfs_released_versions",
            "SELECT target_kind, COALESCE(layer_stack_id, branch_id),
                    COALESCE(layer_id, operation_version_id), root_id,
                    release_generation, request_id
             FROM main.layerfs_released_versions",
        ),
        (
            "SELECT * FROM verification_source.layerfs_transfer_state",
            "SELECT * FROM main.layerfs_transfer_state",
        ),
        (
            "SELECT * FROM verification_source.layerfs_sync_object_pins",
            "SELECT * FROM main.layerfs_sync_object_pins",
        ),
        (
            "SELECT * FROM verification_source.layerfs_sync_batch_receipts",
            "SELECT * FROM main.layerfs_sync_batch_receipts",
        ),
        (
            "SELECT request_id, durable_storage_id, direction, candidate_kind, candidate_id,
                    identity_version, transfer_id, candidate_digest, expected_head_id,
                    expected_generation, expected_root_id,
                    CASE WHEN result IN ('fetched', 'durably_accepted') THEN 1 ELSE 0 END,
                    accepted_head_id, accepted_generation, accepted_root_id, result,
                    unique_bytes, resumed_bytes, retransmitted_bytes, reconciliation_result
             FROM verification_source.layerfs_sync_receipts",
            "SELECT * FROM main.layerfs_sync_receipts",
        ),
        (
            "SELECT tracking_ref_id, target_kind, target_id, target_version_id,
                    generation, root_id, verification_receipt_id, status
             FROM verification_source.layerfs_durable_tracking_refs
             WHERE status = 'verified_complete'",
            "SELECT tracking_ref_id, target_kind, target_id, target_version_id,
                    generation, root_id, verification_receipt_id, status
             FROM main.layerfs_durable_tracking_refs",
        ),
    ] {
        let sql = format!(
            "SELECT EXISTS({source} EXCEPT {candidate})
             OR EXISTS({candidate} EXCEPT {source})"
        );
        if connection
            .query_row(&sql, [], |row| row.get::<_, bool>(0))
            .map_err(map_sqlite_error)?
        {
            return Err(EngineError::InvalidRecord("Full migration equivalence"));
        }
    }
    Ok(())
}
