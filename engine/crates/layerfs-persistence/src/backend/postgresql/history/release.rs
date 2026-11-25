//! A mounted Workspace's owner release on PostgreSQL as one pipelined
//! message (#768 step C): the fenced `DELETE … RETURNING`
//! (`gc_intent_release_owner`, unchanged) and, when this session holds the
//! op's keys, the unlock (`sql/postgresql/mount/unlock.sql`), each request
//! in its own `Sync`, one round trip.
//!
//! The unlock gives the keys back only when the Store is still at the
//! session's generation and no live row of the op remains, so it runs
//! exactly when the statement-by-statement release would have unlocked:
//! after a delete that ran, or one that found the row already gone. A
//! refused or stale delete keeps its row, and the unlock then gives nothing
//! back, so the caller keeps the keys with the op as before.
//!
//! | Delete | Unlock | Answer | Keys |
//! | --- | --- | --- | --- |
//! | ran (generation matches) | one row per key | `Ok` | given back |
//! | ran, row already gone | one row per key | `Ok` | given back |
//! | stale (generation moved) | no row | `StaleStore` | kept with the op |
//! | a definite SQLSTATE | no row | that refusal | kept with the op |
//! | answered | refused, or a reply lost | the delete's answer | unknown: the session is quarantined, its connection closes with every key |
//! | no reply, or a SQLSTATE without a definite rule | — | `UnknownOutcome` | unknown: quarantined |
//!
//! Nothing is resent: an unknown outcome quarantines the session while its
//! lock is still held (`Session::pipeline_decided`).
use super::statements;
use crate::backend::postgresql::session::{
    connection::{Session, SessionKey},
    pipeline::{Refusal, Reply, Request},
    rows,
};
use crate::backend::records::BackendError;
use layerfs_history::error::HistoryError;
use postgres::types::ToSql;
use std::sync::atomic::Ordering;

const UNLOCK: &str = include_str!("../../../../sql/postgresql/mount/unlock.sql");

/// The release's unlock, its schema named.
pub(crate) fn unlock_sql(schema: &str) -> String {
    UNLOCK.replace("{s}", schema)
}

fn delete_sql(schema: &str) -> Result<String, BackendError> {
    statements::bare("gc_intent_release_owner")
        .map(|sql| sql.replace("{s}", schema))
        .ok_or(BackendError::Integrity)
}

/// How one owner release ended.
pub(crate) struct Released {
    /// The delete's answer, as the statement-by-statement release gave it.
    pub(crate) answer: Result<(), HistoryError>,
    /// Whether the session still holds the op's keys (the unlock gave
    /// nothing back): the caller keeps them with the op.
    pub(crate) kept: bool,
}

/// Sends one owner release of `op_id` (see the module documentation), with
/// the unlock of `keys` when there are any. `Err` only when nothing was
/// sent: the caller keeps the keys with the op.
pub(crate) fn release(
    session: &Session,
    op_id: &[u8; 16],
    keys: &[SessionKey],
) -> Result<Released, BackendError> {
    let expected = i64::try_from(session.incarnation.load(Ordering::SeqCst))
        .map_err(|_| BackendError::Integrity)?;
    let schema = session.settings.schema.as_str();
    let delete = delete_sql(schema)?;
    let unlock = unlock_sql(schema);
    let op = op_id.as_slice();
    let (ids, modes): (Vec<i64>, Vec<bool>) = keys.iter().copied().unzip();
    let delete_params: [&(dyn ToSql + Sync); 2] = [&op, &expected];
    let unlock_params: [&(dyn ToSql + Sync); 4] = [&ids, &modes, &op, &expected];
    let mut requests = vec![Request::Statement {
        sql: &delete,
        params: &delete_params,
        bytes: 24,
    }];
    if !keys.is_empty() {
        requests.push(Request::Statement {
            sql: &unlock,
            params: &unlock_params,
            bytes: 9 * ids.len() as u64 + 24,
        });
    }
    session.pipeline_decided(&requests, |replies, trusted| {
        decide(session, keys.len(), expected, replies, trusted)
    })
}

/// A refusal that is definite: a SQLSTATE with a definite rule, or a
/// request the client never sent on a session still trusted.
fn definite(refusal: &Refusal, trusted: bool) -> Option<BackendError> {
    match refusal.code.as_ref() {
        Some(code) => Some(rows::sqlstate(code.code())).filter(|e| *e != BackendError::Unknown),
        None => (!refusal.sent && trusted && refusal.error != BackendError::Unknown)
            .then_some(refusal.error),
    }
}

/// The release's outcome and whether it is unknown (the session is then
/// quarantined).
fn decide(
    session: &Session,
    keys: usize,
    expected: i64,
    replies: Vec<Result<Reply, Refusal>>,
    trusted: bool,
) -> (Released, bool) {
    let mut replies = replies.into_iter();
    let answer = match replies.next() {
        Some(Ok(Reply::Rows(found))) => match found.first().map(|row| row.try_get::<_, i64>(0)) {
            Some(Ok(generation)) if expected != 0 && generation != expected => {
                Some(Err(HistoryError::StaleStore))
            }
            Some(Ok(_)) => Some(Ok(())),
            // The delete may have run: a reply that cannot be read is an
            // unknown outcome.
            _ => None,
        },
        Some(Err(refusal)) => definite(&refusal, trusted).map(|error| Err(error.into())),
        _ => None,
    };
    let Some(answer) = answer else {
        let unknown = Released {
            answer: Err(BackendError::Unknown.into()),
            kept: false,
        };
        return (unknown, true);
    };
    // Some(kept), or None when what the session holds is unknown.
    let kept = match (keys, replies.next()) {
        (0, _) => Some(false),
        (_, Some(Ok(Reply::Rows(found)))) if found.is_empty() => Some(true),
        (_, Some(Ok(Reply::Rows(found)))) if found.len() == keys => {
            // Given back. A pid other than the session's (a pooled
            // connection) gave them back on another backend; as before, the
            // release still answers its delete.
            for row in &found {
                if let Ok(pid) = row.try_get::<_, i64>(1) {
                    let _ = session.note_backend(pid);
                }
            }
            Some(false)
        }
        (_, Some(Err(refusal))) if !refusal.sent && trusted => Some(true),
        _ => None,
    };
    match kept {
        Some(kept) => (Released { answer, kept }, false),
        // The delete's answer stands; the closed connection gives back
        // every key, so none is kept.
        None => (
            Released {
                answer,
                kept: false,
            },
            true,
        ),
    }
}
