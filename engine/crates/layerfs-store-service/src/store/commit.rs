//! One synchronous Content/Save/history producer and exact paired local install.
use super::{
    bind::checked_base, BoundWorkspace, CommitError, CommitFailure, CommitOptions, CommitPhase,
    CommitSuccess, MergeReport,
};
use crate::telemetry::phase;
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::{BranchSnapshot, CommitMeta, CommitOutcome, CommitRequest, CommitVia};
use layerfs_overlay::Capture;
use layerfs_storage::Save;
use layerfs_telemetry::timer::{Active, TimingScope};
use layerfs_workspace::WorkspaceError;
use std::sync::{atomic::Ordering, Arc};

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
    pub(super) fn commit_admitted(
        &self,
        construct: impl FnOnce(
            &Save<'_>,
            Capture,
            &BranchSnapshot,
        ) -> Result<FilesystemRootId, CommitError>,
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
                    retained.published = Some(CommitOutcome::UpToDate {
                        head: current.branch.head_commit,
                        root: current.effective_root,
                    });
                    retained.saved = Some(Default::default());
                    retained.storage = Some(Default::default());
                    return Ok(());
                }
            }
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
                // Two-stage merge (#355). The head is read and merged with
                // before the publication lane, so the merge's records and its
                // whole build stay outside it. In the lane the head is read
                // again: if it moved meanwhile, only what landed in between
                // is merged into the result, in memory, then published.
                let current = phase(timing, "head", || {
                    self.refuse_markers(&save, candidate)?;
                    self.store
                        .history
                        .branch_snapshot(snapshot.branch.id)
                        .map_err(CommitError::History)?
                        .ok_or(CommitError::Context("bound Branch is absent"))
                })?;
                let moved = current.branch.head_commit != snapshot.branch.head_commit
                    || current.branch.base_layer != snapshot.branch.base_layer;
                let root = if moved && !force {
                    let (root, report) = phase(timing, "merge", || {
                        self.merge_candidate(&save, &snapshot, &current, candidate)
                    })?;
                    merge = Some(report);
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
                    self.store.logical_size(node).map_err(CommitError::History)
                })?;
                retained.phase = CommitPhase::Finish;
                let saved = phase(timing, "finish", || {
                    save.finish().map_err(CommitError::Storage)
                })?;
                self.store.note_save(&saved);
                retained.saved = Some(saved);
                // Count only after finish has proved the root's references
                // closed, so a missing dependency stays a Storage refusal.
                let logical = phase(timing, "logical.count", || {
                    self.logical_after(&storage, current.effective_root, root, known)
                })?;
                phase(timing, "bindings", || {
                    let reader = storage.reader().map_err(CommitError::Storage)?;
                    super::hints::record_new_bindings(
                        &reader,
                        &*self.store.history,
                        Some(FilesystemRootId(current.effective_root)),
                        root,
                    )
                })?;
                let lane = phase(timing, "lane", || {
                    self.store
                        .history
                        .publish_lane()
                        .map_err(CommitError::History)
                })?;
                let conflicted = merge
                    .as_ref()
                    .is_some_and(|report: &MergeReport| !report.conflicts.is_empty());
                let (current, root, moved, logical) = if force || conflicted {
                    (current, root, moved, logical)
                } else {
                    let now = phase(timing, "head.lane", || {
                        self.store
                            .history
                            .branch_snapshot(snapshot.branch.id)
                            .map_err(CommitError::History)?
                            .ok_or(CommitError::Context("bound Branch is absent"))
                    })?;
                    if now.branch.head_commit == current.branch.head_commit
                        && now.branch.base_layer == current.branch.base_layer
                    {
                        (current, root, moved, logical)
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
                            saved.canonical_bytes += step_saved.canonical_bytes;
                            saved.stored_bytes += step_saved.stored_bytes;
                            saved.metadata_bytes += step_saved.metadata_bytes;
                        }
                        let logical = phase(timing, "logical.lane", || {
                            self.logical_after(&storage, root.0, stepped, logical)
                        })?;
                        phase(timing, "bindings.lane", || {
                            let reader = storage.reader().map_err(CommitError::Storage)?;
                            super::hints::record_new_bindings(
                                &reader,
                                &*self.store.history,
                                Some(root),
                                stepped,
                            )
                        })?;
                        merge = Some(match merge.take() {
                            Some(first) => MergeReport {
                                from: first.from,
                                lww: first.lww.into_iter().chain(step.lww).collect(),
                                ..step
                            },
                            None => step,
                        });
                        (now, stepped, true, logical)
                    }
                };
                Ok((root, candidate, current, moved, logical, lane))
            })();
            retained.storage = Some(super::producer::since(storage.diagnostics(), before));
            let (candidate, ours, current, moved, logical, lane) = built?;
            if candidate != ours {
                replaced = Some((ours.0, candidate.0));
            }
            producer = Some(storage);
            let conflicted = merge
                .as_ref()
                .is_some_and(|report: &MergeReport| !report.conflicts.is_empty());
            // A merge is published against the head it merged with; a forced
            // or unmoved Commit against the head the Workspace bound.
            let target = if merge.is_some() { &current } else { &snapshot };
            if !conflicted {
                retained.intent = Some(CommitRequest {
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
                        statistics: {
                            let saved = retained
                                .saved
                                .as_ref()
                                .ok_or(CommitError::Context("finished Save"))?;
                            layerfs_history::CommitStatistics {
                                logical,
                                new_canonical: saved.canonical_bytes,
                                new_stored: saved.stored_bytes,
                                new_metadata: saved.metadata_bytes,
                            }
                        },
                        merged_from: merge.as_ref().map(|_| {
                            snapshot
                                .branch
                                .head_commit
                                .map(layerfs_history::NodeId::Commit)
                                .unwrap_or(layerfs_history::NodeId::Layer(
                                    snapshot.branch.base_layer,
                                ))
                        }),
                        lww: merge
                            .as_ref()
                            .map_or_else(Vec::new, |report| report.lww.clone()),
                        ..CommitMeta::new(via)
                    },
                    force: force && moved,
                });
            }
            retained.phase = CommitPhase::Validate;
            let mut next = target.clone();
            next.effective_root = candidate.0;
            retained.prepared = Some(phase(timing, "validate", || {
                let ports = self.store.ports(snapshot.scope);
                let client = ports.client();
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
                self.workspace
                    .scoped(client)
                    .map_err(CommitError::Workspace)?
                    .prepare_base_install(capture, candidate)
                    .map_err(CommitError::Workspace)
            })?);
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
                let outcome = phase(timing, "publish", || {
                    self.store
                        .history
                        .commit_branch(intent)
                        .map_err(CommitError::History)
                })?;
                let (head, root) = match &outcome {
                    CommitOutcome::Committed { record, .. } => (Some(record.id), record.root),
                    CommitOutcome::UpToDate { head, root } => (*head, *root),
                };
                drop(lane);
                retained.published = Some(outcome);
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

    /// Logical size of `after`, from `before`'s known size and the change
    /// between them. File lengths come from the Store catalogue a window at
    /// a time (`Reader::file_lengths`), never from reading file payloads; a
    /// Storage refusal keeps its original error.
    fn logical_after(
        &self,
        storage: &layerfs_storage::Storage,
        before: layerfs_content::ObjectId,
        after: FilesystemRootId,
        known: u64,
    ) -> Result<u64, CommitError> {
        let reader = storage.reader().map_err(CommitError::Storage)?;
        let mut refused = None;
        let counted = layerfs_content::filesystem::statistics::logical_size_after_with(
            &reader,
            FilesystemRootId(before),
            after,
            known,
            &mut |roots| {
                reader.file_lengths(roots).map_err(|error| {
                    refused = Some(error);
                    layerfs_content::ContentError::OutputRejected
                })
            },
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
