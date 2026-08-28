//! Typed Full-family storage identity and admission.

use crate::generation::create::opened_file_identity;
use crate::generation::{NativeGenerationDriver, StoreGenerationDriver};
use crate::sqlite::admission::{admit_full_family_role, admit_full_role_metadata};
use crate::{
    configure_profile_counted, map_sqlite_error, EngineError, EngineResult, SqliteProfile,
    StoreRole, FULL_SCHEMA,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FullStorageCounters {
    pub durable_head_transactions: u64,
}

pub struct FullStorage {
    path: PathBuf,
    role: StoreRole,
    storage_id: [u8; 32],
    durable_storage_id: [u8; 32],
    profile: SqliteProfile,
    connection: Mutex<Connection>,
    counters: Mutex<FullStorageCounters>,
    custody: fs::File,
    maintenance_pin: Option<Mutex<Connection>>,
}

impl FullStorage {
    pub fn create_durable(path: impl AsRef<Path>) -> EngineResult<Self> {
        let storage = Self::create(path.as_ref(), StoreRole::Durable, None, None)?;
        storage.require_authority()?;
        Ok(storage)
    }

    pub fn open_durable(path: impl AsRef<Path>) -> EngineResult<Self> {
        let storage = Self::open_exact(path.as_ref(), StoreRole::Durable, None)?;
        storage.require_authority()?;
        Ok(storage)
    }

    pub fn open_durable_verified(path: impl AsRef<Path>) -> EngineResult<Self> {
        let storage = Self::open_durable(path)?;
        storage.verify_integrity()?;
        Ok(storage)
    }

    pub fn create_cache(
        path: impl AsRef<Path>,
        durable_storage_id: [u8; 32],
    ) -> EngineResult<Self> {
        Self::create(
            path.as_ref(),
            StoreRole::DurableCache,
            None,
            Some(durable_storage_id),
        )
    }

    pub fn open_cache(
        path: impl AsRef<Path>,
        expected_durable_storage_id: [u8; 32],
    ) -> EngineResult<Self> {
        Self::open_exact(
            path.as_ref(),
            StoreRole::DurableCache,
            Some(expected_durable_storage_id),
        )
    }

    pub fn open_cache_verified(
        path: impl AsRef<Path>,
        expected_durable_storage_id: [u8; 32],
    ) -> EngineResult<Self> {
        let storage = Self::open_cache(path, expected_durable_storage_id)?;
        storage.verify_integrity()?;
        Ok(storage)
    }

    pub(crate) fn create_durable_with_id(path: &Path, storage_id: [u8; 32]) -> EngineResult<Self> {
        let storage = Self::create(path, StoreRole::Durable, Some(storage_id), None)?;
        storage.require_authority()?;
        Ok(storage)
    }

    fn open_exact(
        path: &Path,
        expected_role: StoreRole,
        expected_durable_storage_id: Option<[u8; 32]>,
    ) -> EngineResult<Self> {
        let path = path.to_owned();
        let custody = fs::File::open(&path).map_err(crate::io_engine_error)?;
        let custody_identity = opened_file_identity(&custody).map_err(crate::io_engine_error)?;
        let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(map_sqlite_error)?;
        if NativeGenerationDriver
            .file_identity(&path)
            .map_err(crate::io_engine_error)?
            != custody_identity
        {
            return Err(EngineError::InvalidRecord("FullStorage path identity"));
        }
        connection
            .busy_timeout(crate::BUSY_TIMEOUT)
            .map_err(map_sqlite_error)?;
        let before_profile = admit_full_family_role(&connection, expected_role)?;
        if expected_durable_storage_id
            .is_some_and(|expected| expected != before_profile.durable_storage_id)
        {
            return Err(EngineError::InvalidRecord("cache DurableStorageId"));
        }
        let mut statements = 0;
        let profile = configure_profile_counted(&connection, &mut statements)?;
        let after_profile = admit_full_role_metadata(&connection, expected_role)?;
        if before_profile != after_profile {
            return Err(EngineError::SchemaMismatch);
        }
        require_foreign_keys(&connection)?;
        Ok(Self {
            path,
            role: after_profile.role,
            storage_id: after_profile.storage_id,
            durable_storage_id: after_profile.durable_storage_id,
            profile,
            connection: Mutex::new(connection),
            counters: Mutex::new(FullStorageCounters::default()),
            custody,
            maintenance_pin: None,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn role(&self) -> StoreRole {
        self.role
    }

    pub const fn storage_id(&self) -> [u8; 32] {
        self.storage_id
    }

    pub const fn durable_storage_id(&self) -> [u8; 32] {
        self.durable_storage_id
    }

    pub const fn profile(&self) -> &SqliteProfile {
        &self.profile
    }

    pub fn active_connection_count(&self) -> EngineResult<u64> {
        let _connection = self.lock_connection()?;
        Ok(1 + u64::from(self.maintenance_pin.is_some()))
    }

    pub(crate) fn lock_connection(&self) -> EngineResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| EngineError::InvalidRecord("FullStorage connection lock"))
    }

    pub fn counters(&self) -> EngineResult<FullStorageCounters> {
        self.counters
            .lock()
            .map(|counters| *counters)
            .map_err(|_| EngineError::InvalidRecord("FullStorage counters lock"))
    }

    pub fn reset_counters(&self) -> EngineResult<()> {
        *self
            .counters
            .lock()
            .map_err(|_| EngineError::InvalidRecord("FullStorage counters lock"))? =
            FullStorageCounters::default();
        Ok(())
    }

    pub(crate) fn bump_durable_head_transaction(&self) -> EngineResult<()> {
        let mut counters = self
            .counters
            .lock()
            .map_err(|_| EngineError::InvalidRecord("FullStorage counters lock"))?;
        counters.durable_head_transactions = counters
            .durable_head_transactions
            .checked_add(1)
            .ok_or(EngineError::CounterOverflow)?;
        Ok(())
    }

    pub(crate) fn owned_file_identity(&self) -> EngineResult<Vec<u8>> {
        opened_file_identity(&self.custody).map_err(crate::io_engine_error)
    }

    pub(crate) fn attach_maintenance_pin(&mut self, connection: Connection) {
        self.maintenance_pin = Some(Mutex::new(connection));
    }

    pub(crate) fn require_authority(&self) -> EngineResult<()> {
        if self.role == StoreRole::Durable && self.storage_id == self.durable_storage_id {
            Ok(())
        } else {
            Err(EngineError::InvalidRecord("FullStorage authority role"))
        }
    }

    fn verify_integrity(&self) -> EngineResult<()> {
        let connection = self.lock_connection()?;
        let integrity = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .map_err(map_sqlite_error)?;
        if integrity != "ok" {
            return Err(EngineError::SchemaMismatch);
        }
        require_foreign_keys(&connection)?;
        crate::integrity::full::object::authenticate_object_table(&connection)?;
        crate::integrity::full::history::verify_full_accepted_state(
            &connection,
            &self.path,
            self.storage_id,
        )
    }

    fn create(
        path: &Path,
        role: StoreRole,
        storage_id: Option<[u8; 32]>,
        durable_storage_id: Option<[u8; 32]>,
    ) -> EngineResult<Self> {
        Self::create_with_injector(path, role, storage_id, durable_storage_id, &mut |_| Ok(()))
    }

    fn create_with_injector(
        path: &Path,
        role: StoreRole,
        storage_id: Option<[u8; 32]>,
        durable_storage_id: Option<[u8; 32]>,
        inject: &mut dyn FnMut(&'static str) -> EngineResult<()>,
    ) -> EngineResult<Self> {
        let custody = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| EngineError::InvalidRecord("FullStorage create target"))?;
        if let Err(error) = inject("file_created") {
            remove_custodied_file(path, &custody);
            return Err(error);
        }
        let connection = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        {
            Ok(connection) => connection,
            Err(error) => {
                remove_custodied_file(path, &custody);
                return Err(map_sqlite_error(error));
            }
        };
        let prepared = (|| {
            connection
                .busy_timeout(crate::BUSY_TIMEOUT)
                .map_err(map_sqlite_error)?;
            let schema_objects = connection
                .query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(map_sqlite_error)?;
            if schema_objects != 0 {
                return Err(EngineError::SchemaMismatch);
            }
            let mut statements = 0;
            let profile = configure_profile_counted(&connection, &mut statements)?;
            create_full_schema(&connection, role, storage_id, durable_storage_id, &profile)?;
            Ok(())
        })();
        match prepared {
            Ok(()) => {
                drop(connection);
                match Self::open_exact(path, role, durable_storage_id) {
                    Ok(storage) => {
                        if storage.owned_file_identity()?
                            != opened_file_identity(&custody).map_err(crate::io_engine_error)?
                        {
                            return Err(EngineError::InvalidRecord("FullStorage create identity"));
                        }
                        if let Err(error) = storage.verify_integrity() {
                            drop(storage);
                            remove_custodied_file(path, &custody);
                            return Err(error);
                        }
                        Ok(storage)
                    }
                    Err(error) => {
                        remove_custodied_file(path, &custody);
                        Err(error)
                    }
                }
            }
            Err(error) => {
                drop(connection);
                remove_custodied_file(path, &custody);
                Err(error)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn create_durable_with_injector(
        path: &Path,
        inject: &mut dyn FnMut(&'static str) -> EngineResult<()>,
    ) -> EngineResult<Self> {
        Self::create_with_injector(path, StoreRole::Durable, None, None, inject)
    }
}

fn create_full_schema(
    connection: &Connection,
    role: StoreRole,
    storage_id: Option<[u8; 32]>,
    durable_storage_id: Option<[u8; 32]>,
    profile: &SqliteProfile,
) -> EngineResult<()> {
    connection
        .execute_batch("BEGIN EXCLUSIVE")
        .map_err(map_sqlite_error)?;
    let result = (|| {
        for partition in FULL_SCHEMA.table_partitions {
            for (_, sql) in *partition {
                connection.execute_batch(sql).map_err(map_sqlite_error)?;
            }
        }
        for (_, sql) in FULL_SCHEMA.index_schemas {
            connection.execute_batch(sql).map_err(map_sqlite_error)?;
        }
        let mut storage_id = match storage_id {
            Some(storage_id) => storage_id,
            None => random_id(connection)?,
        };
        if role == StoreRole::DurableCache && Some(storage_id) == durable_storage_id {
            storage_id = random_id(connection)?;
        }
        let durable_storage_id = match (role, durable_storage_id) {
            (StoreRole::Durable, None) => storage_id,
            (StoreRole::DurableCache, Some(durable_storage_id))
                if durable_storage_id != storage_id =>
            {
                durable_storage_id
            }
            _ => return Err(EngineError::SchemaMismatch),
        };
        connection
            .execute(
                "INSERT INTO layerfs_store_meta
                 (store_id, format_marker, schema_version, store_role, storage_id,
                  durable_storage_id, next_inode_serial, trusted_history,
                  journal_mode, synchronous, temp_store, mmap_size)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7, ?8, ?9)",
                params![
                    FULL_SCHEMA.format_marker,
                    FULL_SCHEMA.schema_version,
                    role.as_str(),
                    storage_id.as_slice(),
                    durable_storage_id.as_slice(),
                    &profile.journal_mode,
                    profile.synchronous,
                    profile.temp_store,
                    profile.mmap_size,
                ],
            )
            .map_err(map_sqlite_error)?;
        require_foreign_keys(connection)?;
        connection.execute_batch("COMMIT").map_err(map_sqlite_error)
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK");
    }
    result
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

fn random_id(connection: &Connection) -> EngineResult<[u8; 32]> {
    connection
        .query_row("SELECT randomblob(32)", [], |row| row.get::<_, Vec<u8>>(0))
        .map_err(map_sqlite_error)?
        .try_into()
        .map_err(|_| EngineError::SchemaMismatch)
}

fn remove_custodied_file(path: &Path, custody: &fs::File) {
    if let Ok(identity) = opened_file_identity(custody) {
        let _ = NativeGenerationDriver.remove_file_if_identity(path, &identity);
    }
}
