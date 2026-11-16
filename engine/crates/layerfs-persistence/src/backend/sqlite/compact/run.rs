//! `SqliteHandles` compaction: the §7 orchestration (mark, build, verify, install).

use super::build::build;
use super::build::live_page_metrics;
use super::build::pack_lengths;
use super::build::BuildInput;
use super::install::abort_before_install;
use super::install::abort_run;
use super::install::install_fence;
use super::install::prepare_target;
use super::install::remove_scratch;
use super::install::settle;
use super::install::settle_leftover;
use super::install::sync_directory;
use super::install::unfinished_work;
use super::install::WriterGuard;
use super::op::now_ms;
use super::op::op_path;
use super::op::stop_at;
use super::op::visited_path;
use super::op::RunOp;
use super::receipt::receipt;
use super::receipt::RunMetrics;
use super::types::seal_error;
use super::types::CompactFailpoint;
use super::types::CompactKeptWaste;
use super::types::CompactPath;
use super::types::CompactReceipt;
use super::types::CompactRequest;
use super::types::CompactResidue;
use super::verify::immutable_digest;
use super::verify::immutable_tuple;
use super::verify::live_tuple;
use super::verify::verify;
use super::verify::VerifyInput;
use crate::compact_metrics::CompactPhase;
use crate::MarkResult;
use crate::SqliteHandles;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use layerfs_history::records::GcIntent;
use layerfs_history::records::GcIntentKind;
use layerfs_history::records::GcScopeKind;
use std::collections::BTreeMap;
use std::sync::Arc;

impl SqliteHandles {
    /// Runs one §7 SQLite Store compaction on this Store's own machine.
    ///
    /// The call consumes the handles like [`Self::seal`]: it checkpoints and
    /// closes the sole session, and a rebuild installs a new file at `G + 1`.
    /// It takes the Store's exclusive writer lane before its admission intent,
    /// so no admitted writer can interleave between the mark, the seal and the
    /// rename. The caller reopens the Store afterwards.
    ///
    /// A definite failure before the rename removes the artifact set and leaves
    /// the old Store unchanged apart from the seal's checkpoint. A rename or
    /// directory-fsync failure is installed-but-uncertain: the new file and the
    /// op record are kept for the next `store.compact` to judge (§9).
    pub fn compact(self, request: &CompactRequest) -> HistoryResult<CompactReceipt> {
        let mut metrics = RunMetrics::start();
        let result = self.compact_measured(request, &mut metrics);
        metrics.finish(&result);
        result
    }

    /// The measured body of [`Self::compact`]; `metrics` observes it.
    pub(crate) fn compact_measured(
        self,
        request: &CompactRequest,
        metrics: &mut RunMetrics,
    ) -> HistoryResult<CompactReceipt> {
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = (request, metrics);
            return Err(HistoryError::Unsupported("SQLite Store compaction"));
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let SqliteHandles {
                storage,
                history,
                acquisition,
            } = self;
            drop(acquisition);
            let session = storage.session.clone();
            let store = session.path.clone();
            let profile = session.profile.clone();
            let layout = session.layout;
            let acquisition_schema = session.acquisition;
            let synchronous = profile.synchronous;

            // §5: an unfinished Init or merge holds the Store; its working
            // rows are not wiped by a compaction that was never admitted.
            if unfinished_work(&store)? {
                return Err(HistoryError::Busy);
            }

            // §7 step 1: the exclusive lane first, so the generation this run
            // records cannot move between the mark, the seal and the rename.
            let lane = session.hold_lane().map_err(HistoryError::from)?;
            let live = live_tuple(&session)?;
            let generation = u64::try_from(live.generation)
                .map_err(|_| HistoryError::Integrity("Store generation"))?;
            metrics.generation_before = generation;
            settle_leftover(&history, &store, generation, metrics)?;
            // §11 `backendReusable` is the Store's reusable backend space, not
            // the freshly `VACUUM`ed target's free list (which is empty by
            // construction): measure the source Store before the mark, so a
            // rebuild reports the free pages the run judges and reclaims.
            let (source_free, source_page) = live_page_metrics(&session)?;
            let source_reusable = source_free.saturating_mul(source_page);

            let op_id = request.op_id;
            // Spec 5.1: the operation intent is written inside the mark's one
            // write transaction, *before* the first checkpoint. A `none`
            // decision rolls that transaction back, so the intent (like the
            // transient `kept_node` rows) never reaches the live Store.
            let intent = GcIntent::operation(
                op_id,
                GcIntentKind::Compaction,
                GcScopeKind::Store,
                history.catalog_id.as_slice().to_vec(),
            )
            .owned(
                request.owner_runtime.as_deref(),
                request.owner_boot.as_deref(),
            )
            .with_phase(Some("mark"));
            // The op record is written before the mark so a crash inside the
            // mark transaction (which creates the §6.3 visited scratch before
            // it decides) leaves a *recorded* run for the next call to adopt,
            // never an unrecorded staged name (§9). The mark phase overwrites
            // it once the mark's tuple and digest are known.
            let mut op = RunOp {
                op_id,
                generation: live.generation,
                phase: "admission".to_owned(),
                started_ms: now_ms(),
                owner_pid: request.owner_pid,
                owner_pid_start: request.owner_pid_start,
                owner_pid_boot: request.owner_pid_boot,
                tuple: live.clone(),
                live_heads_digest: [0u8; 32],
            };
            if let Err(error) = op.write(&store) {
                return Err(error);
            }

            metrics.begin_phase(CompactPhase::Mark);
            // Every pack descriptor is fixed while the lane is held, so the
            // §6.4 decision reads them once around the mark instead of asking
            // the Store per reached pack.
            let lengths = match pack_lengths(&session) {
                Ok(lengths) => lengths,
                Err(error) => {
                    let _ = std::fs::remove_file(op_path(&store));
                    return Err(error);
                }
            };
            let total = lengths.values().copied().fold(0u64, u64::saturating_add);
            let decide = |mark: &MarkResult| -> HistoryResult<bool> {
                let live_stored = live_stored_bytes(&lengths, mark)?;
                let dead = total.saturating_sub(live_stored);
                Ok(total > 0 && dead.saturating_mul(10) >= total)
            };
            // §5.1/§6.3/§8: the registration, the §6.1 closure and the §6.2
            // walk are ONE write transaction. `rebuild` commits it; `none`
            // rolls it back, so §3's `nothing is written` holds in the Store's
            // own file: no `kept_node` row, no `gc_intent` row and no committed
            // WAL frame survive the call. The visited scratch is the separate
            // temp database §6.3 allows; `none` deletes it below.
            let decided = history.mark_for_compaction(&visited_path(&store), &intent, decide);
            let (mark, rebuild) = match decided {
                Ok(decided) => decided,
                Err(error) => {
                    // The rolled-back mark released nothing, and a sink that
                    // already finished leaves its scratch for us to drop.
                    let _ = remove_scratch(&visited_path(&store));
                    let _ = std::fs::remove_file(op_path(&store));
                    return Err(error);
                }
            };
            let tuple = match live_tuple(&session) {
                Ok(tuple) => tuple,
                Err(error) => {
                    abort_run(&history, &store, &op_id);
                    return Err(error);
                }
            };
            let live_stored = match live_stored_bytes(&lengths, &mark) {
                Ok(live_stored) => live_stored,
                Err(error) => {
                    abort_run(&history, &store, &op_id);
                    return Err(error);
                }
            };
            let dead = total.saturating_sub(live_stored);
            let kept_waste = CompactKeptWaste::from_waste(mark.objects().waste());
            let history_rows = u64::try_from(tuple.history_rows).unwrap_or(0);

            if !rebuild {
                // §3/§6.3 `none`: the mark's write transaction was rolled back,
                // so the live Store keeps no mark state and no admission
                // intent. Only the visited scratch was written, and it counts
                // in `peakTempBytes` until it is deleted before the reply.
                let scratch = visited_path(&store);
                let visited_bytes = match std::fs::metadata(&scratch) {
                    Ok(metadata) => metadata.len(),
                    Err(_) => {
                        abort_before_install(&store, None);
                        return Err(HistoryError::Integrity("visited scratch"));
                    }
                };
                if let Err(error) = remove_scratch(&scratch) {
                    abort_before_install(&store, None);
                    return Err(error);
                }
                let (free, page) = match live_page_metrics(&session) {
                    Ok(metrics) => metrics,
                    Err(error) => {
                        abort_before_install(&store, None);
                        return Err(error);
                    }
                };
                metrics.end_phase();
                if std::fs::remove_file(op_path(&store)).is_err()
                    && std::fs::symlink_metadata(op_path(&store)).is_ok()
                {
                    abort_before_install(&store, None);
                    return Err(HistoryError::Integrity("compaction record"));
                }
                let _ = sync_directory(&store);
                drop(lane);
                return Ok(receipt(
                    CompactPath::None,
                    generation,
                    generation,
                    dead,
                    0,
                    free.saturating_mul(page),
                    0,
                    kept_waste,
                    visited_bytes,
                    CompactResidue::default(),
                    total,
                    history_rows,
                ));
            }

            // §7 step 2: a stop here leaves the open session, the writer lane,
            // the committed intent, the visited scratch and the op record; the
            // live Store is at `G`.
            op.tuple = tuple.clone();
            op.live_heads_digest = mark.live_heads_digest();
            op.phase = "mark".to_owned();
            if let Err(error) = op.write(&store) {
                abort_run(&history, &store, &op_id);
                return Err(error);
            }
            // §7 step 1: with the lane held, no writer can append while the
            // intent, the op record and the WAL are settled together.
            if let Err(error) = session.checkpoint_holding() {
                abort_run(&history, &store, &op_id);
                return Err(HistoryError::from(error));
            }
            metrics.end_phase();
            stop_at(request, CompactFailpoint::BeforeSeal, &op, &store)?;

            // §7 step 2 onward: seal and rebuild. Only the session remains.
            // §5.1/§9: the admission intent is *not* released here. It is
            // copied into `<store>.compact`, lives through the build and the
            // install, and the cleanup settle clears it on the installed Store;
            // a definite failure before the rename releases it with a raw
            // delete (the run no longer holds a session), and a crash leaves the
            // op record for the next run to adopt.
            drop(history);
            drop(storage);
            let mut guard = WriterGuard::new(lane.into_writer());
            let session = match Arc::try_unwrap(session) {
                Ok(session) => session,
                Err(_) => {
                    // A definite failure before the seal: the live Store is
                    // unchanged, so its intent is released and the op record
                    // removed. A held clone elsewhere is `Busy`, never unknown.
                    abort_before_install(&store, Some(&op_id));
                    return Err(HistoryError::Busy);
                }
            };
            let writer = guard.take();
            let (facts, writer) = match session.seal_holding(writer) {
                Ok(sealed) => sealed,
                Err(error) => {
                    let mapped = seal_error(error);
                    if !mapped.unknown() {
                        // Definite: nothing was installed, so the run leaves no
                        // op record and no live intent behind.
                        abort_before_install(&store, Some(&op_id));
                    }
                    return Err(mapped);
                }
            };
            guard.put(writer);
            // §7 step 1.5: the sealed file is about to be read back.
            stop_at(request, CompactFailpoint::BeforeSealRead, &op, &store)?;

            // §7 step 1.5's fence: the seal must not have moved the generation
            // or the heads the mark recorded.
            let sealed_tuple = match immutable_tuple(&facts.path) {
                Ok(tuple) => tuple,
                Err(error) => {
                    abort_before_install(&store, Some(&op_id));
                    return Err(error);
                }
            };
            if sealed_tuple != tuple {
                abort_before_install(&store, Some(&op_id));
                return Err(HistoryError::Conflict("the Store changed after the mark"));
            }
            match immutable_digest(&facts.path) {
                Ok(digest) if digest == mark.live_heads_digest() => {}
                Ok(_) => {
                    abort_before_install(&store, Some(&op_id));
                    return Err(HistoryError::Conflict(
                        "the Store heads changed after the mark",
                    ));
                }
                Err(error) => {
                    abort_before_install(&store, Some(&op_id));
                    return Err(error);
                }
            }
            stop_at(request, CompactFailpoint::AfterSeal, &op, &store)?;

            metrics.begin_phase(CompactPhase::Build);
            op.phase = "build".to_owned();
            if let Err(error) = op.write(&store) {
                // A definite I/O failure after the seal: release the intent and
                // drop the artifact set, so the next call does not meet BusyOp.
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            let capacity = request.failpoint == Some(CompactFailpoint::CapacityAtBuild);
            let built = match build(
                BuildInput {
                    facts: &facts,
                    tuple: &tuple,
                    synchronous,
                },
                capacity,
                request,
                &op,
            ) {
                Ok(built) => built,
                // A stop point leaves the crash state on purpose: never abort
                // an artifact set a recorded run names (§9).
                Err(HistoryError::UnknownOutcome) => return Err(HistoryError::UnknownOutcome),
                Err(error) => {
                    abort_before_install(&store, Some(&op_id));
                    return Err(error);
                }
            };
            // §7 step 7: every artifact and the op record are durable before
            // the verification reads them back.
            if let Err(error) = sync_directory(&store) {
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            stop_at(request, CompactFailpoint::AfterBuild, &op, &store)?;
            stop_at(request, CompactFailpoint::AfterWalSwitch, &op, &store)?;
            metrics.end_phase();

            metrics.begin_phase(CompactPhase::Verify);
            op.phase = "verify".to_owned();
            if let Err(error) = op.write(&store) {
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            if let Err(error) = verify(
                VerifyInput {
                    store: &store,
                    built: &built,
                    mark: &mark,
                    tuple: &tuple,
                    layout,
                    acquisition: acquisition_schema,
                },
                request,
                &op,
            ) {
                if matches!(error, HistoryError::UnknownOutcome) {
                    return Err(error);
                }
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            stop_at(request, CompactFailpoint::AfterVerify, &op, &store)?;
            metrics.end_phase();

            metrics.begin_phase(CompactPhase::Install);
            if let Some(hook) = request.before_install_hook {
                hook(&facts.path);
            }
            if let Err(error) = install_fence(&facts, &tuple) {
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            if let Err(error) = prepare_target(&built.target) {
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            stop_at(request, CompactFailpoint::BeforeInstall, &op, &store)?;

            if std::fs::rename(&built.target, &facts.path).is_err() {
                // Installed-but-uncertain: never delete the new Store as if
                // nothing happened (§7 step 8, §9).
                op.phase = "install".to_owned();
                let _ = op.write(&store);
                return Err(HistoryError::UnknownOutcome);
            }
            // §7 step 7: the rename happened; a crash before the directory
            // fsync leaves an installed-but-uncertain `G + 1`.
            stop_at(request, CompactFailpoint::DuringRename, &op, &store)?;
            if let Err(_error) = sync_directory(&facts.path) {
                op.phase = "install".to_owned();
                let _ = op.write(&store);
                return Err(HistoryError::UnknownOutcome);
            }
            op.phase = "install".to_owned();
            let _ = op.write(&store);
            stop_at(request, CompactFailpoint::AfterInstall, &op, &store)?;
            guard.release()?;
            let residue = settle(&store, request, &op)?;
            // §11 `logicalDiscarded` is the canonical bytes of the objects the
            // rebuild collects, not the history rows' `logical` sum: a shared
            // tree counts once and a collected orphan object still counts.
            let logical_discarded = u64::try_from(tuple.canonical)
                .unwrap_or(0)
                .saturating_sub(built.canonical);
            let physical_shrink = facts.bytes.saturating_sub(built.target_bytes);
            Ok(receipt(
                CompactPath::Rebuild,
                generation,
                generation.saturating_add(1),
                dead,
                logical_discarded,
                source_reusable,
                physical_shrink,
                kept_waste,
                built.peak_temp_bytes(),
                residue,
                total,
                history_rows,
            ))
        }
    }
}

/// The stored bytes of every pack the mark retains, from the one read of the
/// Store's fixed pack descriptors the run takes while it holds the lane.
fn live_stored_bytes(lengths: &BTreeMap<i64, u64>, mark: &MarkResult) -> HistoryResult<u64> {
    let mut live = 0u64;
    for pack in mark.objects().packs() {
        let length = lengths
            .get(pack)
            .ok_or(HistoryError::Integrity("mark pack"))?;
        live = live.saturating_add(*length);
    }
    Ok(live)
}
