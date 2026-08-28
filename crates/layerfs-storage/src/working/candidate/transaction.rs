use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::legacy_store::{checked_add, Engine};
#[cfg(test)]
use crate::object::{
    authenticate_directory_object, decode_delta_parts, delta_record_len, load_root_on_connection,
    put_object_on_connection, root_record_len, visible_root_on_connection,
    write_root_on_connection, DeltaRecord, PutOutcome, RootId, RootRecord,
};
use crate::sqlite::connection::ConnectionGuard;
use layerfs_core::logical::{CandidateRoot, LogicalCounters};
use layerfs_core::ObjectId;
#[cfg(test)]
use rusqlite::{params, OptionalExtension};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WRITER_ID: AtomicU64 = AtomicU64::new(1);

pub struct CandidateWrite<'a> {
    pub(super) engine: &'a Engine,
    pub(super) connection: ConnectionGuard<'a>,
    pub(super) active: bool,
    pub(super) writer_id: u64,
}

impl Engine {
    #[cfg(test)]
    pub(crate) fn put_object_if_absent(
        &self,
        id: ObjectId,
        canonical_bytes: &[u8],
    ) -> EngineResult<PutOutcome> {
        let mut connection = self.lock_write_connection()?;
        let outcome = put_object_on_connection(self, &connection, id, canonical_bytes)?;
        if connection.transaction {
            connection
                .execute_batch("COMMIT")
                .map_err(map_sqlite_error)?;
            connection.transaction = false;
        }
        Ok(outcome)
    }

    #[cfg(test)]
    pub(crate) fn begin_capture(&self, parent: Option<RootId>) -> EngineResult<Capture<'_>> {
        let mut connection = self.lock_write_connection()?;
        self.mark_statement()?;
        if !connection.transaction {
            if let Err(error) = connection.execute_batch("BEGIN IMMEDIATE") {
                self.note_sqlite_error(&error)?;
                return Err(map_sqlite_error(error));
            }
            connection.transaction = true;
        }
        self.bump(|counters| checked_add(&mut counters.transactions_started, 1))?;

        let current = match visible_root_on_connection(self, &connection) {
            Ok(current) => current,
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                connection.transaction = false;
                self.bump_best_effort(|counters| {
                    checked_add(&mut counters.transactions_rolled_back, 1)
                });
                return Err(error);
            }
        };
        if current != parent {
            let _ = connection.execute_batch("ROLLBACK");
            connection.transaction = false;
            self.bump_best_effort(|counters| {
                checked_add(&mut counters.transactions_rolled_back, 1)
            });
            return Err(EngineError::ParentMismatch {
                expected: parent,
                actual: current,
            });
        }
        if let Some(root) = current {
            let record = match load_root_on_connection(self, &connection, root) {
                Ok(record) => record,
                Err(error) => {
                    let _ = connection.execute_batch("ROLLBACK");
                    connection.transaction = false;
                    self.bump_best_effort(|counters| {
                        checked_add(&mut counters.transactions_rolled_back, 1)
                    });
                    return Err(error);
                }
            };
            if let Err(error) =
                authenticate_directory_object(self, &connection, record.directory_object)
            {
                let _ = connection.execute_batch("ROLLBACK");
                connection.transaction = false;
                self.bump_best_effort(|counters| {
                    checked_add(&mut counters.transactions_rolled_back, 1)
                });
                return Err(error);
            }
        }
        Ok(Capture {
            engine: self,
            connection,
            parent,
            delta: None,
            active: true,
            #[cfg(test)]
            fault: None,
        })
    }
}

#[cfg(test)]
pub(crate) struct Capture<'a> {
    engine: &'a Engine,
    connection: ConnectionGuard<'a>,
    parent: Option<RootId>,
    delta: Option<DeltaRecord>,
    active: bool,
    #[cfg(test)]
    fault: Option<FaultPoint>,
}

#[cfg(test)]
impl<'a> Capture<'a> {
    pub(crate) fn put_object_if_absent(
        &mut self,
        id: ObjectId,
        canonical_bytes: &[u8],
    ) -> EngineResult<PutOutcome> {
        self.ensure_active()?;
        put_object_on_connection(self.engine, &self.connection, id, canonical_bytes)
    }

    pub(crate) fn write_delta(&mut self, delta: &DeltaRecord) -> EngineResult<()> {
        self.ensure_active()?;
        delta.validate()?;
        if delta.parent != self.parent {
            return Err(EngineError::ParentMismatch {
                expected: self.parent,
                actual: delta.parent,
            });
        }
        self.engine.mark_statement()?;
        let mut select = self
            .connection
            .prepare_cached(
                "SELECT format_version, parent_root, child_root, payload
                 FROM layerfs_deltas WHERE delta_id = ?1",
            )
            .map_err(map_sqlite_error)?;
        let existing = select
            .query_row(params![delta.id.as_bytes().as_slice()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })
            .optional()
            .map_err(map_sqlite_error)?;
        if let Some((format_version, parent, child, payload)) = existing {
            if format_version != 0 {
                return Err(EngineError::InvalidRecord("legacy delta format"));
            }
            let existing = decode_delta_parts(delta.id, parent, child, payload)?;
            if existing != *delta {
                return Err(EngineError::ImmutableConflict("delta", delta.id));
            }
            self.delta = Some(delta.clone());
            return Ok(());
        }

        self.engine.mark_statement()?;
        let mut insert = self
            .connection
            .prepare_cached(
                "INSERT INTO layerfs_deltas
                 (delta_id, format_version, parent_root, child_root, payload)
                 VALUES (?1, 0, ?2, ?3, ?4)",
            )
            .map_err(map_sqlite_error)?;
        insert
            .execute(params![
                delta.id.as_bytes().as_slice(),
                delta.parent.map(|id| id.to_bytes().to_vec()),
                delta.child.as_bytes().as_slice(),
                &delta.payload,
            ])
            .map_err(map_sqlite_error)?;
        self.engine.bump(|counters| {
            checked_add(&mut counters.logical_delta_bytes, delta_record_len(delta)?)
        })?;
        self.delta = Some(delta.clone());
        Ok(())
    }

    pub(crate) fn commit_root(mut self, root: RootRecord) -> EngineResult<()> {
        self.ensure_active()?;
        if root.parent != self.parent {
            return Err(EngineError::ParentMismatch {
                expected: self.parent,
                actual: root.parent,
            });
        }
        let delta = self
            .delta
            .as_ref()
            .ok_or(EngineError::InvalidRecord("delta"))?;
        if delta.child != root.id {
            return Err(EngineError::InvalidRecord("root/delta linkage"));
        }
        authenticate_directory_object(self.engine, &self.connection, root.directory_object)?;
        write_root_on_connection(self.engine, &self.connection, &root)?;
        #[cfg(test)]
        if self.fault == Some(FaultPoint::BeforeVisibleRoot) {
            return Err(EngineError::InjectedFailure("visible root"));
        }
        self.engine.mark_statement()?;
        let mut update = self
            .connection
            .prepare_cached("UPDATE layerfs_store_meta SET visible_root = ?1 WHERE store_id = 1")
            .map_err(map_sqlite_error)?;
        let changed = update
            .execute(params![root.id.as_bytes().as_slice()])
            .map_err(map_sqlite_error)?;
        if changed != 1 {
            return Err(EngineError::SchemaMismatch);
        }
        drop(update);
        self.engine.sample_rollback_journal();
        self.engine.mark_statement()?;
        self.connection
            .execute_batch("COMMIT")
            .map_err(map_sqlite_error)?;
        self.active = false;
        self.connection.transaction = false;
        self.engine.bump(|counters| {
            checked_add(&mut counters.transactions_committed, 1)?;
            checked_add(&mut counters.logical_root_bytes, root_record_len(&root)?)
        })?;
        Ok(())
    }

    pub(crate) fn ensure_active(&self) -> EngineResult<()> {
        if self.active {
            Ok(())
        } else {
            Err(EngineError::InvalidTransaction)
        }
    }

    #[cfg(test)]
    pub(crate) fn fail_before_visible_root(&mut self) {
        self.fault = Some(FaultPoint::BeforeVisibleRoot);
    }
}

#[cfg(test)]
impl Drop for Capture<'_> {
    fn drop(&mut self) {
        if self.active && self.connection.execute_batch("ROLLBACK").is_ok() {
            self.active = false;
            self.connection.transaction = false;
            self.engine.bump_best_effort(|counters| {
                checked_add(&mut counters.transactions_rolled_back, 1)
            });
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaultPoint {
    BeforeVisibleRoot,
}

pub struct TrustedCandidate {
    pub(super) candidate: CandidateRoot,
    pub(super) store_id: [u8; 32],
    pub(super) writer_id: u64,
}

impl TrustedCandidate {
    pub fn root(&self) -> ObjectId {
        self.candidate.root()
    }

    pub fn counters(&self) -> LogicalCounters {
        self.candidate.counters()
    }
}

impl Engine {
    pub fn begin_candidate_write(&self) -> EngineResult<CandidateWrite<'_>> {
        let mut connection = self.lock_write_connection()?;
        if !connection.transaction {
            connection
                .execute_batch("BEGIN IMMEDIATE")
                .map_err(map_sqlite_error)?;
            connection.transaction = true;
            self.bump(|counters| checked_add(&mut counters.transactions_started, 1))?;
        }
        Ok(CandidateWrite {
            engine: self,
            connection,
            active: true,
            writer_id: NEXT_WRITER_ID
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .map_err(|_| EngineError::CounterOverflow)?,
        })
    }
}

impl Drop for CandidateWrite<'_> {
    fn drop(&mut self) {
        if self.active && self.connection.transaction {
            if self
                .engine
                .commit_dispatch
                .rollback(&self.connection)
                .is_ok()
            {
                let _ = self
                    .engine
                    .bump(|counters| checked_add(&mut counters.transactions_rolled_back, 1));
            }
            self.connection.transaction = false;
            self.active = false;
        }
    }
}
