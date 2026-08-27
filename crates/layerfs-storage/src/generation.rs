//! Durable installation boundary for Store generations.

use crate::integrity::IntegrityMode;
use crate::{Engine, EngineError, EngineResult, SCHEMA_VERSION};
use rusqlite::{Connection, OptionalExtension};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SELECTOR_BYTES: usize = 154;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreSelector {
    pub generation: u64,
    pub schema_version: u32,
    pub store_id: [u8; 32],
    pub profile_id: [u8; 32],
}

impl StoreSelector {
    pub fn encode(&self) -> [u8; SELECTOR_BYTES] {
        let filename = format!("generation-{:016x}.sqlite", self.generation);
        let mut bytes = [0_u8; SELECTOR_BYTES];
        bytes[..8].copy_from_slice(b"LFSCUR1\0");
        bytes[8..10].copy_from_slice(&1_u16.to_be_bytes());
        bytes[10..18].copy_from_slice(&self.generation.to_be_bytes());
        bytes[18..20].copy_from_slice(&34_u16.to_be_bytes());
        bytes[20..54].copy_from_slice(filename.as_bytes());
        bytes[54..58].copy_from_slice(&self.schema_version.to_be_bytes());
        bytes[58..90].copy_from_slice(&self.store_id);
        bytes[90..122].copy_from_slice(&self.profile_id);
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/store-current/v1\0");
        hasher.update(&bytes[..122]);
        bytes[122..].copy_from_slice(hasher.finalize().as_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != SELECTOR_BYTES
            || &bytes[..8] != b"LFSCUR1\0"
            || u16::from_be_bytes(bytes[8..10].try_into().unwrap()) != 1
            || u16::from_be_bytes(bytes[18..20].try_into().unwrap()) != 34
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid Store selector framing",
            ));
        }
        let generation = u64::from_be_bytes(bytes[10..18].try_into().unwrap());
        let filename = std::str::from_utf8(&bytes[20..54]).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid Store selector filename",
            )
        })?;
        if filename != format!("generation-{generation:016x}.sqlite") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Store selector generation mismatch",
            ));
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/store-current/v1\0");
        hasher.update(&bytes[..122]);
        if hasher.finalize().as_bytes() != &bytes[122..] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Store selector checksum mismatch",
            ));
        }
        Ok(Self {
            generation,
            schema_version: u32::from_be_bytes(bytes[54..58].try_into().unwrap()),
            store_id: bytes[58..90].try_into().unwrap(),
            profile_id: bytes[90..122].try_into().unwrap(),
        })
    }
}

/// Installs an already-created, synced selector and syncs its containing directory.
///
/// Implementations must reconcile a potentially visible replacement before a caller
/// attempts another installation.
pub trait StoreGenerationDriver: Send + Sync {
    fn available_bytes(&self, directory: &Path) -> io::Result<u64>;
    fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()>;
    fn sync_directory(&self, directory: &Path) -> io::Result<()>;
    fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()>;
}

pub struct NativeGenerationDriver;

impl StoreGenerationDriver for NativeGenerationDriver {
    fn available_bytes(&self, directory: &Path) -> io::Result<u64> {
        let output = Command::new("df").arg("-Pk").arg(directory).output()?;
        if !output.status.success() {
            return Err(io::Error::other("df failed"));
        }
        let output = std::str::from_utf8(&output.stdout)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "df output"))?;
        let available = output
            .lines()
            .rfind(|line| !line.trim().is_empty())
            .and_then(|line| line.split_whitespace().nth(3))
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "df available bytes"))?;
        available
            .checked_mul(1024)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "df overflow"))
    }

    fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
        fs::rename(prepared, current)
    }

    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        fs::File::open(directory)?.sync_all()
    }

    fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
        native_file_identity(path)
    }

    fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
        if native_file_identity(path)?.as_slice() != expected {
            return Err(io::Error::other("file identity changed"));
        }
        fs::remove_file(path)
    }
}

#[cfg(unix)]
fn native_file_identity(path: &Path) -> io::Result<Vec<u8>> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::other("generation path is not a regular file"));
    }
    let mut identity = Vec::with_capacity(24);
    identity.extend_from_slice(&metadata.dev().to_be_bytes());
    identity.extend_from_slice(&metadata.ino().to_be_bytes());
    identity.extend_from_slice(&metadata.len().to_be_bytes());
    Ok(identity)
}

#[cfg(not(unix))]
fn native_file_identity(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    let modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut identity = Vec::new();
    identity.extend_from_slice(&metadata.len().to_be_bytes());
    identity.extend_from_slice(&modified.as_nanos().to_be_bytes());
    Ok(identity)
}

pub fn open_or_create_with_legacy(
    directory: &Path,
    legacy: &Path,
    driver: &dyn StoreGenerationDriver,
    mode: IntegrityMode,
) -> EngineResult<Engine> {
    fs::create_dir_all(directory).map_err(super::io_engine_error)?;
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
    for entry in fs::read_dir(directory).map_err(super::io_engine_error)? {
        let name = entry.map_err(super::io_engine_error)?.file_name();
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
                .map_err(super::io_engine_error)?,
        )
    } else {
        None
    };
    if !generation.exists() {
        fs::copy(legacy, &generation).map_err(super::io_engine_error)?;
        fs::File::open(&generation)
            .and_then(|file| file.sync_all())
            .map_err(super::io_engine_error)?;
        driver
            .sync_directory(directory)
            .map_err(super::io_engine_error)?;
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
            .map_err(super::io_engine_error)?;
        driver
            .sync_directory(directory)
            .map_err(super::io_engine_error)?;
    } else {
        install(directory, selected, None, driver)?;
    }
    drop(maintenance);
    let engine = open_current(directory, mode)?;
    if let Some(identity) = legacy_identity {
        driver
            .remove_file_if_identity(legacy, &identity)
            .map_err(super::io_engine_error)?;
        if let Some(parent) = legacy.parent() {
            driver
                .sync_directory(parent)
                .map_err(super::io_engine_error)?;
        }
    }
    Ok(engine)
}

fn remove_matching_legacy(
    legacy: &Path,
    selected: &Engine,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    if !legacy.exists() {
        return Ok(());
    }
    let identity = driver
        .file_identity(legacy)
        .map_err(super::io_engine_error)?;
    let legacy_store = Engine::open(legacy)?;
    if legacy_store.store_id()? != selected.store_id()? {
        return Err(EngineError::InvalidRecord("legacy StoreId mismatch"));
    }
    drop(legacy_store);
    driver
        .remove_file_if_identity(legacy, &identity)
        .map_err(super::io_engine_error)?;
    if let Some(parent) = legacy.parent() {
        driver
            .sync_directory(parent)
            .map_err(super::io_engine_error)?;
    }
    Ok(())
}

pub fn open_or_create(
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
    mode: IntegrityMode,
) -> EngineResult<Engine> {
    fs::create_dir_all(directory).map_err(super::io_engine_error)?;
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
    for entry in fs::read_dir(directory).map_err(super::io_engine_error)? {
        let name = entry.map_err(super::io_engine_error)?.file_name();
        if name != "MAINTENANCE.sqlite" && name != "MAINTENANCE.sqlite-journal" {
            return Err(EngineError::InvalidRecord(
                "missing CURRENT in nonempty Store",
            ));
        }
    }
    let generation = directory.join(generation_filename(0));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&generation)
        .map_err(super::io_engine_error)?;
    let mut custody = GenesisCustody {
        generation: generation.clone(),
        current: current.clone(),
        temporary: directory.join("CURRENT.tmp"),
        directory,
        driver,
        armed: true,
    };
    let engine = Engine::open_with_mode(&generation, mode)?;
    fs::File::open(&generation)
        .and_then(|file| file.sync_all())
        .map_err(super::io_engine_error)?;
    let selector = selector(&engine, 0)?;
    install(directory, selector, None, driver)?;
    custody.armed = false;
    drop(engine);
    drop(maintenance);
    open_current(directory, mode)
}

fn recovery_residue_exists(directory: &Path) -> EngineResult<bool> {
    let mut generations = 0_u8;
    for entry in fs::read_dir(directory).map_err(super::io_engine_error)? {
        let name = entry
            .map_err(super::io_engine_error)?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if name == "CURRENT.tmp"
            || (name.starts_with(".layerfs-")
                && (name.ends_with(".sqlite") || name.contains(".sqlite-")))
        {
            return Ok(true);
        }
        if name.starts_with("generation-") && name.ends_with(".sqlite") {
            generations = generations.saturating_add(1);
            if generations > 1 {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

struct GenesisCustody<'a> {
    generation: PathBuf,
    current: PathBuf,
    temporary: PathBuf,
    directory: &'a Path,
    driver: &'a dyn StoreGenerationDriver,
    armed: bool,
}

impl Drop for GenesisCustody<'_> {
    fn drop(&mut self) {
        if self.armed && !self.current.exists() {
            let _ = fs::remove_file(&self.temporary);
            let _ = fs::remove_file(&self.generation);
            let mut journal = self.generation.as_os_str().to_os_string();
            journal.push("-journal");
            let _ = fs::remove_file(PathBuf::from(journal));
            let _ = self.driver.sync_directory(self.directory);
        }
    }
}

pub fn open_current(directory: &Path, mode: IntegrityMode) -> EngineResult<Engine> {
    let pin = pin_connection(directory)?;
    let mut engine = open_selected(directory, mode)?;
    engine.maintenance_pin = Some(std::sync::Mutex::new(pin));
    Ok(engine)
}

fn open_selected(directory: &Path, mode: IntegrityMode) -> EngineResult<Engine> {
    let selector = read_selector(&directory.join("CURRENT"))?;
    if selector.schema_version != SCHEMA_VERSION as u32
        || selector.profile_id != layerfs_core::namespace_codec::profile_id().to_bytes()
    {
        return Err(EngineError::ProfileMismatch);
    }
    let selected_path = directory.join(generation_filename(selector.generation));
    if !selected_path.is_file() {
        return Err(EngineError::InvalidRecord("selected generation missing"));
    }
    let engine = Engine::open_with_mode(&selected_path, mode)?;
    if engine.store_id()? != selector.store_id {
        return Err(EngineError::InvalidRecord("selector StoreId"));
    }
    Ok(engine)
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
    drop(selected);
    let source_bytes = fs::metadata(engine.path())
        .map_err(super::io_engine_error)?
        .len();
    let required_bytes = source_bytes
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add(8 * 1024 * 1024 + SELECTOR_BYTES as u64))
        .ok_or(EngineError::CounterOverflow)?;
    if driver
        .available_bytes(directory)
        .map_err(super::io_engine_error)?
        < required_bytes
    {
        return Err(EngineError::Sqlite {
            kind: crate::SqliteErrorKind::NoSpace,
            message: format!(
                "compaction requires {required_bytes} free bytes for candidate, mark, journal, and selector"
            ),
        });
    }
    let generation = prior
        .generation
        .checked_add(1)
        .ok_or(EngineError::CounterOverflow)?;
    let candidate_path = directory.join(generation_filename(generation));
    let observation = engine.compact_to_observed(&candidate_path)?;
    let candidate = Engine::open(&candidate_path)?;
    let next = selector(&candidate, generation)?;
    drop(candidate);
    drop(engine);
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

fn reject_unresolved_next_generation(
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

fn selector(engine: &Engine, generation: u64) -> EngineResult<StoreSelector> {
    Ok(StoreSelector {
        generation,
        schema_version: SCHEMA_VERSION as u32,
        store_id: engine.store_id()?,
        profile_id: layerfs_core::namespace_codec::profile_id().to_bytes(),
    })
}

fn install(
    directory: &Path,
    selector: StoreSelector,
    prior: Option<&StoreSelector>,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    let prepared = directory.join("CURRENT.tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&prepared)
        .map_err(super::io_engine_error)?;
    file.write_all(&selector.encode())
        .and_then(|_| file.sync_all())
        .map_err(super::io_engine_error)?;
    drop(file);
    let current = directory.join("CURRENT");
    if let Err(error) = driver.install_selector(&prepared, &current) {
        reconcile_selector_install(directory, &current, &selector, prior, error)?;
    }
    if let Err(error) = driver.sync_directory(directory) {
        let _ = error;
        let _ = read_selector_optional(&current);
        return Err(EngineError::AmbiguousDurability);
    }
    Ok(())
}

fn reconcile_selector_install(
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
        Ok(observed) if observed.as_ref() == prior => Err(super::io_engine_error(error)),
        _ => Err(EngineError::AmbiguousDurability),
    }
}

fn read_selector(path: &Path) -> EngineResult<StoreSelector> {
    read_selector_io(path).map_err(super::io_engine_error)
}

fn read_selector_optional(path: &Path) -> EngineResult<Option<StoreSelector>> {
    match read_selector_io(path) {
        Ok(selector) => Ok(Some(selector)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(super::io_engine_error(error)),
    }
}

fn read_selector_io(path: &Path) -> io::Result<StoreSelector> {
    let mut file = fs::File::open(path)?;
    if file.metadata()?.len() != SELECTOR_BYTES as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Store selector length",
        ));
    }
    let mut bytes = [0_u8; SELECTOR_BYTES];
    file.read_exact(&mut bytes)?;
    let mut extra = [0_u8; 1];
    if file.read(&mut extra)? != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Store selector grew while reading",
        ));
    }
    StoreSelector::decode(&bytes)
}

fn generation_filename(generation: u64) -> PathBuf {
    PathBuf::from(format!("generation-{generation:016x}.sqlite"))
}

fn cleanup_owned_residue(
    directory: &Path,
    selected: &StoreSelector,
    owned_prior: Option<u64>,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    let mut removed = false;
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
            && owned_generation(directory, candidate)
        {
            let candidate_path = directory.join(generation_filename(candidate.generation));
            let candidate_identity = driver.file_identity(&candidate_path).ok();
            if let Some(identity) = candidate_identity {
                driver
                    .remove_file_if_identity(&candidate_path, &identity)
                    .map_err(super::io_engine_error)?;
            }
            if let Some(identity) = temporary_identity.as_deref() {
                driver
                    .remove_file_if_identity(&temporary, identity)
                    .map_err(super::io_engine_error)?;
            }
            removed = true;
        }
    }
    if let Some(generation) = owned_prior.filter(|generation| *generation != selected.generation) {
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
                    .map_err(super::io_engine_error)?;
                removed = true;
            }
        }
    }
    if removed {
        driver
            .sync_directory(directory)
            .map_err(super::io_engine_error)?;
    }
    Ok(())
}

fn owned_generation(directory: &Path, candidate: &StoreSelector) -> bool {
    let path = directory.join(generation_filename(candidate.generation));
    if !path.is_file() {
        return false;
    }
    crate::inspect_store_id_readonly(&path).is_ok_and(|store_id| store_id == candidate.store_id)
}

fn verified_owned_generation(directory: &Path, candidate: &StoreSelector) -> bool {
    let path = directory.join(generation_filename(candidate.generation));
    Engine::open(&path).is_ok_and(|engine| engine.store_id().ok() == Some(candidate.store_id))
}

struct MaintenanceLock {
    connection: Option<Connection>,
}

impl Drop for MaintenanceLock {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.execute_batch("ROLLBACK");
        }
    }
}

fn try_acquire_maintenance(directory: &Path) -> EngineResult<Option<MaintenanceLock>> {
    let connection = maintenance_connection(directory)?;
    match connection.execute_batch("BEGIN EXCLUSIVE") {
        Ok(()) => Ok(Some(MaintenanceLock {
            connection: Some(connection),
        })),
        Err(error)
            if matches!(
                super::sqlite_error_kind(&error),
                crate::SqliteErrorKind::Busy | crate::SqliteErrorKind::Locked
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(super::map_sqlite_error(error)),
    }
}

fn acquire_maintenance(directory: &Path) -> EngineResult<MaintenanceLock> {
    try_acquire_maintenance(directory)?.ok_or_else(|| EngineError::Sqlite {
        kind: crate::SqliteErrorKind::Busy,
        message: "Store maintenance is already active".to_owned(),
    })
}

fn maintenance_connection(directory: &Path) -> EngineResult<Connection> {
    let path = directory.join("MAINTENANCE.sqlite");
    let initialize = !path.exists();
    let connection = Connection::open(path).map_err(super::map_sqlite_error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(super::map_sqlite_error)?;
    if initialize {
        connection
            .execute_batch(
                "CREATE TABLE maintenance_guard (
                id INTEGER PRIMARY KEY CHECK (id = 1)
             );
             INSERT INTO maintenance_guard (id) VALUES (1);",
            )
            .map_err(super::map_sqlite_error)?;
    } else {
        preflight_maintenance(&connection)?;
    }
    configure_maintenance(&connection)?;
    Ok(connection)
}

fn preflight_maintenance(connection: &Connection) -> EngineResult<()> {
    let schema = connection
        .query_row(
            "SELECT sql FROM sqlite_schema
             WHERE type = 'table' AND name = 'maintenance_guard'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(super::map_sqlite_error)?
        .ok_or(EngineError::SchemaMismatch)?;
    if super::schema_shape(&schema)
        != super::schema_shape(
            "CREATE TABLE maintenance_guard (
                id INTEGER PRIMARY KEY CHECK (id = 1)
             )",
        )
        || connection
            .query_row(
                "SELECT COUNT(*), MIN(id), MAX(id) FROM maintenance_guard",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .map_err(super::map_sqlite_error)?
            != (1, Some(1), Some(1))
        || connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(super::map_sqlite_error)?
            != 1
    {
        return Err(EngineError::SchemaMismatch);
    }
    Ok(())
}

fn configure_maintenance(connection: &Connection) -> EngineResult<()> {
    let journal = connection
        .query_row("PRAGMA journal_mode=DELETE", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(super::map_sqlite_error)?;
    connection
        .execute_batch("PRAGMA synchronous=FULL; PRAGMA temp_store=FILE; PRAGMA mmap_size=0;")
        .map_err(super::map_sqlite_error)?;
    let synchronous = connection
        .query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))
        .map_err(super::map_sqlite_error)?;
    let temp_store = connection
        .query_row("PRAGMA temp_store", [], |row| row.get::<_, i64>(0))
        .map_err(super::map_sqlite_error)?;
    let mmap = connection
        .query_row("PRAGMA mmap_size", [], |row| row.get::<_, i64>(0))
        .map_err(super::map_sqlite_error)?;
    if !journal.eq_ignore_ascii_case("delete") || synchronous != 2 || temp_store != 1 || mmap != 0 {
        return Err(EngineError::ProfileMismatch);
    }
    Ok(())
}

fn pin_connection(directory: &Path) -> EngineResult<Connection> {
    let connection = maintenance_connection(directory)?;
    connection
        .execute_batch("BEGIN")
        .map_err(super::map_sqlite_error)?;
    connection
        .query_row("SELECT id FROM maintenance_guard WHERE id = 1", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(super::map_sqlite_error)?;
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_core::{encode_bytes_object, ObjectId};
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    struct MemoryDriver(Arc<Mutex<Vec<&'static str>>>);

    #[test]
    fn scratch_crash_child() {
        let Some(store) = std::env::var_os("LAYERFS_SCRATCH_CRASH_STORE") else {
            return;
        };
        let table =
            crate::scratch::DiskTable::create_near(Path::new(&store), "crash-child").unwrap();
        table.put(b"pending", &vec![0xa5; 64 * 1024]).unwrap();
        std::process::exit(91);
    }

    #[test]
    fn unselected_generation_crash_child() {
        let Some(directory) = std::env::var_os("LAYERFS_UNSELECTED_GENERATION_CRASH") else {
            return;
        };
        let directory = PathBuf::from(directory);
        let engine = open_current(&directory, IntegrityMode::Verified).unwrap();
        engine
            .compact_to(&directory.join(generation_filename(1)))
            .unwrap();
        std::process::exit(93);
    }

    struct NativeDriver;
    fn test_file_identity(path: &Path) -> io::Result<Vec<u8>> {
        Ok(path.as_os_str().to_string_lossy().into_owned().into_bytes())
    }
    fn test_remove_file(path: &Path, _expected: &[u8]) -> io::Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
    impl StoreGenerationDriver for NativeDriver {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(u64::MAX)
        }
        fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
            fs::rename(prepared, current)
        }
        fn sync_directory(&self, directory: &Path) -> io::Result<()> {
            fs::File::open(directory)?.sync_all()
        }
        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            test_file_identity(path)
        }
        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            test_remove_file(path, expected)
        }
    }

    struct AdvancesSelectorDuringInstall;
    impl StoreGenerationDriver for AdvancesSelectorDuringInstall {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(u64::MAX)
        }
        fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
            fs::rename(prepared, current)?;
            let directory = current
                .parent()
                .ok_or_else(|| io::Error::other("missing selector parent"))?;
            let selected =
                read_selector(current).map_err(|error| io::Error::other(error.to_string()))?;
            let next_generation = selected
                .generation
                .checked_add(1)
                .ok_or_else(|| io::Error::other("generation overflow"))?;
            let next_path = directory.join(generation_filename(next_generation));
            fs::copy(
                directory.join(generation_filename(selected.generation)),
                &next_path,
            )?;
            let next_engine = Engine::open_with_mode(&next_path, IntegrityMode::TrustedLocalDev)
                .map_err(|error| io::Error::other(error.to_string()))?;
            let next = selector(&next_engine, next_generation)
                .map_err(|error| io::Error::other(error.to_string()))?;
            drop(next_engine);
            let raced = directory.join("CURRENT.raced");
            fs::write(&raced, next.encode())?;
            fs::rename(raced, current)
        }
        fn sync_directory(&self, directory: &Path) -> io::Result<()> {
            fs::File::open(directory)?.sync_all()
        }
        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            test_file_identity(path)
        }
        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            test_remove_file(path, expected)
        }
    }

    struct LostBeforeVisibility;
    impl StoreGenerationDriver for LostBeforeVisibility {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(u64::MAX)
        }
        fn install_selector(&self, _prepared: &Path, _current: &Path) -> io::Result<()> {
            Err(io::Error::other("injected before selector replace"))
        }
        fn sync_directory(&self, _directory: &Path) -> io::Result<()> {
            Ok(())
        }
        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            test_file_identity(path)
        }
        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            test_remove_file(path, expected)
        }
    }

    struct LostAfterVisibility;
    impl StoreGenerationDriver for LostAfterVisibility {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(u64::MAX)
        }
        fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
            fs::rename(prepared, current)?;
            Err(io::Error::other("injected lost selector acknowledgement"))
        }
        fn sync_directory(&self, directory: &Path) -> io::Result<()> {
            fs::File::open(directory)?.sync_all()
        }
        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            test_file_identity(path)
        }
        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            test_remove_file(path, expected)
        }
    }

    struct NoSpace;
    impl StoreGenerationDriver for NoSpace {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(0)
        }
        fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
            fs::rename(prepared, current)
        }
        fn sync_directory(&self, directory: &Path) -> io::Result<()> {
            fs::File::open(directory)?.sync_all()
        }
        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            test_file_identity(path)
        }
        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            test_remove_file(path, expected)
        }
    }

    struct LostFirstDirectorySync(AtomicBool);
    impl StoreGenerationDriver for LostFirstDirectorySync {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(u64::MAX)
        }
        fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
            fs::rename(prepared, current)
        }
        fn sync_directory(&self, directory: &Path) -> io::Result<()> {
            if !self.0.swap(true, Ordering::AcqRel) {
                Err(io::Error::other("injected selector directory sync"))
            } else {
                fs::File::open(directory)?.sync_all()
            }
        }
        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            test_file_identity(path)
        }
        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            test_remove_file(path, expected)
        }
    }

    struct LostCleanupDirectorySync(AtomicUsize);
    impl StoreGenerationDriver for LostCleanupDirectorySync {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(u64::MAX)
        }
        fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
            fs::rename(prepared, current)
        }
        fn sync_directory(&self, directory: &Path) -> io::Result<()> {
            if self.0.fetch_add(1, Ordering::AcqRel) == 1 {
                Err(io::Error::other("injected cleanup directory sync"))
            } else {
                fs::File::open(directory)?.sync_all()
            }
        }
        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            test_file_identity(path)
        }
        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            test_remove_file(path, expected)
        }
    }

    struct SubstituteOnRemove(AtomicBool);
    impl StoreGenerationDriver for SubstituteOnRemove {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(u64::MAX)
        }
        fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
            fs::rename(prepared, current)
        }
        fn sync_directory(&self, directory: &Path) -> io::Result<()> {
            fs::File::open(directory)?.sync_all()
        }
        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            fs::read(path)
        }
        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            if !self.0.swap(true, Ordering::AcqRel) {
                fs::rename(path, path.with_extension("custody-saved"))?;
                fs::write(path, b"substitute")?;
            }
            if fs::read(path)? != expected {
                return Err(io::Error::other("identity changed"));
            }
            fs::remove_file(path)
        }
    }

    impl StoreGenerationDriver for MemoryDriver {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(u64::MAX)
        }
        fn install_selector(&self, _prepared: &Path, _current: &Path) -> io::Result<()> {
            self.0.lock().unwrap().push("install");
            Ok(())
        }

        fn sync_directory(&self, _directory: &Path) -> io::Result<()> {
            self.0.lock().unwrap().push("sync");
            Ok(())
        }
        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            test_file_identity(path)
        }
        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            test_remove_file(path, expected)
        }
    }

    #[test]
    fn port_is_object_safe_and_preserves_install_then_sync_order() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let driver: Box<dyn StoreGenerationDriver> = Box::new(MemoryDriver(calls.clone()));
        driver
            .install_selector(Path::new("CURRENT.tmp"), Path::new("CURRENT"))
            .unwrap();
        driver.sync_directory(Path::new(".")).unwrap();
        assert_eq!(calls.lock().unwrap().as_slice(), ["install", "sync"]);
    }

    #[test]
    fn selector_is_exact_154_bytes_and_strictly_checksummed() {
        let selector = StoreSelector {
            generation: 7,
            schema_version: 1,
            store_id: [3; 32],
            profile_id: [4; 32],
        };
        let bytes = selector.encode();
        assert_eq!(bytes.len(), 154);
        assert_eq!(&bytes[20..54], b"generation-0000000000000007.sqlite");
        assert_eq!(StoreSelector::decode(&bytes).unwrap(), selector);
        let mut corrupt = bytes;
        corrupt[54] ^= 1;
        assert!(StoreSelector::decode(&corrupt).is_err());
    }

    #[test]
    fn genesis_current_compaction_and_reopen_preserve_store_identity() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        let store_id = engine.store_id().unwrap();
        assert_eq!(
            read_selector(&directory.join("CURRENT"))
                .unwrap()
                .generation,
            0
        );
        let engine = compact(engine, &directory, &NativeDriver).unwrap();
        let observation = engine.last_compaction_observation().unwrap();
        assert!(observation.old_generation_bytes > 0);
        assert!(observation.new_generation_bytes > 0);
        assert!(observation.mark_database_bytes > 0);
        assert_eq!(observation.selector_temporary_bytes, SELECTOR_BYTES as u64);
        assert_eq!(
            observation.total_peak_bytes,
            observation.old_generation_bytes
                + observation.new_generation_bytes
                + observation.mark_database_bytes
                + observation.candidate_journal_temp_peak_bytes
                + observation.verification_scratch_peak_bytes
                + observation.selector_temporary_bytes
        );
        assert_eq!(engine.store_id().unwrap(), store_id);
        assert_eq!(
            read_selector(&directory.join("CURRENT"))
                .unwrap()
                .generation,
            1
        );
        assert!(!directory.join(generation_filename(0)).exists());
        drop(engine);
        assert_eq!(
            open_current(&directory, IntegrityMode::Verified)
                .unwrap()
                .store_id()
                .unwrap(),
            store_id
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compaction_authenticates_unreachable_objects_before_discarding_them() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-corrupt-orphan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine =
            open_or_create(&directory, &NativeDriver, IntegrityMode::TrustedLocalDev).unwrap();
        let canonical = encode_bytes_object(b"unreachable").unwrap();
        let id = ObjectId::for_bytes(&canonical);
        engine.put_object_if_absent(id, &canonical).unwrap();
        let selected_path = engine.path().to_owned();
        drop(engine);
        Connection::open(&selected_path)
            .unwrap()
            .execute(
                "UPDATE layerfs_objects SET canonical_bytes = zeroblob(canonical_length) WHERE object_id = ?1",
                rusqlite::params![id.as_bytes().as_slice()],
            )
            .unwrap();

        let before = fs::read(directory.join("CURRENT")).unwrap();
        let engine = open_current(&directory, IntegrityMode::TrustedLocalDev).unwrap();
        assert!(compact(engine, &directory, &NativeDriver).is_err());
        assert_eq!(fs::read(directory.join("CURRENT")).unwrap(), before);
        assert!(!directory.join(generation_filename(1)).exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exact_next_generation_without_selector_is_reported_and_preserved() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-unresolved-next-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        drop(engine);
        let candidate = directory.join(generation_filename(1));
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "generation::tests::unselected_generation_crash_child",
            ])
            .env("LAYERFS_UNSELECTED_GENERATION_CRASH", &directory)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(93));

        assert!(matches!(
            open_or_create(&directory, &NativeDriver, IntegrityMode::Verified),
            Err(EngineError::UnresolvedGenerationResidue { generation: 1 })
        ));
        assert!(candidate.exists());
        assert!(!directory.join("CURRENT.tmp").exists());
        let engine = open_current(&directory, IntegrityMode::Verified).unwrap();
        assert!(matches!(
            compact(engine, &directory, &NativeDriver),
            Err(EngineError::UnresolvedGenerationResidue { generation: 1 })
        ));
        assert!(candidate.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compaction_returns_the_callers_store_lifetime_mode() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-trusted-compact-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine =
            open_or_create(&directory, &NativeDriver, IntegrityMode::TrustedLocalDev).unwrap();
        let engine = compact(engine, &directory, &NativeDriver).unwrap();
        assert_eq!(engine.mode, IntegrityMode::TrustedLocalDev);
        drop(engine);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compaction_rejects_nonempty_legacy_state_without_erasing_it() {
        for legacy in ["root", "delta", "visible"] {
            let directory = std::env::temp_dir().join(format!(
                "layerfs-generation-legacy-{legacy}-compact-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let engine =
                open_or_create(&directory, &NativeDriver, IntegrityMode::TrustedLocalDev).unwrap();
            let selected_path = engine.path().to_owned();
            drop(engine);
            let root = ObjectId::for_bytes(b"legacy root");
            let directory_object = ObjectId::for_bytes(b"legacy directory");
            let connection = Connection::open(&selected_path).unwrap();
            match legacy {
                "root" => connection.execute(
                    "INSERT INTO layerfs_roots (root_id, directory_object, parent_root) VALUES (?1, ?2, NULL)",
                    rusqlite::params![root.as_bytes().as_slice(), directory_object.as_bytes().as_slice()],
                ),
                "delta" => connection.execute(
                    "INSERT INTO layerfs_deltas
                     (delta_id, format_version, parent_root, child_root, payload)
                     VALUES (?1, 0, NULL, ?2, X'00')",
                    rusqlite::params![root.as_bytes().as_slice(), directory_object.as_bytes().as_slice()],
                ),
                "visible" => connection.execute(
                    "UPDATE layerfs_store_meta SET visible_root = ?1 WHERE store_id = 1",
                    rusqlite::params![root.as_bytes().as_slice()],
                ),
                _ => unreachable!(),
            }
            .unwrap();
            drop(connection);

            let before = fs::read(directory.join("CURRENT")).unwrap();
            let engine = open_current(&directory, IntegrityMode::TrustedLocalDev).unwrap();
            assert!(matches!(
                compact(engine, &directory, &NativeDriver),
                Err(EngineError::InvalidRecord("legacy compaction state"))
            ));
            assert_eq!(fs::read(directory.join("CURRENT")).unwrap(), before);
            assert!(!directory.join(generation_filename(1)).exists());
            let connection = Connection::open(&selected_path).unwrap();
            let preserved = match legacy {
                "root" => connection
                    .query_row("SELECT EXISTS(SELECT 1 FROM layerfs_roots)", [], |row| {
                        row.get::<_, bool>(0)
                    })
                    .unwrap(),
                "delta" => connection
                    .query_row("SELECT EXISTS(SELECT 1 FROM layerfs_deltas)", [], |row| {
                        row.get::<_, bool>(0)
                    })
                    .unwrap(),
                "visible" => connection
                    .query_row(
                        "SELECT visible_root IS NOT NULL FROM layerfs_store_meta WHERE store_id = 1",
                        [],
                        |row| row.get::<_, bool>(0),
                    )
                    .unwrap(),
                _ => unreachable!(),
            };
            assert!(preserved, "{legacy} state was erased");
            drop(connection);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn genesis_handoff_reopens_the_generation_selected_after_maintenance() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-handoff-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = open_or_create(
            &directory,
            &AdvancesSelectorDuringInstall,
            IntegrityMode::TrustedLocalDev,
        )
        .unwrap();
        assert_eq!(
            read_selector(&directory.join("CURRENT"))
                .unwrap()
                .generation,
            1
        );
        assert_eq!(engine.path(), directory.join(generation_filename(1)));
        drop(engine);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn definitely_uninstalled_genesis_is_cleaned_in_the_same_call() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-failed-genesis-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(
            open_or_create(&directory, &LostBeforeVisibility, IntegrityMode::Verified).is_err()
        );
        assert!(!directory.join("CURRENT").exists());
        assert!(!directory.join("CURRENT.tmp").exists());
        assert!(!directory.join(generation_filename(0)).exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_selector_install_recovers_prior_and_removes_only_owned_candidate() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-recovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        let store_id = engine.store_id().unwrap();
        assert!(compact(engine, &directory, &LostBeforeVisibility).is_err());
        fs::write(directory.join("unknown-residue"), b"preserve").unwrap();
        let recovered = open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        assert_eq!(recovered.store_id().unwrap(), store_id);
        assert_eq!(
            read_selector(&directory.join("CURRENT"))
                .unwrap()
                .generation,
            0
        );
        assert!(!directory.join("CURRENT.tmp").exists());
        assert!(!directory.join(generation_filename(1)).exists());
        assert!(directory.join("unknown-residue").exists());
        drop(recovered);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn maintenance_lock_blocks_cleanup_and_lost_ack_reconciles_requested_selector() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-maintenance-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut engine =
            open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        let live_scratch = crate::scratch::DiskTable::create_near(engine.path(), "live").unwrap();
        live_scratch.put(b"key", b"value").unwrap();
        assert!(try_acquire_maintenance(&directory).unwrap().is_none());
        assert!(fs::read_dir(&directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("-live-")
        }));
        drop(live_scratch);
        let candidate = directory.join(generation_filename(9));
        engine.compact_to(&candidate).unwrap();
        let candidate_engine = Engine::open(&candidate).unwrap();
        fs::write(
            directory.join("CURRENT.tmp"),
            selector(&candidate_engine, 9).unwrap().encode(),
        )
        .unwrap();
        drop(candidate_engine);
        engine.maintenance_pin.take();
        let maintenance = acquire_maintenance(&directory).unwrap();
        assert!(matches!(
            open_current(&directory, IntegrityMode::Verified),
            Err(EngineError::Sqlite {
                kind: crate::SqliteErrorKind::Busy | crate::SqliteErrorKind::Locked,
                ..
            })
        ));
        assert!(candidate.exists(), "open cleanup crossed maintenance lock");
        drop(maintenance);
        let cleanup = acquire_maintenance(&directory).unwrap();
        let selected = read_selector(&directory.join("CURRENT")).unwrap();
        cleanup_owned_residue(&directory, &selected, None, &NativeDriver).unwrap();
        drop(cleanup);
        drop(open_current(&directory, IntegrityMode::Verified).unwrap());
        assert!(
            candidate.exists(),
            "cleanup deleted an arbitrary valid generation"
        );
        assert!(
            directory.join("CURRENT.tmp").exists(),
            "cleanup deleted an arbitrary valid selector"
        );
        fs::remove_file(directory.join("CURRENT.tmp")).unwrap();
        fs::remove_file(&candidate).unwrap();
        drop(engine);

        let engine = open_current(&directory, IntegrityMode::Verified).unwrap();
        let engine = compact(engine, &directory, &LostAfterVisibility).unwrap();
        assert_eq!(
            read_selector(&directory.join("CURRENT"))
                .unwrap()
                .generation,
            1
        );
        drop(engine);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compaction_preflights_space_before_creating_candidate() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-space-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        assert!(matches!(
            compact(engine, &directory, &NoSpace),
            Err(EngineError::Sqlite {
                kind: crate::SqliteErrorKind::NoSpace,
                ..
            })
        ));
        assert!(!directory.join(generation_filename(1)).exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn selector_directory_sync_failure_is_ambiguous_and_preserves_prior_generation() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-sync-reconcile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        assert!(matches!(
            compact(
                engine,
                &directory,
                &LostFirstDirectorySync(AtomicBool::new(false)),
            ),
            Err(EngineError::AmbiguousDurability)
        ));
        assert_eq!(
            read_selector(&directory.join("CURRENT"))
                .unwrap()
                .generation,
            1
        );
        assert!(directory.join(generation_filename(0)).exists());
        assert!(directory.join(generation_filename(1)).exists());
        let recovered = open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        assert_eq!(
            recovered.store_id().unwrap(),
            read_selector(&directory.join("CURRENT")).unwrap().store_id
        );
        assert!(!directory.join(generation_filename(0)).exists());
        drop(recovered);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn post_visible_cleanup_sync_failure_is_ambiguous() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-cleanup-sync-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        assert!(matches!(
            compact(
                engine,
                &directory,
                &LostCleanupDirectorySync(AtomicUsize::new(0)),
            ),
            Err(EngineError::AmbiguousDurability)
        ));
        assert_eq!(
            read_selector(&directory.join("CURRENT"))
                .unwrap()
                .generation,
            1
        );
        drop(open_current(&directory, IntegrityMode::Verified).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_selector_fails_closed_and_exact_candidate_residue_recovers() {
        let genesis = std::env::temp_dir().join(format!(
            "layerfs-generation-genesis-residue-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&genesis).unwrap();
        let generation = genesis.join(generation_filename(0));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&generation)
            .unwrap();
        drop(Engine::open(&generation).unwrap());
        let before = fs::read(&generation).unwrap();
        assert!(matches!(
            open_or_create(&genesis, &NativeDriver, IntegrityMode::Verified),
            Err(EngineError::InvalidRecord(
                "missing CURRENT in nonempty Store"
            ))
        ));
        assert!(!genesis.join("CURRENT").exists());
        assert_eq!(fs::read(&generation).unwrap(), before);
        fs::remove_dir_all(genesis).unwrap();

        for partial_selector in [false, true] {
            let directory = std::env::temp_dir().join(format!(
                "layerfs-generation-candidate-residue-{partial_selector}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let engine =
                open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
            let candidate = directory.join(generation_filename(1));
            engine.compact_to(&candidate).unwrap();
            if partial_selector {
                fs::write(directory.join("CURRENT.tmp"), b"partial").unwrap();
            }
            let unknown = directory.join(generation_filename(9));
            engine.compact_to(&unknown).unwrap();
            assert!(compact(engine, &directory, &NativeDriver).is_err());
            assert_eq!(
                read_selector(&directory.join("CURRENT"))
                    .unwrap()
                    .generation,
                0
            );
            assert!(candidate.exists(), "unproven candidate was removed");
            assert!(unknown.exists(), "cleanup removed unknown generation");
            if partial_selector {
                assert_eq!(fs::read(directory.join("CURRENT.tmp")).unwrap(), b"partial");
            }
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn candidate_inspection_never_mutates_empty_or_foreign_sqlite() {
        for kind in ["empty", "foreign", "sqliteX"] {
            let directory = std::env::temp_dir().join(format!(
                "layerfs-generation-foreign-candidate-{kind}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let engine =
                open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
            let candidate = directory.join(generation_filename(1));
            if kind != "empty" {
                let connection = Connection::open(&candidate).unwrap();
                let table = if kind == "sqliteX" {
                    "sqliteX_data"
                } else {
                    "caller_data"
                };
                connection
                    .execute_batch(&format!(
                        "CREATE TABLE {table} (value TEXT); INSERT INTO {table} VALUES ('keep');"
                    ))
                    .unwrap();
            } else {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&candidate)
                    .unwrap();
            }
            let before = fs::read(&candidate).unwrap();
            assert!(compact(engine, &directory, &NativeDriver).is_err());
            assert_eq!(fs::read(&candidate).unwrap(), before);
            if kind != "empty" {
                let table = if kind == "sqliteX" {
                    "sqliteX_data"
                } else {
                    "caller_data"
                };
                assert_eq!(
                    Connection::open(&candidate)
                        .unwrap()
                        .query_row(&format!("SELECT value FROM {table}"), [], |row| {
                            row.get::<_, String>(0)
                        })
                        .unwrap(),
                    "keep"
                );
            }
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn exact_next_candidate_cleanup_preserves_valid_unrelated_temporary_selector() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-unrelated-temp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut engine =
            open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        let next_path = directory.join(generation_filename(1));
        engine.compact_to(&next_path).unwrap();
        let unrelated_path = directory.join(generation_filename(9));
        engine.compact_to(&unrelated_path).unwrap();
        let unrelated = Engine::open(&unrelated_path).unwrap();
        fs::write(
            directory.join("CURRENT.tmp"),
            selector(&unrelated, 9).unwrap().encode(),
        )
        .unwrap();
        drop(unrelated);
        engine.maintenance_pin.take();
        let maintenance = acquire_maintenance(&directory).unwrap();
        let selected = read_selector(&directory.join("CURRENT")).unwrap();
        cleanup_owned_residue(&directory, &selected, None, &NativeDriver).unwrap();
        drop(maintenance);
        assert!(
            next_path.exists(),
            "candidate without matching selector was removed"
        );
        assert!(unrelated_path.exists(), "unrelated generation was removed");
        assert!(
            directory.join("CURRENT.tmp").exists(),
            "valid unrelated selector was removed"
        );
        fs::remove_file(directory.join("CURRENT.tmp")).unwrap();
        fs::remove_file(next_path).unwrap();
        fs::remove_file(unrelated_path).unwrap();
        drop(engine);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn selector_cleanup_refuses_path_substitution_after_inspection() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-selector-substitute-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut engine =
            open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        let candidate = directory.join(generation_filename(1));
        engine.compact_to(&candidate).unwrap();
        let candidate_engine = Engine::open(&candidate).unwrap();
        fs::write(
            directory.join("CURRENT.tmp"),
            selector(&candidate_engine, 1).unwrap().encode(),
        )
        .unwrap();
        drop(candidate_engine);
        engine.maintenance_pin.take();
        let maintenance = acquire_maintenance(&directory).unwrap();
        let selected = read_selector(&directory.join("CURRENT")).unwrap();
        assert!(cleanup_owned_residue(
            &directory,
            &selected,
            None,
            &SubstituteOnRemove(AtomicBool::new(false))
        )
        .is_err());
        assert_eq!(fs::read(&candidate).unwrap(), b"substitute");
        assert!(candidate.with_extension("custody-saved").exists());
        assert!(directory.join("CURRENT.tmp").exists());
        drop(maintenance);
        fs::remove_file(&candidate).unwrap();
        fs::remove_file(candidate.with_extension("custody-saved")).unwrap();
        fs::remove_file(directory.join("CURRENT.tmp")).unwrap();
        drop(engine);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exclusive_maintenance_recovers_child_exit_hot_scratch() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-scratch-crash-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        let store = engine.path().to_owned();
        drop(engine);
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "generation::tests::scratch_crash_child"])
            .env("LAYERFS_SCRATCH_CRASH_STORE", &store)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(91));
        let scratch = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("-crash-child-")
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "sqlite")
            })
            .unwrap();
        let journal = PathBuf::from(format!("{}-journal", scratch.display()));
        assert!(journal.exists(), "child exit did not leave a hot journal");

        let maintenance = acquire_maintenance(&directory).unwrap();
        let selected = read_selector(&directory.join("CURRENT")).unwrap();
        let verified = open_selected(&directory, IntegrityMode::Verified).unwrap();
        crate::scratch::recover_owned_near(verified.path(), selected.store_id, &NativeDriver)
            .unwrap();
        assert!(!scratch.exists());
        assert!(!journal.exists());
        drop(verified);
        drop(maintenance);
        drop(open_current(&directory, IntegrityMode::Verified).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cleanup_waits_for_selected_verified_authority() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-corrupt-selected-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut engine =
            open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        let candidate = directory.join(generation_filename(1));
        engine.compact_to(&candidate).unwrap();
        let candidate_engine = Engine::open(&candidate).unwrap();
        fs::write(
            directory.join("CURRENT.tmp"),
            selector(&candidate_engine, 1).unwrap().encode(),
        )
        .unwrap();
        drop(candidate_engine);
        let selected = engine.path().to_owned();
        engine.maintenance_pin.take();
        drop(engine);
        fs::write(&selected, b"corrupt selected generation").unwrap();

        assert!(open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).is_err());
        assert!(candidate.exists());
        assert!(directory.join("CURRENT.tmp").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn trusted_recovery_reopens_in_the_requested_store_lifetime_mode() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-generation-trusted-recovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine =
            open_or_create(&directory, &NativeDriver, IntegrityMode::TrustedLocalDev).unwrap();
        let namespace = layerfs_core::namespace_codec::encode_namespace_root(
            layerfs_core::namespace::NamespaceRootV1 {
                profile_id: layerfs_core::namespace_codec::profile_id(),
                root_directory_inode: layerfs_core::inode::InodeId::allocate([0x61; 32], 0),
                inode_table_root: layerfs_core::ObjectId::for_bytes(b"missing inode table"),
            },
        )
        .unwrap();
        let state = engine
            .begin_publication(None, "main")
            .unwrap()
            .publish_namespace(&namespace)
            .unwrap();
        drop(engine);
        let residue = directory.join(".layerfs-foreign-trigger.sqlite");
        fs::write(&residue, b"not owned scratch").unwrap();

        let recovered =
            open_or_create(&directory, &NativeDriver, IntegrityMode::TrustedLocalDev).unwrap();
        assert_eq!(recovered.read_ref("main").unwrap(), Some(state));
        drop(recovered);
        assert!(open_or_create(&directory, &NativeDriver, IntegrityMode::Verified).is_err());

        fs::remove_file(residue).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mismatched_compaction_source_never_cleans_target_residue() {
        let serial = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let source_directory = std::env::temp_dir().join(format!(
            "layerfs-generation-wrong-source-a-{}-{serial}",
            std::process::id()
        ));
        let target_directory = std::env::temp_dir().join(format!(
            "layerfs-generation-wrong-source-b-{}-{serial}",
            std::process::id()
        ));
        let source =
            open_or_create(&source_directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        let mut target =
            open_or_create(&target_directory, &NativeDriver, IntegrityMode::Verified).unwrap();
        let candidate = target_directory.join(generation_filename(1));
        target.compact_to(&candidate).unwrap();
        let candidate_engine = Engine::open(&candidate).unwrap();
        fs::write(
            target_directory.join("CURRENT.tmp"),
            selector(&candidate_engine, 1).unwrap().encode(),
        )
        .unwrap();
        drop(candidate_engine);
        target.maintenance_pin.take();
        drop(target);

        assert!(matches!(
            compact(source, &target_directory, &NativeDriver),
            Err(EngineError::InvalidRecord("compaction source generation"))
        ));
        assert!(candidate.exists());
        assert!(target_directory.join("CURRENT.tmp").exists());
        fs::remove_dir_all(source_directory).unwrap();
        fs::remove_dir_all(target_directory).unwrap();
    }

    #[test]
    fn selector_reader_rejects_oversize_without_read_to_end() {
        let path = std::env::temp_dir().join(format!(
            "layerfs-selector-oversize-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, vec![0_u8; SELECTOR_BYTES + 1]).unwrap();
        assert!(matches!(
            read_selector(&path),
            Err(EngineError::Sqlite {
                kind: crate::SqliteErrorKind::Io,
                ..
            })
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn preseeded_wal_maintenance_is_admitted_reconfigured_and_pinned() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-maintenance-wal-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("MAINTENANCE.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE maintenance_guard (
                    id INTEGER PRIMARY KEY CHECK (id = 1)
                 );
                 INSERT INTO maintenance_guard (id) VALUES (1);",
            )
            .unwrap();
        drop(connection);

        let pin = pin_connection(&directory).unwrap();
        assert!(try_acquire_maintenance(&directory).unwrap().is_none());
        assert_eq!(
            Connection::open(&path)
                .unwrap()
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap()
                .to_ascii_lowercase(),
            "delete"
        );
        drop(pin);
        drop(acquire_maintenance(&directory).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }
}
