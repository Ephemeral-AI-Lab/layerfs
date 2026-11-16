//! `SqliteHandles` compaction: the §7 orchestration (mark, build, verify, install).

use super::build::build;
use super::build::live_page_metrics;
use super::build::pack_lengths;
use super::build::BuildInput;
use super::install::abort_before_install;
use super::install::abort_run;
use super::install::clear_artifacts;
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
use super::types::MarkTuple;
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
use layerfs_history::HistoryCatalog;
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
            // §7 step 1's sealed source's WAL is emptied after admission, so
            // the intent's own write is checkpointed with it.
            let live = live_tuple(&session)?;
            let generation = u64::try_from(live.generation)
                .map_err(|_| HistoryError::Integrity("Store generation"))?;
            metrics.generation_before = generation;
            settle_leftover(&history, &store, generation, metrics)?;

            let op_id = request.op_id;
            // Spec 5.1: the exclusive operation intent is registered before
            // the first checkpoint, so a removal sees an admitted compaction
            // even while the WAL is being emptied, and the whole run is one
            // admitted sequence.
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
            history.gc_register(&intent)?;
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
                let _ = history.gc_release(&op_id);
                return Err(error);
            }
            // §7 step 1: with the lane held, no writer can append while the
            // intent, the op record and the WAL are settled together.
            if let Err(error) = session.checkpoint_holding() {
                let _ = history.gc_release(&op_id);
                let _ = std::fs::remove_file(op_path(&store));
                return Err(HistoryError::from(error));
            }
            // §8: the registration, the mark and the totals read are one
            // admitted sequence. A definite failure here releases the intent
            // before the artifact set and the op record go, so the Store is
            // left exactly as it was and no later call meets BusyOp.
            metrics.begin_phase(CompactPhase::Mark);
            let admitted =
                (|| -> HistoryResult<(MarkResult, MarkTuple, u64, u64, CompactKeptWaste, u64)> {
                    // §6.3: the visited set is streamed into the temp-file table while the
                    // walk discovers objects, so the build joins it instead of a second pass.
                    let mark = history.mark_with_scratch(&visited_path(&store))?;
                    let tuple = live_tuple(&session)?;
                    op.tuple = tuple.clone();
                    op.live_heads_digest = mark.live_heads_digest();
                    op.phase = "mark".to_owned();
                    op.write(&store)?;
                    let lengths = pack_lengths(&session)?;
                    let total = lengths.values().copied().fold(0u64, u64::saturating_add);
                    let mut live_stored = 0u64;
                    for pack in mark.objects().packs() {
                        let length = lengths
                            .get(pack)
                            .ok_or(HistoryError::Integrity("mark pack"))?;
                        live_stored = live_stored.saturating_add(*length);
                    }
                    let dead = total.saturating_sub(live_stored);
                    let kept_waste = CompactKeptWaste::from_waste(mark.objects().waste());
                    let history_rows = u64::try_from(tuple.history_rows).unwrap_or(0);
                    Ok((mark, tuple, total, dead, kept_waste, history_rows))
                })();
            let (mark, tuple, total, dead, kept_waste, history_rows) = match admitted {
                Ok(admitted) => admitted,
                Err(error) => {
                    abort_run(&history, &store, &op_id);
                    return Err(error);
                }
            };
            let rebuild = total > 0 && dead.saturating_mul(10) >= total;
            metrics.end_phase();

            if !rebuild {
                // §3/§6.3 `none`: mark only. The mark streamed the closure to
                // `<store>.compact-visited`; the none path deletes it before it
                // replies and counts it in `peakTempBytes`, so the live Store
                // keeps no mark state. The transient `kept_node` rows the mark
                // wrote are cleared and the admission intent released, so the
                // generation is unchanged.
                let scratch = visited_path(&store);
                let visited_bytes = match std::fs::metadata(&scratch) {
                    Ok(metadata) => metadata.len(),
                    Err(_) => {
                        abort_run(&history, &store, &op_id);
                        return Err(HistoryError::Integrity("visited scratch"));
                    }
                };
                if let Err(error) = remove_scratch(&scratch) {
                    abort_run(&history, &store, &op_id);
                    return Err(error);
                }
                if let Err(error) = history.clear_kept_node() {
                    abort_run(&history, &store, &op_id);
                    return Err(error);
                }
                let (free, page) = live_page_metrics(&session)?;
                history.gc_release(&op_id)?;
                // The mark created the visited scratch even on this path; the
                // `none` answer leaves no residue, exactly as it leaves no
                // staged Store.
                let _ = clear_artifacts(&store);
                let _ = std::fs::remove_file(op_path(&store));
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
            // the admission intent and the op record; the live Store is at `G`.
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
                    layout,
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
            let logical_discarded = u64::try_from(tuple.logical)
                .unwrap_or(0)
                .saturating_sub(built.logical);
            let physical_shrink = facts.bytes.saturating_sub(built.target_bytes);
            Ok(receipt(
                CompactPath::Rebuild,
                generation,
                generation.saturating_add(1),
                dead,
                logical_discarded,
                built.free_bytes,
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
