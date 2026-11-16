//! `PostgresHandles` compaction: the §8 orchestration (round, settle, entry).

use super::build::build;
use super::cleanup::cleanup;
use super::cleanup::settle_leftover;
use super::op::clear_round;
use super::op::live_stored;
use super::op::load_visited;
use super::op::persist_mark;
use super::op::read_generation;
use super::op::scratch_name;
use super::op::stage;
use super::op::stop;
use super::op::total_stored;
use super::op::RunMetrics;
use super::report::report;
use super::swap::swap;
use super::sync::effective_sync_rank;
use super::sync::standby_precheck;
use super::types::THRESHOLD;
use crate::backend::postgresql::access::compaction_missing;
use crate::backend::postgresql::session::connection::Session;
use crate::backend::records::Param;
use crate::backend::sqlite::compact::CompactFailpoint;
use crate::backend::sqlite::compact::CompactKeptWaste;
use crate::backend::sqlite::compact::CompactPath;
use crate::backend::sqlite::compact::CompactReceipt;
use crate::backend::sqlite::compact::CompactRequest;
use crate::backend::sqlite::compact::CompactResidue;
use crate::compact_metrics::CompactPhase;
use crate::PostgresHandles;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use layerfs_history::records::GcIntent;
use layerfs_history::records::GcIntentKind;
use layerfs_history::records::GcScopeKind;
use layerfs_history::HistoryCatalog;
use std::sync::Arc;

/// One round after admission: mark, decide, build, swap, cleanup.
pub(crate) fn round(
    handles: &PostgresHandles,
    session: &Arc<Session>,
    schema: &str,
    request: &CompactRequest,
    metrics: &mut RunMetrics,
) -> HistoryResult<CompactReceipt> {
    metrics.begin_phase(CompactPhase::PostgresMark);
    let (mark, tuple) = handles.history.mark_gc()?;
    // §11 `markBytes`: the distinct live-pack body bytes this mark read.
    metrics.mark_bytes = mark.mark_bytes();
    load_visited(session, schema, &mark)?;
    let stored = u64::try_from(total_stored(session, schema)?).unwrap_or(0);
    let live = u64::try_from(live_stored(session, schema)?).unwrap_or(0);
    let dead_bytes = stored.saturating_sub(live);
    // The scratch name is the Store's catalog identity, not this caller's
    // operation id: a crash on another machine can still name the artifact.
    let scratch = scratch_name(handles.history.catalog_id.as_slice());
    let mark_dead = persist_mark(session, schema, &mark, &tuple, &scratch, request)?;
    let generation = u64::try_from(tuple.generation).unwrap_or(0);
    let kept = CompactKeptWaste::from_waste(mark.objects().waste());
    let rebuild = stored > 0 && dead_bytes.saturating_mul(THRESHOLD) >= stored;
    if !rebuild {
        metrics.end_phase();
        clear_round(session, schema)?;
        let _ = handles.history.gc_release(&request.op_id);
        return Ok(report(
            CompactPath::None,
            generation,
            generation,
            dead_bytes,
            dead_bytes,
            0,
            0,
            0,
            kept,
            0,
            CompactResidue::default(),
            stored,
            mark.history().len() as u64,
        ));
    }
    metrics.end_phase();
    stop(
        request,
        CompactFailpoint::PostgresAfterMark,
        session,
        schema,
        "mark",
    )?;
    // §11: the op row names the live phase while the run is in it.
    stage(session, |tx| {
        tx.batch(&format!(
            "UPDATE {schema}.gc_op SET phase='build' WHERE id=1"
        ))?;
        Ok(())
    })?;
    metrics.begin_phase(CompactPhase::PostgresBuild);
    let mut built = build(session, schema, &scratch, &mark, &tuple)?;
    metrics.end_phase();
    built.dead.logical = mark_dead.logical;
    stop(
        request,
        CompactFailpoint::PostgresAfterBuild,
        session,
        schema,
        "build",
    )?;
    metrics.begin_phase(CompactPhase::PostgresSwap);
    let swapped = swap(
        handles,
        session,
        schema,
        &tuple,
        &mark,
        &built,
        &request.op_id,
    )?;
    metrics.end_phase();
    stop(
        request,
        CompactFailpoint::PostgresAfterSwap,
        session,
        schema,
        "swap",
    )?;
    // §11: the collected objects' canonical bytes, deduplicated by object id,
    // not the history rows' `logical` sum (which double counts shared trees).
    let logical_discarded = u64::try_from(swapped.dead.canonical).unwrap_or(0);
    metrics.begin_phase(CompactPhase::PostgresCleanup);
    cleanup(
        handles,
        session,
        schema,
        swapped.dead,
        false,
        Some(request.op_id),
    )?;
    metrics.end_phase();
    stop(
        request,
        CompactFailpoint::PostgresAfterCleanup,
        session,
        schema,
        "cleanup",
    )?;
    let shrink = swapped
        .physical_before
        .saturating_sub(swapped.physical_after);
    Ok(report(
        CompactPath::Rebuild,
        generation,
        generation + 1,
        dead_bytes,
        dead_bytes,
        logical_discarded,
        0,
        shrink,
        kept,
        swapped.peak_temp,
        CompactResidue::default(),
        stored,
        mark.history().len() as u64,
    ))
}

/// True while the schema holds an Init or merge working row. Admission
/// already refuses a live Init/merge intent; this catches leftover rows of
/// an operation whose intent was lost, which Phase C must not wipe silently.
pub(crate) fn unfinished_work(session: &Arc<Session>, schema: &str) -> HistoryResult<bool> {
    stage(session, |tx| {
        for table in [
            "merge_operation",
            "merge_record",
            "init_operation",
            "init_entry",
            "init_host_file",
        ] {
            let name = format!("{schema}.{table}");
            let rows = tx.query(
                "SELECT (to_regclass($1) IS NOT NULL)::int",
                vec![Param::Text(name.clone())],
            )?;
            if rows
                .first()
                .map(|row| row.get::<i64>(0))
                .transpose()?
                .unwrap_or(0)
                == 0
            {
                continue;
            }
            let rows = tx.query(&format!("SELECT count(*) FROM {name}"), Vec::new())?;
            if rows
                .first()
                .map(|row| row.get::<i64>(0))
                .transpose()?
                .unwrap_or(0)
                > 0
            {
                return Ok(true);
            }
        }
        Ok(false)
    })
}

/// Runs one §8 PostgreSQL Store compaction on this Store's session.
impl PostgresHandles {
    /// Runs one §8 PostgreSQL Store compaction on this Store's session.
    ///
    /// The call takes the Store's exclusive intent (a session advisory key)
    /// through cleanup, marks at `G`, and on the rebuild path copies the two
    /// payload tables, swaps them in one short transaction at `G + 1`, and
    /// deletes only the persisted dead set. The caller keeps its session; the
    /// Store is usable at `G + 1` from the swap on.
    pub fn compact(&self, request: &CompactRequest) -> HistoryResult<CompactReceipt> {
        let mut metrics = RunMetrics::start();
        let result = self.compact_measured(request, &mut metrics);
        metrics.finish(&result);
        result
    }

    /// The measured body of [`Self::compact`]; `metrics` observes it.
    pub(crate) fn compact_measured(
        &self,
        request: &CompactRequest,
        metrics: &mut RunMetrics,
    ) -> HistoryResult<CompactReceipt> {
        let session = self.storage.session.clone();
        let schema = session.settings.schema.clone();
        // AC-44: a restricted role is refused before the intent, the mark and
        // the scratch schema; the missing privilege is named, never a raw
        // `42501` from whichever statement happened to run first.
        if let Some(missing) = compaction_missing(&session)? {
            return Err(HistoryError::Unsupported(missing));
        }
        // §5: an unfinished Init or merge holds the Store; a compaction must
        // refuse rather than wipe its working rows in Phase C.
        if unfinished_work(&session, &schema)? {
            return Err(HistoryError::Busy);
        }
        // Spec 5.1/§9: the exclusive Store key is taken before the leftover op
        // is judged, so two compactions can never settle the same `gc_op` at
        // once, and a live owner is never terminated off-key.
        let claim = self.history.claim_store_exclusive()?;
        // Read under the key: a swap that committed before the claim is
        // authoritative, and this session's fence must use that generation.
        let generation = read_generation(&session, &schema)?;
        session.incarnation.store(
            u64::try_from(generation).unwrap_or(0),
            std::sync::atomic::Ordering::SeqCst,
        );
        metrics.generation_before = u64::try_from(generation).unwrap_or(0);
        if let Some(receipt) = settle_leftover(self, &session, &schema, metrics)? {
            return Ok(receipt);
        }
        let intent = GcIntent::operation(
            request.op_id,
            GcIntentKind::Compaction,
            GcScopeKind::Store,
            self.history.catalog_id.as_slice().to_vec(),
        )
        .owned(
            request.owner_runtime.as_deref(),
            request.owner_boot.as_deref(),
        )
        .with_phase(Some("mark"));
        // The registration takes its own level of the same key; dropping the
        // claim then leaves exactly that level held until `gc_release`.
        self.history.gc_register(&intent)?;
        drop(claim);
        let result = (|| {
            let rank = effective_sync_rank(&session)?;
            standby_precheck(&session, rank, &request.op_id, "mark")?;
            round(self, &session, &schema, request, metrics)
        })();
        if result.is_err() {
            let _ = self.history.gc_release(&request.op_id);
        }
        result
    }
}
