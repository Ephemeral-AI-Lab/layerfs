//! One exact resolution of retained Commit custody, never a retry.
//!
//! A Commit whose publication answered with an unknown outcome, or whose
//! install failed after a known publication, keeps its capture and its
//! captured reader and operation owner, and the Workspace refuses later
//! Commits. Its record stays here. The Workspace's next Commit request runs
//! [`BoundWorkspace::resolve_retained`] once before its own admission:
//!
//! - publication known, or the head and base unmoved with our candidate root
//!   (an UpToDate publication, which writes no Commit), or the Commit our
//!   attempt published found by that attempt: install that root as the new
//!   base;
//! - no Commit of our attempt while the Branch exists: nothing was
//!   published, so the capture folds back;
//! - anything else (an absent Branch, an intent without an attempt, a failed
//!   read, a lane that does not pass): unresolved, and the custody and its
//!   record stay.
//!
//! Every Commit request carries a fresh attempt (`CommitMeta::attempt`,
//! #776 R1). It is part of the Commit's identity and unique in the Store, so
//! one indexed read answers what a history walk could not: whatever moved
//! the head since (later Commits, a forced Commit, a fast-forward), our
//! Commit is found if and only if it was published and is still stored.
//!
//! Before the history is read the attempt passes the Store's publication
//! lane (`pass_publication_lane`): a lost PostgreSQL `COMMIT` may still be
//! applying on the server, and its row lock or barrier key holds until it
//! ends (#776 C2). A fused Commit inserts its packs before it takes the lane
//! row, so on PostgreSQL the pass first ends the quarantined writer's
//! backend and waits until it is gone (bounded): a `COMMIT` it made is then
//! found by the attempt, and one it had not made is aborted; not proven
//! gone, the outcome stays undetermined. Nothing is resent: the history is read through the control
//! reader (a connection of its own, unaffected by a writer session the
//! unknown outcome quarantined), and the install or fold-back runs once. A
//! record whose publication outcome this attempt could not determine is put
//! back for the next request, whose attempt only reads again. A known
//! publication whose local install failed definitely (nothing installed: a
//! failed read of the candidate's objects, a refused live check, an install
//! job that failed with no uncertain effect) is put back too, now with the
//! publication known, so the next request installs it without reading the
//! history again; taken, the Workspace would refuse every later Commit
//! (`InFlight`). An install whose local effect is uncertain, one that
//! installed, and a fold-back are taken by the attempt whatever follows.
use crate::store::{BoundWorkspace, CapturedConstruction, CommitFailure, CommitPhase};
use layerfs_content::{filesystem::FilesystemRootId, ObjectId};
use layerfs_history::{
    BranchSnapshot, CommitOutcome, CommitRecord, CommitRequest, HistoryCatalog, HistoryError,
};
use layerfs_overlay::{Capture, CapturedReader, OperationOwner};
use layerfs_vfs::WorkspaceError;
use std::sync::{atomic::Ordering, Arc};

/// A failed install of a known publication, and whether it was definite
/// (nothing installed locally).
type Failed = (Box<Unresolved>, bool);

/// What a retained Commit needs for its one resolution.
#[derive(Debug)]
pub(super) struct RetainedCommit {
    capture: Capture,
    intent: CommitRequest,
    /// The binding the Commit started from: Branch identity, scope, profile.
    binding: BranchSnapshot,
    published: Option<CommitOutcome>,
    merged_over: Option<(FilesystemRootId, Arc<[u64]>)>,
    reader: Option<CapturedReader>,
    operation: Option<OperationOwner>,
}

/// A resolved custody. `outcome` is the publication installed, `None` when
/// nothing was published and the capture folded back. `replaced` is what the
/// Workspace showed and the merged root installed under it, for the kernel's
/// invalidation, as a Commit's own `replaced`.
#[derive(Debug)]
pub struct Resolution {
    pub outcome: Option<CommitOutcome>,
    pub replaced: Option<(ObjectId, ObjectId)>,
}

/// The attempt did not resolve: the custody stays, with what it now knows.
#[derive(Debug)]
pub struct Unresolved {
    pub published: Option<CommitOutcome>,
    pub reason: String,
}

impl CommitFailure {
    /// Whether one read of the head and one local step can settle this
    /// failure exactly: an unanswered publication, or an install that failed
    /// definitely after a known one. Local uncertainty never qualifies.
    fn resolvable(&self) -> bool {
        self.capture.is_some()
            && self.intent.is_some()
            && self.binding.is_some()
            && !self.error.locally_uncertain()
            && match self.phase {
                CommitPhase::Publish => self.published.is_none() && self.error.uncertain(),
                CommitPhase::Install => self.published.is_some(),
                _ => false,
            }
    }
}

impl BoundWorkspace {
    /// Keeps what a resolution needs when `failure` retained custody and
    /// qualifies; the failure itself still goes to the caller.
    pub(super) fn retain(&self, failure: &CommitFailure, held: Option<&CapturedConstruction>) {
        if failure.locally_settled
            || !failure.resolvable()
            || held.is_some_and(|held| held.custody.is_some() || held.release_error.is_some())
        {
            return;
        }
        let (Some(capture), Some(intent), Some(binding)) = (
            failure.capture,
            failure.intent.clone(),
            failure.binding.clone(),
        ) else {
            return;
        };
        if let Ok(mut slot) = self.retained.lock() {
            *slot = Some(RetainedCommit {
                capture,
                intent,
                binding,
                published: failure.published.clone(),
                merged_over: failure.merged_over.clone(),
                reader: held.and_then(|held| held.reader),
                operation: held.and_then(|held| held.operation),
            });
        }
    }
    /// True while a retained Commit waits for its resolution.
    pub fn has_retained(&self) -> bool {
        self.retained.lock().is_ok_and(|slot| slot.is_some())
    }
    /// The one resolution attempt (module docs). `Ok(None)`: nothing is
    /// retained here. On `Ok(Some)` the custody is released and the next
    /// Commit may be admitted.
    pub fn resolve_retained(&self) -> Result<Option<Resolution>, Box<Unresolved>> {
        layerfs_persistence::one_call(|| self.resolve_once())
    }
    fn resolve_once(&self) -> Result<Option<Resolution>, Box<Unresolved>> {
        let kept = self
            .retained
            .lock()
            .map_err(|_| unresolved(None, "retained Commit record poisoned".into()))?
            .take();
        let Some(kept) = kept else { return Ok(None) };
        let outcome = match kept.published.clone() {
            Some(outcome) => Some(outcome),
            None => match self.read_outcome(&kept.intent) {
                Ok(outcome) => outcome,
                Err(unresolved) => {
                    // Undetermined: the custody stays, and so does the record,
                    // so the next request can read again; it never resends.
                    if let Ok(mut slot) = self.retained.lock() {
                        *slot = Some(kept);
                    }
                    return Err(unresolved);
                }
            },
        };
        let replaced = match outcome {
            Some(ref published) => match self.install_retained(&kept, published) {
                Ok(replaced) => replaced,
                Err((unresolved, definite)) => {
                    if definite {
                        if let Ok(mut slot) = self.retained.lock() {
                            *slot = Some(RetainedCommit {
                                published: Some(published.clone()),
                                ..kept
                            });
                        }
                    }
                    return Err(unresolved);
                }
            },
            None => {
                self.job(|client: &crate::OwnerClient, route| {
                    client.resolve_failed(route, kept.capture)
                })
                .map_err(|error| {
                    unresolved(
                        None,
                        format!("nothing was published; fold back failed: {error:?}"),
                    )
                })?;
                None
            }
        };
        let mut held = CapturedConstruction {
            reader: kept.reader,
            operation: kept.operation,
            ..Default::default()
        };
        self.release_captured(&mut held);
        if held.retained() {
            return Err(unresolved(
                outcome,
                format!(
                    "resolved, but a captured owner was not released: {:?}",
                    held.release_error
                ),
            ));
        }
        self.committing.store(false, Ordering::Release);
        Ok(Some(Resolution { outcome, replaced }))
    }
    /// The outcome of the retained publication, read through the control
    /// reader once the publication lane has passed. `Some`: our Commit,
    /// found by its attempt (or, with the head unmoved, a head that already
    /// has our root); `None`: proven unpublished.
    fn read_outcome(
        &self,
        intent: &CommitRequest,
    ) -> Result<Option<CommitOutcome>, Box<Unresolved>> {
        self.store
            .history
            .pass_publication_lane()
            .map_err(|error| {
                unresolved(
                    None,
                    format!("the original publication may still be running: {error:?}"),
                )
            })?;
        self.store
            .published(|history| Ok(judge(history, intent)))
            .map_err(|error| unresolved(None, format!("history read failed: {error:?}")))?
    }
    /// Installs a known publication's root, under the same live check and
    /// mutation hold as a Commit's own merged install. A failure says whether
    /// it was definite: nothing was installed locally.
    fn install_retained(
        &self,
        kept: &RetainedCommit,
        outcome: &CommitOutcome,
    ) -> Result<Option<(ObjectId, ObjectId)>, Failed> {
        let known = || Some(outcome.clone());
        let failed = |reason: String| (unresolved(known(), reason), true);
        let candidate = FilesystemRootId(kept.intent.candidate_root);
        let mut target = kept.binding.clone();
        target.branch.head_commit = kept.intent.expected_head;
        target.branch.base_layer = kept.intent.expected_base;
        target.effective_root = kept.intent.expected_root;
        let base = self
            .workspace
            .base()
            .map_err(|error| failed(format!("base: {error}")))?;
        let mut prepared = self
            .validate_candidate(&target, candidate, kept.capture, &base, None)
            .map_err(|error| failed(format!("validate: {error:?}")))?;
        let held = match &kept.merged_over {
            Some((ours, changed)) => {
                let (hold, overlap) = self
                    .hold_live(changed)
                    .map_err(|error| failed(format!("live check: {error:?}")))?;
                if !overlap.is_empty() {
                    let conflict = self.live_conflict(target.scope, *ours, candidate, &overlap);
                    return Err(failed(format!(
                        "published; rows written since overlap the merge: {conflict:?}"
                    )));
                }
                prepared = prepared.merged(changed.clone());
                Some(hold)
            }
            None => None,
        };
        self.job(|client: &crate::OwnerClient, route| {
            client.install_prepared(route, self.workspace.clone(), prepared)
        })
        .map_err(|error| {
            let definite = !crate::store::owner_uncertain(&error);
            (unresolved(known(), format!("install: {error:?}")), definite)
        })?;
        drop(held);
        let (head, root) = match outcome {
            CommitOutcome::Committed { record, .. } => (Some(record.id), record.root),
            CommitOutcome::UpToDate { head, root } => (*head, *root),
        };
        let mut next = target;
        next.effective_root = candidate.0;
        next.branch.head_commit = head;
        next.head_root = head.map(|_| root);
        // Installed: a poisoned binding after it is not definite.
        let poisoned = || {
            let reason = format!("{:?}", WorkspaceError::BindingPoisoned);
            (unresolved(known(), reason), false)
        };
        *self.snapshot.lock().map_err(|_| poisoned())? = next;
        *self.pending.lock().map_err(|_| poisoned())? = None;
        Ok(kept
            .merged_over
            .as_ref()
            .map(|(ours, _)| (ours.0, candidate.0)))
    }
}

/// Judges the retained publication from published history (#776 R1).
///
/// With the head and base unmoved and our candidate root the publication
/// answered UpToDate, which writes no Commit. Otherwise the Commit our attempt
/// published is looked up by that attempt. Found, it is ours, published,
/// however the head moved since. Not found while the Branch exists, nothing
/// was published: the Commit's insert and the Branch's advance are one
/// transaction, which the lane pass proved ended; and a Commit that was ever
/// the head of a live Branch stays in the GC mark's closure from it (a later
/// Commit's first parent, a forced Commit's displaced head, a fast-forward's
/// ancestry through parents and base Layers), so no GC removed it. With the
/// Branch absent, or an intent that carries no attempt, the outcome is left
/// undetermined.
fn judge(
    history: &dyn HistoryCatalog,
    intent: &CommitRequest,
) -> Result<Option<CommitOutcome>, Box<Unresolved>> {
    let read = |error: HistoryError| unresolved(None, format!("history read failed: {error:?}"));
    let head = history.branch_snapshot(intent.branch).map_err(read)?;
    if let Some(head) = &head {
        if head.branch.base_layer == intent.expected_base
            && head.branch.head_commit == intent.expected_head
            && head.effective_root == intent.candidate_root
        {
            return Ok(Some(CommitOutcome::UpToDate {
                head: head.branch.head_commit,
                root: head.effective_root,
            }));
        }
    }
    let attempt = intent
        .meta
        .attempt
        .ok_or_else(|| unresolved(None, "the publication carried no attempt".into()))?;
    match history.commit_by_attempt(attempt).map_err(read)? {
        Some(record) if ours(&record, intent) => Ok(Some(committed(record))),
        Some(record) => Err(unresolved(
            None,
            format!(
                "the attempt names a Commit other than ours: {:?}",
                record.id
            ),
        )),
        None if head.is_some() => Ok(None),
        None => Err(unresolved(
            None,
            "no Commit of this attempt, and the Branch is absent".into(),
        )),
    }
}

fn committed(record: CommitRecord) -> CommitOutcome {
    CommitOutcome::Committed {
        displaced: record.meta.displaced,
        record: Box::new(record),
    }
}

/// Our Commit: our candidate root, published over the head we expected (a
/// Commit found by our attempt always is; anything else is an integrity
/// failure, left undetermined).
fn ours(record: &CommitRecord, intent: &CommitRequest) -> bool {
    record.root == intent.candidate_root
        && record.parent == intent.expected_head
        && record.base_layer == intent.expected_base
}

fn unresolved(published: Option<CommitOutcome>, reason: String) -> Box<Unresolved> {
    Box::new(Unresolved { published, reason })
}
