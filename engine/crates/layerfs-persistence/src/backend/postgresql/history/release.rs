//! A mounted Workspace's owner release on PostgreSQL as one pipelined
//! message (#768 step C): the fenced `DELETE … RETURNING`
//! (`gc_intent_release_owner`) and, when this session holds the op's keys,
//! the unlock (`sql/postgresql/mount/unlock.sql`), each request in its own
//! `Sync`, one round trip.
//!
//! The delete, when it completes at the session's generation, records this
//! attempt's token in the session setting `layerfs.released`; the unlock
//! gives the keys back only when it finds that token. So it gives them back
//! exactly when the statement-by-statement release answered success and
//! then unlocked: after a delete that ran, or one that found the row
//! already gone, even if the generation moves before the unlock runs. A
//! refused or stale delete records nothing, and the unlock gives nothing
//! back, so the caller keeps the keys with the op as before.
//!
//! | Delete | Unlock | Answer | Keys |
//! | --- | --- | --- | --- |
//! | ran (generation matches) | one row per key | `Ok` | given back |
//! | ran, row already gone | one row per key | `Ok` | given back |
//! | ran, then the generation moved | one row per key | `Ok` | given back |
//! | stale (generation moved) | no row | `StaleStore` | kept with the op |
//! | a definite SQLSTATE | no row | that refusal | kept with the op |
//! | answered | refused, or a reply lost | the delete's answer | unknown: the session is quarantined, its connection closes with every key |
//! | no reply, or a SQLSTATE without a definite rule | — | `UnknownOutcome` | unknown: quarantined |
//! | not sent: the turn found the writer quarantined, or a prepare in the flight lost its reply | — | that refusal | gone with the connection; the row is an orphan for the next reopen |
//!
//! Nothing is resent: an unknown outcome quarantines the session while its
//! lock is still held (`Session::decided_turn`).
use super::{fused::nonce, statements};
use crate::backend::error::BackendError;
use crate::backend::postgresql::session::{
    connection::{Session, SessionKey},
    pipeline::{Refusal, Reply, Request},
    rows,
};
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
/// the unlock of the keys this session holds for it. The op is taken from
/// `held_ops` inside the release's own writer turn, after the session is
/// ready (a reopen there took its keys again), and put back there when the
/// unlock gave nothing back, before the turn ends: no reopen sees the op
/// half released. A turn refused on a quarantined writer sends nothing; the
/// closed connection gave the op's keys back, so the op leaves `held_ops`
/// (no reopen takes them again) and its row, which its owner has given up,
/// is an orphan the next reopen deletes (`session/reopen.rs`). `Err` only
/// when nothing was sent.
pub(crate) fn release(session: &Session, op_id: &[u8; 16]) -> Result<Released, BackendError> {
    let expected = i64::try_from(session.incarnation.load(Ordering::SeqCst))
        .map_err(|_| BackendError::Integrity)?;
    let schema = session.settings.schema.as_str();
    let delete = delete_sql(schema)?;
    let unlock = unlock_sql(schema);
    let op = op_id.as_slice();
    let token = nonce();
    session.decided_turn(|turn| {
        if let Err(error) = turn.ready() {
            orphaned(session, op_id);
            return Err(error);
        }
        let keys: Vec<SessionKey> = session.take_op(op_id).unwrap_or_default();
        let (ids, modes): (Vec<i64>, Vec<bool>) = keys.iter().copied().unzip();
        let delete_params: [&(dyn ToSql + Sync); 3] = [&op, &expected, &token];
        let unlock_params: [&(dyn ToSql + Sync); 3] = [&ids, &modes, &token];
        let mut requests = vec![Request::Statement {
            sql: &delete,
            params: &delete_params,
            bytes: 24 + token.len() as u64,
        }];
        if !keys.is_empty() {
            requests.push(Request::Statement {
                sql: &unlock,
                params: &unlock_params,
                bytes: 9 * ids.len() as u64 + token.len() as u64,
            });
        }
        let released = turn.send(&requests, |replies, trusted| {
            decide(session, keys.len(), expected, replies, trusted)
        });
        let kept = match &released {
            Ok(released) => released.kept,
            // Nothing was sent: the keys stay held unless the connection
            // closed (a lost prepare reply), which gave them back; the op
            // was taken above, so its row is recorded here directly.
            Err(_) if turn.quarantined() => {
                session.note_orphan(*op_id);
                false
            }
            Err(_) => true,
        };
        if kept && !keys.is_empty() {
            session.hold_op(*op_id, keys);
        }
        released
    })
}

/// An op whose release could not be sent on a quarantined writer: its keys
/// went with the connection, and its row is left for the next reopen.
fn orphaned(session: &Session, op_id: &[u8; 16]) {
    if session.take_op(op_id).is_some() {
        session.note_orphan(*op_id);
    }
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
