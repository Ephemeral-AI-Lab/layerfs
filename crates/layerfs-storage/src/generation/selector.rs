//! CURRENT selector framing, checksums, reads, and installation.

use super::create::StoreGenerationDriver;
use crate::integrity::IntegrityMode;
use crate::{Engine, EngineError, EngineResult, FullStorage, FULL_SCHEMA, SCHEMA_VERSION};
use rusqlite::{Connection, OptionalExtension};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

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

pub fn open_current(directory: &Path, mode: IntegrityMode) -> EngineResult<Engine> {
    let pin = pin_connection(directory)?;
    let mut engine = open_selected(directory, mode)?;
    engine.maintenance_pin = Some(std::sync::Mutex::new(pin));
    Ok(engine)
}

pub fn open_current_full_durable(directory: &Path) -> EngineResult<FullStorage> {
    let pin = pin_connection(directory)?;
    let mut storage = open_selected_full_durable_verified(directory)?;
    storage.attach_maintenance_pin(pin);
    Ok(storage)
}

pub(crate) fn open_selected_full_durable_verified(directory: &Path) -> EngineResult<FullStorage> {
    let selected = read_selector(&directory.join("CURRENT"))?;
    let path = selected_path(directory, &selected, FULL_SCHEMA.schema_version as u32)?;
    let storage = FullStorage::open_durable_verified(path)?;
    if storage.storage_id() != selected.store_id {
        return Err(EngineError::InvalidRecord("Full selector StoreId"));
    }
    Ok(storage)
}

pub(crate) fn open_selected(directory: &Path, mode: IntegrityMode) -> EngineResult<Engine> {
    let selector = read_selector(&directory.join("CURRENT"))?;
    open_legacy_generation(directory, &selector, mode)
}

pub(crate) fn open_legacy_generation(
    directory: &Path,
    selector: &StoreSelector,
    mode: IntegrityMode,
) -> EngineResult<Engine> {
    let selected_path = selected_path(directory, selector, SCHEMA_VERSION as u32)?;
    let engine = Engine::open_with_mode(&selected_path, mode)?;
    if engine.store_id()? != selector.store_id {
        return Err(EngineError::InvalidRecord("selector StoreId"));
    }
    Ok(engine)
}

fn selected_path(
    directory: &Path,
    selector: &StoreSelector,
    schema_version: u32,
) -> EngineResult<PathBuf> {
    if selector.schema_version != schema_version
        || selector.profile_id != layerfs_core::namespace_codec::profile_id().to_bytes()
    {
        return Err(EngineError::ProfileMismatch);
    }
    let path = directory.join(generation_filename(selector.generation));
    path.is_file()
        .then_some(path)
        .ok_or(EngineError::InvalidRecord("selected generation missing"))
}

pub(crate) fn selector(engine: &Engine, generation: u64) -> EngineResult<StoreSelector> {
    Ok(StoreSelector {
        generation,
        schema_version: SCHEMA_VERSION as u32,
        store_id: engine.store_id()?,
        profile_id: layerfs_core::namespace_codec::profile_id().to_bytes(),
    })
}

pub(crate) fn full_selector(storage: &FullStorage, generation: u64) -> EngineResult<StoreSelector> {
    storage.require_authority()?;
    Ok(StoreSelector {
        generation,
        schema_version: FULL_SCHEMA.schema_version as u32,
        store_id: storage.storage_id(),
        profile_id: layerfs_core::namespace_codec::profile_id().to_bytes(),
    })
}

pub(crate) fn install_exact_selector(
    directory: &Path,
    name: &str,
    requested: &StoreSelector,
    prior: Option<&StoreSelector>,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    let target = directory.join(name);
    let observed = read_selector_optional(&target)?;
    match observed {
        Some(observed) if observed == *requested => {
            return driver
                .sync_directory(directory)
                .map_err(|_| EngineError::AmbiguousDurability);
        }
        Some(observed) if Some(&observed) != prior => {
            return Err(EngineError::AmbiguousDurability);
        }
        _ => {}
    }
    let prepared = directory.join(format!("{name}.tmp"));
    prepare_selector(&prepared, requested, true)?;
    if let Err(error) = driver.install_selector(&prepared, &target) {
        match read_selector_optional(&target)? {
            Some(observed) if observed == *requested => {}
            Some(observed) if Some(&observed) == prior => {
                return Err(crate::io_engine_error(error));
            }
            None => return Err(crate::io_engine_error(error)),
            _ => return Err(EngineError::AmbiguousDurability),
        }
    }
    driver
        .sync_directory(directory)
        .map_err(|_| EngineError::AmbiguousDurability)
}

fn prepare_selector(path: &Path, selector: &StoreSelector, admit_exact: bool) -> EngineResult<()> {
    if admit_exact && read_selector_optional(path)? == Some(selector.clone()) {
        return Ok(());
    }
    if path.exists() {
        return Err(EngineError::InvalidRecord("selector staging identity"));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(crate::io_engine_error)?;
    file.write_all(&selector.encode())
        .and_then(|_| file.sync_all())
        .map_err(crate::io_engine_error)
}

pub(crate) fn install(
    directory: &Path,
    selector: StoreSelector,
    prior: Option<&StoreSelector>,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    let prepared = directory.join("CURRENT.tmp");
    prepare_selector(&prepared, &selector, false)?;
    let current = directory.join("CURRENT");
    if let Err(error) = driver.install_selector(&prepared, &current) {
        super::switch::reconcile_selector_install(directory, &current, &selector, prior, error)?;
    }
    if let Err(error) = driver.sync_directory(directory) {
        let _ = error;
        let _ = read_selector_optional(&current);
        return Err(EngineError::AmbiguousDurability);
    }
    Ok(())
}

pub(crate) fn read_selector(path: &Path) -> EngineResult<StoreSelector> {
    read_selector_io(path).map_err(crate::io_engine_error)
}

pub(crate) fn read_selector_optional(path: &Path) -> EngineResult<Option<StoreSelector>> {
    match read_selector_io(path) {
        Ok(selector) => Ok(Some(selector)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(crate::io_engine_error(error)),
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

pub(crate) fn generation_filename(generation: u64) -> PathBuf {
    PathBuf::from(format!("generation-{generation:016x}.sqlite"))
}

pub(crate) struct MaintenanceLock {
    connection: Option<Connection>,
}

impl Drop for MaintenanceLock {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.execute_batch("ROLLBACK");
        }
    }
}

pub(crate) fn try_acquire_maintenance(directory: &Path) -> EngineResult<Option<MaintenanceLock>> {
    let connection = maintenance_connection(directory)?;
    match connection.execute_batch("BEGIN EXCLUSIVE") {
        Ok(()) => Ok(Some(MaintenanceLock {
            connection: Some(connection),
        })),
        Err(error)
            if matches!(
                crate::sqlite_error_kind(&error),
                crate::SqliteErrorKind::Busy | crate::SqliteErrorKind::Locked
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(crate::map_sqlite_error(error)),
    }
}

pub(crate) fn acquire_maintenance(directory: &Path) -> EngineResult<MaintenanceLock> {
    try_acquire_maintenance(directory)?.ok_or_else(|| EngineError::Sqlite {
        kind: crate::SqliteErrorKind::Busy,
        message: "Store maintenance is already active".to_owned(),
    })
}

fn maintenance_connection(directory: &Path) -> EngineResult<Connection> {
    let path = directory.join("MAINTENANCE.sqlite");
    let initialize = !path.exists();
    let connection = Connection::open(path).map_err(crate::map_sqlite_error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(crate::map_sqlite_error)?;
    if initialize {
        connection
            .execute_batch(
                "CREATE TABLE maintenance_guard (
                    id INTEGER PRIMARY KEY CHECK (id = 1)
                 );
                 INSERT INTO maintenance_guard (id) VALUES (1);",
            )
            .map_err(crate::map_sqlite_error)?;
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
        .map_err(crate::map_sqlite_error)?
        .ok_or(EngineError::SchemaMismatch)?;
    if crate::schema_shape(&schema)
        != crate::schema_shape(
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
            .map_err(crate::map_sqlite_error)?
            != (1, Some(1), Some(1))
        || connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(crate::map_sqlite_error)?
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
        .map_err(crate::map_sqlite_error)?;
    connection
        .execute_batch("PRAGMA synchronous=FULL; PRAGMA temp_store=FILE; PRAGMA mmap_size=0;")
        .map_err(crate::map_sqlite_error)?;
    let synchronous = connection
        .query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))
        .map_err(crate::map_sqlite_error)?;
    let temp_store = connection
        .query_row("PRAGMA temp_store", [], |row| row.get::<_, i64>(0))
        .map_err(crate::map_sqlite_error)?;
    let mmap = connection
        .query_row("PRAGMA mmap_size", [], |row| row.get::<_, i64>(0))
        .map_err(crate::map_sqlite_error)?;
    if !journal.eq_ignore_ascii_case("delete") || synchronous != 2 || temp_store != 1 || mmap != 0 {
        return Err(EngineError::ProfileMismatch);
    }
    Ok(())
}

pub(crate) fn pin_connection(directory: &Path) -> EngineResult<Connection> {
    let connection = maintenance_connection(directory)?;
    connection
        .execute_batch("BEGIN")
        .map_err(crate::map_sqlite_error)?;
    connection
        .query_row("SELECT id FROM maintenance_guard WHERE id = 1", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(crate::map_sqlite_error)?;
    Ok(connection)
}
