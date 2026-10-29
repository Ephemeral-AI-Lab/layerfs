//! One-attempt transactions: one `BEGIN` round trip with its settings, the
//! body's statements, then `COMMIT`. A lost reply quarantines the session.
use super::{
    connection::{Session, State},
    query, rows,
};
use crate::backend::records::{BackendError, OutcomeError, Param, Record};
use layerfs_telemetry::timer::waits::Wait;
use postgres::{binary_copy::BinaryCopyInWriter, types::ToSql, types::Type, Row};
use std::{
    cell::{Cell, RefCell},
    time::Instant,
};

/// What a transaction may do, and whether it waits for the lane row.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Mode {
    /// One read-only snapshot.
    Read,
    /// A write, serialized behind the lane row unless nested in a held lane.
    Write,
    /// A write of new immutable rows only (pack bodies), without the lane.
    Stage,
}

pub(crate) struct Transaction<'a> {
    state: RefCell<&'a mut State>,
    uncertain: Cell<bool>,
}

impl Transaction<'_> {
    fn checked<T>(&self, result: Result<T, postgres::Error>) -> Result<T, BackendError> {
        result.map_err(|e| {
            let error = rows::error(&e);
            if error == BackendError::Unknown {
                self.uncertain.set(true);
            }
            error
        })
    }

    /// One statement with engine-neutral parameters and owned results.
    pub(crate) fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        let start = Instant::now();
        let mut state = self.state.borrow_mut();
        let statement = state.statement(sql);
        let statement = self.checked(statement)?;
        if statement.params().len() != params.len() {
            return Err(BackendError::Integrity);
        }
        let mut bound = Vec::with_capacity(params.len());
        let mut bytes = 0;
        for (param, ty) in params.into_iter().zip(statement.params()) {
            let (value, size) = query::bind(param, ty)?;
            bound.push(value);
            bytes += size;
        }
        let refs: Vec<&(dyn ToSql + Sync)> = bound.iter().map(|b| b.as_ref()).collect();
        let result = state.client.query(&statement, &refs);
        let result = self
            .checked(result)
            .and_then(|rows| rows.iter().map(rows::record).collect::<Result<Vec<_>, _>>());
        let work = &mut state.work;
        work.statements += 1;
        work.bound_bytes += bytes;
        work.statement_ns += start.elapsed().as_nanos() as u64;
        if let Ok(rows) = &result {
            work.returned_rows += rows.len() as u64;
        }
        result
    }

    /// One statement with engine-typed parameters (arrays, borrowed bytes).
    pub(crate) fn typed(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        bytes: u64,
    ) -> Result<Vec<Row>, BackendError> {
        let start = Instant::now();
        let mut state = self.state.borrow_mut();
        let statement = state.statement(sql);
        let statement = self.checked(statement)?;
        let result = state.client.query(&statement, params);
        let result = self.checked(result);
        let work = &mut state.work;
        work.statements += 1;
        work.bound_bytes += bytes;
        work.statement_ns += start.elapsed().as_nanos() as u64;
        if let Ok(rows) = &result {
            work.returned_rows += rows.len() as u64;
        }
        result
    }

    /// One `COPY … FROM STDIN (FORMAT binary)`; `rows` writes every row.
    pub(crate) fn copy_in(
        &self,
        sql: &str,
        types: &[Type],
        rows: impl FnOnce(&mut BinaryCopyInWriter<'_>) -> Result<(), postgres::Error>,
    ) -> Result<u64, BackendError> {
        let start = Instant::now();
        let mut state = self.state.borrow_mut();
        let result = (|| {
            let writer = state.client.copy_in(sql)?;
            let mut writer = BinaryCopyInWriter::new(writer, types);
            rows(&mut writer)?;
            writer.finish()
        })();
        state.work.statements += 1;
        state.work.statement_ns += start.elapsed().as_nanos() as u64;
        drop(state);
        self.checked(result)
    }

    /// A script of statements without parameters (schema creation).
    pub(crate) fn batch(&self, sql: &str) -> Result<(), BackendError> {
        let mut state = self.state.borrow_mut();
        let result = state.client.batch_execute(sql);
        state.work.statements += 1;
        drop(state);
        self.checked(result)
    }
}

impl Session {
    pub(crate) fn run<T, E: From<BackendError> + OutcomeError>(
        &self,
        writable: bool,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        let mode = if writable { Mode::Write } else { Mode::Read };
        self.run_as(mode, body)
    }

    /// Immutable rows read in one snapshot like any other read.
    pub(crate) fn read_immutable<T, E: From<BackendError> + OutcomeError>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.run_as(Mode::Read, body)
    }

    pub(crate) fn run_as<T, E: From<BackendError> + OutcomeError>(
        &self,
        mode: Mode,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        if mode != Mode::Read && !self.writable {
            return Err(BackendError::ReadOnly.into());
        }
        let start = Instant::now();
        let wait = if self.writable {
            Wait::Writer
        } else {
            Wait::Reader
        };
        let turn = self.admission.enter(wait).map_err(E::from)?;
        let mut guard = self
            .state
            .lock()
            .map_err(|_| E::from(BackendError::Unknown))?;
        let state: &mut State = &mut guard;
        if state.quarantined {
            return Err(BackendError::Unknown.into());
        }
        let begin = match mode {
            Mode::Read => self.settings.read_begin(),
            Mode::Write => self.settings.write_begin(!turn.nested),
            Mode::Stage => self.settings.write_begin(false),
        };
        let result = (|| {
            if let Err(e) = state.client.batch_execute(&begin) {
                let error = rows::error(&e);
                if error == BackendError::Unknown || rollback(state).is_err() {
                    state.quarantined = true;
                    return Err(BackendError::Unknown.into());
                }
                state.work.rollbacks += 1;
                return Err(error.into());
            }
            state.work.transactions += 1;
            if mode != Mode::Read {
                state.work.write_transactions += 1;
            }
            let (result, uncertain) = {
                let tx = Transaction {
                    state: RefCell::new(&mut *state),
                    uncertain: Cell::new(false),
                };
                let result = body(&tx);
                let uncertain = tx.uncertain.get()
                    || result.as_ref().err().is_some_and(OutcomeError::uncertain);
                (result, uncertain)
            };
            if uncertain {
                state.quarantined = true;
                return Err(BackendError::Unknown.into());
            }
            match result {
                Ok(value) => {
                    let commit = Instant::now();
                    let outcome = state.client.batch_execute("COMMIT");
                    state.work.commit_ns += commit.elapsed().as_nanos() as u64;
                    match outcome {
                        Ok(()) => {
                            state.work.commits += 1;
                            if mode != Mode::Read {
                                state.work.write_commits += 1;
                            }
                            Ok(value)
                        }
                        Err(e) => {
                            // A COMMIT whose reply was lost is uncertain; one
                            // the server refused (a deferred constraint) rolled back.
                            let error = rows::error(&e);
                            if error == BackendError::Unknown {
                                state.quarantined = true;
                            } else {
                                state.work.rollbacks += 1;
                            }
                            Err(error.into())
                        }
                    }
                }
                Err(e) => {
                    if rollback(state).is_err() {
                        state.quarantined = true;
                        return Err(BackendError::Unknown.into());
                    }
                    state.work.rollbacks += 1;
                    Err(e)
                }
            }
        })();
        state.work.transaction_ns += start.elapsed().as_nanos() as u64;
        result
    }
}

fn rollback(state: &mut State) -> Result<(), postgres::Error> {
    state.client.batch_execute("ROLLBACK")
}
