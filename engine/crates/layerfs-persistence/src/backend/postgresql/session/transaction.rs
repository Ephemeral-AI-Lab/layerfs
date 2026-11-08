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
    /// One publication: pack bodies first, without the lane, then the lane
    /// row ([`Transaction::lane`]) and the rows that reference them, all in
    /// one transaction.
    Publish,
    /// Reads of immutable rows only, each statement on its own (autocommit):
    /// no `BEGIN`/`COMMIT` round trips. Statements name the schema.
    Immutable,
    /// Exactly one read statement on its own (autocommit). One PostgreSQL
    /// statement sees one snapshot, so it may read mutable rows (a Branch
    /// and the roots it resolves to) coherently without `BEGIN`/`COMMIT`.
    /// A second statement is refused before it is sent.
    Statement,
}

pub(crate) struct Transaction<'a> {
    state: RefCell<&'a mut State>,
    uncertain: Cell<bool>,
    /// `Mode::Statement`: whether its one statement was already sent.
    single: Option<Cell<bool>>,
    /// `Mode::Publish` outside a held lane: the lane row is still to lock.
    lane: Cell<bool>,
    /// The Store's checked schema, for statements that name it.
    pub(crate) schema: &'a str,
}

impl Transaction<'_> {
    /// Online engine check preserving the server's concrete corruption verdict.
    /// Other refusals (including permissions) retain ordinary typed custody.
    pub(crate) fn engine_check(
        &self,
        sql: &str,
        oid: i64,
        corrupt: &mut bool,
    ) -> Result<Vec<Record>, BackendError> {
        self.admit()?;
        let start = Instant::now();
        let mut state = self.state.borrow_mut();
        let statement = self.checked(state.statement(sql))?;
        let result = state.client.query(&statement, &[&oid]);
        state.work.statements += 1;
        state.work.round_trips += 1;
        state.work.bound_bytes += 8;
        state.work.statement_ns += start.elapsed().as_nanos() as u64;
        if result
            .as_ref()
            .err()
            .and_then(|e| e.code())
            .is_some_and(|code| matches!(code.code(), "XX001" | "XX002"))
        {
            *corrupt = true;
            return Err(BackendError::Integrity);
        }
        self.checked(result)
            .and_then(|rows| rows.iter().map(rows::record).collect())
    }
    fn checked<T>(&self, result: Result<T, super::driver::Error>) -> Result<T, BackendError> {
        result.map_err(|e| {
            let error = e.backend();
            if error == BackendError::Unknown {
                self.uncertain.set(true);
            }
            error
        })
    }

    /// Refuses a second statement in a `Mode::Statement` body.
    fn admit(&self) -> Result<(), BackendError> {
        match &self.single {
            Some(sent) if sent.replace(true) => Err(BackendError::Integrity),
            _ => Ok(()),
        }
    }

    /// One statement with engine-neutral parameters and owned results.
    pub(crate) fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        self.admit()?;
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
        work.round_trips += 1;
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
        self.admit()?;
        let start = Instant::now();
        let mut state = self.state.borrow_mut();
        let statement = state.statement(sql);
        let statement = self.checked(statement)?;
        let result = state.client.query(&statement, params);
        let result = self.checked(result);
        let work = &mut state.work;
        work.statements += 1;
        work.round_trips += 1;
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
        self.admit()?;
        let start = Instant::now();
        let mut state = self.state.borrow_mut();
        let result = (|| {
            // The cached prepare (counted once per connection), the start
            // (CopyInResponse) and the finish; the rows stream between them.
            let statement = state.statement(sql)?;
            state.work.round_trips += 2;
            let writer = state.client.copy_in(&statement)?;
            let mut writer = BinaryCopyInWriter::new(writer, types);
            rows(&mut writer).map_err(super::driver::Error::from)?;
            writer.finish().map_err(super::driver::Error::from)
        })();
        state.work.statements += 1;
        state.work.statement_ns += start.elapsed().as_nanos() as u64;
        drop(state);
        self.checked(result)
    }

    /// Waits for and locks the Store's lane row, once, in a `Mode::Publish`
    /// transaction that is not already inside this thread's held lane.
    pub(crate) fn lane(&self) -> Result<(), BackendError> {
        if self.lane.replace(false) {
            self.query(super::profile::Settings::LANE_ROW, Vec::new())?;
        }
        Ok(())
    }

    /// A script of statements without parameters (schema creation).
    pub(crate) fn batch(&self, sql: &str) -> Result<(), BackendError> {
        self.admit()?;
        let mut state = self.state.borrow_mut();
        let result = state.client.batch_execute(sql);
        state.work.statements += 1;
        state.work.round_trips += 1;
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

    /// Immutable rows (packs, locations, inode values) read without a
    /// transaction, as on SQLite: no row they read ever changes.
    pub(crate) fn read_immutable<T, E: From<BackendError> + OutcomeError>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.run_as(Mode::Immutable, body)
    }

    /// One read statement on its own snapshot, with no `BEGIN`/`COMMIT`
    /// round trips; its SQL names the schema.
    pub(crate) fn read_statement<T, E: From<BackendError> + OutcomeError>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.run_as(Mode::Statement, body)
    }

    pub(crate) fn run_as<T, E: From<BackendError> + OutcomeError>(
        &self,
        mode: Mode,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        if !matches!(mode, Mode::Read | Mode::Immutable | Mode::Statement) && !self.writable {
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
            Mode::Publish => self.settings.publish_begin(),
            Mode::Immutable | Mode::Statement => {
                let single = (mode == Mode::Statement).then(|| Cell::new(false));
                let result = immutable(state, &self.settings.schema, single, body);
                state.work.transaction_ns += start.elapsed().as_nanos() as u64;
                return result;
            }
        };
        let schema = self.settings.schema.as_str();
        let result = (|| {
            state.work.round_trips += 1;
            if let Err(e) = state.client.batch_execute(&begin) {
                let error = e.backend();
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
                    single: None,
                    lane: Cell::new(mode == Mode::Publish && !turn.nested),
                    schema,
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
                    state.work.round_trips += 1;
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
                            let error = e.backend();
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

/// A body of autocommit statements; a lost reply still quarantines.
fn immutable<T, E: From<BackendError> + OutcomeError>(
    state: &mut State,
    schema: &str,
    single: Option<Cell<bool>>,
    body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
) -> Result<T, E> {
    let (result, uncertain) = {
        let tx = Transaction {
            state: RefCell::new(&mut *state),
            uncertain: Cell::new(false),
            single,
            lane: Cell::new(false),
            schema,
        };
        let result = body(&tx);
        let uncertain = tx.uncertain.get();
        (result, uncertain)
    };
    if uncertain || result.as_ref().err().is_some_and(OutcomeError::uncertain) {
        state.quarantined = true;
        return Err(BackendError::Unknown.into());
    }
    result
}

fn rollback(state: &mut State) -> Result<(), super::driver::Error> {
    state.work.round_trips += 1;
    state.client.batch_execute("ROLLBACK")
}
