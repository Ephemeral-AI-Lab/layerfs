//! Verified selector switching, adoption, and maintenance orchestration.

use super::cleanup::{
    cleanup_owned_residue, recovery_residue_exists, reject_unresolved_next_generation,
    remove_matching_legacy,
};
use super::create::{create_compaction_candidate, create_genesis, StoreGenerationDriver};
use super::selector::{
    full_selector, generation_filename, install, install_exact_selector, open_current,
    open_legacy_generation, open_selected, open_selected_full_durable_verified, read_selector,
    read_selector_optional, selector, StoreSelector,
};
use crate::full::compaction::VerifiedFullCopy;
use crate::integrity::IntegrityMode;
use crate::migration::VerifiedFullCandidate;
use crate::{Engine, EngineError, EngineResult, FullStorage, FULL_SCHEMA, SCHEMA_VERSION};
use std::fs;
use std::io;
use std::path::Path;

pub fn open_or_create_with_legacy(
    directory: &Path,
    legacy: &Path,
    driver: &dyn StoreGenerationDriver,
    mode: IntegrityMode,
) -> EngineResult<Engine> {
    fs::create_dir_all(directory).map_err(crate::io_engine_error)?;
    let current = directory.join("CURRENT");
    if current.exists() {
        let engine = open_or_create(directory, driver, mode)?;
        remove_matching_legacy(legacy, &engine, driver)?;
        return Ok(engine);
    }
    let generation = directory.join(generation_filename(0));
    if !legacy.exists() && !generation.exists() {
        return open_or_create(directory, driver, mode);
    }
    let maintenance = acquire_maintenance(directory)?;
    if current.exists() {
        drop(maintenance);
        let engine = open_or_create(directory, driver, mode)?;
        remove_matching_legacy(legacy, &engine, driver)?;
        return Ok(engine);
    }
    for entry in fs::read_dir(directory).map_err(crate::io_engine_error)? {
        let name = entry.map_err(crate::io_engine_error)?.file_name();
        if name != "MAINTENANCE.sqlite"
            && name != "MAINTENANCE.sqlite-journal"
            && name != "CURRENT.tmp"
            && name != generation_filename(0)
        {
            return Err(EngineError::InvalidRecord("legacy Store adoption residue"));
        }
    }
    let legacy_identity = if legacy.exists() {
        Some(
            driver
                .file_identity(legacy)
                .map_err(crate::io_engine_error)?,
        )
    } else {
        None
    };
    if !generation.exists() {
        fs::copy(legacy, &generation).map_err(crate::io_engine_error)?;
        fs::File::open(&generation)
            .and_then(|file| file.sync_all())
            .map_err(crate::io_engine_error)?;
        driver
            .sync_directory(directory)
            .map_err(crate::io_engine_error)?;
    }
    let candidate = Engine::open(&generation)?;
    if legacy.exists() {
        let legacy_store = Engine::open(legacy)?;
        if legacy_store.store_id()? != candidate.store_id()? {
            return Err(EngineError::InvalidRecord("legacy StoreId mismatch"));
        }
    }
    let selected = selector(&candidate, 0)?;
    drop(candidate);
    let prepared = directory.join("CURRENT.tmp");
    if prepared.exists() {
        if read_selector(&prepared)? != selected {
            return Err(EngineError::InvalidRecord("legacy selector residue"));
        }
        driver
            .install_selector(&prepared, &current)
            .map_err(crate::io_engine_error)?;
        driver
            .sync_directory(directory)
            .map_err(crate::io_engine_error)?;
    } else {
        install(directory, selected, None, driver)?;
    }
    drop(maintenance);
    let engine = open_current(directory, mode)?;
    if let Some(identity) = legacy_identity {
        driver
            .remove_file_if_identity(legacy, &identity)
            .map_err(crate::io_engine_error)?;
        if let Some(parent) = legacy.parent() {
            driver
                .sync_directory(parent)
                .map_err(crate::io_engine_error)?;
        }
    }
    Ok(engine)
}

pub fn open_or_create(
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
    mode: IntegrityMode,
) -> EngineResult<Engine> {
    fs::create_dir_all(directory).map_err(crate::io_engine_error)?;
    let current = directory.join("CURRENT");
    if current.exists() {
        if !recovery_residue_exists(directory)? {
            return open_current(directory, mode);
        }
        if let Some(maintenance) = try_acquire_maintenance(directory)? {
            let selected = read_selector(&current)?;
            let provisional = open_selected(directory, mode)?;
            crate::scratch::recover_owned_near(provisional.path(), selected.store_id, driver)?;
            cleanup_owned_residue(
                directory,
                &selected,
                selected.generation.checked_sub(1),
                driver,
            )?;
            reject_unresolved_next_generation(directory, &selected)?;
            drop(provisional);
            drop(maintenance);
            return open_current(directory, mode);
        }
        return open_current(directory, mode);
    }
    let maintenance = acquire_maintenance(directory)?;
    if current.exists() {
        drop(maintenance);
        return open_current(directory, mode);
    }
    for entry in fs::read_dir(directory).map_err(crate::io_engine_error)? {
        let name = entry.map_err(crate::io_engine_error)?.file_name();
        if name != "MAINTENANCE.sqlite" && name != "MAINTENANCE.sqlite-journal" {
            return Err(EngineError::InvalidRecord(
                "missing CURRENT in nonempty Store",
            ));
        }
    }
    create_genesis(directory, driver, mode)?;
    drop(maintenance);
    open_current(directory, mode)
}

pub fn compact(
    mut engine: Engine,
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<Engine> {
    let mode = engine.mode;
    engine.maintenance_pin.take();
    let maintenance = acquire_maintenance(directory)?;
    let prior = read_selector(&directory.join("CURRENT"))?;
    let selected = open_selected(directory, IntegrityMode::Verified)?;
    if selected.path() != engine.path()
        || engine.path() != directory.join(generation_filename(prior.generation))
        || selected.store_id()? != prior.store_id
        || engine.store_id()? != prior.store_id
    {
        return Err(EngineError::InvalidRecord("compaction source generation"));
    }
    crate::scratch::recover_owned_near(selected.path(), prior.store_id, driver)?;
    cleanup_owned_residue(directory, &prior, None, driver)?;
    reject_unresolved_next_generation(directory, &prior)?;
    let (next, observation) = create_compaction_candidate(engine, directory, &prior, driver)?;
    install(directory, next, Some(&prior), driver)?;
    let reopened = open_selected(directory, IntegrityMode::Verified)?;
    let selected = read_selector(&directory.join("CURRENT"))?;
    cleanup_owned_residue(directory, &selected, Some(prior.generation), driver)
        .map_err(|_| EngineError::AmbiguousDurability)?;
    driver
        .sync_directory(directory)
        .map_err(|_| EngineError::AmbiguousDurability)?;
    drop(maintenance);
    drop(reopened);
    let mut engine = open_current(directory, mode)?;
    engine.last_compaction = Some(observation);
    Ok(engine)
}

pub fn compact_full_durable(
    storage: FullStorage,
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<FullStorage> {
    compact_full_durable_inner(storage, directory, driver, &mut |_| Ok(()))
}

fn compact_full_durable_inner(
    storage: FullStorage,
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
    inject: &mut dyn FnMut(&'static str) -> EngineResult<()>,
) -> EngineResult<FullStorage> {
    let prior = read_selector(&directory.join("CURRENT"))?;
    validate_full_source(&storage, directory, &prior, driver)?;
    require_copy_space(storage.path(), directory, driver)?;
    let generation = prior
        .generation
        .checked_add(1)
        .ok_or(EngineError::CounterOverflow)?;
    let path = directory.join(generation_filename(generation));
    let candidate = if path.exists() {
        storage.verify_full_copy(&path)?
    } else {
        let candidate = storage.create_verified_full_copy(&path)?;
        driver
            .sync_directory(directory)
            .map_err(crate::io_engine_error)?;
        candidate
    };
    inject("candidate_verified")?;
    let requested = full_selector(candidate.storage(), generation)?;
    validate_copy_path(&candidate, &path, driver)?;
    drop(storage);
    let maintenance = acquire_maintenance(directory)?;
    let current = read_selector(&directory.join("CURRENT"))?;
    if current == requested {
        if read_selector(&directory.join("ROLLBACK"))? != prior {
            return Err(EngineError::InvalidRecord(
                "Full compaction rollback selector",
            ));
        }
        validate_copy_path(&candidate, &path, driver)?;
        drop(maintenance);
        return super::selector::open_current_full_durable(directory);
    }
    if current != prior {
        return Err(EngineError::InvalidRecord("Full compaction prior selector"));
    }
    validate_copy_path(&candidate, &path, driver)?;
    let retained = read_selector_optional(&directory.join("ROLLBACK"))?;
    if let Some(retained) = &retained {
        validate_retained_selector(directory, retained)?;
    }
    install_exact_selector(directory, "ROLLBACK", &prior, retained.as_ref(), driver)?;
    inject("rollback_visible")?;
    install_exact_selector(directory, "CURRENT", &requested, Some(&prior), driver)?;
    inject("current_visible")?;
    validate_copy_path(&candidate, &path, driver)?;
    drop(candidate);
    drop(maintenance);
    super::selector::open_current_full_durable(directory)
}

pub fn restore_full_durable_backup(
    backup: &Path,
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<FullStorage> {
    restore_full_durable_backup_inner(backup, directory, driver, &mut |_| Ok(()))
}

fn restore_full_durable_backup_inner(
    backup: &Path,
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
    inject: &mut dyn FnMut(&'static str) -> EngineResult<()>,
) -> EngineResult<FullStorage> {
    if !fs::symlink_metadata(backup)
        .map_err(crate::io_engine_error)?
        .file_type()
        .is_file()
    {
        return Err(EngineError::InvalidRecord("Full restore backup"));
    }
    let backup = FullStorage::open_durable_verified(
        fs::canonicalize(backup).map_err(crate::io_engine_error)?,
    )?;
    fs::create_dir_all(directory).map_err(crate::io_engine_error)?;
    let maintenance = acquire_maintenance(directory)?;
    validate_restore_directory(directory)?;
    let path = directory.join(generation_filename(0));
    let candidate = if path.exists() {
        backup.verify_full_copy(&path)?
    } else {
        let candidate = backup.create_verified_full_copy(&path)?;
        driver
            .sync_directory(directory)
            .map_err(crate::io_engine_error)?;
        candidate
    };
    inject("candidate_verified")?;
    let requested = full_selector(candidate.storage(), 0)?;
    validate_copy_path(&candidate, &path, driver)?;
    match read_selector_optional(&directory.join("CURRENT"))? {
        Some(current) if current == requested => {}
        Some(_) => return Err(EngineError::InvalidRecord("Full restore CURRENT")),
        None => install_exact_selector(directory, "CURRENT", &requested, None, driver)?,
    }
    inject("current_visible")?;
    if read_selector_optional(&directory.join("ROLLBACK"))?.is_some() {
        return Err(EngineError::InvalidRecord("Full restore rollback selector"));
    }
    validate_copy_path(&candidate, &path, driver)?;
    drop(candidate);
    drop(backup);
    drop(maintenance);
    super::selector::open_current_full_durable(directory)
}

fn validate_restore_directory(directory: &Path) -> EngineResult<()> {
    let generation = generation_filename(0);
    for entry in fs::read_dir(directory).map_err(crate::io_engine_error)? {
        let name = entry.map_err(crate::io_engine_error)?.file_name();
        if name != "MAINTENANCE.sqlite"
            && name != "MAINTENANCE.sqlite-journal"
            && name != "CURRENT"
            && name != "CURRENT.tmp"
            && name != generation
        {
            return Err(EngineError::InvalidRecord("Full restore residue"));
        }
    }
    Ok(())
}

fn validate_full_source(
    storage: &FullStorage,
    directory: &Path,
    selector: &StoreSelector,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    if selector.schema_version != FULL_SCHEMA.schema_version as u32
        || selector.store_id != storage.storage_id()
        || selector.profile_id != layerfs_core::namespace_codec::profile_id().to_bytes()
        || storage.path() != directory.join(generation_filename(selector.generation))
        || driver
            .file_identity(storage.path())
            .map_err(crate::io_engine_error)?
            != storage.owned_file_identity()?
    {
        return Err(EngineError::InvalidRecord("Full compaction source"));
    }
    Ok(())
}

fn validate_copy_path(
    copy: &VerifiedFullCopy,
    path: &Path,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    if copy.storage().path() != path
        || driver.file_identity(path).map_err(crate::io_engine_error)? != copy.file_identity()
    {
        return Err(EngineError::InvalidRecord("Full candidate path identity"));
    }
    Ok(())
}

fn validate_retained_selector(directory: &Path, retained: &StoreSelector) -> EngineResult<()> {
    if retained.profile_id != layerfs_core::namespace_codec::profile_id().to_bytes() {
        return Err(EngineError::InvalidRecord("retained generation profile"));
    }
    if retained.schema_version == FULL_SCHEMA.schema_version as u32 {
        let storage = FullStorage::open_durable_verified(
            directory.join(generation_filename(retained.generation)),
        )?;
        if storage.storage_id() == retained.store_id {
            return Ok(());
        }
    } else if retained.schema_version == SCHEMA_VERSION as u32 {
        drop(open_legacy_generation(
            directory,
            retained,
            IntegrityMode::Verified,
        )?);
        return Ok(());
    }
    Err(EngineError::InvalidRecord("retained generation identity"))
}

fn require_copy_space(
    source: &Path,
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    let source = fs::metadata(source).map_err(crate::io_engine_error)?.len();
    let required = source
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(8 * 1024 * 1024 + 2 * super::SELECTOR_BYTES as u64))
        .ok_or(EngineError::CounterOverflow)?;
    if driver
        .available_bytes(directory)
        .map_err(crate::io_engine_error)?
        < required
    {
        return Err(EngineError::Sqlite {
            kind: crate::SqliteErrorKind::NoSpace,
            message: format!("Full compaction requires {required} free bytes"),
        });
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn compact_full_durable_with_injector(
    storage: FullStorage,
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
    inject: &mut dyn FnMut(&'static str) -> EngineResult<()>,
) -> EngineResult<FullStorage> {
    compact_full_durable_inner(storage, directory, driver, inject)
}

pub(crate) fn install_verified_full_candidate_locked(
    directory: &Path,
    candidate: &VerifiedFullCandidate,
    driver: &dyn StoreGenerationDriver,
    _maintenance: &MaintenanceLock,
) -> EngineResult<FullStorage> {
    let requested = full_selector(candidate.storage(), candidate.generation())?;
    let prior = candidate.prior();
    if candidate.storage().path() != directory.join(generation_filename(candidate.generation())) {
        return Err(EngineError::InvalidRecord("Full candidate generation path"));
    }
    let current = read_selector(&directory.join("CURRENT"))?;
    if current == requested {
        let retained = read_selector(&directory.join("ROLLBACK"))?;
        if retained != *prior {
            return Err(EngineError::InvalidRecord("Full rollback selector"));
        }
        drop(open_legacy_generation(
            directory,
            &retained,
            IntegrityMode::Verified,
        )?);
        driver
            .sync_directory(directory)
            .map_err(|_| EngineError::AmbiguousDurability)?;
        return open_selected_full_durable_verified(directory);
    }
    if current != *prior
        || prior.schema_version != SCHEMA_VERSION as u32
        || prior.store_id != requested.store_id
        || prior.profile_id != requested.profile_id
        || prior.generation.checked_add(1) != Some(requested.generation)
    {
        return Err(EngineError::InvalidRecord(
            "Full migration selector identity",
        ));
    }
    drop(open_legacy_generation(
        directory,
        prior,
        IntegrityMode::Verified,
    )?);
    install_exact_selector(directory, "ROLLBACK", prior, None, driver)?;
    install_exact_selector(directory, "CURRENT", &requested, Some(prior), driver)?;
    let opened = open_selected_full_durable_verified(directory)
        .map_err(|_| EngineError::AmbiguousDurability)?;
    Ok(opened)
}

pub(crate) fn reconcile_selector_install(
    directory: &Path,
    current: &Path,
    requested: &StoreSelector,
    prior: Option<&StoreSelector>,
    error: io::Error,
) -> EngineResult<()> {
    match read_selector_optional(current) {
        Ok(Some(observed)) if observed == *requested => {
            let engine = open_selected(directory, IntegrityMode::Verified)
                .map_err(|_| EngineError::AmbiguousDurability)?;
            let verified = selector(&engine, requested.generation)
                .map_err(|_| EngineError::AmbiguousDurability)?;
            if verified != *requested {
                return Err(EngineError::AmbiguousDurability);
            }
            Ok(())
        }
        Ok(observed) if observed.as_ref() == prior => Err(crate::io_engine_error(error)),
        _ => Err(EngineError::AmbiguousDurability),
    }
}

#[cfg(test)]
pub(crate) use super::selector::pin_connection;
pub(crate) use super::selector::{acquire_maintenance, try_acquire_maintenance, MaintenanceLock};
