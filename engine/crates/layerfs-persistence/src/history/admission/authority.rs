//! Owner release and the ordered worker-start reconcile.
//!
//! The Store keeps no machine-authority row: a worker start records its
//! ordered incarnation in the home `MachineRecord`, and the reconcile here
//! compares the intents recorded at that machine with the new value. A
//! recorded incarnation older than the new one is a previous worker's
//! placement and is settled; one that is equal is current, and one that is
//! newer can only come from a restore, so it stays uncertain, never dead.
use super::*;

/// Deletes one intent, and on PostgreSQL releases the session keys it held.
///
/// On PostgreSQL the keys are the ones this session took when it registered
/// the op, kept with it, whatever the row's state or whether it still
/// exists: an operator's release (`release_done`) runs in the operator's own
/// session, so it retains the row as `done` but cannot unlock the keys the
/// owner's session took at registration, and a reconcile or another session
/// may have deleted the row. The owner's own release then still unlocks
/// them, and leaves a `done` row as it is (the delete never touches one).
/// Only an op this session registered and still holds is unlocked, once: a
/// repeated release, or one of a row another session registered, unlocks
/// nothing, so no key level a live mount of this session holds is dropped.
///
/// The delete is one fenced autocommit statement outside the publication
/// lane (#768), and runs strictly before the unlock: until the row is gone
/// the keys still exclude a removal or compaction. A failed delete unlocks
/// nothing and keeps the op held, so a later release still can.
pub(crate) fn release(provider: &HistoryProvider, op_id: &[u8; 16]) -> HistoryResult<()> {
    match &provider.engine {
        Engine::Sqlite(_) => provider.write(|tx| release_in(tx, op_id)),
        Engine::Postgres(session) => {
            release_owned(session, op_id)?;
            if let Some(keys) = session.take_op(op_id) {
                let _ = session_unlock(session, &keys);
            }
            Ok(())
        }
    }
}

/// The retained `done` rows one scope (a mount's Branch) keeps: retaining a
/// new one deletes the oldest beyond this many, in the same transaction, so
/// the audit stays bounded without a schema change.
pub const DONE_RETAINED: i64 = 100;

/// Retains one released intent as the durable `done` audit row. The owner
/// columns are cleared so the unique owner key cannot block a later mount;
/// admission queries filter `state <> 'done'`, and `gc_intent_done` is the
/// only read that sees the row. The scope's oldest done rows beyond
/// [`DONE_RETAINED`] go. Returns `false` when the op has no live intent (none,
/// or one already retained), so a concurrent or repeated repair invents none.
///
/// The row is stamped `max(now, the scope's newest done time + 1)`: a host
/// whose clock is behind another's still ranks the row it retains newest, so
/// the cap in the same transaction keeps it (the cap also never deletes the
/// op it was given) and the keyset audit returns it after every older row.
///
/// The stamps of one scope follow commit order because retaining releases
/// are serialized: SQLite's `BEGIN IMMEDIATE` writer, and on PostgreSQL the
/// Store lane row every lane write transaction locks at its `BEGIN` (READ
/// COMMITTED), so the newest-time read runs after any earlier release has
/// committed and no other release commits before this one does. The owner's
/// deleting release runs outside the lane on PostgreSQL (#768); it never
/// writes or deletes a `done` row, so it cannot reorder the stamps. The
/// floor is per scope, that is per Branch identity: a Branch removed and
/// created again under the same project and Branch names starts from its own
/// rows, not from the earlier identity's, which the name-scoped audit also
/// reads (see `gc_intent_done_branch.sql` for the guarantee that leaves).
/// Finding the earlier identity's rows would need a scan of every retained
/// row, as no index covers the names, so the floor does not.
fn retain_done_in(tx: &Transaction<'_>, op_id: &[u8; 16], evidence: &str) -> HistoryResult<bool> {
    let newest = one(tx, DONE_NEWEST, [op_id.as_slice()], |row| {
        cell::<Option<i64>>(row, 0)
    })?
    .flatten();
    let now = now_ms();
    let stamp = newest.map_or(now, |newest| now.max(newest.saturating_add(1)));
    let rows = tx.query(RELEASE_DONE, params![evidence, stamp, op_id.as_slice()])?;
    let Some(row) = rows.first() else {
        return Ok(false);
    };
    let scope_kind: String = cell(row, 0)?;
    let scope_id: Option<Vec<u8>> = cell(row, 1)?;
    tx.query(
        DONE_CAP,
        params![scope_kind, scope_id, op_id.as_slice(), DONE_RETAINED],
    )?;
    Ok(true)
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
            // a second release finds no live intent and answers `false`
            // (`NotFound` to the caller), as SQLite does; only a row this
            // release retained has its session keys released. It stays on
            // the lane: its done stamp relies on releases being serialized.
            let retained = provider.write(|tx| retain_done_in(tx, op_id, evidence))?;
            if retained {
                if let Some(keys) = session.take_op(op_id) {
                    let _ = session_unlock(session, &keys);
                }
            }
            Ok(retained)
        }
    }
}

/// What admission would say of each of `intents` now: the owner evidence,
/// then the place view scoped on this thread, as [`PlaceView::liveness`]
/// judges a conflict. The view is the one a removal or compaction on the
/// scope admits with: the supplied view alone, never a held intent's own
/// recorded place as the current one, so the verdict is the very one that
/// admission gives. One read transaction; nothing is written.
pub(crate) fn liveness(
    provider: &HistoryProvider,
    intents: &[GcIntent],
) -> HistoryResult<Vec<Liveness>> {
    let postgres = postgres(provider);
    provider.read(|tx| {
        let owners = OwnerView::now();
        let mut place = PlaceView::supplied();
        (intents.iter())
            .map(|intent| place.liveness(tx, intent, postgres, &owners))
            .collect()
    })
}

/// Settles the intents at `machine` its new ordered incarnation proves dead.
///
/// One write transaction: no other machine's rows are read, and only a dead
/// intent that is not uncertain is released. A recorded incarnation newer
/// than `incarnation` (a home restore) is kept uncertain like an unordered
/// value that differs.
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
