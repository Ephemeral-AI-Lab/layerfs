//! Owner release, fencing, and the ordered worker-start reconcile.
//!
//! The Store keeps no machine-authority row: a worker start records its
//! ordered incarnation in the home `MachineRecord`, and the reconcile here
//! compares the intents recorded at that machine with the new value. A
//! recorded incarnation older than the new one is a previous worker's
//! placement and is settled; one that is equal is current, and one that is
//! newer can only come from a restore, so it stays uncertain, never dead.
use super::*;

/// Refuses a presented stale fencing token. A caller that presents none is
/// the unfenced owner release the call has always been: production owners do
/// not carry a token yet, and only a caller that asserts one is checked.
pub(super) fn fence_check(intent: &GcIntent, fence_token: Option<&str>) -> HistoryResult<()> {
    match fence_token {
        Some(fence) if !intent.authority().fence_matches(Some(fence)) => {
            Err(HistoryError::Conflict("gc intent fence"))
        }
        _ => Ok(()),
    }
}

/// Deletes one intent if it is absent or still fenced to the caller. A caller
/// with no token is the plain owner release and skips the read.
fn release_checked_in(
    tx: &Transaction<'_>,
    op_id: &[u8; 16],
    fence_token: Option<&str>,
) -> HistoryResult<()> {
    if let Some(fence) = fence_token {
        let Some(intent) = get_in(tx, op_id)? else {
            return Ok(());
        };
        fence_check(&intent, Some(fence))?;
    }
    release_in(tx, op_id)
}

/// Deletes one intent, and on PostgreSQL releases the session keys it held.
pub(crate) fn release(provider: &HistoryProvider, op_id: &[u8; 16]) -> HistoryResult<()> {
    release_fenced(provider, op_id, None)
}

/// [`release`] for an owner that presents its fencing token: a stale token is
/// refused and leaves the row for the newer owner.
pub(crate) fn release_fenced(
    provider: &HistoryProvider,
    op_id: &[u8; 16],
    fence_token: Option<&str>,
) -> HistoryResult<()> {
    match &provider.engine {
        Engine::Sqlite(_) => provider.write(|tx| release_checked_in(tx, op_id, fence_token)),
        Engine::Postgres(session) => {
            let found = provider.write(|tx| {
                let Some(intent) = get_in(tx, op_id)? else {
                    return Ok(None);
                };
                fence_check(&intent, fence_token)?;
                release_in(tx, op_id)?;
                Ok(Some(keys_for_release(tx, provider.catalog_id, &intent)?))
            })?;
            if let Some(ReleaseKeys { keys }) = found {
                let _ = session_unlock(session, provider.catalog_id, &keys);
            }
            Ok(())
        }
    }
}

/// Retains one released intent as the durable `done` audit row. The owner
/// columns are cleared so the unique owner key cannot block a later mount;
/// admission queries filter `state <> 'done'`, and `gc_intent_done` is the
/// only read that sees the row. Returns `false` when the op has no intent, so
/// a concurrent repair does not invent one.
fn retain_done_in(tx: &Transaction<'_>, op_id: &[u8; 16], evidence: &str) -> HistoryResult<bool> {
    let rows = tx.query(RELEASE_DONE, params![evidence, now_ms(), op_id.as_slice()])?;
    Ok(!rows.is_empty())
}

/// [`release`] for the operator repair: it keeps the released row as the
/// durable audit instead of deleting it. PostgreSQL releases this session's
/// advisory keys for the row, as the deleting release always has.
pub(crate) fn release_done(
    provider: &HistoryProvider,
    op_id: &[u8; 16],
    evidence: &str,
) -> HistoryResult<bool> {
    match &provider.engine {
        Engine::Sqlite(_) => provider.write(|tx| retain_done_in(tx, op_id, evidence)),
        Engine::Postgres(session) => {
            let found = provider.write(|tx| {
                let Some(intent) = get_in(tx, op_id)? else {
                    return Ok(None);
                };
                let keys = keys_for_release(tx, provider.catalog_id, &intent)?;
                retain_done_in(tx, op_id, evidence)?;
                Ok(Some(keys))
            })?;
            let retained = found.is_some();
            if let Some(ReleaseKeys { keys }) = found {
                let _ = session_unlock(session, provider.catalog_id, &keys);
            }
            Ok(retained)
        }
    }
}

/// Settles the intents at `machine` its new ordered incarnation proves dead.
///
/// One write transaction: no other machine's rows are read, and only a dead
/// intent that is not uncertain is released. An expired lease is uncertain,
/// never dead, and a recorded incarnation newer than `incarnation` (a home
/// restore) is kept uncertain like an unordered value that differs.
pub(crate) fn reconcile_machine(
    provider: &HistoryProvider,
    machine: &str,
    incarnation: &str,
) -> HistoryResult<GcReconcile> {
    if machine.is_empty() || incarnation.is_empty() {
        return Err(HistoryError::InvalidInput("GC machine authority"));
    }
    let postgres = postgres(provider);
    provider.write(|tx| {
        let found = query(tx, MACHINE_INTENTS, params![machine])?;
        let owners = OwnerView::now();
        let mut place = PlaceView::for_machine(machine, incarnation);
        let mut reconcile = GcReconcile {
            machine: machine.to_owned(),
            incarnation: incarnation.to_owned(),
            ..GcReconcile::default()
        };
        for intent in found {
            match place.liveness(tx, &intent, postgres, &owners)? {
                Liveness::Dead if !intent.uncertain => {
                    release_in(tx, &intent.op_id)?;
                    reconcile.settled.push(intent.op_id);
                }
                liveness => {
                    reconcile.held += 1;
                    if intent.uncertain || liveness == Liveness::Uncertain {
                        reconcile.uncertain += 1;
                    }
                }
            }
        }
        Ok(reconcile)
    })
}
