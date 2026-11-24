//! PostgreSQL session-scoped advisory keys: the shared/exclusive keys a
//! mount or removal keeps across its statements, taken and given back in one
//! statement each; the owner's release; and the refusal a
//! transaction-pooled connection gets.

use super::*;
use crate::backend::postgresql::history::statements;
use crate::backend::postgresql::session::connection::{Session, SessionKey};
use crate::backend::postgresql::session::transaction::Transaction as Statements;
use crate::backend::records::BackendError;
use std::sync::{atomic::Ordering, Arc};

const LOCK_SESSION: &str = "gc_lock_session";
const UNLOCK_SESSION: &str = "gc_unlock_session";
const RELEASE_OWNER: &str = "gc_intent_release_owner";

/// The refusal a transaction-pooled connection gets for a session lock.
const POOLED: &str = "transaction-pooled PostgreSQL connection";

/// One `Mode::Bare` statement's text, with the Store's schema named.
fn bare(tx: &Statements<'_>, key: &str) -> Result<String, BackendError> {
    statements::bare(key)
        .map(|sql| sql.replace("{s}", tx.schema))
        .ok_or(BackendError::Integrity)
}

/// The generation the session's statements must still see: `0` before its
/// open set one, which leaves them unfenced as every other statement is.
fn expected(session: &Session) -> Result<i64, BackendError> {
    i64::try_from(session.incarnation.load(Ordering::SeqCst)).map_err(|_| BackendError::Integrity)
}

/// One row of the lock statement: whether its key was granted (`None`: not
/// tried), the backend that ran it and the Store's generation.
struct Granted {
    granted: Option<bool>,
    pid: i64,
    generation: i64,
}

/// Takes all of one intent's session keys in one statement and returns them
/// as this session now holds them (spec §5.2: all or none). A stale session
/// takes none and is `StaleStore`; a refused key unlocks exactly the keys the
/// statement granted, then reports the refusal of the first refused key in
/// Store → project → Branch order.
///
/// The statement's reply decides what is held. A reply that cannot be read,
/// or a failed statement (one may fail after granting some keys, and session
/// locks survive the rollback), leaves the held keys unknown: it is
/// `Unknown`, which quarantines the session and closes its connection, so
/// the server gives back every key it holds.
pub(super) fn session_lock(
    session: &Arc<Session>,
    catalog: CatalogId,
    keys: &[(u8, Vec<u8>, bool)],
) -> HistoryResult<Vec<SessionKey>> {
    let held: Vec<SessionKey> = (keys.iter())
        .map(|(kind, id, shared)| (advisory_key(catalog, *kind, id), *shared))
        .collect();
    let (ids, modes): (Vec<i64>, Vec<bool>) = held.iter().copied().unzip();
    let expected = expected(session)?;
    let rows = session.run_bare(|tx| -> Result<Vec<Granted>, BackendError> {
        let sql = bare(tx, LOCK_SESSION)?;
        let bytes = 9 * ids.len() as u64 + 8;
        let found = (tx.typed(&sql, &[&ids, &modes, &expected], bytes))
            .map_err(|_| BackendError::Unknown)?;
        if found.is_empty() {
            // No generation row: the statement tried no key.
            return Err(BackendError::Integrity);
        }
        let mut rows: Vec<Option<Granted>> = (0..ids.len()).map(|_| None).collect();
        for row in &found {
            let read = || -> Result<(usize, Granted), postgres::Error> {
                let ord: i64 = row.try_get(0)?;
                let granted = Granted {
                    granted: row.try_get(1)?,
                    pid: row.try_get(2)?,
                    generation: row.try_get(3)?,
                };
                Ok((usize::try_from(ord).unwrap_or(0), granted))
            };
            let (ord, granted) = read().map_err(|_| BackendError::Unknown)?;
            let slot = (ord.checked_sub(1))
                .and_then(|index| rows.get_mut(index))
                .ok_or(BackendError::Unknown)?;
            *slot = Some(granted);
        }
        rows.into_iter()
            .collect::<Option<Vec<Granted>>>()
            .ok_or(BackendError::Unknown)
    })?;
    let granted: Vec<SessionKey> = (held.iter().zip(&rows))
        .filter(|(_, row)| row.granted == Some(true))
        .map(|(key, _)| *key)
        .collect();
    if rows
        .iter()
        .any(|row| expected != 0 && row.generation != expected)
    {
        // The statement tries no key on a moved Store, so none was taken
        // and then answered `Stale` (#776 P2); any reported granted anyway
        // is given back.
        let _ = session_unlock(session, &granted);
        return Err(HistoryError::StaleStore);
    }
    // Spec §5.2 item 34: a session advisory lock only means presence if the
    // same backend keeps it. The lock statement returns the backend that
    // took it; a transaction-pooling proxy hands the next statement to a
    // different one, so mounts and compaction are `Unsupported` there and
    // every caller refuses (#776 C1). The unlock is best effort: behind such
    // a proxy it may reach another backend, which then keeps the key until
    // the proxy recycles it.
    if rows.iter().any(|row| !session.note_backend(row.pid)) {
        let _ = session_unlock(session, &granted);
        return Err(HistoryError::Unsupported(POOLED));
    }
    if let Some(index) = rows.iter().position(|row| row.granted != Some(true)) {
        let _ = session_unlock(session, &granted);
        let (kind, id, shared) = &keys[index];
        return Err(session_lock_refusal(
            session,
            *kind,
            id,
            held[index].0,
            *shared,
        )?);
    }
    Ok(held)
}

/// The owner's release (#768): one autocommit statement deletes the live row
/// of `op_id` only while the Store is still at this session's generation,
/// outside the publication lane. A stale session is `StaleStore` with nothing
/// deleted. A retained `done` row is never deleted, and an absent row is no
/// error: the caller then unlocks the keys this session holds for the op.
pub(super) fn release_owned(session: &Arc<Session>, op_id: &[u8; 16]) -> HistoryResult<()> {
    let expected = expected(session)?;
    let generation = session.run_bare(|tx| -> Result<i64, BackendError> {
        let sql = bare(tx, RELEASE_OWNER)?;
        let found = tx.typed(&sql, &[&op_id.as_slice(), &expected], 24)?;
        // The statement may already have deleted the row: a reply that
        // cannot be read is an unknown outcome.
        let row = found.first().ok_or(BackendError::Unknown)?;
        row.try_get(0).map_err(|_| BackendError::Unknown)
    })?;
    if expected != 0 && generation != expected {
        return Err(HistoryError::StaleStore);
    }
    Ok(())
}

/// The refusal a failed session key reports: the intents at its scope, or the
/// backend the server names; never a holderless `MountsActive`/`Busy` when a
/// holder is available.
fn session_lock_refusal(
    session: &Arc<Session>,
    kind: u8,
    id: &[u8],
    key: i64,
    shared: bool,
) -> HistoryResult<HistoryError> {
    // A read transaction, not `Mode::Statement`: the intent queries are the
    // search-path ones the write path uses.
    let mut found = session.run(false, |statements| {
        scope_intents(&Transaction::new(statements), kind, id)
    })?;
    if shared {
        // A shared key is refused only by an exclusive holder (a compaction
        // or a removal); mounts never exclude mounts, so peer mount rows are
        // not the holder and are not reported as one (#776 G4).
        found.retain(|intent| intent.kind != GcIntentKind::Mount);
    }
    if !found.is_empty() {
        return Ok(classify(&found));
    }
    let scope = match kind {
        2 => GcScopeKind::Project,
        3 => GcScopeKind::Branch,
        _ => GcScopeKind::Store,
    };
    let mut conflict = OpConflict::new(
        if kind == 1 {
            GcIntentKind::Compaction
        } else {
            GcIntentKind::Removal
        },
        scope,
        if kind == 1 {
            "store".to_owned()
        } else {
            hex(id)
        },
        None,
        String::new(),
        false,
    );
    if let Some((pid, client_addr, backend_start)) = session.run(false, |statements| {
        advisory_holder(&Transaction::new(statements), key)
    })? {
        conflict = conflict.with_backend(Some(pid), client_addr, backend_start);
    }
    Ok(HistoryError::BusyOp(Box::new(conflict)))
}
/// Gives back one level of each of `keys` in one statement, which attempts
/// every key: one not held does not stop the ones after it. It runs whatever
/// the Store's generation: keys are only given back. A failed statement may
/// have given back some keys and not others: it is `Unknown`, which
/// quarantines the session and closes its connection, so the server gives
/// back every key it holds.
pub(super) fn session_unlock(session: &Arc<Session>, keys: &[SessionKey]) -> HistoryResult<()> {
    if keys.is_empty() {
        return Ok(());
    }
    let (ids, modes): (Vec<i64>, Vec<bool>) = keys.iter().copied().unzip();
    let pids = session.run_bare(|tx| -> Result<Vec<i64>, BackendError> {
        let sql = bare(tx, UNLOCK_SESSION)?;
        let bytes = 9 * ids.len() as u64;
        let found = (tx.typed(&sql, &[&ids, &modes], bytes)).map_err(|_| BackendError::Unknown)?;
        (found.iter())
            .map(|row| row.try_get(1).map_err(|_| BackendError::Integrity))
            .collect()
    })?;
    if pids.iter().any(|pid| !session.note_backend(*pid)) {
        return Err(HistoryError::Unsupported(POOLED));
    }
    Ok(())
}
