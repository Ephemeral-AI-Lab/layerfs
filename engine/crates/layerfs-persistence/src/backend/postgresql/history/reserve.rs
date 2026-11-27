//! One inode reservation as one pipelined message (#818 W4): `BEGIN` with
//! the lane row, the generation fence and attempt token, the reservation
//! (`allocation_reserve`: an upsert whose checks run on the server), and the
//! token-checked `COMMIT`, in one round trip once its statement is prepared
//! (the SDK open prepares it, `mount::create_texts`). The answers and their
//! order are `allocation::reserve_inodes`'s: the scope's authority, then the
//! capacity. A refusal writes nothing and commits the empty transaction. A
//! lost reply quarantines the session and answers unknown; nothing is
//! resent.
use super::super::session::attempt::nonce;
use crate::backend::{
    error::BackendError,
    postgresql::session::{
        connection::Session,
        pipeline::{Refusal, Reply, Request},
        profile::barrier_leftover,
    },
};
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::identity::CatalogId;
use layerfs_history::records::{Reservation, ReserveRequest};
use postgres::{error::SqlState, SimpleQueryMessage};
use std::sync::atomic::Ordering;

const RESERVE: &str =
    include_str!("../../../../sql/postgresql/history/node/allocation_reserve.sql");

/// The reservation's statement for the Store's `schema`.
pub(crate) fn text(schema: &str) -> String {
    RESERVE.replace("{s}", schema)
}

/// What the reservation's row says.
enum Answer {
    /// The range was taken: the high-water before it.
    Reserved(u64),
    /// Nothing was written: the scope's row as it stands.
    Current(u64, Vec<u8>),
    /// Nothing was written and the scope has no row.
    Absent,
}

/// Reserves `request.count` (at least 1) serials of `request.scope` for
/// `catalog` in one message.
pub(crate) fn run(
    session: &Session,
    catalog: CatalogId,
    request: &ReserveRequest,
) -> HistoryResult<Reservation> {
    let generation = session.incarnation.load(Ordering::SeqCst);
    let schema = &session.settings.schema;
    let attempt = nonce();
    let begin = session.settings.write_begin(true);
    // As `binding_append`: the fence after the lane, a request of its own.
    let fence = format!(
        "SELECT CASE WHEN g IS NULL THEN (32768 + (g IS NULL)::int)::smallint::int \
         ELSE 1/(g={generation})::int END FROM \
         (SELECT (SELECT catalog_incarnation FROM {schema}.history_meta WHERE id=1) AS g) m; \
         SELECT set_config('layerfs.attempt','{attempt}',true)"
    );
    let commit = format!(
        "SELECT current_setting('layerfs.attempt',true) WHERE \
         1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM '{attempt}')::int=1; \
         COMMIT"
    );
    let sql = text(schema);
    let scope = request.scope.as_bytes().as_slice();
    let authority = catalog.as_slice();
    // A count past the column's range fits no range: bound as its largest.
    let count = i64::try_from(request.count).unwrap_or(i64::MAX);
    let requests = [
        Request::Simple(&begin),
        Request::Simple(&fence),
        Request::Statement {
            sql: &sql,
            params: &[&scope, &authority, &count, &attempt],
            bytes: 72 + attempt.len() as u64,
        },
        Request::Simple(&commit),
    ];
    let cleanup = format!("; ROLLBACK{}", barrier_leftover(schema));
    let outcome = session.pipeline_judged(&requests, &cleanup, |replies, trusted| {
        let answer = decide(&replies, trusted, &attempt);
        let unknown = matches!(answer, Err(BackendError::Unknown));
        (answer, unknown)
    })?;
    session.count_fused(outcome.is_ok());
    let refused = |what| Err(HistoryError::Integrity(what));
    let capacity = Err(HistoryError::Capacity("inode serials"));
    match outcome? {
        Answer::Reserved(highwater) => Ok(Reservation {
            scope: request.scope,
            start: highwater + 1,
            count: request.count,
        }),
        Answer::Current(_, owner) if owner.as_slice() != authority => refused("scope authority"),
        Answer::Current(highwater, _) => match highwater.checked_add(request.count) {
            Some(end) if end < i64::MAX as u64 => refused("scope high-water"),
            _ => capacity,
        },
        Answer::Absent if request.count >= i64::MAX as u64 => capacity,
        Answer::Absent => refused("scope high-water"),
    }
}

fn decide(
    replies: &[Result<Reply, Refusal>],
    trusted: bool,
    attempt: &str,
) -> Result<Answer, BackendError> {
    if !trusted {
        return Err(BackendError::Unknown);
    }
    for (index, reply) in replies.iter().enumerate() {
        if let Err(refusal) = reply {
            return Err(match &refusal.code {
                Some(code) if index == 1 && *code == SqlState::DIVISION_BY_ZERO => {
                    BackendError::Stale
                }
                Some(_) => refusal.error,
                None if !refusal.sent => refusal.error,
                None => BackendError::Unknown,
            });
        }
    }
    let [Ok(Reply::Messages(_)), Ok(Reply::Messages(fence)), Ok(Reply::Rows(rows)), Ok(Reply::Messages(commit))] =
        replies
    else {
        return Err(BackendError::Unknown);
    };
    use SimpleQueryMessage::{CommandComplete, Row, RowDescription};
    let valid_fence = matches!(
        fence.as_slice(),
        [RowDescription(_), Row(ok), CommandComplete(1), RowDescription(_), Row(token), CommandComplete(1)]
            if ok.try_get(0).ok().flatten() == Some("1")
            && token.try_get(0).ok().flatten() == Some(attempt)
    );
    let valid_commit = matches!(
        commit.as_slice(),
        [RowDescription(_), Row(token), CommandComplete(1), CommandComplete(0)]
            if token.try_get(0).ok().flatten() == Some(attempt)
    );
    if !(valid_fence && valid_commit) {
        return Err(BackendError::Unknown);
    }
    let unsigned = |value: i64| u64::try_from(value).map_err(|_| BackendError::Integrity);
    match rows.as_slice() {
        [] => Ok(Answer::Absent),
        [row] => {
            let cell = |at| {
                row.try_get::<_, Option<i64>>(at)
                    .map_err(|_| BackendError::Integrity)
            };
            let owner: Option<Vec<u8>> = row.try_get(2).map_err(|_| BackendError::Integrity)?;
            match (cell(0)?, cell(1)?, owner) {
                (Some(reserved), None, None) => Ok(Answer::Reserved(unsigned(reserved)?)),
                (None, Some(highwater), Some(owner)) => {
                    Ok(Answer::Current(unsigned(highwater)?, owner))
                }
                _ => Err(BackendError::Integrity),
            }
        }
        _ => Err(BackendError::Integrity),
    }
}
