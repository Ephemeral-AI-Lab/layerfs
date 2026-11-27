//! One-attempt transaction ownership and quarantine on uncertain outcomes.
use super::connection::{Session, SqlWork};
use super::query;
use crate::backend::error::{BackendError, OutcomeError, Param, Record};
use rusqlite::Connection;
use std::{
    cell::{Cell, RefCell},
    time::Instant,
};
pub(crate) struct Transaction<'a> {
    pub(crate) connection: &'a Connection,
    pub(crate) work: &'a RefCell<SqlWork>,
    uncertain: Cell<bool>,
    owner: &'a Session,
}
impl Transaction<'_> {
    pub(crate) fn layout(&self) -> crate::SqlitePackLayout {
        self.owner.layout
    }
    /// The directory that holds the Store file.
    pub(crate) fn store_directory(&self) -> std::path::PathBuf {
        match self.owner.path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => std::path::PathBuf::from("."),
        }
    }
    pub(crate) fn schema_version(&self) -> i64 {
        self.owner.schema_version()
    }
    pub(crate) fn reclamation_enabled(&self) -> bool {
        self.owner.profile.auto_vacuum == 2
    }
    pub(crate) fn input_limit(
        &self,
        bindings: usize,
        prefix: usize,
        per_row: usize,
    ) -> Result<usize, BackendError> {
        let count = (self.variable_limit()? / bindings)
            .min(self.sql_length_limit()?.saturating_sub(prefix) / per_row);
        if count == 0 {
            return Err(BackendError::Capacity);
        }
        Ok(count)
    }

    pub(crate) fn bootstrap(&self, script: &str) -> Result<(), BackendError> {
        let r = query::batch(self.connection, script, self.work);
        if r.as_ref().err() == Some(&BackendError::Unknown) {
            self.uncertain.set(true);
        }
        r
    }

    pub(crate) fn history(
        &self,
        key: &str,
        params: Vec<Param>,
    ) -> Result<Vec<Record>, BackendError> {
        self.query(query::history(key)?, params)
    }
    pub(crate) fn variable_limit(&self) -> Result<usize, BackendError> {
        self.connection
            .limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER)
            .map(|v| v as usize)
            .map_err(super::rows::error)
    }
    pub(crate) fn sql_length_limit(&self) -> Result<usize, BackendError> {
        self.connection
            .limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH)
            .map(|v| v as usize)
            .map_err(super::rows::error)
    }
    pub(crate) fn borrowed(
        &self,
        sql: &str,
        values: &[&dyn rusqlite::ToSql],
        bytes: u64,
    ) -> Result<Vec<Record>, BackendError> {
        let r = query::borrowed(self.connection, sql, values, bytes, self.work);
        if r.as_ref().err() == Some(&BackendError::Unknown) {
            self.uncertain.set(true);
        }
        r
    }
    /// Leases one statement for repeated executions inside this transaction.
    pub(crate) fn prepare(&self, sql: &str) -> Result<super::prepared::Prepared<'_>, BackendError> {
        let result =
            super::prepared::Prepared::new(self.connection, sql, self.work, Some(&self.uncertain));
        if result.as_ref().err() == Some(&BackendError::Unknown) {
            self.uncertain.set(true);
        }
        result
    }
    pub(crate) fn mapped<T>(
        &self,
        sql: &str,
        values: &[&dyn rusqlite::ToSql],
        bytes: u64,
        capacity: usize,
        decode: impl FnMut(&rusqlite::Row<'_>) -> Result<T, BackendError>,
    ) -> Result<Vec<T>, BackendError> {
        let result = query::mapped(
            self.connection,
            sql,
            values,
            bytes,
            capacity,
            self.work,
            decode,
        );
        if result.as_ref().err() == Some(&BackendError::Unknown) {
            self.uncertain.set(true);
        }
        result
    }
    pub(crate) fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        {
            let r = query::run(self.connection, sql, params, self.work);
            if r.as_ref().err() == Some(&BackendError::Unknown) {
                self.uncertain.set(true);
            }
            r
        }
    }
}

/// What [`Session::run_decided`] does with the transaction its body wrote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Completion {
    /// Keep the body's writes: COMMIT.
    Commit,
    /// Discard the body's writes: ROLLBACK, so nothing it wrote reaches the
    /// Store and no WAL frame is committed.
    Rollback,
}

impl Session {
    /// The wait kind of this connection's admission queue.
    fn wait_kind(&self) -> layerfs_telemetry::timer::waits::Wait {
        if self.writable {
            layerfs_telemetry::timer::waits::Wait::Writer
        } else {
            layerfs_telemetry::timer::waits::Wait::Reader
        }
    }

    /// One read-only statement in its own implicit SQLite snapshot. Its SQL
    /// must carry every authority predicate needed for the returned payload.
    /// No facade for arbitrary unframed multi-statement work is exposed.
    pub(crate) fn read_mapped<T, E: From<BackendError> + OutcomeError>(
        &self,
        sql: &str,
        values: &[&dyn rusqlite::ToSql],
        bytes: u64,
        capacity: usize,
        decode: impl FnMut(&rusqlite::Row<'_>) -> Result<Option<T>, E>,
    ) -> Result<Vec<T>, E> {
        let start = Instant::now();
        let _turn = self.admission.enter(self.wait_kind()).map_err(E::from)?;
        let mut state = self.state();
        super::thread_work::attributed(&mut state, |state| {
            if state.quarantined {
                return Err(BackendError::Unknown.into());
            }
            self.check_identity().map_err(E::from)?;
            // An earlier uncertain transaction may still exist. It cannot be
            // inherited by an independently owned snapshot or guessed complete.
            if !state.connection.is_autocommit() {
                state.quarantined = true;
                return Err(BackendError::Unknown.into());
            }
            let result = (|| {
                let mut statement =
                    super::prepared::Prepared::new(&state.connection, sql, &state.work, None)
                        .map_err(E::from)?;
                if !statement.readonly() {
                    return Err(BackendError::Integrity.into());
                }
                statement.mapped_filter(values, bytes, capacity, decode)
            })();
            state.work.borrow_mut().read_snapshot_ns += start.elapsed().as_nanos() as u64;
            // The statement/cursor have been dropped. A read leaves autocommit
            // intact; failed I/O or an unexpected transaction retains quarantine.
            if !state.connection.is_autocommit()
                || result.as_ref().err().is_some_and(OutcomeError::uncertain)
            {
                state.quarantined = true;
                return Err(BackendError::Unknown.into());
            }
            if result.is_ok() {
                state.work.borrow_mut().read_snapshots += 1;
            }
            result
        })
    }

    /// Reads of immutable objects only (#352 F-035): object locations,
    /// published pack descriptors, units and bodies, and stored inode values. `body` runs through the
    /// transaction facade with no BEGIN/COMMIT, so each of its statements and
    /// BLOB reads takes its own implicit snapshot. That is safe only because
    /// every row such a body reads never changes once visible:
    /// `object_location` is insert-only (`ON CONFLICT DO NOTHING`, no UPDATE
    /// or DELETE anywhere), `pack` and `pack_unit` refuse UPDATE and DELETE
    /// by trigger, `inode_value` is written by one plain INSERT per ordinal
    /// (never updated or deleted; a second INSERT of an ordinal fails), pack
    /// ids come from a counter that never goes back, an ordinal is given
    /// back only while nothing was written under it, and
    /// reclamation frees only empty pages. Two such reads can never see an
    /// inconsistent pair; a row not yet visible is simply absent. Anything
    /// that picks a snapshot (branch heads, roots, Layer and Commit rows) or
    /// writes must use [`Self::run`], never this.
    pub(crate) fn read_immutable<T, E: From<BackendError> + OutcomeError>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        let _turn = self.admission.enter(self.wait_kind()).map_err(E::from)?;
        let mut state = self.state();
        super::thread_work::attributed(&mut state, |state| {
            if state.quarantined {
                return Err(BackendError::Unknown.into());
            }
            self.check_identity().map_err(E::from)?;
            // An earlier uncertain transaction cannot be inherited or guessed done.
            if !state.connection.is_autocommit() {
                state.quarantined = true;
                return Err(BackendError::Unknown.into());
            }
            let tx = Transaction {
                connection: &state.connection,
                work: &state.work,
                uncertain: Cell::new(false),
                owner: self,
            };
            let result = body(&tx);
            let uncertain =
                tx.uncertain.get() || result.as_ref().err().is_some_and(OutcomeError::uncertain);
            // Every statement and BLOB handle has been released: a read leaves
            // autocommit intact, anything else keeps the session quarantined.
            if uncertain || !state.connection.is_autocommit() {
                state.quarantined = true;
                return Err(BackendError::Unknown.into());
            }
            result
        })
    }

    pub(crate) fn run<T, E: From<BackendError> + OutcomeError>(
        &self,
        writable: bool,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.run_decided(writable, |tx| {
            body(tx).map(|value| (value, Completion::Commit))
        })
    }

    /// [`Self::run`] with the body's [`Completion`] deciding the outcome: a
    /// `Rollback` body restores the pre-transaction Store, so the caller can
    /// take a decision on data the body wrote without persisting it.
    pub(crate) fn run_decided<T, E: From<BackendError> + OutcomeError>(
        &self,
        writable: bool,
        body: impl FnOnce(&Transaction<'_>) -> Result<(T, Completion), E>,
    ) -> Result<T, E> {
        if writable && !self.writable {
            return Err(BackendError::ReadOnly.into());
        }
        let start = Instant::now();
        let turn = self.admission.enter(self.wait_kind()).map_err(E::from)?;
        let background = self.background.as_ref();
        // The cross-process writer lock is waited for outside the state lock
        // (finding #20) and outside the FIFO turn (`admit_writer`): a wait on
        // another Session's or process's lane must block neither
        // `diagnostics()` nor this Session's reads. A held publication lane
        // already owns the writer for its nested work.
        let (_turn, writer) = if writable && !turn.nested {
            let (turn, writer) = self
                .admit_writer(turn, |state| {
                    if state.quarantined {
                        // A writer an earlier attempt retained is released
                        // here, after its connection closes under it (#776 S3).
                        let _ = state.finish_writer(background);
                        return Err(BackendError::Unknown);
                    }
                    Ok(())
                })
                .map_err(E::from)?;
            (turn, Some(writer))
        } else {
            (turn, None)
        };
        // A poisoned lock is recovered quarantined, so `writer` goes to
        // `finish_writer` below: nothing has been attempted under it.
        let mut s = self.state();
        // A panicking body is caught so the connection is closed and the
        // writer released here, then the panic resumes once the state lock
        // is dropped (#776 S3). Unwinding with them inside a poisoned lock
        // kept the lane (or, nested, an open write transaction) for as long
        // as a caller that catches panics kept the Session.
        let mut panicked = None;
        let result = super::thread_work::attributed(&mut s, |s| {
            // State owns the writer before BEGIN, including across a poisoned
            // unwind, and every exit below hands it to `finish_writer`. A
            // call that took none leaves a writer retained after an earlier
            // quarantine where it is.
            if let Some(writer) = writer {
                s.writer_hold = Some(writer);
            }
            if s.quarantined {
                s.finish_writer(background).map_err(E::from)?;
                return Err(BackendError::Unknown.into());
            }
            // Checked with the lane held, so a Store replaced while this call
            // waited for it is refused before BEGIN.
            if let Err(error) = self.check_identity() {
                s.finish_writer(background).map_err(E::from)?;
                return Err(error.into());
            }
            let tx = Transaction {
                connection: &s.connection,
                work: &s.work,
                uncertain: Cell::new(false),
                owner: self,
            };
            if let Err(e) = tx.query(if writable { "BEGIN IMMEDIATE" } else { "BEGIN" }, vec![]) {
                if e == BackendError::Unknown {
                    s.quarantined = true;
                }
                s.finish_writer(background).map_err(E::from)?;
                s.work.borrow_mut().transaction_ns += start.elapsed().as_nanos() as u64;
                return Err(e.into());
            }
            tx.work.borrow_mut().transactions += 1;
            if writable {
                tx.work.borrow_mut().write_transactions += 1;
            }
            let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&tx)))
            {
                Ok(result) => result,
                Err(payload) => {
                    s.quarantined = true;
                    // Nested, there is no writer here: the held lane closes
                    // the quarantined connection before it lets go.
                    let _ = s.finish_writer(background);
                    panicked = Some(payload);
                    return Err(BackendError::Unknown.into());
                }
            };
            let uncertain =
                tx.uncertain.get() || result.as_ref().err().is_some_and(OutcomeError::uncertain);
            if uncertain {
                s.quarantined = true;
                s.finish_writer(background).map_err(E::from)?;
                s.work.borrow_mut().transaction_ns += start.elapsed().as_nanos() as u64;
                return Err(BackendError::Unknown.into());
            }
            let mut committed = false;
            let result = match result {
                Ok((value, Completion::Commit)) => {
                    match query::run(&s.connection, "COMMIT", vec![], &s.work) {
                        Ok(_) => {
                            committed = true;
                            s.work.borrow_mut().commits += 1;
                            if writable {
                                s.work.borrow_mut().write_commits += 1;
                            }
                            Ok(value)
                        }
                        Err(e) => {
                            if e == BackendError::Unknown {
                                s.quarantined = true;
                                Err(e.into())
                            } else if !s.connection.is_autocommit()
                                && query::run(&s.connection, "ROLLBACK", vec![], &s.work).is_err()
                            {
                                s.quarantined = true;
                                Err(BackendError::Unknown.into())
                            } else {
                                s.work.borrow_mut().rollbacks += 1;
                                Err(e.into())
                            }
                        }
                    }
                }
                Ok((value, Completion::Rollback)) => {
                    if s.connection.is_autocommit() {
                        // The body wrote nothing, so there is nothing to undo.
                        s.work.borrow_mut().rollbacks += 1;
                        Ok(value)
                    } else {
                        match query::run(&s.connection, "ROLLBACK", vec![], &s.work) {
                            Ok(_) => {
                                s.work.borrow_mut().rollbacks += 1;
                                Ok(value)
                            }
                            Err(_) => {
                                s.quarantined = true;
                                Err(BackendError::Unknown.into())
                            }
                        }
                    }
                }
                Err(e) => {
                    // A SQL I/O error is marked in the transaction facade before returning.
                    if s.quarantined {
                        Err(BackendError::Unknown.into())
                    } else if !s.connection.is_autocommit() {
                        match query::run(&s.connection, "ROLLBACK", vec![], &s.work) {
                            Ok(_) => {
                                s.work.borrow_mut().rollbacks += 1;
                                Err(e)
                            }
                            Err(_) => {
                                s.quarantined = true;
                                Err(BackendError::Unknown.into())
                            }
                        }
                    } else {
                        Err(e)
                    }
                }
            };
            s.finish_writer(background).map_err(E::from)?;
            s.work.borrow_mut().transaction_ns += start.elapsed().as_nanos() as u64;
            // Only a COMMIT can lengthen the log; a rolled-back body must not
            // wake the checkpoint thread as if it had written.
            if committed {
                if let Some(background) = &self.background {
                    background.committed(&self.path);
                }
            }
            result
        });
        if let Some(payload) = panicked {
            drop(s);
            std::panic::resume_unwind(payload);
        }
        result
    }
}
