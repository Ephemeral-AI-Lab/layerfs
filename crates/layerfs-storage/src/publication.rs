use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::legacy_store::{checked_add, Engine, SQL_FAMILY_PUBLICATION};
use crate::integrity::{self, IntegrityMode};
use crate::object::{
    core_store_error, put_canonical_object_on_connection,
    with_authenticated_canonical_on_connection,
};
use crate::refs::{read_ref_on_connection, validate_ref_name, RefState};
use crate::sqlite::connection::{
    add_verification_progress_counters, mark_known_trusted_history, read_ref_reconcile_readonly,
    reopen_store_primary, ConnectionGuard,
};
use layerfs_core::content::rope::ObjectStore;
use layerfs_core::inode::InodeId;
use layerfs_core::namespace_codec::decode_namespace_root;
use layerfs_core::{CoreError, ObjectId};
use rusqlite::params;
use std::cell::Cell;

pub struct Publication<'a> {
    engine: &'a Engine,
    connection: ConnectionGuard<'a>,
    name: String,
    expected: Option<RefState>,
    verified_retained_root: Option<ObjectId>,
    active: bool,
}

impl Engine {
    pub fn begin_publication<'a>(
        &'a self,
        expected: Option<&RefState>,
        name: &str,
    ) -> EngineResult<Publication<'a>> {
        validate_ref_name(name)?;
        if expected.is_some_and(|state| state.name != name) {
            return Err(EngineError::InvalidRecord("expected ref name"));
        }
        let mut connection = self.lock_write_connection()?;
        if !connection.transaction {
            connection
                .execute_batch("BEGIN IMMEDIATE")
                .map_err(map_sqlite_error)?;
            connection.transaction = true;
            self.bump(|counters| {
                checked_add(&mut counters.transactions_started, 1)?;
                checked_add(&mut counters.publication_transactions_started, 1)?;
                checked_add(&mut counters.statements, 1)?;
                checked_add(&mut counters.publication_statements, 1)
            })?;
        }
        self.mark_statement()?;
        let actual = read_ref_on_connection(&connection, name)?;
        if actual.as_ref() != expected {
            let store_id = self.store_id()?;
            let discarded = finalize_rollback(self, &mut connection);
            if discarded {
                let observed = read_ref_reconcile_readonly(self, name, store_id);
                if observed.as_ref().ok() != Some(&actual) {
                    return Err(EngineError::AmbiguousDurability);
                }
                restore_primary(self, &mut connection, store_id, name, &actual)?;
            }
            return Err(EngineError::PublicationConflict);
        }
        Ok(Publication {
            engine: self,
            connection,
            name: name.to_owned(),
            expected: expected.cloned(),
            verified_retained_root: None,
            active: true,
        })
    }
}

impl Publication<'_> {
    pub fn allocate_inode_id(&mut self) -> EngineResult<InodeId> {
        self.ensure_active()?;
        self.engine.mark_statement()?;
        let serial = self
            .connection
            .query_row(
                "SELECT next_inode_serial FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        let serial =
            u64::try_from(serial).map_err(|_| EngineError::InvalidRecord("inode serial"))?;
        let next = serial.checked_add(1).ok_or(EngineError::CounterOverflow)?;
        self.engine.mark_statement()?;
        self.connection
            .execute(
                "UPDATE layerfs_authority SET next_inode_serial = ?1 WHERE authority_id = 1",
                params![i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?],
            )
            .map_err(map_sqlite_error)?;
        Ok(InodeId::allocate(self.engine.store_id()?, serial))
    }

    pub fn put_object(&mut self, canonical: &[u8]) -> EngineResult<ObjectId> {
        self.ensure_active()?;
        let (id, _) = put_canonical_object_on_connection(self.engine, &self.connection, canonical)?;
        Ok(id)
    }

    pub fn publish_namespace(mut self, canonical: &[u8]) -> EngineResult<RefState> {
        decode_namespace_root(canonical).map_err(EngineError::Core)?;
        let root = self.put_object(canonical)?;
        self.commit_ref(root)
    }

    pub(crate) fn retain_existing_root(&mut self, root: ObjectId) -> EngineResult<()> {
        self.ensure_active()?;
        self.engine.mark_statement()?;
        let retained = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_retained_roots WHERE root_id = ?1)",
                params![root.as_bytes().as_slice()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)?;
        if !retained {
            return Err(EngineError::MissingRoot(root));
        }
        with_authenticated_canonical_on_connection(
            self.engine,
            &self.connection,
            root,
            true,
            true,
            |_, bytes| {
                decode_namespace_root(bytes)
                    .map(|_| ())
                    .map_err(EngineError::Core)
            },
        )?;
        self.verified_retained_root = Some(root);
        Ok(())
    }

    pub(crate) fn commit_ref(mut self, root: ObjectId) -> EngineResult<RefState> {
        self.ensure_active()?;
        if let Some(expected) = &self.expected {
            if expected.root == root {
                let expected = expected.clone();
                let store_id = self.engine.store_id()?;
                let discarded = finalize_rollback(self.engine, &mut self.connection);
                self.active = false;
                if discarded {
                    let observed = read_ref_reconcile_readonly(self.engine, &self.name, store_id);
                    if observed.as_ref().ok() != Some(&Some(expected.clone())) {
                        return Err(EngineError::AmbiguousDurability);
                    }
                    restore_primary(
                        self.engine,
                        &mut self.connection,
                        store_id,
                        &self.name,
                        &Some(expected.clone()),
                    )?;
                }
                return Ok(expected);
            }
        }
        if self.engine.mode == IntegrityMode::Verified && self.verified_retained_root != Some(root)
        {
            let statements = Cell::new(0);
            let failed = Cell::new(integrity::VerificationObservation::default());
            let observation = integrity::verify_root(
                &self.connection,
                &self.engine.path,
                self.engine.store_id()?,
                root,
                &statements,
                &failed,
            );
            self.engine
                .mark_family_sql(SQL_FAMILY_PUBLICATION, statements.get())?;
            if observation.is_err() {
                self.engine
                    .bump(|counters| add_verification_progress_counters(counters, failed.get()))?;
            }
            let observation = observation?;
            self.engine.bump(|counters| {
                checked_add(&mut counters.root_verifications, 1)?;
                checked_add(&mut counters.root_verification_objects, observation.objects)?;
                checked_add(&mut counters.root_verification_bytes, observation.bytes)?;
                checked_add(&mut counters.fetched_rows, observation.fetched_rows)?;
                checked_add(
                    &mut counters.fetched_row_authentication_passes,
                    observation.authentication_passes,
                )?;
                checked_add(
                    &mut counters.fetched_row_role_decode_passes,
                    observation.role_decode_passes,
                )?;
                checked_add(&mut counters.scratch_tables, observation.scratch_tables)?;
                checked_add(
                    &mut counters.scratch_statements,
                    observation.scratch_statements,
                )?;
                checked_add(&mut counters.scratch_rows, observation.scratch_rows)?;
                counters.scratch_high_water_bytes = counters
                    .scratch_high_water_bytes
                    .max(observation.scratch_bytes);
                checked_add(
                    &mut counters.objects_validated,
                    observation.authentication_passes,
                )?;
                checked_add(&mut counters.object_bytes_read, observation.bytes)?;
                checked_add(&mut counters.publication_closure_passes, 1)?;
                checked_add(&mut counters.namespace_graph_verification_passes, 1)
            })?;
        }
        let generation = self.expected.as_ref().map_or(Ok(0), |state| {
            state
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)
        })?;
        self.engine.mark_statement()?;
        self.connection.execute("INSERT INTO layerfs_retained_roots (root_id) VALUES (?1) ON CONFLICT(root_id) DO NOTHING", params![root.as_bytes().as_slice()]).map_err(map_sqlite_error)?;
        if self.expected.is_some() {
            self.engine.mark_statement()?;
            self.connection
                .execute(
                    "UPDATE layerfs_refs SET generation = ?1, root_id = ?2 WHERE name = ?3",
                    params![
                        i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
                        root.as_bytes().as_slice(),
                        &self.name
                    ],
                )
                .map_err(map_sqlite_error)?;
        } else {
            self.engine.mark_statement()?;
            self.connection
                .execute(
                    "INSERT INTO layerfs_refs (name, generation, root_id) VALUES (?1, 0, ?2)",
                    params![&self.name, root.as_bytes().as_slice()],
                )
                .map_err(map_sqlite_error)?;
        }
        if self.engine.mode == IntegrityMode::TrustedLocalDev {
            self.engine.mark_statement()?;
            mark_known_trusted_history(&self.connection)?;
        }
        let store_id = self.engine.store_id()?;
        self.engine.mark_statement()?;
        match self.engine.commit_dispatch.commit(&self.connection) {
            Ok(()) => {
                self.active = false;
                self.connection.transaction = false;
                self.engine.bump(|counters| {
                    checked_add(&mut counters.transactions_committed, 1)?;
                    checked_add(&mut counters.publication_commits, 1)
                })?;
                Ok(RefState {
                    name: self.name.clone(),
                    generation,
                    root,
                })
            }
            Err(error) => {
                let _ = self.engine.note_sqlite_error(&error);
                finalize_rollback(self.engine, &mut self.connection);
                self.connection.guard.take();
                self.active = false;
                let observed = read_ref_reconcile_readonly(self.engine, &self.name, store_id);
                match observed {
                    Ok(Some(state)) if state.generation == generation && state.root == root => {
                        restore_primary(
                            self.engine,
                            &mut self.connection,
                            store_id,
                            &self.name,
                            &Some(state.clone()),
                        )?;
                        self.engine.bump(|counters| {
                            checked_add(&mut counters.transactions_committed, 1)?;
                            checked_add(&mut counters.publication_commits, 1)
                        })?;
                        Ok(state)
                    }
                    Ok(observed) if observed == self.expected => {
                        restore_primary(
                            self.engine,
                            &mut self.connection,
                            store_id,
                            &self.name,
                            &observed,
                        )?;
                        Err(map_sqlite_error(error))
                    }
                    _ => Err(EngineError::AmbiguousDurability),
                }
            }
        }
    }

    fn ensure_active(&self) -> EngineResult<()> {
        if self.active {
            Ok(())
        } else {
            Err(EngineError::InvalidTransaction)
        }
    }
}

impl ObjectStore for Publication<'_> {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        with_authenticated_canonical_on_connection(
            self.engine,
            &self.connection,
            id,
            false,
            false,
            |_, bytes| Ok(bytes.to_vec()),
        )
        .map_err(core_store_error)
    }

    fn put(&mut self, canonical: &[u8]) -> Result<ObjectId, CoreError> {
        self.put_object(canonical).map_err(core_store_error)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> Result<T, CoreError>
    where
        F: FnOnce(&[u8]) -> Result<T, CoreError>,
    {
        with_authenticated_canonical_on_connection(
            self.engine,
            &self.connection,
            id,
            true,
            true,
            |_, bytes| callback(bytes).map_err(EngineError::Core),
        )
        .map_err(core_store_error)
    }
}

impl Drop for Publication<'_> {
    fn drop(&mut self) {
        if self.active {
            let store_id = self.engine.store_id().ok();
            let discarded = finalize_rollback(self.engine, &mut self.connection);
            if discarded {
                if let Some(store_id) = store_id {
                    let observed = read_ref_reconcile_readonly(self.engine, &self.name, store_id);
                    if let Ok(observed) = observed {
                        if observed == self.expected {
                            let _ = restore_primary(
                                self.engine,
                                &mut self.connection,
                                store_id,
                                &self.name,
                                &observed,
                            );
                        }
                    }
                }
            }
            self.active = false;
        }
    }
}

fn finalize_rollback(engine: &Engine, connection: &mut ConnectionGuard<'_>) -> bool {
    let active = !connection.is_autocommit();
    if active {
        engine.bump_best_effort(|counters| {
            checked_add(&mut counters.statements, 1)?;
            checked_add(&mut counters.publication_statements, 1)
        });
    }
    let failed = active && engine.commit_dispatch.rollback(connection).is_err();
    connection.transaction = false;
    if failed {
        connection.guard.take();
    } else if active {
        engine.bump_best_effort(|counters| {
            checked_add(&mut counters.transactions_rolled_back, 1)?;
            checked_add(&mut counters.publication_transactions_rolled_back, 1)
        });
    }
    failed
}

fn restore_primary(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    store_id: [u8; 32],
    ref_name: &str,
    expected_ref: &Option<RefState>,
) -> EngineResult<()> {
    let reopened = reopen_store_primary(engine, store_id, ref_name, expected_ref)
        .map_err(|_| EngineError::AmbiguousDurability)?;
    *connection.guard = Some(reopened);
    Ok(())
}

#[cfg(test)]
mod tests;
