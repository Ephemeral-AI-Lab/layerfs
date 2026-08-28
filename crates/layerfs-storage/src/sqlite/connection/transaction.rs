use crate::integrity;
use crate::{
    add_retained_scrub_counters, add_verification_progress_counters, checked_add,
    clear_known_trusted_history, map_sqlite_error, mark_sql_family, observe_time,
    sqlite_error_kind, trusted_history, Engine, EngineCounters, EngineError, EngineResult,
    SqliteErrorKind, SQL_FAMILY_COMPACTION, SQL_FAMILY_LIVE_INTEGRITY, SQL_FAMILY_NONE,
    SQL_FAMILY_PRIMARY_READ, SQL_FAMILY_PUBLICATION,
};
use rusqlite::Connection;
use std::cell::Cell;
#[cfg(test)]
use std::fs;
use std::ops::{Deref, DerefMut};
#[cfg(test)]
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::MutexGuard;
use std::time::Instant;

pub(crate) trait CommitDispatch: Send + Sync {
    fn commit(&self, connection: &Connection) -> rusqlite::Result<()>;

    fn rollback(&self, connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch("ROLLBACK")
    }
}

impl Engine {
    pub(crate) fn lock_connection(&self) -> EngineResult<ConnectionGuard<'_>> {
        self.lock_connection_mode(false)
    }

    pub(crate) fn lock_write_connection(&self) -> EngineResult<ConnectionGuard<'_>> {
        self.lock_connection_mode(true)
    }

    pub(crate) fn lock_connection_mode(&self, write: bool) -> EngineResult<ConnectionGuard<'_>> {
        let wait_started = Instant::now();
        let connection = self.connection.lock().map_err(|_| EngineError::Sqlite {
            kind: SqliteErrorKind::Other,
            message: "connection mutex poisoned".to_owned(),
        })?;
        observe_time(&self.timings.connection_mutex_wait_ns, wait_started);
        if connection.is_none() {
            return Err(EngineError::AmbiguousDurability);
        }
        let mut connection = ConnectionGuard {
            engine: self,
            guard: connection,
            transaction: false,
            commit_scrub_on_drop: false,
            integrity_transaction: false,
        };
        let family = if write {
            SQL_FAMILY_PUBLICATION
        } else {
            SQL_FAMILY_PRIMARY_READ
        };
        self.sql_family_scope.store(family, Ordering::Release);
        let trust_started = Instant::now();
        if self.mode == integrity::IntegrityMode::Verified {
            connection
                .execute_batch(if write { "BEGIN IMMEDIATE" } else { "BEGIN" })
                .map_err(map_sqlite_error)?;
            connection.transaction = true;
            if write {
                self.bump(|counters| {
                    checked_add(&mut counters.transactions_started, 1)?;
                    checked_add(&mut counters.publication_transactions_started, 1)?;
                    checked_add(&mut counters.statements, 1)?;
                    mark_sql_family(counters, SQL_FAMILY_PUBLICATION, 1)
                })?;
            } else {
                connection.integrity_transaction = true;
                self.integrity_scope.store(true, Ordering::Release);
                self.bump(|counters| {
                    checked_add(&mut counters.integrity_transactions_started, 1)?;
                    checked_add(&mut counters.statements, 1)?;
                    mark_sql_family(counters, SQL_FAMILY_PRIMARY_READ, 1)?;
                    checked_add(&mut counters.integrity_statements, 1)
                })?;
            }
            if write {
                self.mark_statement()?;
            } else {
                self.mark_integrity_sql(1)?;
            }
            if trusted_history(&connection)? {
                if !write {
                    connection
                        .execute_batch("ROLLBACK; BEGIN IMMEDIATE")
                        .map_err(map_sqlite_error)?;
                    self.bump(|counters| {
                        checked_add(&mut counters.integrity_transactions_rolled_back, 1)?;
                        checked_add(&mut counters.integrity_transactions_started, 1)?;
                        checked_add(&mut counters.statements, 2)?;
                        mark_sql_family(counters, SQL_FAMILY_PRIMARY_READ, 1)?;
                        mark_sql_family(counters, SQL_FAMILY_LIVE_INTEGRITY, 1)?;
                        checked_add(&mut counters.integrity_statements, 2)
                    })?;
                    self.sql_family_scope
                        .store(SQL_FAMILY_LIVE_INTEGRITY, Ordering::Release);
                }
                let scrub_statements = Cell::new(0);
                let scrub_failed = Cell::new(integrity::VerificationObservation::default());
                let scrubbed = integrity::verify_retained_union_observed_counted(
                    &connection,
                    &self.path,
                    self.store_id,
                    &scrub_statements,
                    &scrub_failed,
                );
                self.bump(|counters| {
                    checked_add(&mut counters.integrity_statements, scrub_statements.get())?;
                    checked_add(&mut counters.statements, scrub_statements.get())?;
                    mark_sql_family(counters, SQL_FAMILY_LIVE_INTEGRITY, scrub_statements.get())
                })?;
                if scrubbed.is_err() {
                    self.bump(|counters| {
                        add_verification_progress_counters(counters, scrub_failed.get())
                    })?;
                }
                let scrubbed = scrubbed.and_then(|observation| {
                    self.bump(|counters| {
                        checked_add(&mut counters.statements, 1)?;
                        mark_sql_family(counters, SQL_FAMILY_LIVE_INTEGRITY, 1)?;
                        checked_add(&mut counters.integrity_statements, 1)
                    })?;
                    clear_known_trusted_history(&connection)?;
                    self.bump(|counters| {
                        add_retained_scrub_counters(counters, observation.verification)
                    })
                });
                if let Err(error) = scrubbed {
                    if connection.integrity_transaction {
                        self.bump_best_effort(|counters| {
                            checked_add(&mut counters.statements, 1)?;
                            mark_sql_family(counters, SQL_FAMILY_LIVE_INTEGRITY, 1)?;
                            checked_add(&mut counters.integrity_statements, 1)
                        });
                    } else if write {
                        self.bump_best_effort(|counters| {
                            checked_add(&mut counters.statements, 1)?;
                            mark_sql_family(counters, SQL_FAMILY_PUBLICATION, 1)
                        });
                    }
                    let rollback = self.commit_dispatch.rollback(&connection);
                    if rollback.is_ok() {
                        if connection.integrity_transaction {
                            self.bump_best_effort(|counters| {
                                checked_add(&mut counters.integrity_transactions_rolled_back, 1)
                            });
                        } else if write {
                            self.bump_best_effort(|counters| {
                                checked_add(&mut counters.transactions_rolled_back, 1)?;
                                checked_add(&mut counters.publication_transactions_rolled_back, 1)
                            });
                        }
                    } else {
                        connection.guard.take();
                    }
                    connection.transaction = false;
                    connection.integrity_transaction = false;
                    self.integrity_scope.store(false, Ordering::Release);
                    self.sql_family_scope
                        .store(SQL_FAMILY_NONE, Ordering::Release);
                    observe_time(&self.timings.trust_guard_ns, trust_started);
                    return Err(error);
                }
                connection.commit_scrub_on_drop = true;
            }
        }
        observe_time(&self.timings.trust_guard_ns, trust_started);
        Ok(connection)
    }

    pub(crate) fn mark_statement(&self) -> EngineResult<()> {
        let integrity = self.integrity_scope.load(Ordering::Acquire);
        let family = self.sql_family_scope.load(Ordering::Acquire);
        self.bump(|counters| {
            checked_add(&mut counters.statements, 1)?;
            mark_sql_family(counters, family, 1)?;
            if integrity {
                checked_add(&mut counters.integrity_statements, 1)?;
            }
            Ok(())
        })
    }

    pub(crate) fn mark_integrity_sql(&self, statements: u64) -> EngineResult<()> {
        let family = self.sql_family_scope.load(Ordering::Acquire);
        self.bump(|counters| {
            checked_add(&mut counters.statements, statements)?;
            mark_sql_family(counters, family, statements)?;
            checked_add(&mut counters.integrity_statements, statements)
        })
    }

    pub(crate) fn mark_family_sql(&self, family: u8, statements: u64) -> EngineResult<()> {
        self.bump(|counters| {
            checked_add(&mut counters.statements, statements)?;
            mark_sql_family(counters, family, statements)
        })
    }

    pub(crate) fn mark_compaction_sql(&self, statements: u64) -> EngineResult<()> {
        self.mark_family_sql(SQL_FAMILY_COMPACTION, statements)
    }

    #[cfg(test)]
    pub(crate) fn sample_rollback_journal(&self) {
        let mut sample = match self.rollback_journal_sample.lock() {
            Ok(sample) => sample,
            Err(_) => return,
        };
        let mut journal_path = self.path.as_os_str().to_os_string();
        journal_path.push("-journal");
        if let Ok(metadata) = fs::metadata(PathBuf::from(journal_path)) {
            *sample = Some(sample.map_or(metadata.len(), |current| current.max(metadata.len())));
        }
    }

    pub(crate) fn bump<F>(&self, update: F) -> EngineResult<()>
    where
        F: FnOnce(&mut EngineCounters) -> EngineResult<()>,
    {
        let mut counters = self.counters.lock().map_err(|_| EngineError::Sqlite {
            kind: SqliteErrorKind::Other,
            message: "counter mutex poisoned".to_owned(),
        })?;
        update(&mut counters)
    }

    pub(crate) fn bump_best_effort<F>(&self, update: F)
    where
        F: FnOnce(&mut EngineCounters) -> EngineResult<()>,
    {
        if let Ok(mut counters) = self.counters.lock() {
            let _ = update(&mut counters);
        }
    }

    pub(crate) fn note_sqlite_error(&self, error: &rusqlite::Error) -> EngineResult<()> {
        match sqlite_error_kind(error) {
            SqliteErrorKind::Busy => {
                self.bump(|counters| checked_add(&mut counters.busy_events, 1))
            }
            SqliteErrorKind::Locked => {
                self.bump(|counters| checked_add(&mut counters.locked_events, 1))
            }
            _ => Ok(()),
        }
    }
}

pub(crate) struct SqliteCommit;

impl CommitDispatch for SqliteCommit {
    fn commit(&self, connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch("COMMIT")
    }
}

#[cfg(any(test, feature = "test-hooks"))]
pub(crate) struct LostCommitAcknowledgementHook;

#[cfg(any(test, feature = "test-hooks"))]
impl CommitDispatch for LostCommitAcknowledgementHook {
    fn commit(&self, connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch("COMMIT")?;
        Err(rusqlite::Error::InvalidQuery)
    }
}

pub(crate) struct ConnectionGuard<'a> {
    pub(crate) engine: &'a Engine,
    pub(crate) guard: MutexGuard<'a, Option<Connection>>,
    pub(crate) transaction: bool,
    pub(crate) commit_scrub_on_drop: bool,
    pub(crate) integrity_transaction: bool,
}

impl Deref for ConnectionGuard<'_> {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().expect("checked when locked")
    }
}

impl DerefMut for ConnectionGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().expect("checked when locked")
    }
}

impl Drop for ConnectionGuard<'_> {
    fn drop(&mut self) {
        if self.transaction {
            if let Some(connection) = self.guard.as_ref() {
                if self.integrity_transaction {
                    let family = self.engine.sql_family_scope.load(Ordering::Acquire);
                    self.engine
                        .bump_best_effort(|counters: &mut EngineCounters| {
                            checked_add(&mut counters.statements, 1)?;
                            mark_sql_family(counters, family, 1)?;
                            checked_add(&mut counters.integrity_statements, 1)
                        });
                }
                let result = connection.execute_batch(if self.commit_scrub_on_drop {
                    "COMMIT"
                } else {
                    "ROLLBACK"
                });
                if self.integrity_transaction && result.is_ok() {
                    let committed = self.commit_scrub_on_drop;
                    self.engine.bump_best_effort(|counters| {
                        if committed {
                            checked_add(&mut counters.integrity_transactions_committed, 1)
                        } else {
                            checked_add(&mut counters.integrity_transactions_rolled_back, 1)
                        }
                    });
                }
                if result.is_err() {
                    self.guard.take();
                }
            }
            self.transaction = false;
        }
        if self.integrity_transaction {
            self.engine.integrity_scope.store(false, Ordering::Release);
        }
        self.engine
            .sql_family_scope
            .store(SQL_FAMILY_NONE, Ordering::Release);
    }
}
