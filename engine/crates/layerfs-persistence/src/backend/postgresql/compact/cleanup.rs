//! §8 Phase C: cleanup, the owner guards and §9 leftover settling.

use super::build::relation_bytes;
use super::op::clear_round;
use super::op::hex;
use super::op::history_rows;
use super::op::read_generation;
use super::op::read_op;
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

/// Phase C: delete only the persisted `gc_dead` members, settle the totals
/// once, and clear the op. The Store is usable at `G + 1` throughout.
pub(crate) fn cleanup(
    handles: &PostgresHandles,
    session: &Arc<Session>,
    schema: &str,
    dead: DeadTotals,
    totals_applied: bool,
    op_id: Option<[u8; 16]>,
) -> HistoryResult<()> {
    stage(session, |tx| {
        tx.batch(&format!(
            "DELETE FROM {schema}.\"commit\" c WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='commit' AND d.id=c.commit_id);\
             DELETE FROM {schema}.layer l WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='layer' AND d.id=l.layer_id);\
             DELETE FROM {schema}.branch b WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='branch' AND d.id=b.branch_id);\
             DELETE FROM {schema}.layer_stack s WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='stack' AND d.id=s.layer_stack_id);\
             DELETE FROM {schema}.layer_lineage ll WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer l WHERE l.layer_id=ll.layer_id);\
             DELETE FROM {schema}.commit_lineage cl WHERE NOT EXISTS (SELECT 1 FROM {schema}.\"commit\" c WHERE c.commit_id=cl.commit_id);\
             DELETE FROM {schema}.absorbed a WHERE NOT EXISTS (SELECT 1 FROM {schema}.commit_lineage cl WHERE cl.run=a.run);\
             DELETE FROM {schema}.reach r WHERE r.key NOT IN (SELECT layer_stack_id FROM {schema}.layer_stack UNION SELECT run FROM {schema}.commit_lineage);\
             DELETE FROM {schema}.branch_retired br WHERE NOT EXISTS (SELECT 1 FROM {schema}.branch b WHERE b.branch_id=br.branch_id);\
             DELETE FROM {schema}.layer_stack_retired lr WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer_stack s WHERE s.layer_stack_id=lr.layer_stack_id);\
             DELETE FROM {schema}.path_removed pr WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer_stack s WHERE s.layer_stack_id=pr.layer_stack_id);\
             DELETE FROM {schema}.project_epoch pe WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer_stack s WHERE s.layer_stack_id=pe.layer_stack_id);\
             DELETE FROM {schema}.content_signature cs WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='signature' AND d.id=int8send(cs.slot)) AND cs.stamp <= COALESCE((SELECT mark_stamp FROM {schema}.gc_op WHERE id=1),0);\
             DELETE FROM {schema}.file_digest fd WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='digest' AND d.id=fd.root);\
             DELETE FROM {schema}.binding_hint_block;\
             DELETE FROM {schema}.init_operation;\
             DELETE FROM {schema}.init_entry;\
             DELETE FROM {schema}.init_host_file;"
        ))?;
        // §11: `cleanup` is written even when the totals were already
        // applied by the run being resumed.
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
    })?;
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
