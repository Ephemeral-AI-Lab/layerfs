//! Pipelined requests: several statements sent in one flight, every reply
//! collected in order, counted as one round trip.
//!
//! tokio-postgres sends a request when its future is first polled: an
//! extended-protocol statement as Bind/Execute/Sync, a simple text as one
//! Query, each on the connection's request channel without waiting. The
//! connection task writes every queued request before it flushes the socket,
//! and on the current-thread runtime it runs only once every request future
//! has been polled once and is waiting for its reply. So a flight is one
//! socket flush and one network wait, however many requests it carries.
//! Each request ends in its own Sync, so a failed statement does not stop
//! the server from answering the rest: inside a transaction they fail
//! `25P02` and a `COMMIT` rolls it back; outside one each runs on its own.
use super::{
    connection::{Session, State},
    driver::Error,
};
use crate::backend::error::BackendError;
use layerfs_telemetry::timer::waits::Wait;
use postgres::{error::SqlState, types::ToSql, Row, SimpleQueryMessage, Statement};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

/// One request of a session pipeline.
pub enum Request<'a> {
    /// One extended-protocol statement with binary parameters, prepared once
    /// per connection (the statement cache).
    Statement {
        /// The statement's text, also its cache key.
        sql: &'a str,
        /// One value per placeholder, sent in binary.
        params: &'a [&'a (dyn ToSql + Sync)],
        /// What the parameters bind, as `Transaction::typed` counts it.
        bytes: u64,
    },
    /// Simple-protocol text without parameters: one statement or several
    /// separated by `;` (the server stops that text at its first error).
    Simple(&'a str),
}

/// One request with its statement prepared, as the driver sends it.
pub(crate) enum Bound<'a> {
    Prepared(&'a Statement, &'a [&'a (dyn ToSql + Sync)]),
    Simple(&'a str),
}

/// The server's answer to one request.
pub enum Reply {
    /// An extended-protocol statement's rows.
    Rows(Vec<Row>),
    /// A simple text's messages: rows and one completion per statement.
    Messages(Vec<SimpleQueryMessage>),
}

/// One request the server or the client refused.
#[derive(Debug)]
pub struct Refusal {
    /// The ordinary classification (`rows::error`). A request the server
    /// skipped because an earlier one failed in the same transaction is
    /// `25P02`, which classifies `Unknown`; read `code` to tell it apart.
    pub error: BackendError,
    /// The server's SQLSTATE, when the server answered with an error.
    pub code: Option<SqlState>,
    /// Whether the request went out: an unsent one (unencodable, or after
    /// one that was) never reached the server.
    pub sent: bool,
}

/// What one driver flight sent and what came back.
pub(crate) struct Flight {
    /// Requests sent: a prefix of the flight. A request that could not be
    /// encoded is not sent, and neither is any request after it, so a
    /// transaction's `COMMIT` never goes out without its statements.
    pub(crate) sent: usize,
    /// One reply per request, in request order; an unsent request's is
    /// its encoding error or [`Error::Unsent`].
    pub(crate) replies: Vec<Result<Reply, Error>>,
}

pub(crate) type Pending<'a, T> = Pin<Box<dyn Future<Output = Result<T, postgres::Error>> + 'a>>;

/// Polls `pending` in order until every one has answered, never stopping at
/// an error. A future that answers on its first poll was never sent (a sent
/// request waits for the connection task, which cannot run inside this
/// poll), so it and every later future are not polled again: they answer
/// [`Error::Unsent`]. Returns how many were sent, and every reply.
pub(crate) async fn in_order<T>(
    mut pending: Vec<Pending<'_, T>>,
) -> (usize, Vec<Result<T, Error>>) {
    let mut replies: Vec<Option<Result<T, postgres::Error>>> =
        pending.iter().map(|_| None).collect();
    let (mut sent, mut stopped) = (0, false);
    std::future::poll_fn(|cx: &mut Context<'_>| {
        let mut waiting = false;
        for (index, future) in pending.iter_mut().enumerate() {
            if replies[index].is_some() {
                continue;
            }
            let first = index >= sent;
            if first && stopped {
                break;
            }
            match future.as_mut().poll(cx) {
                Poll::Ready(reply) => {
                    let unsent = first && reply.is_err();
                    if first && !unsent {
                        sent = index + 1;
                    }
                    replies[index] = Some(reply);
                    if unsent {
                        stopped = true;
                        break;
                    }
                }
                Poll::Pending => {
                    sent = sent.max(index + 1);
                    waiting = true;
                }
            }
        }
        match waiting {
            true => Poll::Pending,
            false => Poll::Ready(()),
        }
    })
    .await;
    let replies = replies
        .into_iter()
        .map(|reply| match reply {
            Some(reply) => reply.map_err(Error::from),
            None => Err(Error::Unsent),
        })
        .collect();
    (sent, replies)
}

/// One flight of `requests` on `client`: see the module documentation.
pub(crate) async fn flight(client: &tokio_postgres::Client, requests: &[Bound<'_>]) -> Flight {
    let pending = requests
        .iter()
        .map(|request| -> Pending<'_, Reply> {
            match *request {
                Bound::Prepared(statement, params) => {
                    Box::pin(async move { client.query(statement, params).await.map(Reply::Rows) })
                }
                Bound::Simple(sql) => {
                    Box::pin(async move { client.simple_query(sql).await.map(Reply::Messages) })
                }
            }
        })
        .collect();
    let (sent, replies) = in_order(pending).await;
    Flight { sent, replies }
}

impl Session {
    /// Sends `requests` in one flight on this session's connection and
    /// returns every reply in order. Statements not yet prepared on the
    /// connection are prepared first, all in one flight of their own.
    ///
    /// Counted: one round trip for the flight (none when nothing was sent),
    /// plus one for the prepares when any statement was not cached
    /// (`prepares` counts each), one statement per request sent, the bound
    /// bytes of the statements sent, every returned row and the wall in
    /// `statement_ns`. Transactions, commits and rollbacks are the caller's:
    /// the requests may begin and end one (and must end any they begin).
    ///
    /// A request that could not be encoded stops the flight before it (a
    /// definite client refusal, `Integrity`: it never reached the server);
    /// when earlier requests were sent, one `ROLLBACK` (one more round trip)
    /// ends a transaction they may have begun. A reply lost with the
    /// connection (a sent request's, never an unsent one's) quarantines the
    /// session (`State::quarantine`); a reply the server gave is returned as
    /// it is, and its classification is the caller's. `Err` means nothing of
    /// the flight ran: a read-only or quarantined session, a statement that
    /// did not prepare, or a parameter count that does not match its
    /// statement.
    pub(crate) fn pipeline(
        &self,
        requests: &[Request<'_>],
    ) -> Result<Vec<Result<Reply, Refusal>>, BackendError> {
        self.pipeline_turn(requests, true, false, None, |replies, _| (replies, false))
    }

    /// [`Self::pipeline`] refused (`Integrity`, nothing sent) inside this
    /// thread's held lane: a flight that takes the lane row itself (a fused
    /// Commit) would wait for its own lane connection. `judge` reads the
    /// replies while the session's turn and lock are still held, with
    /// whether the session is still trusted (no reply lost, a partly sent
    /// flight rolled back), and answers what to return and whether the
    /// flight's outcome is unknown: the session is then quarantined before
    /// the lock is released, so no other caller uses the connection between
    /// the flight and that verdict.
    ///
    /// `release` (statements, each after `; `) runs on the flight's failure
    /// path only: when any request was refused or not sent and no reply was
    /// lost, after the flight (appended to its `ROLLBACK` when it was partly
    /// sent) and before `judge`, so before the session is used again: one
    /// more round trip. If it fails the session is quarantined and `judge`
    /// sees it untrusted. A lost reply quarantines and closes the
    /// connection, which ends every session-level lock, so it runs nothing.
    pub(crate) fn pipeline_judged<T>(
        &self,
        requests: &[Request<'_>],
        release: &str,
        judge: impl FnOnce(Vec<Result<Reply, Refusal>>, bool) -> (T, bool),
    ) -> Result<T, BackendError> {
        self.pipeline_turn(requests, false, false, Some(release), judge)
    }

    /// [`Self::pipeline_judged`] that may also run inside this thread's held
    /// lane, for a flight that never takes the lane row (a mount's
    /// admission, an owner's release), and whose requests are built inside
    /// the turn. Statements not yet prepared on the connection are prepared
    /// inside a read transaction whose `SET LOCAL search_path` resolves their
    /// unqualified names (one round trip and one read transaction, counted),
    /// in the same turn as the flight, so no other caller can evict them
    /// from the cache in between. `run` makes the session ready ([`DecidedTurn::ready`]: a quarantined
    /// writer reopens, `Session::ready`), and may act on a refusal there
    /// before the turn ends; it reads what the connection decides, such as
    /// the backend its session keys are on, sends the flight with
    /// [`DecidedTurn::send`], and records what the session now holds while
    /// the turn is still held, so no reopen can come in between.
    pub(crate) fn decided_turn<T>(
        &self,
        run: impl FnOnce(&mut DecidedTurn<'_>) -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        if !self.writable {
            return Err(BackendError::ReadOnly);
        }
        let turn = self.admission.enter(Wait::Writer)?;
        let mut guard = self.state.lock().map_err(|_| BackendError::Unknown)?;
        let mut decided = DecidedTurn {
            session: self,
            state: &mut guard,
            nested: turn.nested,
            ready: false,
            unit: None,
            failed_unit: false,
        };
        let answer = run(&mut decided);
        decided.end_unit();
        answer
    }

    fn pipeline_turn<T>(
        &self,
        requests: &[Request<'_>],
        nested: bool,
        scoped: bool,
        release: Option<&str>,
        judge: impl FnOnce(Vec<Result<Reply, Refusal>>, bool) -> (T, bool),
    ) -> Result<T, BackendError> {
        if !self.writable {
            return Err(BackendError::ReadOnly);
        }
        let turn = self.admission.enter(Wait::Writer)?;
        if turn.nested && !nested {
            return Err(BackendError::Integrity);
        }
        let mut guard = self.state.lock().map_err(|_| BackendError::Unknown)?;
        let state: &mut State = &mut guard;
        self.ready(state, turn.nested)?;
        self.flight(state, requests, scoped, release, judge)
    }

    /// One flight on `state`, whose turn and lock the caller holds.
    fn flight<T>(
        &self,
        state: &mut State,
        requests: &[Request<'_>],
        scoped: bool,
        release: Option<&str>,
        judge: impl FnOnce(Vec<Result<Reply, Refusal>>, bool) -> (T, bool),
    ) -> Result<T, BackendError> {
        let start = Instant::now();
        let texts: Vec<&str> = requests
            .iter()
            .filter_map(|request| match request {
                Request::Statement { sql, .. } => Some(*sql),
                Request::Simple(_) => None,
            })
            .collect();
        let begin = scoped.then(|| self.settings.read_begin());
        let statements = match state.statements_in(&texts, begin.as_deref()) {
            Ok(statements) => statements,
            Err(e) => {
                state.work.statement_ns += start.elapsed().as_nanos() as u64;
                return Err(lost(state, e.backend()));
            }
        };
        let mut prepared = statements.iter();
        let mut bound = Vec::with_capacity(requests.len());
        for request in requests {
            bound.push(match request {
                Request::Statement { params, .. } => {
                    let statement = prepared.next().ok_or(BackendError::Integrity)?;
                    if statement.params().len() != params.len() {
                        return Err(BackendError::Integrity);
                    }
                    Bound::Prepared(statement, params)
                }
                Request::Simple(sql) => Bound::Simple(sql),
            });
        }
        let ending = |sent: usize, partial: bool, failed: bool| match (
            partial,
            release.filter(|_| failed),
        ) {
            _ if sent == 0 => None,
            (true, release) => Some(format!("ROLLBACK{}", release.unwrap_or_default())),
            (false, Some(release)) => Some(release.trim_start_matches("; ").to_owned()),
            (false, None) => None,
        };
        self.fly(state, requests, &bound, start, ending, judge)
    }

    /// Sends one bound flight on `state` and judges it: the accounting, the
    /// lost-reply quarantine and, when no reply was lost, the failure path
    /// `ending` gives (`sent`, partly sent, any request failed), run in one
    /// more round trip before `judge`.
    fn fly<T>(
        &self,
        state: &mut State,
        requests: &[Request<'_>],
        bound: &[Bound<'_>],
        start: Instant,
        ending: impl FnOnce(usize, bool, bool) -> Option<String>,
        judge: impl FnOnce(Vec<Result<Reply, Refusal>>, bool) -> (T, bool),
    ) -> Result<T, BackendError> {
        let flight = state.client.pipeline(bound);
        let flight = match flight {
            Ok(flight) => flight,
            Err(e) => return Err(lost(state, e.backend())),
        };
        let work = &mut state.work;
        if flight.sent > 0 {
            work.round_trips += 1;
        }
        work.statements += flight.sent as u64;
        for request in &requests[..flight.sent] {
            if let Request::Statement { bytes, .. } = request {
                work.bound_bytes += bytes;
            }
        }
        let mut lost_reply = false;
        let sent = flight.sent;
        let replies: Vec<_> = flight
            .replies
            .into_iter()
            .enumerate()
            .map(|(index, reply)| {
                reply.map_err(|e| {
                    // An unsent request's failure is the client's own: it
                    // never reached the server, so no reply of it was lost.
                    let sent = index < sent;
                    let e = if sent { e } else { e.unsent() };
                    let code = e.code().cloned();
                    let error = e.backend();
                    // A sent request with no SQLSTATE and no definite
                    // client refusal (`rows::error`): the reply went with
                    // the connection.
                    lost_reply |= sent && code.is_none() && error == BackendError::Unknown;
                    Refusal { error, code, sent }
                })
            })
            .collect();
        work.returned_rows += replies
            .iter()
            .map(|reply| match reply {
                Ok(Reply::Rows(rows)) => rows.len() as u64,
                Ok(Reply::Messages(messages)) => messages
                    .iter()
                    .filter(|m| matches!(m, SimpleQueryMessage::Row(_)))
                    .count() as u64,
                Err(_) => 0,
            })
            .sum::<u64>();
        let partial = flight.sent < requests.len();
        let failed = replies.iter().any(Result::is_err);
        let ending = match lost_reply {
            true => None,
            false => ending(flight.sent, partial, failed),
        };
        if let Some(ending) = ending {
            work.round_trips += 1;
            lost_reply = state.client.batch_execute(&ending).is_err();
        }
        state.work.statement_ns += start.elapsed().as_nanos() as u64;
        if lost_reply {
            state.quarantine();
        }
        let (answer, unknown) = judge(replies, !state.quarantined);
        if unknown {
            state.quarantine();
        }
        Ok(answer)
    }
}

/// A writer turn [`Session::decided_turn`] holds.
pub(crate) struct DecidedTurn<'s> {
    session: &'s Session,
    state: &'s mut State,
    nested: bool,
    ready: bool,
    /// The release of the write unit whose transaction a flight of this
    /// turn left open ([`Self::send_in_unit`]), until a flight closes it.
    unit: Option<String>,
    /// A flight of this turn's write unit failed: its transaction was
    /// rolled back or never began. No further unit flight is sent in the
    /// turn ([`Self::send_in_unit`]).
    failed_unit: bool,
}

/// The statements of one write unit (#818 I3b), prepared before its
/// transaction opens ([`DecidedTurn::prepare_unit`]) and held for the turn,
/// so a statement-cache clear cannot drop one between the unit's flights.
pub struct UnitStatements {
    statements: HashMap<String, Statement>,
}

/// A turn that ends with a unit's transaction open never commits it: the
/// turn's end rolls it back ([`DecidedTurn::end_unit`]). Only a panic
/// between the unit's flights reaches this drop with it open; the
/// connection may then be mid-message, so it is closed instead, which ends
/// the transaction and every session-level lock on the server.
impl Drop for DecidedTurn<'_> {
    fn drop(&mut self) {
        if self.unit.take().is_some() {
            self.state.quarantine();
        }
    }
}

impl DecidedTurn<'_> {
    /// Makes the session ready for this turn (`Session::ready`), once.
    pub(crate) fn ready(&mut self) -> Result<(), BackendError> {
        if !self.ready {
            self.session.ready(self.state, self.nested)?;
            self.ready = true;
        }
        Ok(())
    }

    /// The epoch of the connection this turn runs on, for a later turn
    /// pinned to it (`Session::ready_pinned`).
    pub(crate) fn epoch(&self) -> u64 {
        self.state.closed
    }

    /// Whether the session is quarantined: its connection closed.
    pub(crate) fn quarantined(&self) -> bool {
        self.state.quarantined
    }

    /// Sends one flight in this turn (see [`Session::decided_turn`]), once
    /// [`Self::ready`] succeeded.
    ///
    /// Refused (`Integrity`, nothing sent) while a unit's transaction is
    /// open: its scoped prepares would send `BEGIN` and `COMMIT` inside it.
    /// No caller of this flight opens one (only [`Self::send_in_unit`]
    /// does), so this is a guard.
    pub(crate) fn send<T>(
        &mut self,
        requests: &[Request<'_>],
        judge: impl FnOnce(Vec<Result<Reply, Refusal>>, bool) -> (T, bool),
    ) -> Result<T, BackendError> {
        if self.unit.is_some() {
            return Err(BackendError::Integrity);
        }
        self.ready()?;
        self.session.flight(self.state, requests, true, None, judge)
    }

    /// Prepares every one of `texts` (a write unit's statements, #818 I3b)
    /// not yet prepared on the connection, in one flight before the unit's
    /// transaction opens, outside any transaction: the texts name their
    /// tables with the schema. Counted as [`Session::prepare`] counts (one
    /// round trip, none when all are cached; each in `prepares`); a refused
    /// prepare is the caller's definite failure and a lost reply
    /// quarantines. The answer holds the handles for the rest of the turn.
    /// Refused (`Integrity`, nothing sent) while a unit's transaction is
    /// open, and inside this thread's held lane (a unit takes the lane row
    /// in its `BEGIN`); `Unknown` on a session this turn quarantined.
    pub(crate) fn prepare_unit(&mut self, texts: &[&str]) -> Result<UnitStatements, BackendError> {
        if self.unit.is_some() || self.nested {
            return Err(BackendError::Integrity);
        }
        self.ready()?;
        if self.state.quarantined {
            return Err(BackendError::Unknown);
        }
        match self.state.statements_in(texts, None) {
            Ok(statements) => Ok(UnitStatements {
                statements: (texts.iter().map(|text| (*text).to_owned()))
                    .zip(statements)
                    .collect(),
            }),
            Err(e) => Err(lost(self.state, e.backend())),
        }
    }

    /// Sends one flight of a write unit (#818 I3b): a transaction that
    /// spans several flights of this turn, each sent once its predecessor's
    /// replies are read. Its statements bind only `prepared` handles; one
    /// whose text is not there, or whose parameter count differs, refuses
    /// the flight (`Integrity`) before anything is sent. Nothing is
    /// prepared, and no other text goes out between the unit's `BEGIN` and
    /// its own `COMMIT`. The first flight carries the `BEGIN` (with the
    /// lane row, `write_begin(true)`); `open` says the flight leaves the
    /// transaction open (it ends with neither `COMMIT` nor `ROLLBACK`); the
    /// last ends with the unit's `COMMIT`, its own request.
    ///
    /// Counted as [`Session::pipeline_judged`] counts a flight; the unit's
    /// one transaction is the caller's ([`Self::count_unit`]). A lost reply
    /// quarantines before `judge`, which then sees the session untrusted;
    /// `judge` answering unknown quarantines too. When a request was
    /// refused or not sent and no reply was lost, `ROLLBACK` and `release`
    /// (as [`Session::pipeline_judged`]'s) go in one more round trip
    /// before `judge` (only `release` after a last flight whose `COMMIT`
    /// was sent and ended the transaction); if that fails the session is
    /// quarantined. The transaction is then closed. Nothing is resent.
    /// Refused (`Integrity`) inside this thread's held lane and after a
    /// failed flight of this turn's unit (its transaction is closed, so a
    /// later flight would run outside it), and `Unknown` on a session this
    /// turn quarantined. A first flight that sent nothing began no
    /// transaction and took no barrier, so it sends no `ROLLBACK`.
    pub(crate) fn send_in_unit<T>(
        &mut self,
        prepared: &UnitStatements,
        requests: &[Request<'_>],
        open: bool,
        release: &str,
        judge: impl FnOnce(Vec<Result<Reply, Refusal>>, bool) -> (T, bool),
    ) -> Result<T, BackendError> {
        if self.nested || self.failed_unit {
            return Err(BackendError::Integrity);
        }
        self.ready()?;
        if self.state.quarantined {
            return Err(BackendError::Unknown);
        }
        let start = Instant::now();
        let mut bound = Vec::with_capacity(requests.len());
        for request in requests {
            bound.push(match request {
                Request::Statement { sql, params, .. } => {
                    let statement = prepared.statements.get(*sql);
                    let statement = statement.ok_or(BackendError::Integrity)?;
                    if statement.params().len() != params.len() {
                        return Err(BackendError::Integrity);
                    }
                    Bound::Prepared(statement, params)
                }
                Request::Simple(sql) => Bound::Simple(sql),
            });
        }
        let was_open = self.unit.is_some();
        let mut failed_flight = false;
        let ending = |sent: usize, partial: bool, failed: bool| {
            failed_flight = failed;
            match failed {
                false => None,
                true if sent == 0 && !was_open => None,
                true if partial || open || sent == 0 => Some(format!("ROLLBACK{release}")),
                true if sent > 0 && !release.is_empty() => {
                    Some(release.trim_start_matches("; ").to_owned())
                }
                true => None,
            }
        };
        let answer = (self.session).fly(self.state, requests, &bound, start, ending, judge);
        // Open only after a whole flight that left it open: a failed one
        // was rolled back (or ended by its `COMMIT`), a lost one closed the
        // connection.
        let still_open = answer.is_ok() && open && !failed_flight && !self.state.quarantined;
        self.unit = still_open.then(|| release.to_owned());
        self.failed_unit |= failed_flight || answer.is_err();
        answer
    }

    /// Counts the write unit's one transaction, committed or rolled back,
    /// once a flight of it closed it (its `COMMIT`, or the `ROLLBACK` the
    /// session sent after a refusal). A unit the turn's end rolls back is
    /// counted there instead.
    pub(crate) fn count_unit(&mut self, committed: bool) {
        count_transaction(self.state, committed);
    }

    /// The turn's end: a unit's transaction still open is rolled back with
    /// its release, in one round trip, and counted as a rolled-back write;
    /// that failing quarantines the session. Never a `COMMIT`.
    fn end_unit(&mut self) {
        let Some(release) = self.unit.take() else {
            return;
        };
        if self.state.quarantined {
            return;
        }
        self.state.work.round_trips += 1;
        let rolled_back = self
            .state
            .client
            .batch_execute(&format!("ROLLBACK{release}"));
        if rolled_back.is_err() {
            self.state.quarantine();
        }
        count_transaction(self.state, false);
    }
}

impl Session {
    /// Counts one transaction a pipelined flight began and ended: a write,
    /// committed or rolled back.
    pub(crate) fn count_fused(&self, committed: bool) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        count_transaction(&mut state, committed);
    }
}

/// One write transaction, committed or rolled back.
fn count_transaction(state: &mut State, committed: bool) {
    let work = &mut state.work;
    work.transactions += 1;
    work.write_transactions += 1;
    if committed {
        work.commits += 1;
        work.write_commits += 1;
    } else {
        work.rollbacks += 1;
    }
}

/// A refusal before the flight: an unknown outcome quarantines the session.
fn lost(state: &mut State, error: BackendError) -> BackendError {
    if error == BackendError::Unknown {
        state.quarantine();
    }
    error
}
