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
        let found = session.read_statement(|statements| {
            Transaction::new(statements).query(
                if *shared {
                    LOCK_SESSION_SHARED
                } else {
                    LOCK_SESSION_EXCLUSIVE
                },
                params![key],
            )
        })?;
        if found.is_empty() {
            let _ = session_unlock(session, catalog, &taken);
            return Err(session_lock_refusal(session, *kind, id, key, *shared)?);
        }
        // Spec §5.2 item 34: a session advisory lock only means presence if the
        // same backend keeps it. The lock statement returns the backend that
        // took it; a transaction-pooling proxy hands the next statement to a
        // different one, so mounts and compaction are `Unsupported` there.
        let pid: i64 = found[0].get(1)?;
        if !session.note_backend(pid) {
            let _ = session_unlock(session, catalog, &taken);
            return Err(HistoryError::Unsupported(POOLED));
        }
        taken.push((*kind, id.clone(), *shared));
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
    let found = session.run(false, |statements| {
        scope_intents(&Transaction::new(statements), kind, id)
    })?;
    if !found.is_empty() {
        return Ok(classify(&found));
    }
    if shared {
        return Ok(HistoryError::MountsActive(Box::new(MountHolders::new(
            Vec::new(),
        ))));
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
pub(super) fn session_unlock(
    session: &std::sync::Arc<crate::backend::postgresql::session::connection::Session>,
    catalog: CatalogId,
    keys: &[(u8, Vec<u8>, bool)],
) -> HistoryResult<()> {
    for (kind, id, shared) in keys {
        let key = advisory_key(catalog, *kind, id);
        let found = session.read_statement(|statements| {
            Transaction::new(statements).query(
                if *shared {
                    UNLOCK_SESSION_SHARED
                } else {
                    UNLOCK_SESSION_EXCLUSIVE
                },
                params![key],
            )
        })?;
        if let Some(row) = found.first() {
            let pid: i64 = row.get(1)?;
            if !session.note_backend(pid) {
                return Err(HistoryError::Unsupported(POOLED));
            }
        }
    }
    Ok(())
}
