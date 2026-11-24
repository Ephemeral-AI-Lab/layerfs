//! One-attempt transactions: one `BEGIN` round trip with its settings, the
//! body's statements, then `COMMIT`. A lost reply quarantines the session.
use super::{
    connection::{Session, State},
    query, rows,
};
use crate::backend::records::{BackendError, OutcomeError, Param, Record};
use layerfs_telemetry::timer::waits::Wait;
use postgres::{types::ToSql, types::Type, Row, SimpleQueryMessage};
use std::{
    cell::{Cell, RefCell},
    sync::atomic::Ordering,
    time::Instant,
};

/// What a transaction may do, and whether it waits for the lane row.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Mode {
    /// One read-only snapshot.
    Read,
    /// A write, serialized behind the lane row unless nested in a held lane.
    Write,
    /// A writable repeatable-read snapshot that never waits for the lane row:
    /// the mark's closure and its `kept_node` write see one fixed snapshot
    /// while publications continue. A second mark is the only row-level
    /// contention, and it reports the engine's serialization failure.
    Snapshot,
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
    /// A GC admission write that never waits for the lane row (#768): a
    /// mount's registration. `READ COMMITTED` with `synchronous_commit` and
    /// `lock_timeout`, fenced in its `BEGIN` batch like every transaction.
    /// It writes only `gc_intent`, which no publication reads, and its
    /// conflicts are excluded by the advisory keys taken before it.
    Admit,
    /// Exactly one statement on its own (autocommit) that may write, sent as
    /// written: no generation column is joined around it, because its text
    /// carries the check it needs (the GC session keys, an owner's release).
    /// A second statement is refused before it is sent.
    Bare,
}

pub(crate) struct Transaction<'a> {
    state: RefCell<&'a mut State>,
    uncertain: Cell<bool>,
    /// The first statement the server refused with a SQLSTATE: what a
    /// `COMMIT` of the aborted transaction it left reports (#768).
    refused: Cell<Option<BackendError>>,
    diagnostic: bool,
    /// `Mode::Statement`: whether its one statement was already sent.
    single: Option<Cell<bool>>,
    /// `Mode::Publish` outside a held lane: the lane row is still to lock.
    lane: Cell<bool>,
    /// The generation an autocommit read must still see (spec 5.2). The read's
    /// one statement joins it as a trailing column, so the check costs no extra
    /// round trip. `None` inside a transaction (its `BEGIN` batch checks) and on
    /// a session whose open has not set a generation yet (the open check).
    fence: Option<u64>,
    /// The Store's checked schema, for statements that name it.
    pub(crate) schema: &'a str,
    /// Pack rows held for the scoped reads a caller announced
    /// (`objects::read::expect`).
    pub(crate) packs: RefCell<super::super::objects::read::HeldPacks>,
    /// Inode value blocks this transaction has read
    /// (`metadata::reads::inode_values_ahead`).
    pub(crate) values: RefCell<super::super::metadata::reads::HeldBlocks>,
    /// Locators this transaction was told to expect lookups of
    /// (`metadata::reads::expect_locations`).
    pub(crate) located: RefCell<super::super::metadata::reads::HeldLocations>,
}

impl Transaction<'_> {
    /// Whether statements run as written, with no generation fence wrapped
    /// around them: true inside a transaction.
    pub(crate) fn unfenced(&self) -> bool {
        self.fence.is_none()
    }

    /// Statements this session has sent so far, this transaction's included.
    pub(crate) fn statements(&self) -> u64 {
        self.state.borrow().work.statements
    }

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
            let error = if self.diagnostic && e.code().is_some_and(|c| c.code() == "42501") {
                BackendError::ReadOnly
            } else {
                e.backend()
            };
            if error == BackendError::Unknown {
                self.uncertain.set(true);
            } else if e.code().is_some() && self.refused.get().is_none() {
                self.refused.set(Some(error));
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
        let text = self.fenced(sql);
        let statement = state.statement(&text);
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
        let result = self.checked(result).and_then(|rows| {
            let mut records = rows
                .iter()
                .map(rows::record)
                .collect::<Result<Vec<_>, _>>()?;
            self.check_fence(&mut records)?;
            Ok(records)
        });
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
        let text = self.fenced(sql);
        let statement = state.statement(&text);
        let statement = self.checked(statement)?;
        let result = state.client.query(&statement, params);
        let result = self.checked(result);
        if let Ok(rows) = &result {
            check_typed_fence(rows, self.fence)?;
        }
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
        rows: impl FnOnce(&mut super::driver::CopyRows<'_>) -> Result<(), postgres::Error>,
    ) -> Result<u64, BackendError> {
        self.admit()?;
        let start = Instant::now();
        let mut state = self.state.borrow_mut();
        let result = (|| {
            // The cached prepare (counted once per connection), the start
            // (CopyInResponse) and the finish; the rows stream between them.
            let statement = state.statement(sql)?;
            state.work.round_trips += 2;
            let mut writer = state.client.copy_in(&statement, types)?;
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
            self.batch(&super::profile::Settings::lane_row(self.schema))?;
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
        self.run_mode(mode, true, body)
    }

    /// [`Self::run`] without the generation fence: the Store diagnostic is an
    /// observation of a Store whose authority may have changed, and reports the
    /// change itself instead of refusing with `Stale` (spec 5.2). It is
    /// read-only on a read-only session exactly like [`Self::run`].
    pub(crate) fn run_observed<T, E: From<BackendError> + OutcomeError>(
        &self,
        writable: bool,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        let mode = if writable { Mode::Write } else { Mode::Read };
        self.run_mode(mode, false, body)
    }

    /// One writable repeatable-read snapshot without the publication lane.
    /// Used by the mark, whose only writes are `kept_node` rows.
    pub(crate) fn run_snapshot<T, E: From<BackendError> + OutcomeError>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.run_as(Mode::Snapshot, body)
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

    /// One GC admission write outside the publication lane ([`Mode::Admit`]).
    pub(crate) fn run_admit<T, E: From<BackendError> + OutcomeError>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.run_as(Mode::Admit, body)
    }

    /// One autocommit statement sent as written ([`Mode::Bare`]); refused on
    /// a read-only session. The GC session advisory keys use it unfenced: a
    /// fenced form would take or drop a key and then answer `Stale`, leaving
    /// the caller unaware of the change (#776 P2); their statements carry
    /// their own generation check where they need one.
    pub(crate) fn run_bare<T, E: From<BackendError> + OutcomeError>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.run_as(Mode::Bare, body)
    }

    pub(crate) fn run_as<T, E: From<BackendError> + OutcomeError>(
        &self,
        mode: Mode,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.run_mode(mode, true, body)
    }

    fn run_mode<T, E: From<BackendError> + OutcomeError>(
        &self,
        mode: Mode,
        fenced: bool,
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
        // Spec 5.2: the generation check rides in the statements a client
        // already sends. A transaction checks it in the same round trip as its
        // `BEGIN`, so the `AccessShareLock` it takes on `history_meta` holds
        // until `COMMIT`: a swap that starts after this point meets
        // `LOCK TABLE ... ACCESS EXCLUSIVE NOWAIT` and abandons. An autocommit
        // read (`Statement`/`Immutable`) joins the same column into its one
        // statement below. A session whose open has not set a generation (the
        // open's own check) is unfenced.
        let generation = fenced
            .then(|| self.incarnation.load(Ordering::SeqCst))
            .filter(|generation| *generation != 0);
        let transactional = matches!(
            mode,
            Mode::Read | Mode::Write | Mode::Snapshot | Mode::Stage | Mode::Publish | Mode::Admit
        );
        let expected = generation.filter(|_| transactional);
        let autocommit = generation.filter(|_| matches!(mode, Mode::Immutable | Mode::Statement));
        // A write nested in this thread's held lane relies on the lane
        // connection's row lock: it holds the lane barrier shared and checks
        // that lane transaction in its COMMIT round trip (finding #10).
        let in_lane = turn.nested && matches!(mode, Mode::Write | Mode::Publish);
        let begin = match mode {
            Mode::Read => self.settings.read_begin(),
            Mode::Snapshot => self.settings.snapshot_begin(),
            Mode::Write if in_lane => self.settings.write_begin(false) + &self.settings.in_lane(),
            Mode::Write => self.settings.write_begin(true),
            Mode::Stage => self.settings.write_begin(false),
            Mode::Publish if in_lane => self.settings.publish_begin() + &self.settings.in_lane(),
            Mode::Publish => self.settings.publish_begin(),
            // A write without the lane row, under the lane's lock_timeout.
            Mode::Admit => self.settings.publish_begin(),
            Mode::Immutable | Mode::Statement | Mode::Bare => {
                let single = matches!(mode, Mode::Statement | Mode::Bare).then(|| Cell::new(false));
                let result = immutable(state, &self.settings.schema, single, autocommit, body);
                state.work.transaction_ns += start.elapsed().as_nanos() as u64;
                return result;
            }
        };
        let schema = self.settings.schema.as_str();
        let result = (|| {
            state.work.round_trips += 1;
            if let Some(expected) = expected {
                let check = format!(
                    "{begin}; SELECT catalog_incarnation::text FROM {schema}.history_meta WHERE id=1"
                );
                match state.client.simple_query(&check) {
                    Ok(messages) => {
                        let mut actual: Option<&str> = None;
                        for message in &messages {
                            if let SimpleQueryMessage::Row(row) = message {
                                actual = row.try_get(0).ok().flatten();
                            }
                        }
                        let generation = actual
                            .and_then(|text| text.parse::<u64>().ok())
                            .ok_or(BackendError::Integrity)?;
                        if generation != expected {
                            if rollback(state).is_err() {
                                state.quarantine();
                                return Err(BackendError::Unknown.into());
                            }
                            state.work.rollbacks += 1;
                            return Err(BackendError::Stale.into());
                        }
                    }
                    Err(e) => {
                        let error = e.backend();
                        if error == BackendError::Unknown || rollback(state).is_err() {
                            state.quarantine();
                            return Err(BackendError::Unknown.into());
                        }
                        state.work.rollbacks += 1;
                        return Err(error.into());
                    }
                }
            } else if let Err(e) = state.client.batch_execute(&begin) {
                let error = e.backend();
                if error == BackendError::Unknown || rollback(state).is_err() {
                    state.quarantine();
                    return Err(BackendError::Unknown.into());
                }
                state.work.rollbacks += 1;
                return Err(error.into());
            }
            state.work.transactions += 1;
            if mode != Mode::Read {
                state.work.write_transactions += 1;
            }
            let (result, uncertain, refused) = {
                let tx = Transaction {
                    diagnostic: matches!(state.client, super::driver::Driver::Diagnostic(_)),
                    state: RefCell::new(&mut *state),
                    uncertain: Cell::new(false),
                    refused: Cell::new(None),
                    single: None,
                    lane: Cell::new(mode == Mode::Publish && !turn.nested),
                    fence: None,
                    schema,
                    packs: RefCell::default(),
                    values: RefCell::default(),
                    located: RefCell::default(),
                };
                let result = body(&tx);
                let uncertain = tx.uncertain.get()
                    || result.as_ref().err().is_some_and(OutcomeError::uncertain);
                (result, uncertain, tx.refused.get())
            };
            if uncertain {
                state.quarantine();
                return Err(BackendError::Unknown.into());
            }
            // A nested write's lane check rides in its COMMIT round trip.
            // Either way a statement runs before the `COMMIT` in the same
            // text: in a transaction the server aborted (a refused statement
            // the body did not propagate) it fails `25P02` and the `COMMIT`
            // is skipped. A `COMMIT` of an aborted transaction rolls it back
            // and answers like success (its tag, `ROLLBACK`, is not read),
            // so it is never sent as one (#768).
            let commit_sql = match in_lane && result.is_ok() {
                true => self.lane_commit().ok(),
                false => Some(GUARDED_COMMIT.to_owned()),
            };
            match result {
                Ok(_) if commit_sql.is_none() => {
                    // No lane transaction is recorded: nothing is kept.
                    lost_lane(state)
                }
                Ok(value) => {
                    let commit = Instant::now();
                    state.work.round_trips += 1;
                    let outcome = state
                        .client
                        .batch_execute(commit_sql.as_deref().unwrap_or("COMMIT"));
                    state.work.commit_ns += commit.elapsed().as_nanos() as u64;
                    match outcome {
                        Ok(()) => {
                            state.work.commits += 1;
                            if mode != Mode::Read {
                                state.work.write_commits += 1;
                            }
                            Ok(value)
                        }
                        Err(e) if aborted(&e) => {
                            // The transaction was already aborted: nothing
                            // was committed. It reports the statement the
                            // server refused first.
                            if rollback(state).is_err() {
                                state.quarantine();
                                return Err(BackendError::Unknown.into());
                            }
                            state.work.rollbacks += 1;
                            Err(refused.unwrap_or(BackendError::Integrity).into())
                        }
                        Err(e) if in_lane && lane_lost(&e) => {
                            // The check failed and the server skipped the
                            // COMMIT: the lane was lost, nothing is kept.
                            self.close_lane();
                            lost_lane(state)
                        }
                        Err(e) => {
                            // A COMMIT whose reply was lost is uncertain; one
                            // the server refused (a deferred constraint) rolled
                            // back. A nested write's batch may have stopped
                            // before its COMMIT, so it is rolled back too.
                            let error = e.backend();
                            if error == BackendError::Unknown
                                || (in_lane && rollback(state).is_err())
                            {
                                state.quarantine();
                                return Err(BackendError::Unknown.into());
                            }
                            state.work.rollbacks += 1;
                            Err(error.into())
                        }
                    }
                }
                Err(e) => {
                    if rollback(state).is_err() {
                        state.quarantine();
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

impl Transaction<'_> {
    /// The one statement with the generation it must still see joined as its
    /// trailing column (spec 5.2). Autocommit reads name the schema, so the
    /// fence's `history_meta` read is schema-qualified too. A statement that
    /// reads no rows returns no rows: the swap retains exactly the live set
    /// (`live_heads_digest` is unchanged), so an absence at `G` is an absence
    /// at `G + 1` and needs no generation to be answered.
    fn fenced<'a>(&self, sql: &'a str) -> std::borrow::Cow<'a, str> {
        match self.fence {
            None => std::borrow::Cow::Borrowed(sql),
            Some(_) => {
                // A shipped statement ends in `;`, which the extended protocol
                // accepts as one statement but a parenthesized subquery does not.
                let statement = sql.trim_end();
                let statement = statement.strip_suffix(';').unwrap_or(statement).trim_end();
                std::borrow::Cow::Owned(format!(
                    "SELECT q.*,f.__layerfs_incarnation FROM ({statement}) q CROSS JOIN \
                     (SELECT catalog_incarnation AS __layerfs_incarnation FROM {}.history_meta WHERE id=1) f",
                    self.schema
                ))
            }
        }
    }

    /// Checks and removes the joined generation column of a fenced read.
    fn check_fence(&self, rows: &mut [Record]) -> Result<(), BackendError> {
        let Some(expected) = self.fence else {
            return Ok(());
        };
        for row in rows.iter_mut() {
            let actual = match row.cells.pop() {
                Some(crate::backend::records::Value::Integer(generation)) if generation > 0 => {
                    generation as u64
                }
                _ => return Err(BackendError::Integrity),
            };
            if actual != expected {
                return Err(BackendError::Stale);
            }
        }
        Ok(())
    }
}

/// The same check for a fenced `typed` read, whose raw rows keep the trailing
/// column (its callers read the leading ones by index).
fn check_typed_fence(rows: &[Row], fence: Option<u64>) -> Result<(), BackendError> {
    let Some(expected) = fence else {
        return Ok(());
    };
    for row in rows {
        let actual: i64 = row
            .try_get(row.len() - 1)
            .map_err(|_| BackendError::Integrity)?;
        if actual <= 0 {
            return Err(BackendError::Integrity);
        }
        if actual as u64 != expected {
            return Err(BackendError::Stale);
        }
    }
    Ok(())
}

/// A body of autocommit statements; a lost reply still quarantines.
fn immutable<T, E: From<BackendError> + OutcomeError>(
    state: &mut State,
    schema: &str,
    single: Option<Cell<bool>>,
    fence: Option<u64>,
    body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
) -> Result<T, E> {
    let (result, uncertain) = {
        let tx = Transaction {
            diagnostic: matches!(state.client, super::driver::Driver::Diagnostic(_)),
            state: RefCell::new(&mut *state),
            uncertain: Cell::new(false),
            refused: Cell::new(None),
            single,
            lane: Cell::new(false),
            fence,
            schema,
            packs: RefCell::default(),
            values: RefCell::default(),
            located: RefCell::default(),
        };
        let result = body(&tx);
        let uncertain = tx.uncertain.get();
        (result, uncertain)
    };
    if uncertain || result.as_ref().err().is_some_and(OutcomeError::uncertain) {
        state.quarantine();
        return Err(BackendError::Unknown.into());
    }
    result
}

/// A transaction's `COMMIT`, after a statement that fails `25P02` when the
/// transaction is aborted, so the `COMMIT` of an aborted transaction is
/// never sent ([`aborted`]).
const GUARDED_COMMIT: &str = "SELECT 1; COMMIT";

/// Whether a `COMMIT` batch found its transaction already aborted.
fn aborted(error: &super::driver::Error) -> bool {
    error
        .code()
        .is_some_and(|code| *code == postgres::error::SqlState::IN_FAILED_SQL_TRANSACTION)
}

/// Whether a nested write's COMMIT batch failed its lane check: division by
/// zero (the lane transaction is not in progress) or an id the server does
/// not know (a different server).
fn lane_lost(error: &super::driver::Error) -> bool {
    error.code().is_some_and(|code| {
        *code == postgres::error::SqlState::DIVISION_BY_ZERO
            || *code == postgres::error::SqlState::INVALID_PARAMETER_VALUE
    })
}

/// Rolls back a nested write whose lane was lost: a definite `Closed`.
fn lost_lane<T, E: From<BackendError>>(state: &mut State) -> Result<T, E> {
    if rollback(state).is_err() {
        state.quarantine();
        return Err(BackendError::Unknown.into());
    }
    state.work.rollbacks += 1;
    Err(BackendError::Closed.into())
}

fn rollback(state: &mut State) -> Result<(), super::driver::Error> {
    state.work.round_trips += 1;
    state.client.batch_execute("ROLLBACK")
}
