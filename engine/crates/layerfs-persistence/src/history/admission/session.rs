//! PostgreSQL session-scoped advisory keys: the shared/exclusive keys a
//! mount or removal keeps across its statements, and the refusal a
//! transaction-pooled connection gets.

use super::*;

const LOCK_SESSION_SHARED: &str = "gc_lock_session_shared";
const LOCK_SESSION_EXCLUSIVE: &str = "gc_lock_session_exclusive";
const UNLOCK_SESSION_SHARED: &str = "gc_unlock_session_shared";
const UNLOCK_SESSION_EXCLUSIVE: &str = "gc_unlock_session_exclusive";

/// The refusal a transaction-pooled connection gets for a session lock.
const POOLED: &str = "transaction-pooled PostgreSQL connection";
pub(super) fn session_lock(
    session: &std::sync::Arc<crate::backend::postgresql::session::connection::Session>,
    catalog: CatalogId,
    keys: &[(u8, Vec<u8>, bool)],
) -> HistoryResult<()> {
    let mut taken: Vec<(u8, Vec<u8>, bool)> = Vec::new();
    for (kind, id, shared) in keys {
        let key = advisory_key(catalog, *kind, id);
        // Unfenced (#776 P2): a fenced lock would be taken and then answered
        // `Stale`. A failed statement may still have taken the key, so every
        // error exit unlocks it with the keys before it.
        let found: HistoryResult<_> = session.session_statement(|statements| {
            Transaction::new(statements).query(
                if *shared {
                    LOCK_SESSION_SHARED
                } else {
                    LOCK_SESSION_EXCLUSIVE
                },
                params![key],
            )
        });
        let found = match found {
            Ok(found) => found,
            Err(error) => {
                taken.push((*kind, id.clone(), *shared));
                let _ = session_unlock(session, catalog, &taken);
                return Err(error);
            }
        };
        if found.is_empty() {
            let _ = session_unlock(session, catalog, &taken);
            return Err(session_lock_refusal(session, *kind, id, key, *shared)?);
        }
        // This key is held from here: every exit below unlocks it too.
        taken.push((*kind, id.clone(), *shared));
        // Spec §5.2 item 34: a session advisory lock only means presence if the
        // same backend keeps it. The lock statement returns the backend that
        // took it; a transaction-pooling proxy hands the next statement to a
        // different one, so mounts and compaction are `Unsupported` there and
        // every caller refuses (#776 C1). The unlock is best effort: behind
        // such a proxy it may reach another backend, which then keeps the key
        // until the proxy recycles it.
        let pid: i64 = match found[0].get(1) {
            Ok(pid) => pid,
            Err(error) => {
                let _ = session_unlock(session, catalog, &taken);
                return Err(error.into());
            }
        };
        if !session.note_backend(pid) {
            let _ = session_unlock(session, catalog, &taken);
            return Err(HistoryError::Unsupported(POOLED));
        }
    }
    Ok(())
}

/// The refusal a failed session key reports: the intents at its scope, or the
/// backend the server names; never a holderless `MountsActive`/`Busy` when a
/// holder is available.
fn session_lock_refusal(
    session: &std::sync::Arc<crate::backend::postgresql::session::connection::Session>,
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
/// Unlocks every key it is given: a failed key does not stop the ones after
/// it, and the first failure is returned once all were attempted.
pub(super) fn session_unlock(
    session: &std::sync::Arc<crate::backend::postgresql::session::connection::Session>,
    catalog: CatalogId,
    keys: &[(u8, Vec<u8>, bool)],
) -> HistoryResult<()> {
    let mut first: Option<HistoryError> = None;
    for (kind, id, shared) in keys {
        let key = advisory_key(catalog, *kind, id);
        let found: HistoryResult<_> = session.session_statement(|statements| {
            Transaction::new(statements).query(
                if *shared {
                    UNLOCK_SESSION_SHARED
                } else {
                    UNLOCK_SESSION_EXCLUSIVE
                },
                params![key],
            )
        });
        let outcome = found.and_then(|found| match found.first() {
            Some(row) => {
                let pid: i64 = row.get(1)?;
                match session.note_backend(pid) {
                    true => Ok(()),
                    false => Err(HistoryError::Unsupported(POOLED)),
                }
            }
            None => Ok(()),
        });
        if let Err(error) = outcome {
            first.get_or_insert(error);
        }
    }
    first.map_or(Ok(()), Err)
}
