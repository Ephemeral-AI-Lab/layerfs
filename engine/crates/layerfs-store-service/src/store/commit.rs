//! One synchronous Content/Save/history producer and exact paired local install.
use super::{
    bind::checked_base, BoundWorkspace, CommitError, CommitFailure, CommitOptions, CommitPhase,
    CommitSuccess, MergeReport,
};
use crate::telemetry::phase;
use layerfs_content::compare::ChangedDirectories;
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::{
    error::MovedState, BranchSnapshot, CommitAttempt, CommitOutcome, CommitVia, HistoryError,
    NodeId,
};
use layerfs_overlay::Capture;
use layerfs_storage::Save;
use layerfs_telemetry::timer::{Active, TimingScope};
use layerfs_workspace::{CanonicalCache, CanonicalClient, ConstructedObjects, WorkspaceError};
use std::sync::{atomic::Ordering, Arc};

/// The private cache allowance of one validation over a built set: the
/// candidate's root, the pages down to its root inode and that directory's
/// first page and attributes.
const VALIDATION_CACHE_BYTES: usize = 4 * 1024 * 1024;

/// The node a Branch answers as its head: its head Commit, else its base
/// Layer (`branch get`, `--expected`).
pub(super) fn answered(snapshot: &BranchSnapshot) -> NodeId {
    snapshot
        .branch
        .head_commit
        .map_or(NodeId::Layer(snapshot.branch.base_layer), NodeId::Commit)
}

/// A definite refusal for a lane or lock wait: a lock timeout, a busy lane
/// or another live operation.
fn busy(error: &CommitError) -> bool {
    match error {
        CommitError::History(error) => {
            matches!(error.cause(), HistoryError::Busy | HistoryError::BusyOp(_))
        }
        CommitError::Storage(layerfs_storage::StorageError::Busy)
        | CommitError::Construction {
            storage: layerfs_storage::StorageError::Busy,
            ..
        } => true,
        _ => false,
    }
}

/// A fresh Commit attempt (#776 R1): 16 bytes from the kernel's random
/// source. A failed read fails the Commit before anything is published.
fn fresh_attempt() -> Result<CommitAttempt, CommitError> {
    use std::io::Read;
    let mut bytes = [0; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|_| CommitError::Context("Commit attempt: /dev/urandom read failed"))?;
    Ok(CommitAttempt::from_bytes(bytes))
}

/// `HeadMoved` for a Branch whose head is not the `expected` one.
pub(super) fn expected_moved(expected: NodeId, current: &BranchSnapshot) -> CommitError {
    let (expected_head, expected_base) = match expected {
        NodeId::Commit(id) => (Some(id), current.branch.base_layer),
        NodeId::Layer(layer) => (None, layer),
    };
    CommitError::History(HistoryError::HeadMoved(Box::new(MovedState {
        expected_head,
        actual_head: current.branch.head_commit,
        expected_base,
        actual_base: current.branch.base_layer,
    })))
}

/// Admission belongs to exactly one invocation. It stays held unless that
/// invocation establishes a safe disposition, including its captured releases.
/// Unwinding or an unknown result conservatively retains admission.
pub(super) struct CommitAdmission<'a> {
    workspace: &'a BoundWorkspace,
    pub(super) released: bool,
}
impl Drop for CommitAdmission<'_> {
    fn drop(&mut self) {
        if self.released {
            self.workspace.committing.store(false, Ordering::Release);
        }
    }
}

impl BoundWorkspace {
    /// Captures once and drives one caller-owned Content producer over that
    /// exact context. The producer must represent the capture faithfully; live
    /// namespace normalization is a separate constructor, not an implicit Init.
    /// No owner or Workspace mutex spans construction or provider I/O. On definite
    /// nonpublication one local resolution preserves the accepted view; unknown
    /// and known-publication/install failure retain original custody.
    pub fn commit(
        &self,
        construct: impl FnOnce(
            &Save<'_>,
            Capture,
            &BranchSnapshot,
        ) -> Result<FilesystemRootId, CommitError>,
    ) -> Result<CommitSuccess, Box<CommitFailure>> {
        let mut admission = self.admit_commit()?;
        let result = crate::telemetry::timed("commit", |timing| {
            self.commit_admitted(
                construct,
                || None,
                || (),
                &CommitOptions::default(),
                CommitVia::Workspace,
                timing,
            )
        });
        admission.released = match &result {
            Ok(_) => true,
            Err(failure) => failure.locally_settled,
        };
        if let Err(failure) = &result {
            self.retain(failure, None);
        }
        result
    }
    pub(super) fn admit_commit(&self) -> Result<CommitAdmission<'_>, Box<CommitFailure>> {
        self.committing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                Box::new(CommitFailure::new(
                    CommitError::InFlight,
                    self.identity,
                    self.route(),
                ))
            })?;
        Ok(CommitAdmission {
            workspace: self,
            released: false,
        })
    }
    /// The admitted invocation owns the flag through any wrapper's releases.
    /// `published` runs once, only after History answered this publication
    /// with the saved root; a failed, unknown or conflicted publication, and
    /// every earlier failure, never runs it.
    ///
    /// When the Branch head moved since the Workspace bound, the candidate is
    /// merged into the head in the same Save unless `force`. A clean merge is
    /// published against that head and installed as the new base. A conflict
    /// publishes nothing: the merge, with marker blocks, is installed as the
    /// Workspace's local base and its history binding moves to the head.
    /// Each phase is a child of `timing`; untraced, it records nothing.
    ///
    /// `staged` lends validation what `construct` built, if it kept it
    /// (`captured.rs`); its candidate is then checked from those bytes, not
    /// read back from the Store.
    pub(super) fn commit_admitted(
        &self,
        construct: impl FnOnce(
            &Save<'_>,
            Capture,
            &BranchSnapshot,
        ) -> Result<FilesystemRootId, CommitError>,
        staged: impl Fn() -> Option<Arc<ConstructedObjects>>,
        published: impl FnOnce(),
        options: &CommitOptions,
        via: CommitVia,
        timing: &TimingScope<'_, Active>,
    ) -> Result<CommitSuccess, Box<CommitFailure>> {
        let force = options.force;
        let mut retained = CommitFailure::new(
            CommitError::Context("incomplete Commit"),
            self.identity,
            self.route(),
        );
        let mut producer = None;
        let mut merge = None;
        let mut replaced = None;
        let result = (|| {
            let snapshot = self.snapshot().map_err(CommitError::Workspace)?;
            retained.binding = Some(snapshot.clone());
            let local = self.local_root(&snapshot)?;
            let original_base = self.workspace.base().map_err(CommitError::Workspace)?;
            // Nothing above the installed floor and the head where the
            // Workspace bound it: UpToDate with no capture, Save, lane or
            // install (#352 c04 records why the answer stays exact).
            let state = phase(timing, "uptodate.state", || {
                self.job(|job_client: &crate::OwnerClient, job_route| job_client.state(job_route))
                    .map_err(CommitError::Owner)
            })?;
            if state.dirty_inodes == 0
                && state.dirty_directory_entries == 0
                && state.captured.is_none()
                && state.consolidating.is_none()
                && state.base_root == local.to_bytes()
                && self
                    .pending_conflicts()
                    .map_err(CommitError::Workspace)?
                    .is_none()
            {
                let current = phase(timing, "uptodate.head", || {
                    self.store
                        .history
                        .branch_snapshot(snapshot.branch.id)
                        .map_err(CommitError::History)?
                        .ok_or(CommitError::Context("bound Branch is absent"))
                })?;
                if current.branch.head_commit == snapshot.branch.head_commit
                    && current.branch.base_layer == snapshot.branch.base_layer
                {
                    // `expected` holds here too: a clean Workspace answers
                    // UpToDate only over the head the caller named (#776 R2).
                    if let Some(expected) = options.expected {
                        if answered(&current) != expected {
                            return Err(expected_moved(expected, &current));
                        }
                    }
                    retained.published = Some(CommitOutcome::UpToDate {
                        head: current.branch.head_commit,
                        root: current.effective_root,
                    });
                    retained.saved = Some(Default::default());
                    retained.storage = Some(Default::default());
                    return Ok(());
                }
            }
            // The attempt this Commit publishes under, drawn before anything
            // is captured: a failed draw leaves nothing to fold back.
            let attempt = fresh_attempt()?;
            retained.phase = CommitPhase::Capture;
            let capture = phase(timing, "capture", || {
                self.job(|job_client: &crate::OwnerClient, job_route| job_client.capture(job_route))
                    .map_err(CommitError::Owner)
            })?;
            retained.capture = Some(capture);
            if capture.base_root != local.to_bytes() || original_base.identity().0 != local {
                return Err(CommitError::Context(
                    "capture differs from bound history root",
                ));
            }
            retained.phase = CommitPhase::Begin;
            let (storage, before) = phase(timing, "producer", || {
                self.producer
                    .take(&self.store)
                    .map_err(CommitError::Storage)
            })?;
            let built = (|| {
                let save = phase(timing, "begin", || {
                    storage.begin_save().map_err(CommitError::Storage)
                })?;
                retained.phase = CommitPhase::Construct;
                let candidate =
                    match phase(timing, "construct", || construct(&save, capture, &snapshot)) {
                        Ok(root) => root,
                        Err(original) => {
                            return Err(match save.take_failure() {
                                Some(storage) => CommitError::Construction {
                                    original: Box::new(original),
                                    storage,
                                },
                                None => original,
                            })
                        }
                    };
                // §4.3: the candidate is a new node above the frontier, so
                // an entry inherited unchanged from the bound base must be
                // removed explicitly; a re-creation keeps its new inode. The
                // bound node's markers are read only when this Workspace
                // does not already know them (`removal.rs`).
                let candidate = phase(timing, "removals", || {
                    let found = self
                        .commit_removals(snapshot.branch.stack, crate::removal::node_of(&snapshot))
                        .map_err(crate::removal::commit_error)?;
                    crate::removal::materialize_commit(
                        &save,
                        FilesystemRootId(local),
                        found,
                        candidate,
                    )
                    .map_err(crate::removal::commit_error)
                })?;
                // Two-stage merge (#355). The head is read and merged with
                // before the publication lane, so the merge's records and its
                // whole build stay outside it. In the lane the head is read
                // again: if it moved meanwhile, only what landed in between
                // is merged into the result, in memory, then published.
                // A fused Commit reads no head: its transaction asserts the
                // bound one (#768 stage 2, `commit_fused.rs`). Its last
                // publication is held until then, so its validation must
                // read none of this Commit's objects from the Store: only a
                // candidate whose built set covers it is fused.
                let fused = self.fused_eligible(&snapshot, options)
                    && staged().is_some_and(|built| built.covers(candidate.0));
                let current = phase(timing, "head", || {
                    self.refuse_markers(&save, candidate)?;
                    if fused {
                        return Ok(snapshot.clone());
                    }
                    self.store
                        .history
                        .branch_snapshot(snapshot.branch.id)
                        .map_err(CommitError::History)?
                        .ok_or(CommitError::Context("bound Branch is absent"))
                })?;
                let moved = current.branch.head_commit != snapshot.branch.head_commit
                    || current.branch.base_layer != snapshot.branch.base_layer;
                // `expected`: the Commit is made over that head only; the
                // publication checks it again in its own transaction.
                if let Some(expected) = options.expected {
                    if answered(&current) != expected {
                        return Err(expected_moved(expected, &current));
                    }
                }
                // What a merge changed against our candidate, for the check of
                // rows written since the capture (`live.rs`).
                let mut changed = Vec::new();
                let root = if moved && !force {
                    let (root, report) = phase(timing, "merge", || {
                        self.merge_candidate(&save, &snapshot, &current, candidate)
                    })?;
                    merge = Some(report);
                    changed = phase(timing, "merge.changed", || {
                        super::live::changed_serials(&save, candidate, root)
                    })?;
                    root
                } else {
                    candidate
                };
                // Everything that grows with the Commit runs before the lane:
                // the Save's finish, the logical count and the binding hints.
                let node = current
                    .branch
                    .head_commit
                    .map(layerfs_history::NodeId::Commit)
                    .unwrap_or(layerfs_history::NodeId::Layer(current.branch.base_layer));
                let known = phase(timing, "logical.known", || {
                    self.known_logical(node).map_err(CommitError::History)
                })?;
                retained.phase = CommitPhase::Finish;
                // A fused Commit holds the Save's last publication for its
                // own transaction; its reads below still see it.
                let tail = if fused {
                    Some(phase(timing, "finish", || {
                        save.finish_prepared().map_err(CommitError::Storage)
                    })?)
                } else {
                    let saved = phase(timing, "finish", || {
                        save.finish().map_err(CommitError::Storage)
                    })?;
                    self.store.note_save(&saved);
                    retained.saved = Some(saved);
                    None
                };
                // Count only after finish has proved the root's references
                // closed, so a missing dependency stays a Storage refusal.
                // One reader continues the Save for both phases, so they
                // share its caches and the bodies and values it published.
                let reader = storage.reader_after_save().map_err(CommitError::Storage)?;
                // One difference walk counts the logical size and notes the
                // directories whose listing changed; the hints then read
                // only those directories (#810).
                let mut directories = ChangedDirectories::default();
                let logical = phase(timing, "logical.count", || {
                    self.logical_after(
                        &reader,
                        current.effective_root,
                        root,
                        known,
                        Some(&mut directories),
                    )
                })?;
                let hints = phase(timing, "bindings", || match fused {
                    true => super::hints::collect_directory_bindings(&reader, &directories),
                    false => super::hints::record_directory_bindings(
                        &reader,
                        &*self.store.history,
                        &directories,
                    )
                    .map(|_| None),
                })?;
                drop(reader);
                // Rows written since the capture are checked first here,
                // before the lane and unheld: an overlap already present
                // refuses without taking the lane. The held check before the
                // install repeats it for rows written since (#776 R3).
                if !changed.is_empty() {
                    let overlap = phase(timing, "live.early", || self.live_overlap(&changed))?;
                    if !overlap.is_empty() {
                        return Err(self.live_conflict(snapshot.scope, candidate, root, &overlap));
                    }
                }
                // New bindings past one transaction's rows: the Save is
                // published and then every binding recorded, as on the
                // separate path, so no hint row is written before the
                // objects it describes; then the Commit alone is attempted
                // fused, as after an overflow.
                let tail = match (tail, &hints) {
                    (Some(tail), None) => {
                        let saved = phase(timing, "finish.tail", || {
                            tail.publish().map_err(CommitError::Storage)
                        })?;
                        self.store.note_save(&saved);
                        retained.saved = Some(saved);
                        let reader = storage.reader_after_save().map_err(CommitError::Storage)?;
                        phase(timing, "bindings", || {
                            super::hints::record_directory_bindings(
                                &reader,
                                &*self.store.history,
                                &directories,
                            )
                        })?;
                        None
                    }
                    (tail, _) => tail,
                };
                // The candidate is validated and its install prepared before
                // the lane: they read only immutable published objects. Only
                // an in-lane merge step, which changes the candidate, is
                // validated again inside the lane (layerfs-project #67).
                retained.phase = CommitPhase::Validate;
                let early_target = if merge.is_some() { &current } else { &snapshot };
                let early = phase(timing, "validate", || {
                    self.validate_candidate(early_target, root, capture, &original_base, staged())
                })?;
                let early = (Self::target_key(early_target, root), early);
                let mut fused_outcome = None;
                if fused {
                    retained.phase = CommitPhase::Publish;
                    fused_outcome = self.publish_fused(
                        tail,
                        &snapshot,
                        root,
                        logical,
                        hints,
                        options,
                        via,
                        attempt,
                        &mut retained,
                        timing,
                    )?;
                }
                let fused_published = fused_outcome.is_some();
                // A fused attempt that published no Commit (a race, or no
                // change against the bound root) continues on today's path
                // from its head read, before the lane: a moved head is
                // merged with there, in a Save of its own.
                let (current, root, logical, early) = if fused && !fused_published {
                    let again = self.reread_after_fused(
                        &storage,
                        &snapshot,
                        root,
                        logical,
                        options,
                        &mut retained,
                        timing,
                    )?;
                    let early = match again.merge {
                        Some(report) => {
                            merge = Some(report);
                            changed = again.changed;
                            // The fallback's merge gets the same unheld check
                            // before the lane as a first merge (#776 R3).
                            if !changed.is_empty() {
                                let overlap =
                                    phase(timing, "live.early", || self.live_overlap(&changed))?;
                                if !overlap.is_empty() {
                                    return Err(self.live_conflict(
                                        snapshot.scope,
                                        root,
                                        again.root,
                                        &overlap,
                                    ));
                                }
                            }
                            retained.phase = CommitPhase::Validate;
                            let checked = phase(timing, "validate", || {
                                self.validate_candidate(
                                    &again.current,
                                    again.root,
                                    capture,
                                    &original_base,
                                    staged(),
                                )
                            })?;
                            (Self::target_key(&again.current, again.root), checked)
                        }
                        None => early,
                    };
                    (again.current, again.root, again.logical, early)
                } else {
                    (current, root, logical, early)
                };
                retained.phase = CommitPhase::Finish;
                // A conflicted merge publishes nothing, so it never takes the
                // lane.
                let conflicted = merge
                    .as_ref()
                    .is_some_and(|report: &MergeReport| !report.conflicts.is_empty());
                // Taking the lane also checks, in the same round trip where
                // the provider can, that the head is still `current` (#768).
                let (lane, unmoved) = if conflicted || fused_published {
                    (None, false)
                } else {
                    let (lane, unmoved) = phase(timing, "lane", || match force {
                        true => Ok((self.store.history.publish_lane()?, false)),
                        false => self.store.history.publish_lane_unmoved(
                            snapshot.branch.id,
                            current.branch.head_commit,
                            current.branch.base_layer,
                        ),
                    })
                    .map_err(CommitError::History)?;
                    (Some(lane), unmoved)
                };
                // A forced Commit is decided by the publication itself,
                // which reads the head inside the lane.
                let (current, root, logical) = if force || conflicted || unmoved || fused_published
                {
                    (current, root, logical)
                } else {
                    let now = phase(timing, "head.lane", || {
                        self.store
                            .history
                            .branch_snapshot(snapshot.branch.id)
                            .map_err(CommitError::History)?
                            .ok_or(CommitError::Context("bound Branch is absent"))
                    })?;
                    // With `expected`, a head that moved again is not merged
                    // with: the publication refuses it `HeadMoved`.
                    if (now.branch.head_commit == current.branch.head_commit
                        && now.branch.base_layer == current.branch.base_layer)
                        || options.expected.is_some()
                    {
                        (current, root, logical)
                    } else {
                        // The in-lane step has its own Save; its count and
                        // hints cover only what it changed in the result.
                        let step_save = storage.begin_save().map_err(CommitError::Storage)?;
                        let (stepped, step) = phase(timing, "merge.lane", || {
                            self.merge_in_lane(&step_save, &current, &now, root, candidate)
                        })?;
                        let step_saved = phase(timing, "finish.lane", || {
                            step_save.finish().map_err(CommitError::Storage)
                        })?;
                        self.store.note_save(&step_saved);
                        if let Some(saved) = retained.saved.as_mut() {
                            super::commit_fused::add_saved(saved, &step_saved);
                        }
                        let reader = storage.reader_after_save().map_err(CommitError::Storage)?;
                        let mut directories = ChangedDirectories::default();
                        let logical = phase(timing, "logical.lane", || {
                            self.logical_after(
                                &reader,
                                root.0,
                                stepped,
                                logical,
                                Some(&mut directories),
                            )
                        })?;
                        phase(timing, "bindings.lane", || {
                            super::hints::record_directory_bindings(
                                &reader,
                                &*self.store.history,
                                &directories,
                            )
                        })?;
                        let step_changed = phase(timing, "merge.lane.changed", || {
                            super::live::changed_serials(&reader, root, stepped)
                        })?;
                        changed = super::live::union(&changed, &step_changed);
                        merge = Some(match merge.take() {
                            Some(first) => MergeReport {
                                from: first.from,
                                lww: first.lww.into_iter().chain(step.lww).collect(),
                                ..step
                            },
                            None => step,
                        });
                        (now, stepped, logical)
                    }
                };
                Ok((
                    root,
                    candidate,
                    current,
                    logical,
                    lane,
                    early,
                    changed,
                    fused_outcome,
                ))
            })();
            retained.storage = Some(super::producer::since(storage.diagnostics(), before));
            // Held here from now on, so a failure below still has it.
            producer = Some(storage);
            let (candidate, ours, current, logical, lane, early, changed, mut fused_outcome) =
                built?;
            if candidate != ours {
                replaced = Some((ours.0, candidate.0));
            }
            let conflicted = merge
                .as_ref()
                .is_some_and(|report: &MergeReport| !report.conflicts.is_empty());
            // Only a publication holds the lane: an in-lane step that
            // conflicted releases it before validation and install.
            let lane = lane.filter(|_| !conflicted);
            // A merge is published against the head it merged with; a forced
            // or unmoved Commit against the head the Workspace bound.
            let target = if merge.is_some() { &current } else { &snapshot };
            if !conflicted && fused_outcome.is_none() {
                let saved = retained
                    .saved
                    .as_ref()
                    .ok_or(CommitError::Context("finished Save"))?;
                retained.intent = Some(super::commit_fused::commit_request(
                    &snapshot,
                    target,
                    candidate,
                    logical,
                    saved,
                    merge.as_ref(),
                    options,
                    via,
                    attempt,
                ));
            }
            retained.phase = CommitPhase::Validate;
            let mut next = target.clone();
            next.effective_root = candidate.0;
            retained.prepared = Some(if early.0 == Self::target_key(target, candidate) {
                early.1
            } else {
                phase(timing, "validate.lane", || {
                    self.validate_candidate(target, candidate, capture, &original_base, staged())
                })?
            });
            // A merge installs a root other than our own. Rows written since
            // the capture were computed against the old base: mutations are
            // held from here through the install, and an active row that
            // overlaps what the merge changed refuses it before anything is
            // published or installed (`live.rs`).
            let held = if candidate != ours {
                let (hold, overlap) = phase(timing, "live", || self.hold_live(&changed))?;
                if !overlap.is_empty() {
                    drop(hold);
                    drop(lane);
                    return Err(self.live_conflict(snapshot.scope, ours, candidate, &overlap));
                }
                let changed: Arc<[u64]> = changed.into();
                retained.merged_over = Some((ours, changed.clone()));
                retained.prepared = retained.prepared.take().map(|ready| ready.merged(changed));
                Some(hold)
            } else {
                None
            };
            let pending = if conflicted {
                // Nothing is published: the history binding follows the head
                // merged with, and the merge stays the local base.
                next = current.clone();
                Some(super::commit_merge::PendingMerge {
                    local_root: candidate.0,
                    conflicted: merge
                        .as_ref()
                        .map(|report| report.conflicts.iter().map(|c| c.path.clone()).collect())
                        .unwrap_or_default(),
                })
            } else {
                retained.phase = CommitPhase::Publish;
                let intent = retained.intent.as_ref().expect("checked intent");
                let fused = fused_outcome.is_some();
                let outcome = match fused_outcome.take() {
                    Some(outcome) => Ok(outcome),
                    None => phase(timing, "publish", || {
                        self.store
                            .history
                            .commit_branch(intent)
                            .map_err(CommitError::History)
                    }),
                };
                drop(lane);
                let outcome = outcome?;
                let (head, root) = match &outcome {
                    CommitOutcome::Committed { record, .. } => (Some(record.id), record.root),
                    CommitOutcome::UpToDate { head, root } => (*head, *root),
                };
                if let CommitOutcome::Committed { record, .. } = &outcome {
                    // Facts of the published Commit, kept for the next one.
                    self.remember_logical(
                        NodeId::Commit(record.id),
                        record.meta.statistics.logical,
                    );
                    self.published_removals(NodeId::Commit(record.id));
                }
                retained.published = Some(outcome);
                // Unreachable after a fused publication: its record's root is
                // the candidate it was sent with. Were it reached there, this
                // Context error in phase Publish with `published` set would
                // keep the Workspace refusing InFlight, as the fused
                // settlement once did (#768 verification finding 8): such a
                // check must move to phase Install first.
                debug_assert!(!fused || root == candidate.0, "fused record root");
                if root != candidate.0 {
                    return Err(CommitError::Context(
                        "publication differs from saved candidate",
                    ));
                }
                published();
                next.branch.head_commit = head;
                next.head_root = head.map(|_| root);
                None
            };
            retained.phase = CommitPhase::Install;
            let prepared = retained.prepared.take().expect("prepared install");
            phase(timing, "install", || {
                self.job(|job_client: &crate::OwnerClient, route| {
                    job_client.install_prepared(route, self.workspace.clone(), prepared)
                })
                .map_err(CommitError::Owner)
            })?;
            // Mutations resume before the daemon invalidates the kernel's
            // caches: a held FUSE mutation keeps its directory's inode lock
            // or its locked pages in the kernel, which the invalidation waits
            // for, so a hold kept through it would deadlock (#776 R4).
            drop(held);
            *self
                .snapshot
                .lock()
                .map_err(|_| CommitError::Workspace(WorkspaceError::BindingPoisoned))? = next;
            *self
                .pending
                .lock()
                .map_err(|_| CommitError::Workspace(WorkspaceError::BindingPoisoned))? = pending;
            Ok(())
        })();
        if let Err(error) = result {
            retained.error = error;
            // A failed Commit drops its handle: on a definite failure the
            // ordinals it kept (from a finished Save, or the unused rest of
            // one that failed definitely) go back to the Store first, one
            // write on the writer (#768); after an unknown outcome they are
            // left lost. After a lane Busy or lock timeout that write could
            // wait again before the Workspace settles: the handle is kept
            // for the Workspace's next Commit instead, with its block, and
            // returns it at unmount or Store close if unused.
            if let Some(storage) = producer.take() {
                if !retained.uncertain() && !retained.error.uncertain() {
                    if busy(&retained.error) {
                        self.producer.keep(storage);
                    } else {
                        let _ = storage.release_kept_ordinals();
                    }
                }
            }
            return Err(self.settle_failure(retained));
        }
        if let Some(storage) = producer {
            self.producer.keep(storage);
        }
        Ok(CommitSuccess {
            history: retained.published,
            merge,
            saved: retained.saved.expect("known Save"),
            storage: retained.storage.expect("producer observations"),
            capture: retained.capture,
            namespace: None,
            replaced,
        })
    }

    /// Checks the candidate root as the next base of `target` and prepares
    /// its local install; reads only immutable objects. What `built` holds
    /// (the objects this Commit's construction built, which its Save has
    /// finished) is checked from those bytes; anything else is read through
    /// the Workspace's own reader, opened at bind (#95: a Commit opens no
    /// control reader for them). The checks are the same either way, and the
    /// install gets the ordinary client: the built set is not kept.
    pub(super) fn validate_candidate(
        &self,
        target: &BranchSnapshot,
        candidate: FilesystemRootId,
        capture: Capture,
        original_base: &layerfs_workspace::BaseView,
        built: Option<Arc<ConstructedObjects>>,
    ) -> Result<layerfs_workspace::PreparedBase, CommitError> {
        let mut next = target.clone();
        next.effective_root = candidate.0;
        let ports = self.store.ports_for(target.scope, self.identity);
        let ordinary = ports.client();
        // A private allowance: the built bytes reach the shared cache only
        // once the Commit is published (`admit_constructed`).
        let client = match built {
            Some(objects) => Arc::new(CanonicalClient::with_cache(
                Arc::new(super::captured::Built {
                    objects,
                    store: ordinary.clone(),
                }),
                Some(ports.clone()),
                Arc::new(CanonicalCache::new(VALIDATION_CACHE_BYTES)),
            )),
            None => ordinary.clone(),
        };
        let checked =
            checked_base(client.clone(), &next).map_err(|error| CommitError::Content {
                error,
                provider: ports
                    .failure()
                    .unwrap_or_else(|error| Some(Arc::new(error))),
            })?;
        if checked.root().root_inode() != original_base.root().root_inode() {
            return Err(CommitError::Context(
                "candidate changes root inode identity",
            ));
        }
        let prepared = self
            .workspace
            .scoped(client)
            .map_err(CommitError::Workspace)?
            .prepare_base_install(capture, candidate)
            .map_err(CommitError::Workspace)?;
        Ok(prepared.with_client(ordinary))
    }

    /// What a validation depends on: the target's head, base, root, scope
    /// and profile, and the candidate.
    fn target_key(target: &BranchSnapshot, candidate: FilesystemRootId) -> impl PartialEq + use<> {
        (
            target.branch.head_commit,
            target.branch.base_layer,
            target.effective_root,
            target.scope,
            target.profile,
            candidate,
        )
    }

    /// Logical size of `after`, from `before`'s known size and the change
    /// between them. File lengths come from the Store catalogue a window at
    /// a time (`Reader::file_lengths`), never from reading file payloads; a
    /// Storage refusal keeps its original error. The same walk notes in
    /// `directories` the directories whose listing changed, for the binding
    /// hints (`hints::record_directory_bindings`).
    pub(super) fn logical_after(
        &self,
        reader: &layerfs_storage::Reader<'_>,
        before: layerfs_content::ObjectId,
        after: FilesystemRootId,
        known: u64,
        directories: Option<&mut ChangedDirectories>,
    ) -> Result<u64, CommitError> {
        let mut refused = None;
        let counted = layerfs_content::filesystem::statistics::logical_size_and_directories(
            reader,
            FilesystemRootId(before),
            after,
            known,
            &mut |roots| {
                reader.file_lengths(roots).map_err(|error| {
                    refused = Some(error);
                    layerfs_content::ContentError::OutputRejected
                })
            },
            directories,
        );
        match (counted, refused) {
            (Ok(size), _) => Ok(size),
            (Err(_), Some(error)) => Err(CommitError::Storage(error)),
            (Err(error), None) => Err(CommitError::Content {
                error,
                provider: None,
            }),
        }
    }
}
