//! Legacy Full compaction and verified backup copy pipeline.

use super::legacy::{
    add_compaction_verification_counters, authenticate_complete_object_index,
    candidate_auxiliary_bytes, reject_legacy_compaction_state,
};
use crate::generation;
use crate::generation::{NativeGenerationDriver, StoreGenerationDriver};
use crate::integrity;
use crate::schema;
use crate::{
    add_verification_progress_counters, checked_add, configure_profile_counted,
    initialize_schema_counted, io_engine_error, map_sqlite_error, CompactionStorageObservation,
    Engine, EngineError, EngineResult, FullStorage, SchemaState, SqliteErrorKind, BUSY_TIMEOUT,
    FULL_SCHEMA, SQL_FAMILY_COMPACTION,
};
use rusqlite::{params, Connection};
use std::cell::Cell;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

impl Engine {
    pub fn compact_to(&self, destination: &Path) -> EngineResult<()> {
        self.compact_to_observed(destination).map(drop)
    }

    pub fn backup_to(&self, destination: &Path) -> EngineResult<()> {
        let parent = destination
            .parent()
            .ok_or(EngineError::InvalidRecord("backup path"))?;
        fs::create_dir_all(parent).map_err(io_engine_error)?;
        let parent = fs::canonicalize(parent).map_err(io_engine_error)?;
        let name = destination
            .file_name()
            .ok_or(EngineError::InvalidRecord("backup path"))?;
        let destination = parent.join(name);
        if fs::symlink_metadata(&destination).is_ok() {
            return Err(EngineError::InvalidRecord("backup destination exists"));
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| EngineError::InvalidRecord("system clock"))?
            .as_nanos();
        let temporary = parent.join(format!(
            ".layerfs-backup-{}-{nonce}.sqlite",
            std::process::id()
        ));
        let result = (|| {
            let primary = self.connection.lock().map_err(|_| EngineError::Sqlite {
                kind: SqliteErrorKind::Other,
                message: "connection mutex poisoned".to_owned(),
            })?;
            let connection = primary.as_ref().ok_or(EngineError::AmbiguousDurability)?;
            self.mark_family_sql(SQL_FAMILY_COMPACTION, 1)?;
            connection
                .execute(
                    "VACUUM INTO ?1",
                    params![temporary
                        .to_str()
                        .ok_or(EngineError::InvalidRecord("backup path"))?],
                )
                .map_err(map_sqlite_error)?;
            drop(primary);
            fs::File::open(&temporary)
                .and_then(|file| file.sync_all())
                .map_err(io_engine_error)?;
            let verified = Engine::open(&temporary)?;
            if verified.store_id()? != self.store_id()? {
                return Err(EngineError::InvalidRecord("backup StoreId"));
            }
            drop(verified);
            fs::hard_link(&temporary, &destination).map_err(io_engine_error)?;
            fs::remove_file(&temporary).map_err(io_engine_error)?;
            fs::File::open(&parent)
                .and_then(|directory| directory.sync_all())
                .map_err(io_engine_error)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub(crate) fn compact_to_observed(
        &self,
        destination: &Path,
    ) -> EngineResult<CompactionStorageObservation> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(io_engine_error)?;
        let result = self.compact_to_created(destination);
        if result.is_err() {
            let _ = fs::remove_file(destination);
            let mut journal = destination.as_os_str().to_os_string();
            journal.push("-journal");
            let _ = fs::remove_file(PathBuf::from(journal));
        }
        result
    }

    fn compact_to_created(&self, destination: &Path) -> EngineResult<CompactionStorageObservation> {
        let old_generation_bytes = fs::metadata(&self.path).map_err(io_engine_error)?.len();
        let source = self.lock_connection()?;
        self.sql_family_scope
            .store(SQL_FAMILY_COMPACTION, Ordering::Release);
        self.mark_compaction_sql(1)?;
        reject_legacy_compaction_state(&source)?;
        self.mark_compaction_sql(1)?;
        authenticate_complete_object_index(&source)?;
        let candidate = Connection::open(destination).map_err(map_sqlite_error)?;
        candidate
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(map_sqlite_error)?;
        let mut statements = 0;
        let profile = configure_profile_counted(&candidate, &mut statements);
        self.mark_compaction_sql(statements)?;
        let profile = profile?;
        statements = 0;
        let initialized =
            initialize_schema_counted(&candidate, &profile, SchemaState::Empty, &mut statements);
        self.mark_compaction_sql(statements)?;
        initialized?;
        let source_path = self
            .path
            .to_str()
            .ok_or(EngineError::InvalidRecord("non-UTF-8 Store path"))?;
        self.mark_compaction_sql(1)?;
        candidate
            .execute("ATTACH DATABASE ?1 AS source", params![source_path])
            .map_err(map_sqlite_error)?;
        self.mark_compaction_sql(1)?;
        candidate.execute_batch("BEGIN").map_err(map_sqlite_error)?;
        let retained_statements = Cell::new(0);
        let retained_failed = Cell::new(integrity::VerificationObservation::default());
        let retained = integrity::retained_union(
            &source,
            &self.path,
            self.store_id,
            &retained_statements,
            &retained_failed,
        );
        self.mark_compaction_sql(retained_statements.get())?;
        if retained.is_err() {
            self.bump(|counters| {
                add_verification_progress_counters(counters, retained_failed.get())
            })?;
        }
        let retained = match retained {
            Ok(retained) => retained,
            Err(error) => {
                self.rollback_compaction_candidate(&candidate);
                return Err(error);
            }
        };
        let mark_database_bytes = match retained.work.storage_bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = self.finish_retained_compaction(retained);
                self.rollback_compaction_candidate(&candidate);
                return Err(error);
            }
        };
        let copied = self.copy_retained_to_candidate(&source, retained, &candidate);
        if let Err(error) = copied {
            self.rollback_compaction_candidate(&candidate);
            return Err(error);
        }
        let candidate_journal_temp_peak_bytes = candidate_auxiliary_bytes(destination);
        self.mark_compaction_sql(1)?;
        if let Err(error) = candidate.execute_batch("COMMIT") {
            self.rollback_compaction_candidate(&candidate);
            return Err(map_sqlite_error(error));
        }
        let verification_statements = Cell::new(0);
        let verification_failed = Cell::new(integrity::VerificationObservation::default());
        let verification = integrity::verify_retained_union_observed_counted(
            &candidate,
            destination,
            self.store_id,
            &verification_statements,
            &verification_failed,
        );
        self.mark_compaction_sql(verification_statements.get())?;
        if verification.is_err() {
            self.bump(|counters| {
                add_verification_progress_counters(counters, verification_failed.get())
            })?;
        }
        let verification = verification?;
        self.bump(|counters| {
            add_compaction_verification_counters(counters, verification.verification)
        })?;
        let verification_scratch_peak_bytes = verification.peak_bytes;
        self.mark_compaction_sql(1)?;
        candidate
            .execute_batch("DETACH DATABASE source")
            .map_err(map_sqlite_error)?;
        drop(candidate);
        fs::File::open(destination)
            .and_then(|file| file.sync_all())
            .map_err(io_engine_error)?;
        let new_generation_bytes = fs::metadata(destination).map_err(io_engine_error)?.len();
        let selector_temporary_bytes = generation::SELECTOR_BYTES as u64;
        let total_peak_bytes = old_generation_bytes
            .checked_add(new_generation_bytes)
            .and_then(|value| value.checked_add(mark_database_bytes))
            .and_then(|value| value.checked_add(candidate_journal_temp_peak_bytes))
            .and_then(|value| value.checked_add(verification_scratch_peak_bytes))
            .and_then(|value| value.checked_add(selector_temporary_bytes))
            .ok_or(EngineError::CounterOverflow)?;
        Ok(CompactionStorageObservation {
            old_generation_bytes,
            new_generation_bytes,
            mark_database_bytes,
            candidate_journal_temp_peak_bytes,
            verification_scratch_peak_bytes,
            selector_temporary_bytes,
            total_peak_bytes,
        })
    }

    pub(crate) fn copy_compaction_metadata(&self, candidate: &Connection) -> EngineResult<()> {
        for sql in [
            "DELETE FROM layerfs_store_meta",
            "DELETE FROM layerfs_authority",
            "INSERT INTO layerfs_store_meta SELECT * FROM source.layerfs_store_meta",
            "UPDATE layerfs_store_meta SET visible_root = NULL",
            "INSERT INTO layerfs_authority SELECT * FROM source.layerfs_authority",
            "INSERT INTO layerfs_refs SELECT * FROM source.layerfs_refs",
            "INSERT INTO layerfs_retained_roots SELECT * FROM source.layerfs_retained_roots",
            "INSERT INTO layerfs_deltas SELECT * FROM source.layerfs_deltas",
        ] {
            self.mark_compaction_sql(1)?;
            candidate.execute(sql, []).map_err(map_sqlite_error)?;
        }
        for &(table, _) in schema::PRODUCT_SCHEMAS.into_iter().flatten() {
            self.mark_compaction_sql(1)?;
            candidate
                .execute(
                    &format!("INSERT INTO {table} SELECT * FROM source.{table}"),
                    [],
                )
                .map_err(map_sqlite_error)?;
        }
        Ok(())
    }

    fn rollback_compaction_candidate(&self, candidate: &Connection) {
        self.bump_best_effort(|counters| {
            checked_add(&mut counters.statements, 1)?;
            checked_add(&mut counters.compaction_statements, 1)
        });
        let _ = candidate.execute_batch("ROLLBACK");
    }

    pub(crate) fn copy_retained_to_candidate(
        &self,
        source: &Connection,
        retained: integrity::RetainedUnion,
        candidate: &Connection,
    ) -> EngineResult<()> {
        let copied = (|| {
            self.copy_compaction_metadata(candidate)?;
            let mut select = source
                .prepare(
                    "SELECT kind, canonical_length, canonical_bytes FROM layerfs_objects WHERE object_id = ?1",
                )
                .map_err(map_sqlite_error)?;
            let mut insert = candidate
                .prepare(
                    "INSERT INTO layerfs_objects (object_id, kind, canonical_length, canonical_bytes) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(object_id) DO NOTHING",
                )
                .map_err(map_sqlite_error)?;
            retained.work.for_each_key(|key| {
                if key.len() != 34 {
                    return Err(EngineError::InvalidRecord("closure key"));
                }
                let id = &key[..32];
                self.mark_compaction_sql(1)?;
                let row = select
                    .query_row(params![id], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, Vec<u8>>(2)?,
                        ))
                    })
                    .map_err(map_sqlite_error)?;
                self.mark_compaction_sql(1)?;
                insert
                    .execute(params![id, row.0, row.1, row.2])
                    .map_err(map_sqlite_error)?;
                Ok(())
            })
        })();
        let finished = self.finish_retained_compaction(retained);
        match copied {
            Err(error) => Err(error),
            Ok(()) => finished,
        }
    }

    fn finish_retained_compaction(&self, retained: integrity::RetainedUnion) -> EngineResult<()> {
        let work = retained.work.finish();
        self.bump(|counters| {
            add_compaction_verification_counters(counters, retained.observation)?;
            if let Ok(work) = &work {
                checked_add(&mut counters.scratch_tables, work.tables)?;
                checked_add(&mut counters.scratch_statements, work.statements)?;
                checked_add(&mut counters.scratch_rows, work.rows)?;
                counters.scratch_high_water_bytes =
                    counters.scratch_high_water_bytes.max(work.high_water_bytes);
            }
            Ok(())
        })?;
        work.map(drop)
    }
}

impl FullStorage {
    pub(crate) fn create_verified_full_copy(
        &self,
        destination: &Path,
    ) -> EngineResult<VerifiedFullCopy> {
        self.require_authority()?;
        if destination == self.path() || destination.exists() {
            return Err(EngineError::InvalidRecord("Full copy destination"));
        }
        let source_identity = self.owned_file_identity()?;
        self.lock_connection()?
            .execute(
                "VACUUM INTO ?1",
                params![destination
                    .to_str()
                    .ok_or(EngineError::InvalidRecord("Full copy path"))?],
            )
            .map_err(map_sqlite_error)?;
        fs::File::open(destination)
            .and_then(|file| file.sync_all())
            .map_err(io_engine_error)?;
        let copy = self.verify_full_copy(destination)?;
        if self.owned_file_identity()? != source_identity {
            return Err(EngineError::InvalidRecord("Full copy source identity"));
        }
        Ok(copy)
    }

    pub(crate) fn verify_full_copy(&self, path: &Path) -> EngineResult<VerifiedFullCopy> {
        self.require_authority()?;
        let storage = FullStorage::open_durable_verified(path)?;
        if storage.storage_id() != self.storage_id() || storage.profile() != self.profile() {
            return Err(EngineError::InvalidRecord("Full copy identity"));
        }
        let connection = self.lock_connection()?;
        connection
            .execute(
                "ATTACH DATABASE ?1 AS full_copy",
                params![path
                    .to_str()
                    .ok_or(EngineError::InvalidRecord("Full copy path"))?],
            )
            .map_err(map_sqlite_error)?;
        let compared = FULL_SCHEMA.table_names.iter().try_for_each(|table| {
            let sql = format!(
                "SELECT EXISTS(SELECT * FROM main.{table} EXCEPT SELECT * FROM full_copy.{table})
                    OR EXISTS(SELECT * FROM full_copy.{table} EXCEPT SELECT * FROM main.{table})"
            );
            if connection
                .query_row(&sql, [], |row| row.get::<_, bool>(0))
                .map_err(map_sqlite_error)?
            {
                Err(EngineError::InvalidRecord("Full copy rows"))
            } else {
                Ok(())
            }
        });
        let detached = connection
            .execute_batch("DETACH DATABASE full_copy")
            .map_err(map_sqlite_error);
        compared.and(detached)?;
        let file_identity = storage.owned_file_identity()?;
        Ok(VerifiedFullCopy {
            storage,
            file_identity,
        })
    }

    pub fn backup_to(&self, destination: &Path) -> EngineResult<()> {
        self.require_authority()?;
        let parent = destination
            .parent()
            .ok_or(EngineError::InvalidRecord("Full backup path"))?;
        fs::create_dir_all(parent).map_err(io_engine_error)?;
        let parent = fs::canonicalize(parent).map_err(io_engine_error)?;
        let name = destination
            .file_name()
            .ok_or(EngineError::InvalidRecord("Full backup path"))?;
        let destination = parent.join(name);
        if destination.exists() {
            return Err(EngineError::InvalidRecord("Full backup destination exists"));
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| EngineError::InvalidRecord("system clock"))?
            .as_nanos();
        let temporary = parent.join(format!(
            ".layerfs-full-backup-{}-{nonce}.sqlite",
            std::process::id()
        ));
        let result = (|| {
            let verified = self.create_verified_full_copy(&temporary)?;
            fs::hard_link(&temporary, &destination).map_err(io_engine_error)?;
            let identity = verified.file_identity().to_vec();
            drop(verified);
            NativeGenerationDriver
                .remove_file_if_identity(&temporary, &identity)
                .map_err(io_engine_error)?;
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(io_engine_error)
        })();
        if result.is_err() {
            if let Ok(verified) = self.verify_full_copy(&temporary) {
                let identity = verified.file_identity().to_vec();
                drop(verified);
                let _ = NativeGenerationDriver.remove_file_if_identity(&temporary, &identity);
            }
        }
        result
    }
}

pub(crate) struct VerifiedFullCopy {
    storage: FullStorage,
    file_identity: Vec<u8>,
}

impl VerifiedFullCopy {
    pub(crate) fn storage(&self) -> &FullStorage {
        &self.storage
    }

    pub(crate) fn file_identity(&self) -> &[u8] {
        &self.file_identity
    }
}
