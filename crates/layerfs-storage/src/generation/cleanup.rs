//! Owned prior, staging, and recovery-residue cleanup.

use super::create::StoreGenerationDriver;
use super::selector::{generation_filename, read_selector, read_selector_optional, StoreSelector};
use crate::{Engine, EngineError, EngineResult, FULL_SCHEMA, SCHEMA_VERSION};
use std::fs;
use std::path::Path;

pub(crate) fn remove_matching_legacy(
    legacy: &Path,
    selected: &Engine,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    if !legacy.exists() {
        return Ok(());
    }
    let identity = driver
        .file_identity(legacy)
        .map_err(crate::io_engine_error)?;
    let legacy_store = Engine::open(legacy)?;
    if legacy_store.store_id()? != selected.store_id()? {
        return Err(EngineError::InvalidRecord("legacy StoreId mismatch"));
    }
    drop(legacy_store);
    driver
        .remove_file_if_identity(legacy, &identity)
        .map_err(crate::io_engine_error)?;
    if let Some(parent) = legacy.parent() {
        driver
            .sync_directory(parent)
            .map_err(crate::io_engine_error)?;
    }
    Ok(())
}

pub(crate) fn recovery_residue_exists(directory: &Path) -> EngineResult<bool> {
    let selected = read_selector(&directory.join("CURRENT")).ok();
    let retained = selected
        .as_ref()
        .and_then(|selected| retained_selector(directory, selected).ok().flatten());
    let normal_generation = |name: &str| {
        selected
            .iter()
            .chain(retained.iter())
            .any(|selector| name == generation_filename(selector.generation).to_string_lossy())
    };
    for entry in fs::read_dir(directory).map_err(crate::io_engine_error)? {
        let name = entry
            .map_err(crate::io_engine_error)?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if name == "CURRENT.tmp"
            || name == "ROLLBACK.tmp"
            || (name == "ROLLBACK" && retained.is_none())
            || (name.starts_with(".layerfs-")
                && (name.ends_with(".sqlite") || name.contains(".sqlite-")))
        {
            return Ok(true);
        }
        if name.starts_with("generation-") && name.ends_with(".sqlite") && !normal_generation(&name)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn reject_unresolved_next_generation(
    directory: &Path,
    selected: &StoreSelector,
) -> EngineResult<()> {
    let generation = selected
        .generation
        .checked_add(1)
        .ok_or(EngineError::CounterOverflow)?;
    if directory.join(generation_filename(generation)).exists() {
        Err(EngineError::UnresolvedGenerationResidue { generation })
    } else {
        Ok(())
    }
}

pub(crate) fn cleanup_owned_residue(
    directory: &Path,
    selected: &StoreSelector,
    owned_prior: Option<u64>,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    let mut removed = false;
    let retained = retained_selector(directory, selected)?;
    let retained_generation = retained.as_ref().map(|retained| retained.generation);
    let temporary = directory.join("CURRENT.tmp");
    let temporary_identity = driver.file_identity(&temporary).ok();
    let temporary_selector = read_selector(&temporary).ok();
    if let Some(candidate) = &temporary_selector {
        if candidate.store_id == selected.store_id
            && candidate.profile_id == selected.profile_id
            && candidate.schema_version == selected.schema_version
            && selected
                .generation
                .checked_add(1)
                .is_some_and(|next| candidate.generation == next)
            && retained_generation != Some(candidate.generation)
            && owned_generation(directory, candidate)
        {
            let candidate_path = directory.join(generation_filename(candidate.generation));
            let candidate_identity = driver.file_identity(&candidate_path).ok();
            if let Some(identity) = candidate_identity {
                driver
                    .remove_file_if_identity(&candidate_path, &identity)
                    .map_err(crate::io_engine_error)?;
            }
            if let Some(identity) = temporary_identity.as_deref() {
                driver
                    .remove_file_if_identity(&temporary, identity)
                    .map_err(crate::io_engine_error)?;
            }
            removed = true;
        }
    }
    if let Some(generation) = owned_prior.filter(|generation| {
        ![selected.generation, retained_generation.unwrap_or(u64::MAX)].contains(generation)
    }) {
        let candidate = StoreSelector {
            generation,
            schema_version: selected.schema_version,
            store_id: selected.store_id,
            profile_id: selected.profile_id,
        };
        let path = directory.join(generation_filename(generation));
        let identity = driver.file_identity(&path).ok();
        if verified_owned_generation(directory, &candidate) {
            if let Some(identity) = identity {
                driver
                    .remove_file_if_identity(&path, &identity)
                    .map_err(crate::io_engine_error)?;
                removed = true;
            }
        }
    }
    if removed {
        driver
            .sync_directory(directory)
            .map_err(crate::io_engine_error)?;
    }
    Ok(())
}

fn retained_selector(
    directory: &Path,
    selected: &StoreSelector,
) -> EngineResult<Option<StoreSelector>> {
    let retained = read_selector_optional(&directory.join("ROLLBACK"))?;
    if retained.as_ref().is_some_and(|retained| {
        retained.store_id != selected.store_id
            || retained.profile_id != selected.profile_id
            || selected.schema_version != FULL_SCHEMA.schema_version as u32
            || !matches!(retained.schema_version, version
                if version == SCHEMA_VERSION as u32
                    || version == FULL_SCHEMA.schema_version as u32)
            || !(retained == selected
                || retained.generation.checked_add(1) == Some(selected.generation))
    }) {
        return Err(EngineError::InvalidRecord("retained generation selector"));
    }
    Ok(retained)
}

fn owned_generation(directory: &Path, candidate: &StoreSelector) -> bool {
    let path = directory.join(generation_filename(candidate.generation));
    if !path.is_file() {
        return false;
    }
    if candidate.schema_version == FULL_SCHEMA.schema_version as u32 {
        crate::FullStorage::open_durable(&path)
            .is_ok_and(|storage| storage.storage_id() == candidate.store_id)
    } else {
        crate::inspect_store_id_readonly(&path).is_ok_and(|store_id| store_id == candidate.store_id)
    }
}

fn verified_owned_generation(directory: &Path, candidate: &StoreSelector) -> bool {
    let path = directory.join(generation_filename(candidate.generation));
    if candidate.schema_version == FULL_SCHEMA.schema_version as u32 {
        crate::FullStorage::open_durable_verified(&path)
            .is_ok_and(|storage| storage.storage_id() == candidate.store_id)
    } else {
        Engine::open(&path).is_ok_and(|engine| engine.store_id().ok() == Some(candidate.store_id))
    }
}
