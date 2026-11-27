//! Write units that wait (#818 I3b part A): `put_entries` and
//! `place_children` as one transaction over several flights of one writer
//! turn (`DecidedTurn::prepare_unit`, `send_in_unit`).
//!
//! A unit's statements go through [`Writes`]: on today's route each runs at
//! once in its transaction; on a [`Unit`] a statement whose reply decides
//! what is sent next (the dependency check, a dependent row's binding) is a
//! wait and ends a flight, and every other statement is queued and sent, in
//! order, at the head of the next flight. A queued statement's `check` reads
//! its rows when its flight returns, in request order, before any later
//! reply is read, so a decoded `Changed` comes where the per-statement loop
//! found it. The first flight opens with `BEGIN` (the lane row and barrier),
//! the generation fence and attempt token ([`flights::opening`]) and the
//! owner assertion; the last sends what is queued, `CHARGED`, the token
//! check and `COMMIT`, its own request. The first refusal decides, as in
//! [`flights`]; a lost reply is `Unknown` (the session quarantines); a
//! refusal from the client (a bound, a malformed reply) first sends what is
//! queued, so an earlier server refusal still wins; a unit the body leaves
//! open is rolled back by the turn's end. Nothing is resent.
use super::{
    flights::{self, divided, opening, refused, text, Identity, CHARGED, OWNER_LIVE},
    placement, writes,
};
use crate::backend::{
    error::BackendError,
    postgresql::session::{
        connection::Session,
        pipeline::{DecidedTurn, Refusal, Reply, Request, UnitStatements},
        profile::barrier_leftover,
        transaction::Transaction,
    },
    sqlite::acquisition::statements::Failure,
};
use postgres::{types::ToSql, Row};

/// One owned parameter of a queued statement.
pub(super) type Value = Box<dyn ToSql + Sync>;
/// Reads a queued statement's rows when its flight returns.
pub(super) type Check<'a> = Box<dyn FnOnce(&[Row]) -> Result<(), Failure> + 'a>;

/// Boxes each expression as a queued statement's parameter.
macro_rules! values {
    ($($value:expr),* $(,)?) => {
        vec![$(Box::new($value) as crate::backend::postgresql::acquisition::unit::Value),*]
    };
}
pub(super) use values;

/// Where a write unit's statements run. `sql` names the schema as `{s}`.
pub(super) trait Writes<'a> {
    /// One statement whose rows the unit reads before it goes on: a wait.
    fn ask(
        &mut self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        bytes: u64,
    ) -> Result<Vec<Row>, Failure>;
    /// One statement the unit goes on without; `check` reads its rows once
    /// they are back, before any later statement's.
    fn queue(
        &mut self,
        sql: &str,
        params: Vec<Value>,
        bytes: u64,
        check: Option<Check<'a>>,
    ) -> Result<(), Failure>;
    /// Runs everything queued (with the unit's opening), so a refusal of
    /// what came before decides before the caller's next step.
    fn settle(&mut self) -> Result<(), Failure>;
}

impl<'a> Writes<'a> for &Transaction<'_> {
    fn ask(
        &mut self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        bytes: u64,
    ) -> Result<Vec<Row>, Failure> {
        Ok(self.typed(&text(sql, self.schema), params, bytes)?)
    }

    fn queue(
        &mut self,
        sql: &str,
        params: Vec<Value>,
        bytes: u64,
        check: Option<Check<'a>>,
    ) -> Result<(), Failure> {
        let params: Vec<&(dyn ToSql + Sync)> = params.iter().map(|value| &**value as _).collect();
        let rows = self.typed(&text(sql, self.schema), &params, bytes)?;
        check.map_or(Ok(()), |check| check(&rows))
    }

    fn settle(&mut self) -> Result<(), Failure> {
        Ok(())
    }
}

struct Queued<'a> {
    sql: String,
    params: Vec<Value>,
    bytes: u64,
    check: Option<Check<'a>>,
}

/// What ends a flight.
enum Tail<'p> {
    Ask(String, &'p [&'p (dyn ToSql + Sync)], u64),
    Settle,
    /// The charge's rows and bytes; the flight closes the unit.
    Finish(i64, i64),
}

/// One write unit in flight: its turn, prepared statements and queue.
pub(super) struct Unit<'t, 's, 'a> {
    turn: &'t mut DecidedTurn<'s>,
    statements: UnitStatements,
    schema: String,
    identity: Identity,
    begin: String,
    check: String,
    release: String,
    begun: bool,
    /// The unit's transaction ended (committed, rolled back or uncertain),
    /// or a send failed before it went out: nothing more is sent.
    closed: bool,
    queued: Vec<Queued<'a>>,
}

/// Runs `body` as one unit on `session`, then closes it with the charge
/// `body` answers. Its one prepare covers every acquisition write
/// statement (this unit's, the other waiting unit's and the flights'), so
/// on a session that runs Init the later unit kinds find theirs cached.
pub(super) fn run<'a, T>(
    session: &Session,
    identity: Identity,
    body: impl FnOnce(&mut Unit<'_, '_, 'a>) -> Result<(T, i64, i64), Failure>,
) -> Result<T, Failure> {
    let schema = session.settings.schema.clone();
    let texts = flights::TEXTS
        .iter()
        .chain(writes::TEXTS)
        .chain(placement::TEXTS);
    let qualified: Vec<String> = texts.map(|sql| text(sql, &schema)).collect();
    let (begin, check) = opening(session);
    let release = barrier_leftover(&schema);
    session.decided_turn(|turn| {
        let prepared: Vec<&str> = qualified.iter().map(String::as_str).collect();
        let statements = turn.prepare_unit(&prepared)?;
        let mut unit = Unit {
            turn,
            statements,
            schema,
            identity,
            begin,
            check,
            release,
            begun: false,
            closed: false,
            queued: Vec::new(),
        };
        Ok(match body(&mut unit) {
            Ok((answer, rows, bytes)) => unit.finish(rows, bytes).map(|()| answer),
            // A refusal from the client comes after what it queued.
            Err(failure) => unit.settle().and(Err(failure)),
        })
    })?
}

impl<'a> Writes<'a> for Unit<'_, '_, 'a> {
    fn ask(
        &mut self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        bytes: u64,
    ) -> Result<Vec<Row>, Failure> {
        let sql = text(sql, &self.schema);
        self.fly(Tail::Ask(sql, params, bytes))
    }

    fn queue(
        &mut self,
        sql: &str,
        params: Vec<Value>,
        bytes: u64,
        check: Option<Check<'a>>,
    ) -> Result<(), Failure> {
        let sql = text(sql, &self.schema);
        self.queued.push(Queued {
            sql,
            params,
            bytes,
            check,
        });
        Ok(())
    }

    fn settle(&mut self) -> Result<(), Failure> {
        match self.closed || (self.begun && self.queued.is_empty()) {
            true => Ok(()),
            false => self.fly(Tail::Settle).map(drop),
        }
    }
}

impl Unit<'_, '_, '_> {
    /// Sends what is queued, the charge, the token check and `COMMIT`; what
    /// is queued with a check is settled first, since the commit comes
    /// after its reply is read.
    fn finish(&mut self, rows: i64, bytes: i64) -> Result<(), Failure> {
        if self.queued.iter().any(|queued| queued.check.is_some()) {
            self.fly(Tail::Settle)?;
        }
        self.fly(Tail::Finish(rows, bytes)).map(drop)
    }

    /// One flight: the opening once, what is queued, then `tail`.
    fn fly(&mut self, tail: Tail<'_>) -> Result<Vec<Row>, Failure> {
        if self.closed {
            return Err(BackendError::Integrity.into());
        }
        let (operation, epoch) = self.identity;
        let owner_sql = text(OWNER_LIVE, &self.schema);
        let charged_sql = text(CHARGED, &self.schema);
        let owner: [&(dyn ToSql + Sync); 2] = [&operation, &epoch];
        let opening = !self.begun;
        let first = if opening { 2 } else { 0 };
        let mut queued = std::mem::take(&mut self.queued);
        let checks: Vec<(usize, Check<'_>)> = (queued.iter_mut().enumerate())
            .filter_map(|(i, q)| q.check.take().map(|check| (first + i, check)))
            .collect();
        let params: Vec<Vec<&(dyn ToSql + Sync)>> = queued
            .iter()
            .map(|q| q.params.iter().map(|value| &**value as _).collect())
            .collect();
        let mut requests = Vec::with_capacity(first + queued.len() + 3);
        if opening {
            requests.push(Request::Simple(&self.begin));
            requests.push(Request::Statement {
                sql: &owner_sql,
                params: &owner,
                bytes: 16,
            });
        }
        for (q, params) in queued.iter().zip(&params) {
            requests.push(Request::Statement {
                sql: &q.sql,
                params,
                bytes: q.bytes,
            });
        }
        let charge;
        let (mut charged, mut ask, mut finish) = (None, false, false);
        match &tail {
            Tail::Ask(sql, params, bytes) => {
                ask = true;
                requests.push(Request::Statement {
                    sql,
                    params,
                    bytes: *bytes,
                });
            }
            Tail::Settle => {}
            Tail::Finish(rows, bytes) => {
                finish = true;
                charge = [&operation, &epoch, rows, bytes] as [&(dyn ToSql + Sync); 4];
                charged = Some(requests.len());
                requests.push(Request::Statement {
                    sql: &charged_sql,
                    params: &charge,
                    bytes: 32,
                });
                requests.push(Request::Simple(&self.check));
                requests.push(Request::Simple("COMMIT"));
            }
        }
        let total = requests.len();
        let sent = self.turn.send_in_unit(
            &self.statements,
            &requests,
            !finish,
            &self.release,
            |replies, trusted| {
                let (verdict, closed) = judge(
                    replies, trusted, total, opening, charged, checks, ask, finish,
                );
                let unknown = matches!(verdict, Err(Failure::Backend(BackendError::Unknown)));
                ((verdict, closed || unknown), unknown)
            },
        );
        self.begun = true;
        let (verdict, closed) = match sent {
            Ok(judged) => judged,
            Err(error) => {
                // Refused before sending, or a lost prepare: an open unit is
                // the turn's end's to roll back.
                self.closed = true;
                return Err(error.into());
            }
        };
        if closed {
            self.closed = true;
            self.turn.count_unit(finish && verdict.is_ok());
        }
        verdict
    }
}

/// A flight's answer and whether it ended the unit's transaction.
#[allow(clippy::too_many_arguments)]
fn judge(
    replies: Vec<Result<Reply, Refusal>>,
    trusted: bool,
    total: usize,
    opening: bool,
    charged: Option<usize>,
    checks: Vec<(usize, Check<'_>)>,
    ask: bool,
    finish: bool,
) -> (Result<Vec<Row>, Failure>, bool) {
    let mut checks = checks.into_iter().peekable();
    let count = replies.len();
    // A refused or unsent request ends the transaction (the session rolls
    // it back), even after an earlier queued check failed.
    let failed = count != total || replies.iter().any(Result::is_err);
    let mut last = None;
    for (index, reply) in replies.into_iter().enumerate() {
        let reply = match reply {
            Ok(reply) => reply,
            Err(refusal) => {
                let failure = match divided(&refusal) {
                    // The generation fence, as the ordinary `BEGIN` batch.
                    true if opening && index == 0 => BackendError::Stale.into(),
                    true if (opening && index == 1) || charged == Some(index) => Failure::Stale,
                    _ => refused(&refusal, trusted),
                };
                return (Err(failure), true);
            }
        };
        if let Some((_, check)) = checks.next_if(|(at, _)| *at == index) {
            let Reply::Rows(rows) = &reply else {
                return (Err(BackendError::Unknown.into()), true);
            };
            if let Err(failure) = check(rows) {
                return (Err(failure), finish || failed || !trusted);
            }
        }
        last = Some(reply);
    }
    if !trusted || count != total {
        return (Err(BackendError::Unknown.into()), true);
    }
    let answer = match (ask, last) {
        (true, Some(Reply::Rows(rows))) => Ok(rows),
        (true, _) => Err(BackendError::Unknown.into()),
        (false, _) => Ok(Vec::new()),
    };
    let closed = finish || answer.is_err();
    (answer, closed)
}
