//! §8 Phase C: cleanup, the owner guards and §9 leftover settling.

use super::build::relation_bytes;
use super::op::clear_round;
use super::op::hex;
use super::op::history_rows;
use super::op::read_generation;
use super::op::read_op;
use super::op::scalar;
use super::op::scalar_session;
use super::op::stage;
use super::op::RunMetrics;
use super::report::report;
use super::types::DeadTotals;
use super::types::OpRow;
use crate::backend::postgresql::session::connection::Session;
use crate::backend::records::Param;
use crate::backend::sqlite::compact::CompactPath;
use crate::backend::sqlite::compact::CompactReceipt;
use crate::backend::sqlite::compact::CompactResidue;
use crate::compact_metrics::CompactPhase;
use crate::PostgresHandles;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use layerfs_history::error::OpConflict;
use layerfs_history::records::GcIntentKind;
use layerfs_history::records::GcScopeKind;
use layerfs_history::HistoryCatalog;
use std::sync::Arc;

/// The recorded owner backend's `pg_stat_activity` state: `Some` while a
/// backend with the recorded `(pid, backend_start)` is alive, `None` when it
/// has provably ended.
pub(crate) fn current_backend_pid(session: &Arc<Session>) -> HistoryResult<i64> {
    scalar_session(session, "SELECT pg_backend_pid()", Vec::new())
}

pub(crate) fn owner_state(session: &Arc<Session>, op: &OpRow) -> HistoryResult<Option<String>> {
    let (Some(pid), Some(start)) = (op.pid, op.backend_start.as_deref()) else {
        return Ok(None);
    };
    stage(session, |tx| {
        let rows = tx.query(
            "SELECT state::text FROM pg_stat_activity \
              WHERE pid=$1 AND backend_start::text=$2",
            vec![Param::I64(pid), Param::Text(start.to_owned())],
        )?;
        rows.first()
            .map(|row| row.get(0))
            .transpose()
            .map_err(Into::into)
    })
}

/// The recorded owner's client_addr, when the server still reports it.
pub(crate) fn owner_client_addr(
    session: &Arc<Session>,
    op: &OpRow,
) -> HistoryResult<Option<String>> {
    let (Some(pid), Some(start)) = (op.pid, op.backend_start.as_deref()) else {
        return Ok(None);
    };
    stage(session, |tx| {
        let rows = tx.query(
            "SELECT client_addr::text FROM pg_stat_activity \
              WHERE pid=$1 AND backend_start::text=$2",
            vec![Param::I64(pid), Param::Text(start.to_owned())],
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        Ok(row.get::<Option<String>>(0)?)
    })
}

/// §9/§12: bounded poll for a recorded backend that is already ending. A live
/// backend is never terminated. The caller holds the exclusive Store key, so a
/// recorded owner that were still inside this op would hold it and the claim
/// would have failed; an idle backend is a healthy pooled connection, not a
/// dead one, and must be left alone.
pub(crate) fn poll_owner_gone(
    session: &Arc<Session>,
    op: &OpRow,
    polls: u32,
) -> HistoryResult<bool> {
    for _ in 0..polls {
        if owner_state(session, op)?.is_none() {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Ok(false)
}

pub(crate) fn drop_scratch(session: &Arc<Session>, scratch: &str) -> HistoryResult<()> {
    stage(session, |tx| {
        tx.batch(&format!("DROP SCHEMA IF EXISTS {scratch} CASCADE"))?;
        Ok(())
    })
}

pub(crate) fn compaction_intent(
    session: &Arc<Session>,
    schema: &str,
) -> HistoryResult<Option<[u8; 16]>> {
    stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT op_id FROM {schema}.gc_intent WHERE kind='compaction' ORDER BY updated_ms LIMIT 1"
            ),
            Vec::new(),
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let bytes: Vec<u8> = row.get(0)?;
        let op: [u8; 16] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| HistoryError::Integrity("compaction intent op"))?;
        Ok(Some(op))
    })
}

/// §9: judge a recorded leftover. `G` removes the artifact and clears the
/// op; `G + 1` finishes Phase C and answers the installed outcome. `None`
/// means there was nothing to resume.
pub(crate) fn settle_leftover(
    handles: &PostgresHandles,
    session: &Arc<Session>,
    schema: &str,
    metrics: &mut RunMetrics,
) -> HistoryResult<Option<CompactReceipt>> {
    let Some(op) = read_op(session, schema)? else {
        return Ok(None);
    };
    let current = read_generation(session, schema)?;
    // The op judged the Store at `current`; after a resume this session must
    // fence its later writes at that generation, not the one it opened at.
    session.incarnation.store(
        u64::try_from(current).unwrap_or(0),
        std::sync::atomic::Ordering::SeqCst,
    );
    if current == op.generation {
        // The caller's own session is the recorded owner when it retries its
        // own uncertain round: adopt it without terminating itself.
        if op.pid != Some(current_backend_pid(session)?) {
            // The caller holds the exclusive Store key: a recorded owner that
            // is still inside this op would hold it, so the claim above proves
            // the op is no longer running. A recorded backend the server still
            // reports is a healthy pooled connection, not a dead owner:
            // §9/§12 do not terminate it. Poll briefly so a backend that is
            // already ending is allowed to finish, then settle under the key.
            if let Some(state) = owner_state(session, &op)? {
                let conflict = || -> HistoryResult<HistoryError> {
                    let op_id = compaction_intent(session, schema)?.unwrap_or([0u8; 16]);
                    let client_addr = owner_client_addr(session, &op)?;
                    Ok(HistoryError::BusyOp(Box::new(
                        OpConflict::new(
                            GcIntentKind::Compaction,
                            GcScopeKind::Store,
                            "store",
                            Some(op.phase.clone()),
                            hex(&op_id),
                            false,
                        )
                        .with_backend(
                            op.pid,
                            client_addr,
                            op.backend_start.clone(),
                        ),
                    )))
                };
                if state != "idle" {
                    // A live owner stays Busy, naming the op and the backend.
                    return Err(conflict()?);
                }
                // The backend is alive but not executing the op: it is a
                // healthy pooled connection, so poll it until it ends and
                // settle under the exclusive Store key.
                let _ = poll_owner_gone(session, &op, 40)?;
            }
        }
        metrics.begin_phase(CompactPhase::PostgresCleanup);
        if let Some(artifact) = &op.artifact {
            drop_scratch(session, artifact)?;
        }
        clear_round(session, schema)?;
        stage(session, |tx| {
            tx.batch(&format!(
                "DELETE FROM {schema}.gc_intent WHERE kind='compaction'"
            ))?;
            Ok(())
        })?;
        metrics.end_phase();
        metrics.leftover_settled = true;
        return Ok(None);
    }
    if current != op.generation + 1 {
        return Err(HistoryError::Integrity("compaction op generation"));
    }
    metrics.begin_phase(CompactPhase::PostgresCleanup);
    let op_id = compaction_intent(session, schema)?;
    // §11: Phase C deletes history, signature and digest rows but no `pack` or
    // `object_location` row, so the relation size read before the resumed
    // cleanup is the swap's `physical_after` the original reply would have
    // measured. The op carries the rest of the original report.
    let physical_after = relation_bytes(session, schema, &["pack", "object_location"])?;
    let rows = history_rows(session, schema)?;
    cleanup(handles, session, schema, op.dead, op.totals_applied, op_id)?;
    metrics.end_phase();
    metrics.leftover_settled = true;
    let dead_bytes = u64::try_from(op.dead.stored).unwrap_or(0);
    Ok(Some(report(
        CompactPath::Rebuild,
        u64::try_from(op.generation).unwrap_or(0),
        u64::try_from(current).unwrap_or(0),
        dead_bytes,
        dead_bytes,
        u64::try_from(op.dead.canonical).unwrap_or(0),
        0,
        op.physical_before.saturating_sub(physical_after),
        op.kept_waste.clone(),
        op.peak_temp,
        CompactResidue::default(),
        u64::try_from(op.total_stored).unwrap_or(0),
        u64::try_from(rows).unwrap_or(0),
    )))
}

/// The mark-time watermarks recorded in `gc_op`, used only as guards: a row
/// created after the mark is never a `gc_dead` member, and these keep a
/// mis-keyed member from naming a newer row.
fn mark_watermarks(session: &Arc<Session>, schema: &str) -> HistoryResult<(i64, i64, i64)> {
    stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT COALESCE(mark_serial,0),COALESCE(mark_stamp,0),COALESCE(mark_visited,0) FROM {schema}.gc_op WHERE id=1"
            ),
            Vec::new(),
        )?;
        let row = rows
            .first()
            .ok_or(HistoryError::Integrity("compaction watermarks"))?;
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })
}

/// §8 step 11: the swap copied exactly the visited set, so the retained
/// `object_location` must have `count(*) = count(DISTINCT object_id) =
/// |visited|` before Phase C touches anything.
fn assert_retained_object_set(
    session: &Arc<Session>,
    schema: &str,
    visited: i64,
) -> HistoryResult<()> {
    stage(session, |tx| {
        let objects = scalar(
            tx,
            &format!("SELECT count(*) FROM {schema}.object_location"),
            Vec::new(),
        )?;
        let distinct = scalar(
            tx,
            &format!("SELECT count(DISTINCT object_id) FROM {schema}.object_location"),
            Vec::new(),
        )?;
        // Phase A already asserted the exact `count = count(DISTINCT) =
        // |visited|` on the scratch tables. The Store is usable at `G + 1`
        // from the swap on, so a save that lands between the swap and this
        // cleanup (or a resumed one) can only add object rows: the live set
        // must never shrink below the mark's copy and must stay duplicate-free.
        if objects != distinct || objects < visited {
            return Err(HistoryError::Integrity("compaction retained object set"));
        }
        Ok(())
    })
}

/// Every layer stack that has a dead row, ascending. One stack's dead rows
/// form a closed reference unit (no retained row references them), so each
/// stack can be deleted in its own transaction.
fn dead_layer_stacks(session: &Arc<Session>, schema: &str) -> HistoryResult<Vec<Vec<u8>>> {
    stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT s FROM ( \
                   SELECT c.layer_stack_id AS s FROM {schema}.\"commit\" c \
                    JOIN {schema}.gc_dead d ON d.kind='commit' AND d.id=c.commit_id \
                   UNION SELECT l.layer_stack_id FROM {schema}.layer l \
                    JOIN {schema}.gc_dead d ON d.kind='layer' AND d.id=l.layer_id \
                   UNION SELECT b.layer_stack_id FROM {schema}.branch b \
                    JOIN {schema}.gc_dead d ON d.kind='branch' AND d.id=b.branch_id \
                   UNION SELECT g.id FROM {schema}.gc_dead g WHERE g.kind='stack' \
                 ) u ORDER BY s"
            ),
            Vec::new(),
        )?;
        rows.iter()
            .map(|row| row.get::<Vec<u8>>(0).map_err(Into::into))
            .collect()
    })
}

/// Deletes one stack's dead rows in one transaction. The FKs are deferred, so
/// the commit sees a state where every dead reference inside the stack is gone
/// together; every other stack is independent.
fn delete_stack_batch(
    session: &Arc<Session>,
    schema: &str,
    stack: &[u8],
    mark_serial: i64,
) -> HistoryResult<()> {
    stage(session, |tx| {
        let stack = Param::Bytes(stack.to_vec());
        // The serial is a guard only: membership in `gc_dead` is the real
        // filter, and a row created after the mark is never a member.
        tx.query(
            &format!(
                "DELETE FROM {schema}.\"commit\" t USING {schema}.gc_dead d \
                 WHERE d.kind='commit' AND d.id=t.commit_id AND t.layer_stack_id=$1 \
                   AND (t.history_serial < $2 OR t.history_serial = 0)"
            ),
            vec![stack.clone(), Param::I64(mark_serial)],
        )?;
        tx.query(
            &format!(
                "DELETE FROM {schema}.layer t USING {schema}.gc_dead d \
                 WHERE d.kind='layer' AND d.id=t.layer_id AND t.layer_stack_id=$1 \
                   AND (t.history_serial < $2 OR t.history_serial = 0)"
            ),
            vec![stack.clone(), Param::I64(mark_serial)],
        )?;
        tx.query(
            &format!(
                "DELETE FROM {schema}.branch t USING {schema}.gc_dead d \
                 WHERE d.kind='branch' AND d.id=t.branch_id AND t.layer_stack_id=$1"
            ),
            vec![stack.clone()],
        )?;
        tx.query(
            &format!(
                "DELETE FROM {schema}.layer_stack s USING {schema}.gc_dead d \
                 WHERE d.kind='stack' AND d.id=s.layer_stack_id AND s.layer_stack_id=$1"
            ),
            vec![stack],
        )?;
        Ok(())
    })
}

/// Deletes the `content_signature` rows whose exact `(slot, stamp)` is in the
/// persisted dead set, in bounded batches. The pair makes a slot refilled
/// after the mark (a new stamp) survive; `stamp <= mark_stamp` is the guard.
/// The `gc_dead` rows stay until [`settle`], so a crash here resumes the same
/// persisted set instead of re-deriving it.
fn delete_dead_signatures(
    session: &Arc<Session>,
    schema: &str,
    mark_stamp: i64,
) -> HistoryResult<()> {
    loop {
        let deleted = stage(session, |tx| -> HistoryResult<usize> {
            let rows = tx.query(
                &format!(
                    "WITH batch AS (SELECT d.id FROM {schema}.gc_dead d \
                                    JOIN {schema}.content_signature cs \
                                      ON cs.slot = ('x'||encode(substr(d.id,1,8),'hex'))::bit(64)::bigint \
                                     AND cs.stamp = ('x'||encode(substr(d.id,9,8),'hex'))::bit(64)::bigint \
                                    WHERE d.kind='signature' AND cs.stamp <= $1 \
                                    ORDER BY d.id LIMIT 1000) \
                     DELETE FROM {schema}.content_signature cs USING batch b \
                      WHERE cs.slot = ('x'||encode(substr(b.id,1,8),'hex'))::bit(64)::bigint \
                        AND cs.stamp = ('x'||encode(substr(b.id,9,8),'hex'))::bit(64)::bigint \
                        AND cs.stamp <= $1 RETURNING cs.slot"
                ),
                vec![Param::I64(mark_stamp)],
            )?;
            Ok(rows.len())
        })?;
        if deleted == 0 {
            return Ok(());
        }
    }
}

/// Deletes the `file_digest` rows whose root object is in the persisted dead
/// set, in bounded batches. The dead set holds roots, not digests, so a row
/// with a dead root is removed even when a different digest names it. The
/// `gc_dead` rows stay until [`settle`], so a crash here resumes them.
fn delete_dead_digests(session: &Arc<Session>, schema: &str) -> HistoryResult<()> {
    loop {
        let deleted = stage(session, |tx| -> HistoryResult<usize> {
            let rows = tx.query(
                &format!(
                    "WITH batch AS (SELECT d.id FROM {schema}.gc_dead d \
                                    JOIN {schema}.file_digest fd ON fd.root=d.id \
                                    WHERE d.kind='digest' ORDER BY d.id LIMIT 1000) \
                     DELETE FROM {schema}.file_digest fd USING batch b \
                      WHERE fd.root=b.id RETURNING fd.digest"
                ),
                Vec::new(),
            )?;
            Ok(rows.len())
        })?;
        if deleted == 0 {
            return Ok(());
        }
    }
}

/// The final Phase C transaction: remove the derived lineage rows, clear the
/// advisory hints, settle the four totals once, assert the Store sums and
/// clear the op. It runs after every dead row batch, so it holds no history
/// delete.
fn settle(
    session: &Arc<Session>,
    schema: &str,
    dead: DeadTotals,
    totals_applied: bool,
) -> HistoryResult<()> {
    // §6.1: the mark's `kept_node` closure is working state, not
    // retained history. The settle clears it with the dead set so the installed
    // Store carries no mark state afterwards, as SQLite's settle does.
    stage(session, |tx| {
        tx.batch(&format!(
            "DELETE FROM {schema}.layer_lineage ll WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer l WHERE l.layer_id=ll.layer_id);\
             DELETE FROM {schema}.commit_lineage cl WHERE NOT EXISTS (SELECT 1 FROM {schema}.\"commit\" c WHERE c.commit_id=cl.commit_id);\
             DELETE FROM {schema}.absorbed a WHERE NOT EXISTS (SELECT 1 FROM {schema}.commit_lineage cl WHERE cl.run=a.run);\
             DELETE FROM {schema}.reach r WHERE r.key NOT IN (SELECT layer_stack_id FROM {schema}.layer_stack UNION SELECT run FROM {schema}.commit_lineage);\
             DELETE FROM {schema}.branch_retired br WHERE NOT EXISTS (SELECT 1 FROM {schema}.branch b WHERE b.branch_id=br.branch_id);\
             DELETE FROM {schema}.layer_stack_retired lr WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer_stack s WHERE s.layer_stack_id=lr.layer_stack_id);\
             DELETE FROM {schema}.path_removed pr WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer_stack s WHERE s.layer_stack_id=pr.layer_stack_id);\
             DELETE FROM {schema}.project_epoch pe WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer_stack s WHERE s.layer_stack_id=pe.layer_stack_id);\
             DELETE FROM {schema}.binding_hint_block;\
             DELETE FROM {schema}.kept_node;"
        ))?;
        tx.batch(&format!(
            "UPDATE {schema}.gc_op SET phase='cleanup' WHERE id=1"
        ))?;
        if !totals_applied {
            tx.query(
                &format!(
                    "UPDATE {schema}.store_totals SET \
                     canonical=canonical-$1,stored=stored-$2,metadata=metadata-$3,logical=logical-$4 WHERE id=1"
                ),
                vec![
                    Param::I64(dead.canonical),
                    Param::I64(dead.stored),
                    Param::I64(dead.metadata),
                    Param::I64(dead.logical),
                ],
            )?;
            tx.query(
                &format!("UPDATE {schema}.gc_op SET totals_applied=TRUE WHERE id=1"),
                Vec::new(),
            )?;
        }
        let totals = tx.query(
            &format!(
                "SELECT logical,canonical,stored,metadata FROM {schema}.store_totals WHERE id=1"
            ),
            Vec::new(),
        )?;
        let logical: i64 = totals[0].get(0)?;
        let canonical: i64 = totals[0].get(1)?;
        let stored: i64 = totals[0].get(2)?;
        let metadata: i64 = totals[0].get(3)?;
        let sums = tx.query(
            &format!(
                "SELECT (SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.\"commit\")+(SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.layer),\
                        (SELECT COALESCE(SUM(canonical_length)::bigint,0) FROM {schema}.object_location),\
                        (SELECT COALESCE(SUM(length)::bigint,0) FROM {schema}.pack),\
                        (SELECT COALESCE(SUM(length)::bigint,0) FROM {schema}.pack WHERE domain=0)"
            ),
            Vec::new(),
        )?;
        let sum_logical: i64 = sums[0].get(0)?;
        let sum_canonical: i64 = sums[0].get(1)?;
        let sum_stored: i64 = sums[0].get(2)?;
        let sum_metadata: i64 = sums[0].get(3)?;
        if logical != sum_logical
            || canonical != sum_canonical
            || stored != sum_stored
            || metadata != sum_metadata
        {
            return Err(HistoryError::Integrity("compaction settled totals"));
        }
        tx.batch(&format!(
            "DELETE FROM {schema}.gc_dead; DELETE FROM {schema}.gc_op;"
        ))?;
        Ok(())
    })
}

/// Phase C: delete only the persisted `gc_dead` members, in bounded batches
/// (one layer stack's closed closure per transaction, then the signature and
/// digest sets), settle the totals once, and clear the op. The Store is usable
/// at `G + 1` throughout, and the retained object set is asserted exactly.
///
/// §8: Phase C never touches Init's three working tables. `unfinished_work`
/// refuses a Store that already holds one, but an Init can begin after that
/// check and is live: emptying the tables wholesale here would roll this
/// transaction back on the FK (`23503`) after the swap committed, or silently
/// delete an empty `init_operation` whose next write then fails. Acquisition
/// owns that cleanup: its release empties the tables under the lane,
/// `ACCESS EXCLUSIVE`, the emptiness check and a savepoint.
pub(crate) fn cleanup(
    handles: &PostgresHandles,
    session: &Arc<Session>,
    schema: &str,
    dead: DeadTotals,
    totals_applied: bool,
    op_id: Option<[u8; 16]>,
) -> HistoryResult<()> {
    let (mark_serial, mark_stamp, visited) = mark_watermarks(session, schema)?;
    assert_retained_object_set(session, schema, visited)?;
    for stack in dead_layer_stacks(session, schema)? {
        delete_stack_batch(session, schema, &stack, mark_serial)?;
    }
    delete_dead_signatures(session, schema, mark_stamp)?;
    delete_dead_digests(session, schema)?;
    settle(session, schema, dead, totals_applied)?;
    if let Some(op) = op_id {
        let _ = handles.history.gc_release(&op);
    } else {
        stage(session, |tx| {
            tx.batch(&format!(
                "DELETE FROM {schema}.gc_intent WHERE kind='compaction'"
            ))?;
            Ok(())
        })?;
    }
    Ok(())
}
