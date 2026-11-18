//! `SqliteHandles` compaction: the §7 orchestration (mark, build, verify, install).

use super::build::build;
use super::build::live_page_metrics;
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
use super::lengths::PackLengths;
use super::op::mark_returned;
use super::op::now_ms;
use super::op::remove_op_record;
use super::op::stop_at;
use super::op::visited_path;
use super::op::InstalledReport;
use super::op::RunOp;
use super::receipt::receipt;
use super::staged::StagedLocators;
use super::types::seal_error;
use super::types::CompactFailpoint;
use super::types::CompactKeptWaste;
use super::types::CompactPath;
use super::types::CompactReceipt;
use super::types::CompactRequest;
use super::types::CompactResidue;
use super::verify::live_tuple;
use super::verify::sealed_state;
use super::verify::verify;
use super::verify::VerifyInput;
use crate::backend::records::Param;
use crate::compact_metrics::CompactPhase;
use crate::compact_metrics::RunMetrics;
use crate::SqliteHandles;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use layerfs_history::records::GcIntent;
use layerfs_history::records::GcIntentKind;
use layerfs_history::records::GcScopeKind;
use std::sync::Arc;

/// The history statement that reads the stored lengths of the packs after
/// one pack, a bounded piece of them.
const PACK_LENGTHS: &str = "gc_pack_lengths";
/// Packs one piece reads.
const PACK_LENGTH_ROWS: i64 = 4096;

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
        self.compact_observed(request, &mut |_| {})
    }

    /// [`Self::compact`], telling `on_phase` each phase as it starts (`mark`,
    /// `build`, `verify`, `install`). The observer changes nothing about the
    /// run; `store.status` stays the record another process reads.
    pub fn compact_observed(
        self,
        request: &CompactRequest,
        on_phase: &mut dyn FnMut(&'static str),
    ) -> HistoryResult<CompactReceipt> {
        let store = self.storage.session.path.clone();
        let mut metrics = RunMetrics::start(None, on_phase);
        let result = self.compact_measured(request, &mut metrics);
        // §9: the run is replying, so its own record (when it leaves one) is
        // no longer in flight. Mark it before the reply so the next
        // `store.compact` judges the leftover even while this process is still
        // alive; the window before this point stays protected by the owner
        // liveness guard.
        let _ = mark_returned(&store, &request.op_id);
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

            // §5.1/§7 admission: a client that holds the Store keeps its
            // sidecars, and the seal cannot remove one its owner keeps: it
            // would fail `Conflict` only after the whole mark. Another
            // process's open descriptions name that client, so the run
            // refuses `Busy` here, before the lane, the intent and the mark,
            // and leaves no artifact or op record. A crash remnant has no
            // description and is not refused; the next clean seal removes it
            // as before.
            if crate::backend::sqlite::exclusion::held_by_other_process(&store) {
                return Err(HistoryError::Busy);
            }

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
            if let Some(installed) =
                settle_leftover(&history, &store, generation, metrics, request.owner_live)?
            {
                // §9: a leftover judged at `G + 1` was installed; the record
                // answered its §11 report, so this call returns it rather than
                // re-deciding on the already rebuilt Store.
                return Ok(installed);
            }
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
            // mark transaction (which writes the §6.3 visited scratch before
            // a rebuild's commit) leaves a *recorded* run for the next call to
            // adopt, never an unrecorded staged name (§9). The mark phase
            // overwrites it once the mark's tuple and digest are known.
            let mut op = RunOp {
                op_id,
                generation: live.generation,
                phase: "admission".to_owned(),
                started_ms: now_ms(),
                owner_pid: request.owner_pid,
                owner_pid_start: request.owner_pid_start,
                owner_pid_boot: request.owner_pid_boot,
                // This process writes the record, so this is its namespace.
                owner_pid_namespace: crate::history::admission::current_namespace(),
                tuple: live.clone(),
                live_heads_digest: [0u8; 32],
                returned: false,
                installed: None,
            };
            op.write(&store)?;

            metrics.begin_phase(CompactPhase::Mark);
            // Every pack descriptor is fixed while the lane is held, so the
            // §6.4 decision reads them all once instead of asking the Store
            // per reached pack. It reads them inside the mark's transaction,
            // after the walk: in the layout that keeps a pack's payload in
            // its row, the scan visits one table page per pack, and the walk
            // has just read those pages for every pack it reached. The list
            // is bounded: see `PackLengths`.
            // Its file, when it needs one, goes where the mark's sort files
            // go, so it is made when the mark is.
            let mut lengths: Option<PackLengths> = None;
            // The stored bytes of the packs the mark retains.
            let mut live_stored = 0u64;
            // §5.1/§6.3/§8: the registration, the §6.1 closure and the §6.2
            // walk are ONE write transaction. `rebuild` commits it; `none`
            // rolls it back, so §3's `nothing is written` holds in the Store's
            // own file: no `kept_node` row, no `gc_intent` row and no committed
            // WAL frame survive the call. The visited scratch is the separate
            // temp database §6.3 allows; only a `rebuild` writes it, after the
            // decision and before that commit.
            let decided =
                history.mark_for_compaction(&visited_path(&store), &intent, |tx, mark| {
                    let lengths = lengths.insert(PackLengths::new(mark.sort_space()));
                    let mut after = i64::MIN;
                    loop {
                        let rows = tx.query(
                            PACK_LENGTHS,
                            vec![Param::I64(after), Param::I64(PACK_LENGTH_ROWS)],
                        )?;
                        for row in &rows {
                            let length = u64::try_from(row.get::<i64>(1)?)
                                .map_err(|_| HistoryError::Integrity("pack length"))?;
                            after = row.get::<i64>(0)?;
                            lengths.push(after, length)?;
                        }
                        if (rows.len() as i64) < PACK_LENGTH_ROWS {
                            break;
                        }
                    }
                    let total = lengths.total();
                    live_stored = lengths.stored(mark.objects().retained_packs())?;
                    // A rebuild frees the wholly dead packs and, since it
                    // writes partly dead packs again, the dead records the
                    // mark found inside retained ones: both count towards
                    // the 10%.
                    let dead = total
                        .saturating_sub(live_stored)
                        .saturating_add(mark.objects().waste().dead_in_retained_packs_bytes());
                    Ok(total > 0 && dead.saturating_mul(10) >= total)
                });
            let (mark, rebuild) = match decided {
                Ok(decided) => decided,
                Err(error) => {
                    // The rolled-back mark released nothing, and a sink that
                    // already finished leaves its scratch for us to drop.
                    let _ = remove_scratch(&visited_path(&store));
                    let _ = remove_op_record(&store, &op_id);
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
            let Some(mut lengths) = lengths else {
                abort_run(&history, &store, &op_id);
                return Err(HistoryError::Integrity("pack lengths"));
            };
            let total = lengths.total();
            let dead = total.saturating_sub(live_stored);
            let mut kept_waste = CompactKeptWaste::from_waste(mark.objects().waste());
            let history_rows = u64::try_from(tuple.history_rows).unwrap_or(0);

            if !rebuild {
                // §3/§6.3 `none`: the mark's write transaction was rolled back,
                // so the live Store keeps no mark state and no admission
                // intent. The visited scratch is written only for a rebuild,
                // so this path created no temporary file and `peakTempBytes`
                // is 0 with the `none-path` reason.
                let (free, page) = match live_page_metrics(&session) {
                    Ok(metrics) => metrics,
                    Err(error) => {
                        abort_before_install(&store, &op_id, false);
                        return Err(error);
                    }
                };
                metrics.end_phase();
                if let Err(error) = remove_op_record(&store, &op_id) {
                    abort_before_install(&store, &op_id, false);
                    return Err(error);
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
                    // No scratch is written; the most the mark's sorting and
                    // the pack lengths held together in their unnamed files,
                    // and zero when they wrote none.
                    mark.sort_space().peak_bytes(),
                    CompactResidue::default(),
                    total,
                    history_rows,
                ));
            }

            // §7 step 2: a stop here leaves the open session, the writer lane,
            // the committed intent, the visited scratch and the op record; the
            // live Store is at `G`. The admission record already names this
            // run, its generation and its owner, which is all a later call
            // judges a leftover by, and `store.status` reads its phase as
            // `mark`; the mark's tuple and digest go out with the next write.
            op.tuple = tuple.clone();
            op.live_heads_digest = mark.live_heads_digest();
            op.phase = "mark".to_owned();
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
                    abort_before_install(&store, &op_id, true);
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
                        abort_before_install(&store, &op_id, true);
                    }
                    return Err(mapped);
                }
            };
            guard.put(writer);
            // §7 step 1.5: the sealed file is about to be read back.
            stop_at(request, CompactFailpoint::BeforeSealRead, &op, &store)?;

            // §7 step 1.5's fence: the seal must not have moved the generation
            // or the heads the mark recorded.
            let (sealed_tuple, sealed_digest) = match sealed_state(&facts.path) {
                Ok(state) => state,
                Err(error) => {
                    abort_before_install(&store, &op_id, true);
                    return Err(error);
                }
            };
            if sealed_tuple != tuple {
                abort_before_install(&store, &op_id, true);
                return Err(HistoryError::Conflict("the Store changed after the mark"));
            }
            if sealed_digest != mark.live_heads_digest() {
                abort_before_install(&store, &op_id, true);
                return Err(HistoryError::Conflict(
                    "the Store heads changed after the mark",
                ));
            }
            stop_at(request, CompactFailpoint::AfterSeal, &op, &store)?;

            metrics.begin_phase(CompactPhase::Build);
            op.phase = "build".to_owned();
            if let Err(error) = op.write(&store) {
                // A definite I/O failure after the seal: release the intent and
                // drop the artifact set, so the next call does not meet BusyOp.
                abort_before_install(&store, &op_id, true);
                return Err(error);
            }
            let capacity = request.failpoint == Some(CompactFailpoint::CapacityAtBuild);
            let built = match build(
                BuildInput {
                    facts: &facts,
                    tuple: &tuple,
                    synchronous,
                    payload_bytes: live_stored,
                    lengths: &mut lengths,
                    kept: mark.objects(),
                    space: mark.sort_space(),
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
                    abort_before_install(&store, &op_id, true);
                    return Err(error);
                }
            };
            // §11 `keptWaste` states what the Store keeps when the call
            // returns: after a rebuild, what a mark of the new Store finds.
            // The dead records of the packs this build wrote again are gone
            // from it, so they are no longer kept.
            kept_waste.dead_in_retained_packs_bytes = kept_waste
                .dead_in_retained_packs_bytes
                .saturating_sub(built.repacked.marked_dead);
            // A record kept only as a delta base is charged the width a read
            // of it pays, which changed where its group was written again.
            kept_waste.delta_bases_bytes = kept_waste
                .delta_bases_bytes
                .saturating_sub(built.repacked.bases_before)
                .saturating_add(built.repacked.bases_after);
            // The rebuilt Store holds only the blocks a kept leaf reads.
            kept_waste.inode_blocks_bytes = built.inode_blocks_bytes;
            // §11 `peakTempBytes`: the most the run's files hold at one
            // time. Either that was during the mark (its sort files and the
            // pack lengths, counted together by their one space), or it is
            // now: the fresh file, the scratch, what the space still holds
            // and the verification's sorted locators, which join it next.
            let space = mark.sort_space();
            let peak_temp = built
                .peak_temp_bytes()
                .saturating_add(space.held_bytes())
                .saturating_add(StagedLocators::sort_file_bytes(
                    space,
                    mark.objects().len() as u64,
                ))
                .max(space.peak_bytes());
            // §9/§11: record the installed outcome's totals now that the build
            // produced them, so a crash after the rename still lets the next
            // call answer the original report instead of re-deciding.
            op.installed = Some(InstalledReport {
                dead_bytes: dead,
                logical_discarded: u64::try_from(tuple.canonical)
                    .unwrap_or(0)
                    .saturating_sub(built.canonical),
                backend_reusable: source_reusable,
                physical_shrink: facts.bytes.saturating_sub(built.target_bytes),
                kept_waste: kept_waste.clone(),
                peak_temp,
                total_stored: total,
            });
            stop_at(request, CompactFailpoint::AfterBuild, &op, &store)?;
            stop_at(request, CompactFailpoint::AfterWalSwitch, &op, &store)?;
            metrics.end_phase();

            metrics.begin_phase(CompactPhase::Verify);
            op.phase = "verify".to_owned();
            if let Err(error) = op.write(&store) {
                abort_before_install(&store, &op_id, true);
                return Err(error);
            }
            if let Err(error) = verify(
                VerifyInput {
                    store: &store,
                    built: &built,
                    lengths: &mut lengths,
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
                abort_before_install(&store, &op_id, true);
                return Err(error);
            }
            stop_at(request, CompactFailpoint::AfterVerify, &op, &store)?;
            metrics.end_phase();

            metrics.begin_phase(CompactPhase::Install);
            if let Some(hook) = request.before_install_hook {
                hook(&facts.path);
            }
            if let Err(error) = install_fence(&facts, &tuple) {
                abort_before_install(&store, &op_id, true);
                return Err(error);
            }
            if let Err(error) = prepare_target(&built.target) {
                abort_before_install(&store, &op_id, true);
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
            // The writer lane is free from here: a second compaction can take
            // it while this run settles. `settle_leftover` judges that race by
            // the owner the op record names; this test hook only names the
            // window. The settle takes the lane again for its one write.
            if let Some(hook) = request.before_settle_hook {
                hook(&store);
            }
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
                peak_temp,
                residue,
                total,
                history_rows,
            ))
        }
    }
}
