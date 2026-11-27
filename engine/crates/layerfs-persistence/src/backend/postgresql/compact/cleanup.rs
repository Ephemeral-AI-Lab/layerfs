//! §8 Phase C: cleanup, the owner guards and §9 leftover settling.

use super::op::clear_round;
use super::op::hex;
use super::op::history_rows;
use super::op::read_generation;
use super::op::read_op;
use super::op::scalar;
use super::op::scalar_session;
use super::op::set_phase;
use super::op::stage;
use super::receipt::report;
use super::types::DeadTotals;
use super::types::OpRow;
use crate::backend::postgresql::session::connection::Session;
use crate::backend::seam::Param;
use crate::gc::compact::CompactPath;
use crate::gc::compact::CompactReceipt;
use crate::gc::compact::CompactResidue;
use crate::gc::metrics::CompactPhase;
use crate::gc::metrics::RunMetrics;
use crate::HistoryProvider;
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

/// §9/§12: bounded poll (25 ms steps) until the recorded backend is gone.
/// `settle_leftover` uses it for an idle owner the exclusive key proved is no
/// longer inside the op; the §8 lost-`COMMIT` recovery uses it after it has
/// terminated exactly the recorded backend.
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

/// The message a lost-`COMMIT` recovery reports when the role may not signal
/// the recorded backend.
const TERMINATE_PRIVILEGE: &str =
    "pg_signal_backend (or membership) to terminate the recorded lost-COMMIT owner";

/// The most 25 ms polls the lost-`COMMIT` recovery makes for a terminated
/// backend (the termination itself waits up to 30 s in the server).
const TERMINATE_POLLS: u32 = 1200;

/// §8 lost `COMMIT` reply: resolve a recorded owner **before** the exclusive
/// Store key is taken.
///
/// A recorded owner that still holds the Store's exclusive key is a live
/// compaction: the claim that follows reports `Busy` with that backend named,
/// and it is never terminated (§12, the runbook rule). A recorded owner that
/// no longer holds the key is a lost reply (a run that failed definitely
/// drops its owner identity before it releases the key, so it names none):
/// its `COMMIT` may or may not have
/// reached the server, so the call opens a second connection, proves the role
/// may signal it, terminates exactly `(pid, backend_start, usename,
/// datname)`, polls until it is gone, and returns so the caller reads the
/// incarnation and settles the leftover. The caller's own session is adopted
/// by [`settle_leftover`], never terminated.
pub(crate) fn resolve_lost_commit(
    history: &HistoryProvider,
    session: &Arc<Session>,
    schema: &str,
) -> HistoryResult<()> {
    let Some(op) = read_op(session, schema)? else {
        return Ok(());
    };
    let (Some(pid), Some(start), Some(usename)) =
        (op.pid, op.backend_start.as_deref(), op.usename.as_deref())
    else {
        return Ok(());
    };
    if pid == current_backend_pid(session)? {
        return Ok(());
    }
    if owner_state(session, &op)?.is_none() {
        return Ok(());
    }
    if history.store_key_held_by(pid)? {
        // A live compaction: the claim names it as `Busy`; never terminate it.
        return Ok(());
    }
    terminate_recorded_owner(session, pid, start, usename)?;
    if !poll_owner_gone(session, &op, TERMINATE_POLLS)? {
        return Err(HistoryError::Busy);
    }
    Ok(())
}

/// One short second connection for the lost-`COMMIT` recovery: prove the role
/// may signal the recorded backend (a superuser, a member of
/// `pg_signal_backend`, or the owner's own role), terminate exactly the
/// recorded tuple, and poll `pg_stat_activity` until it is gone. The
/// connection is closed before the caller takes the exclusive key.
fn terminate_recorded_owner(
    session: &Arc<Session>,
    pid: i64,
    backend_start: &str,
    usename: &str,
) -> HistoryResult<()> {
    let mut recovery = session.connector.connect().map_err(HistoryError::from)?;
    let allowed = recovery
        .query_one(
            "SELECT (r.rolsuper OR pg_has_role(current_user,'pg_signal_backend','MEMBER') \
                    OR current_user=$1)::int FROM pg_roles r WHERE r.rolname=current_user",
            &[&usename],
        )
        .map_err(recovery_error)?
        .get::<_, i32>(0);
    if allowed == 0 {
        return Err(HistoryError::Unsupported(TERMINATE_PRIVILEGE));
    }
    let terminated: bool = recovery
        .query_one(
            "SELECT COALESCE(bool_or(pg_terminate_backend(a.pid, 30000)), false) \
               FROM pg_stat_activity a \
              WHERE a.pid::bigint=$1 AND a.backend_start::text=$2 \
                AND a.usename::text=$3 AND a.datname=current_database() \
                AND a.pid<>pg_backend_pid()",
            &[&pid, &backend_start, &usename],
        )
        .map_err(recovery_error)?
        .get(0);
    // The boolean only says the server accepted the signal; the poll is the
    // proof, and it also covers a backend that ended before the statement.
    let _ = terminated;
    for _ in 0..TERMINATE_POLLS {
        let alive = recovery
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
                  WHERE pid::bigint=$1 AND backend_start::text=$2 \
                    AND usename::text=$3 AND datname=current_database())",
                &[&pid, &backend_start, &usename],
            )
            .map_err(recovery_error)?
            .get::<_, bool>(0);
        if !alive {
            let _ = recovery.close();
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Err(HistoryError::Busy)
}

/// The typed refusal for a lost-`COMMIT` recovery statement: a missing signal
/// privilege is `Unsupported` (the §8 precheck shape), never a raw `42501`.
fn recovery_error(error: postgres::Error) -> HistoryError {
    if error.code().is_some_and(|code| code.code() == "42501") {
        return HistoryError::Unsupported(TERMINATE_PRIVILEGE);
    }
    HistoryError::from(crate::backend::postgresql::session::rows::error(&error))
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
                "SELECT op_id FROM {schema}.gc_intent WHERE kind='compaction' AND state <> 'done' ORDER BY updated_ms LIMIT 1"
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
                "DELETE FROM {schema}.gc_intent WHERE kind='compaction' AND state <> 'done'"
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
    // §11: the swap persisted its own `physical_after`, so a save that lands
    // after the swap (AC-28) does not inflate the resumed `G + 1` report's
    // `physicalShrink`. The op carries the rest of the original report too.
    let rows = history_rows(session, schema)?;
    cleanup(
        handles,
        session,
        schema,
        op.dead,
        op.totals_applied,
        op_id,
        op.artifact.as_deref(),
    )?;
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
        op.physical_before.saturating_sub(op.physical_after),
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
        // One statement, so both counts read one snapshot: in this READ
        // COMMITTED transaction two statements could see a save land between
        // them and report a corruption that is not there (#776 P4).
        let rows = tx.query(
            &format!(
                "SELECT count(*)::bigint, count(DISTINCT object_id)::bigint \
                   FROM {schema}.object_location"
            ),
            Vec::new(),
        )?;
        let row = rows
            .first()
            .ok_or(HistoryError::Integrity("compaction retained object set"))?;
        let objects: i64 = row.get(0)?;
        let distinct: i64 = row.get(1)?;
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

/// Deletes one stack's dead rows in one transaction: the persisted history
/// rows, the derived lineage closure and the stack row itself. The FKs are
/// deferred, so the commit sees a state where every dead reference inside the
/// stack is gone together; every other stack is independent.
///
/// The lineage closure (`layer_lineage`, `commit_lineage`, `absorbed`,
/// `reach`) is a member of `gc_dead` like the history rows, so it is deleted
/// here with the project instead of by a Store-wide orphan predicate in the
/// final settle. The order matters: the run-scoped `absorbed` and `reach` ids
/// are resolved through the `commit_lineage`/`commit` rows, which still exist
/// when they are deleted.
fn delete_stack_batch(
    session: &Arc<Session>,
    schema: &str,
    stack: &[u8],
    mark_serial: i64,
) -> HistoryResult<()> {
    stage(session, |tx| {
        let stack = Param::Bytes(stack.to_vec());
        tx.query(
            &format!(
                "DELETE FROM {schema}.layer_lineage t USING {schema}.gc_dead d \
                 WHERE d.kind='layer_lineage' AND d.id=t.layer_id \
                   AND t.layer_stack_id=$1"
            ),
            vec![stack.clone()],
        )?;
        tx.query(
            &format!(
                "DELETE FROM {schema}.absorbed t USING {schema}.gc_dead d \
                 WHERE d.kind='absorbed' AND d.id=t.run||int8send(t.depth) \
                   AND t.run IN (SELECT cl.run FROM {schema}.commit_lineage cl \
                                  JOIN {schema}.\"commit\" c ON c.commit_id=cl.commit_id \
                                 WHERE c.layer_stack_id=$1)"
            ),
            vec![stack.clone()],
        )?;
        tx.query(
            &format!(
                "DELETE FROM {schema}.reach t USING {schema}.gc_dead d \
                 WHERE d.kind='reach' AND d.id=t.key||int8send(t.x) \
                   AND (t.key=$1 OR t.key IN (SELECT cl.run \
                        FROM {schema}.commit_lineage cl \
                        JOIN {schema}.\"commit\" c ON c.commit_id=cl.commit_id \
                       WHERE c.layer_stack_id=$1))"
            ),
            vec![stack.clone()],
        )?;
        tx.query(
            &format!(
                "DELETE FROM {schema}.commit_lineage t USING {schema}.gc_dead d \
                 WHERE d.kind='commit_lineage' AND d.id=t.commit_id \
                   AND t.commit_id IN (SELECT commit_id FROM {schema}.\"commit\" \
                                        WHERE layer_stack_id=$1)"
            ),
            vec![stack.clone()],
        )?;
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

/// Rows one `file_digest` delete transaction removes.
const DIGEST_BATCH: i64 = 1000;

/// Deletes the `file_digest` rows whose root object is in the persisted dead
/// set, in bounded batches. The dead set holds roots, not digests, so a row
/// with a dead root is removed even when a different digest names it.
/// `file_digest` has no index on `root`, so one scan first lists the keys of
/// the rows to delete in a numbered session table; each batch then deletes
/// its own key range by primary key and checks the root against `gc_dead`
/// again. The `gc_dead` rows stay until [`settle`], so a crash here resumes
/// them: the next run lists what is left.
fn delete_dead_digests(session: &Arc<Session>, schema: &str) -> HistoryResult<()> {
    let total = stage(session, |tx| -> HistoryResult<i64> {
        tx.batch(&format!(
            "DROP TABLE IF EXISTS pg_temp.gc_dead_digest;\
             CREATE TEMP TABLE gc_dead_digest ON COMMIT PRESERVE ROWS AS \
               SELECT row_number() OVER () AS n,fd.digest,fd.length \
                 FROM {schema}.file_digest fd \
                WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d \
                               WHERE d.kind='digest' AND d.id=fd.root);\
             CREATE UNIQUE INDEX gc_dead_digest_n ON pg_temp.gc_dead_digest (n);\
             ANALYZE pg_temp.gc_dead_digest;"
        ))?;
        scalar(
            tx,
            "SELECT count(*) FROM pg_temp.gc_dead_digest",
            Vec::new(),
        )
    })?;
    let mut done = 0;
    while done < total {
        stage(session, |tx| {
            tx.query(
                &format!(
                    "DELETE FROM {schema}.file_digest fd USING pg_temp.gc_dead_digest b \
                      WHERE b.n > $1 AND b.n <= $2 \
                        AND fd.digest=b.digest AND fd.length=b.length \
                        AND EXISTS (SELECT 1 FROM {schema}.gc_dead d \
                                     WHERE d.kind='digest' AND d.id=fd.root)"
                ),
                vec![Param::I64(done), Param::I64(done + DIGEST_BATCH)],
            )?;
            Ok(())
        })?;
        done += DIGEST_BATCH;
    }
    stage(session, |tx| {
        tx.batch("DROP TABLE IF EXISTS pg_temp.gc_dead_digest")?;
        Ok(())
    })
}

/// The final Phase C transaction: clear the Store-global advisory hint block,
/// drop the mark's transient `kept_node` closure, settle the four totals once,
/// assert the Store sums and clear the op. Every dead history row and its
/// derived lineage closure was deleted with its project by
/// [`delete_stack_batch`]; the marker rows (`branch_retired`,
/// `layer_stack_retired`, `path_removed`, `project_epoch`) went with their FK
/// parent in those same transactions, so this transaction holds no history
/// delete and no Store-wide orphan scan.
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
            "DELETE FROM {schema}.binding_hint_block;\
             DELETE FROM {schema}.kept_node;"
        ))?;
        // The totals and the Store-wide sums come from one statement, so one
        // snapshot: a publication adds to both in one transaction, so it is in
        // both or neither. The `store_totals` row is not locked while they are
        // read; every publication updates that row, so locking it first held
        // the whole Store's publications behind these scans (#776 P3).
        let rows = tx.query(
            &format!(
                "SELECT t.logical,t.canonical,t.stored,t.metadata,\
                        (SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.\"commit\")+(SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.layer),\
                        (SELECT COALESCE(SUM(canonical_length)::bigint,0) FROM {schema}.object_location),\
                        (SELECT COALESCE(SUM(length)::bigint,0) FROM {schema}.pack),\
                        (SELECT COALESCE(SUM(length)::bigint,0) FROM {schema}.pack WHERE domain=0) \
                   FROM {schema}.store_totals t WHERE t.id=1"
            ),
            Vec::new(),
        )?;
        let row = rows
            .first()
            .ok_or(HistoryError::Integrity("compaction settled totals"))?;
        // Totals not yet settled still count the dead set the swap removed.
        let pending = match totals_applied {
            true => DeadTotals::default(),
            false => dead,
        };
        let logical = row.get::<i64>(0)? - pending.logical;
        let canonical = row.get::<i64>(1)? - pending.canonical;
        let stored = row.get::<i64>(2)? - pending.stored;
        let metadata = row.get::<i64>(3)? - pending.metadata;
        if logical != row.get::<i64>(4)?
            || canonical != row.get::<i64>(5)?
            || stored != row.get::<i64>(6)?
            || metadata != row.get::<i64>(7)?
        {
            return Err(HistoryError::Integrity("compaction settled totals"));
        }
        tx.batch(&format!("DELETE FROM {schema}.gc_dead"))?;
        // The row lock is taken last, for the few statements before COMMIT.
        // The update is a relative delta, so a publication that landed after
        // the read above stays counted.
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
        }
        tx.batch(&format!("DELETE FROM {schema}.gc_op"))?;
        Ok(())
    })
}

/// Phase C: drop the scratch schema, which holds the payload tables the swap
/// retired (their files are unlinked here, under no Store lock), then delete
/// only the persisted `gc_dead` members, in bounded batches (one layer
/// stack's closed closure per transaction, then the signature and digest
/// sets), settle the totals once, and clear the op. The Store is usable at
/// `G + 1` throughout, and the retained object set is asserted exactly.
/// `scratch` is the op's recorded artifact; a resumed run drops it the same
/// way, and a scratch already dropped is no error.
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
    scratch: Option<&str>,
) -> HistoryResult<()> {
    if let Some(scratch) = scratch {
        drop_scratch(session, scratch)?;
    }
    let (mark_serial, mark_stamp, visited) = mark_watermarks(session, schema)?;
    assert_retained_object_set(session, schema, visited)?;
    // F50R2-3: the `cleanup` phase is its own small transaction before the
    // first batch, so a long Phase C is observable in `store.status` and a
    // crash during the batches leaves `cleanup` recorded for the resume. It is
    // never written into the transaction that deletes `gc_op` itself.
    set_phase(session, schema, "cleanup")?;
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
                "DELETE FROM {schema}.gc_intent WHERE kind='compaction' AND state <> 'done'"
            ))?;
            Ok(())
        })?;
    }
    Ok(())
}
