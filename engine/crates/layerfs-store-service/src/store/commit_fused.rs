//! A Workspace Commit over an unmoved head in one write transaction (#768
//! stage 2, `layerfs_persistence::Committer`): the Save's held last
//! publication, the new tree's binding hints and the Commit, all or nothing.
//! On PostgreSQL it is one pipelined message, one round trip.
//!
//! The head is not read before it: the Workspace's bound head is the
//! expected one, and the transaction asserts it. Only a Commit whose
//! construction kept its whole built set is fused: its validation reads its
//! own objects from those bytes, never from the Store, which does not hold
//! the held publication yet. Any other Commit (one without a built set, an
//! oversized one, one whose removals rebuilt the root) is not fused. The
//! Commit's own reads before the transaction (the logical count, the
//! binding hints) go through the Save's Storage handle, which serves the
//! held publication whole from the batch itself (`layerfs_storage`
//! `read/held.rs`). A definite race refusal
//! (the head moved, another writer stored one of the publication's objects,
//! the Commit's identity exists; nothing written) continues on the separate
//! path in the same call, once: the held publication is published on its
//! own (its lost identities compared with their winners, as a finish does),
//! the hints are recorded (both in phase Finish with no Commit intent: a
//! lost reply there folds the capture back as a failed finish does, and
//! never leaves the Workspace refusing Commits with nothing to resolve),
//! and today's path follows from its head read: a
//! moved head is merged with before the lane, in a Save of its own, then
//! the lane and, if the head moved again, an in-lane merge step. The same
//! message is never sent again, and an unknown outcome never continues:
//! the session is quarantined and custody kept. The pack ids the attempt
//! carried are published unchanged: its transaction wrote nothing, and the
//! ids stay reserved to this handle (a pack body does not hold its id).
use super::{
    commit_types::{CommitFailure, CommitPhase},
    BoundWorkspace, CommitError, CommitOptions, MergeReport,
};
use crate::telemetry::phase;
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::{
    BindingHint, BranchSnapshot, CommitMeta, CommitOutcome, CommitRequest, CommitVia, NodeId,
};
use layerfs_persistence::Fused;
use layerfs_storage::{PreparedTail, Storage, WriteOutcome};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::sync::atomic::{AtomicU64, Ordering};

/// Where a fused attempt that published no Commit continues: the head it
/// read and, after a merge with it, the merged root, its logical size, the
/// merge and the serials it changed against the attempt's root.
pub(super) struct Reread {
    pub(super) current: BranchSnapshot,
    pub(super) root: FilesystemRootId,
    pub(super) logical: u64,
    pub(super) merge: Option<MergeReport>,
    pub(super) changed: Vec<u64>,
}

/// Adds a merge Save's counts to the Commit's: its objects, records, packs
/// and bytes.
pub(super) fn add_saved(total: &mut WriteOutcome, saved: &WriteOutcome) {
    total.inserted += saved.inserted;
    total.reused += saved.reused;
    total.full_records += saved.full_records;
    total.prefix_records += saved.prefix_records;
    total.packs += saved.packs;
    total.canonical_bytes += saved.canonical_bytes;
    total.stored_bytes += saved.stored_bytes;
    total.metadata_bytes += saved.metadata_bytes;
}

/// Fused Commits made, and fused attempts a race sent on the separate path.
#[derive(Default)]
pub(crate) struct FusedCounts {
    committed: AtomicU64,
    fallbacks: AtomicU64,
}

impl super::Store {
    /// `(fused Commits made, race refusals that continued on the separate
    /// path)` since the Store opened.
    pub fn fused_counts(&self) -> (u64, u64) {
        (
            self.fused.committed.load(Ordering::Relaxed),
            self.fused.fallbacks.load(Ordering::Relaxed),
        )
    }
}

/// The publication request of one Commit of `snapshot`'s Workspace over
/// `target`, as the separate path's `commit_branch` and the fused
/// transaction both receive it.
#[allow(clippy::too_many_arguments)]
pub(super) fn commit_request(
    snapshot: &BranchSnapshot,
    target: &BranchSnapshot,
    candidate: FilesystemRootId,
    logical: u64,
    saved: &WriteOutcome,
    merge: Option<&MergeReport>,
    options: &CommitOptions,
    via: CommitVia,
) -> CommitRequest {
    CommitRequest {
        branch: snapshot.branch.id,
        expected_head: target.branch.head_commit,
        expected_base: target.branch.base_layer,
        expected_root: target.effective_root,
        candidate_root: candidate.0,
        profile: snapshot.profile,
        scope: snapshot.scope,
        second_parent: None,
        meta: CommitMeta {
            message: Some(
                options
                    .message
                    .clone()
                    .unwrap_or_else(|| "Commit Workspace changes".into()),
            ),
            statistics: layerfs_history::CommitStatistics {
                logical,
                new_canonical: saved.canonical_bytes,
                new_stored: saved.stored_bytes,
                new_metadata: saved.metadata_bytes,
            },
            merged_from: merge.map(|_| {
                snapshot
                    .branch
                    .head_commit
                    .map(NodeId::Commit)
                    .unwrap_or(NodeId::Layer(snapshot.branch.base_layer))
            }),
            lww: merge.map_or_else(Vec::new, |report| report.lww.clone()),
            ..CommitMeta::new(via)
        },
        // The transaction reads the head in the lane: forced over whatever
        // head it finds there, unless `expected`.
        force: options.force,
        required_head: options.expected,
    }
}

impl BoundWorkspace {
    /// Whether this Commit is made fused: the Store has a committer, the
    /// Commit is not forced (a forced Commit reads the head it displaces in
    /// the lane) and the head it must be made over, if any, is the bound one.
    pub(super) fn fused_eligible(
        &self,
        snapshot: &BranchSnapshot,
        options: &CommitOptions,
    ) -> bool {
        let bound = snapshot
            .branch
            .head_commit
            .map_or(NodeId::Layer(snapshot.branch.base_layer), NodeId::Commit);
        self.store.committer.is_some()
            && !options.force
            && options.expected.is_none_or(|node| node == bound)
    }

    /// After a fused attempt that published no Commit: the head read and,
    /// when it moved, the merge of today's path, before the lane. The merge
    /// has a Save of its own (the attempt's Save is finished: its objects
    /// are published), and its logical count and hints cover what it
    /// changed against `root`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reread_after_fused(
        &self,
        storage: &Storage,
        snapshot: &BranchSnapshot,
        root: FilesystemRootId,
        logical: u64,
        options: &CommitOptions,
        retained: &mut CommitFailure,
        timing: &TimingScope<'_, Active>,
    ) -> Result<Reread, CommitError> {
        retained.phase = CommitPhase::Construct;
        let current = phase(timing, "head", || {
            self.store
                .history
                .branch_snapshot(snapshot.branch.id)
                .map_err(CommitError::History)?
                .ok_or(CommitError::Context("bound Branch is absent"))
        })?;
        if let Some(expected) = options.expected {
            if super::commit::answered(&current) != expected {
                return Err(super::commit::expected_moved(expected, &current));
            }
        }
        let moved = current.branch.head_commit != snapshot.branch.head_commit
            || current.branch.base_layer != snapshot.branch.base_layer;
        if !moved || options.force {
            return Ok(Reread {
                current,
                root,
                logical,
                merge: None,
                changed: Vec::new(),
            });
        }
        let save = storage.begin_save().map_err(CommitError::Storage)?;
        let (merged, report) = phase(timing, "merge", || {
            self.merge_candidate(&save, snapshot, &current, root)
        })?;
        let changed = phase(timing, "merge.changed", || {
            super::live::changed_serials(&save, root, merged)
        })?;
        retained.phase = CommitPhase::Finish;
        let saved = phase(timing, "finish", || {
            save.finish().map_err(CommitError::Storage)
        })?;
        self.store.note_save(&saved);
        if let Some(total) = retained.saved.as_mut() {
            add_saved(total, &saved);
        }
        let reader = storage.reader_after_save().map_err(CommitError::Storage)?;
        let logical = phase(timing, "logical.count", || {
            self.logical_after(&reader, root.0, merged, logical)
        })?;
        phase(timing, "bindings", || {
            super::hints::record_new_bindings(&reader, &*self.store.history, Some(root), merged)
        })?;
        Ok(Reread {
            current,
            root: merged,
            logical,
            merge: Some(report),
            changed,
        })
    }

    /// Publishes `tail`, `hints` (`None`: already recorded) and the Commit
    /// of `candidate` over the bound `snapshot` in one transaction. `Some`:
    /// published, the Save completed (`retained.saved`). `None`: a race
    /// refusal, after which `tail` was published and the hints recorded on
    /// their own ([`Self::publish_apart`]), and the caller continues on the
    /// separate path. Without a `tail` the Save was published on its own
    /// already (`retained.saved`; its new bindings passed one hint
    /// transaction and were recorded after it) and the Commit is attempted
    /// alone.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn publish_fused(
        &self,
        tail: Option<PreparedTail<'_>>,
        snapshot: &BranchSnapshot,
        candidate: FilesystemRootId,
        logical: u64,
        hints: Option<Vec<BindingHint>>,
        options: &CommitOptions,
        via: CommitVia,
        retained: &mut CommitFailure,
        timing: &TimingScope<'_, Active>,
    ) -> Result<Option<CommitOutcome>, CommitError> {
        let committer = self
            .store
            .committer
            .as_ref()
            .ok_or(CommitError::Context("fused Commit without a committer"))?;
        // Nothing changed against the bound root: not a fused Commit (the
        // separate path answers UpToDate).
        if candidate.0 == snapshot.effective_root {
            self.publish_apart(tail, hints, retained)?;
            return Ok(None);
        }
        let mut saved = match &tail {
            Some(_) => None,
            None => Some(
                retained
                    .saved
                    .ok_or(CommitError::Context("finished Save"))?,
            ),
        };
        let mut tail = tail;
        let mut hints = hints;
        let stack = snapshot.branch.stack;
        let attempt = loop {
            let outcome = match (&saved, &tail) {
                (Some(saved), _) => *saved,
                (None, Some(tail)) => tail.prospective(),
                (None, None) => return Err(CommitError::Context("fused Commit without a Save")),
            };
            let request = commit_request(
                snapshot, snapshot, candidate, logical, &outcome, None, options, via,
            );
            retained.phase = CommitPhase::Publish;
            retained.intent = Some(request.clone());
            let publication = tail.as_ref().and_then(|tail| tail.publication());
            let rows = hints.as_deref().unwrap_or(&[]);
            let attempt = phase(timing, "publish.fused", || {
                committer.commit(publication, stack, &request, rows)
            });
            // Too large for one message: the tail and the hints go first,
            // each on its own as the separate path sends them, then the
            // Commit alone.
            if matches!(attempt, Err(Fused::Overflow)) && saved.is_none() {
                saved = self.publish_apart(tail.take(), hints.take(), retained)?;
                continue;
            }
            break attempt;
        };
        match attempt {
            Ok(outcome) => {
                retained.published = Some(outcome.clone());
                if let Some(tail) = tail {
                    // The Commit is known published; what is left is local:
                    // the Save's settlement, then the install. A failure
                    // there keeps the custody of a known publication whose
                    // install is still to come, which the next Commit's
                    // resolution installs (`resolve.rs`), as after a failed
                    // install.
                    retained.phase = CommitPhase::Install;
                    let saved = tail.acknowledged().map_err(CommitError::Storage)?;
                    self.settle_saved(saved, retained);
                    retained.phase = CommitPhase::Publish;
                }
                self.store.fused.committed.fetch_add(1, Ordering::Relaxed);
                Ok(Some(outcome))
            }
            Err(Fused::Race) => {
                self.store.fused.fallbacks.fetch_add(1, Ordering::Relaxed);
                self.publish_apart(tail, hints, retained)?;
                Ok(None)
            }
            Err(error) => {
                // A definite refusal rolled the whole transaction back: the
                // Save's unused ids go back to its handle, whose owner
                // returns the ordinals with the failed Commit (`commit.rs`).
                // After an unknown outcome the tail is dropped and the block
                // lost.
                let definite = !matches!(&error, Fused::History(error) if error.unknown());
                if let (true, Some(tail)) = (definite, tail) {
                    tail.refused();
                }
                Err(match error {
                    Fused::History(error) => CommitError::History(error),
                    _ => CommitError::Context("fused Commit bounds"),
                })
            }
        }
    }

    /// Publishes the held `tail` (`None`: already published) on its own,
    /// as `finish` does, and records `hints` (`None`: already recorded), as
    /// the separate path's finish and binding phases do: in phase Finish,
    /// with no Commit intent outstanding. No Commit was made or is pending,
    /// so a lost reply here is an unknown Storage or hint outcome before any
    /// publication and folds the capture back as a failed `finish` does; it
    /// never leaves the Workspace refusing later Commits with nothing to
    /// resolve. Returns the published Save's outcome, if published here.
    fn publish_apart(
        &self,
        tail: Option<PreparedTail<'_>>,
        hints: Option<Vec<BindingHint>>,
        retained: &mut CommitFailure,
    ) -> Result<Option<WriteOutcome>, CommitError> {
        retained.phase = CommitPhase::Finish;
        retained.intent = None;
        let saved = match tail {
            Some(tail) => {
                let saved = tail.publish().map_err(CommitError::Storage)?;
                self.settle_saved(saved, retained);
                Some(saved)
            }
            None => None,
        };
        if let Some(rows) = hints {
            super::hints::record_rows(&*self.store.history, &rows)?;
        }
        Ok(saved)
    }

    fn settle_saved(&self, saved: WriteOutcome, retained: &mut CommitFailure) {
        self.store.note_save(&saved);
        retained.saved = Some(saved);
    }
}
