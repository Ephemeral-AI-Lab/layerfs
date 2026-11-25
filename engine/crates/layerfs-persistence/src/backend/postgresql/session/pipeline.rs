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
use crate::backend::records::BackendError;
use layerfs_telemetry::timer::waits::Wait;
use postgres::{error::SqlState, types::ToSql, Row, SimpleQueryMessage, Statement};
use std::{
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

    /// [`Self::pipeline_judged`] that may also run inside this thread's
    /// held lane: for a flight that never takes the lane row (a mount's
    /// admission, an owner's release). Statements not yet prepared on the
    /// connection are prepared inside a read transaction whose `SET LOCAL
    /// search_path` resolves their unqualified names (one round trip and one
    /// read transaction, counted), in the same turn as the flight, so no
    /// other caller can evict them from the cache in between.
    pub(crate) fn pipeline_decided<T>(
        &self,
        requests: &[Request<'_>],
        judge: impl FnOnce(Vec<Result<Reply, Refusal>>, bool) -> (T, bool),
    ) -> Result<T, BackendError> {
        self.pipeline_turn(requests, true, true, None, judge)
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
        if state.quarantined {
            return Err(BackendError::Unknown);
        }
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
        let flight = state.client.pipeline(&bound);
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
        let ending = match (partial, release.filter(|_| failed)) {
            _ if lost_reply || flight.sent == 0 => None,
            (true, release) => Some(format!("ROLLBACK{}", release.unwrap_or_default())),
            (false, Some(release)) => Some(release.trim_start_matches("; ").to_owned()),
            (false, None) => None,
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

impl Session {
    /// Counts one transaction a pipelined flight began and ended: a write,
    /// committed or rolled back.
    pub(crate) fn count_fused(&self, committed: bool) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
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
}

/// A refusal before the flight: an unknown outcome quarantines the session.
fn lost(state: &mut State, error: BackendError) -> BackendError {
    if error == BackendError::Unknown {
        state.quarantine();
    }
    error
}
