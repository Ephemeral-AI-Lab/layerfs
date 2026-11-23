//! One exact resolution of retained Commit custody, never a retry.
//!
//! A Commit whose publication answered with an unknown outcome, or whose
//! install failed after a known publication, keeps its capture and its
//! captured reader and operation owner, and the Workspace refuses later
//! Commits. Its record stays here. The Workspace's next Commit request runs
//! [`BoundWorkspace::resolve_retained`] once before its own admission:
//!
//! - publication known, or the head now our Commit (our candidate root, over
//!   the head we published against): install that root as the new base;
//! - the head still the one we published against, with another root: nothing
//!   was published, so the capture folds back;
//! - any other head: unresolved, and the custody stays.
//!
//! Nothing is resent: the head is read once, through the control reader (a
//! connection of its own, unaffected by a writer session the unknown outcome
//! quarantined), and the install or fold-back runs once. The record is taken
//! by the attempt whatever it finds.
use super::{BoundWorkspace, CapturedConstruction, CommitFailure, CommitPhase};
use layerfs_content::{filesystem::FilesystemRootId, ObjectId};
use layerfs_history::{BranchSnapshot, CommitOutcome, CommitRecord, CommitRequest};
use layerfs_overlay::{Capture, CapturedReader, OperationOwner};
use layerfs_workspace::WorkspaceError;
use std::sync::{atomic::Ordering, Arc};

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
        let kept = self
            .retained
            .lock()
            .map_err(|_| unresolved(None, "retained Commit record poisoned".into()))?
            .take();
        let Some(kept) = kept else { return Ok(None) };
        let outcome = match kept.published.clone() {
            Some(outcome) => Some(outcome),
            None => self.read_outcome(&kept.intent)?,
        };
        let replaced = match outcome {
            Some(ref published) => self.install_retained(&kept, published)?,
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
    /// One read of the head through the control reader. `Some`: the head is
    /// our Commit (or, unmoved, already has our root); `None`: the head is
    /// unmoved with another root, so nothing was published.
    fn read_outcome(
        &self,
        intent: &CommitRequest,
    ) -> Result<Option<CommitOutcome>, Box<Unresolved>> {
        let (head, record) = self
            .store
            .published(|history| {
                let head = history.branch_snapshot(intent.branch)?;
                let record = match head.as_ref().and_then(|head| head.branch.head_commit) {
                    Some(id) => history.commit(id)?,
                    None => None,
                };
                Ok((head, record))
            })
            .map_err(|error| unresolved(None, format!("head read failed: {error:?}")))?;
        let head = head.ok_or_else(|| unresolved(None, "the Branch is absent".into()))?;
        let unmoved = head.branch.head_commit == intent.expected_head
            && head.branch.base_layer == intent.expected_base;
        if unmoved {
            return Ok((head.effective_root == intent.candidate_root).then_some(
                CommitOutcome::UpToDate {
                    head: head.branch.head_commit,
                    root: head.effective_root,
                },
            ));
        }
        match record {
            Some(record) if ours(&record, intent) => Ok(Some(CommitOutcome::Committed {
                displaced: record.meta.displaced,
                record: Box::new(record),
            })),
            _ => Err(unresolved(
                None,
                format!(
                    "the head is {:?}: neither this Commit nor the head it was published against",
                    head.branch.head_commit
                ),
            )),
        }
    }
    /// Installs a known publication's root, under the same live check and
    /// mutation hold as a Commit's own merged install.
    fn install_retained(
        &self,
        kept: &RetainedCommit,
        outcome: &CommitOutcome,
    ) -> Result<Option<(ObjectId, ObjectId)>, Box<Unresolved>> {
        let known = || Some(outcome.clone());
        let candidate = FilesystemRootId(kept.intent.candidate_root);
        let mut target = kept.binding.clone();
        target.branch.head_commit = kept.intent.expected_head;
        target.branch.base_layer = kept.intent.expected_base;
        target.effective_root = kept.intent.expected_root;
        let base = self
            .workspace
            .base()
            .map_err(|error| unresolved(known(), format!("base: {error}")))?;
        let mut prepared = self
            .validate_candidate(&target, candidate, kept.capture, &base)
            .map_err(|error| unresolved(known(), format!("validate: {error:?}")))?;
        let held = match &kept.merged_over {
            Some((ours, changed)) => {
                let (hold, overlap) = self
                    .hold_live(changed)
                    .map_err(|error| unresolved(known(), format!("live check: {error:?}")))?;
                if !overlap.is_empty() {
                    let conflict = self.live_conflict(target.scope, *ours, candidate, &overlap);
                    return Err(unresolved(
                        known(),
                        format!("published; rows written since overlap the merge: {conflict:?}"),
                    ));
                }
                prepared = prepared.merged(changed.clone());
                Some(hold)
            }
            None => None,
        };
        self.job(|client: &crate::OwnerClient, route| {
            client.install_prepared(route, self.workspace.clone(), prepared)
        })
        .map_err(|error| unresolved(known(), format!("install: {error:?}")))?;
        drop(held);
        let (head, root) = match outcome {
            CommitOutcome::Committed { record, .. } => (Some(record.id), record.root),
            CommitOutcome::UpToDate { head, root } => (*head, *root),
        };
        let mut next = target;
        next.effective_root = candidate.0;
        next.branch.head_commit = head;
        next.head_root = head.map(|_| root);
        let poisoned = || unresolved(known(), format!("{:?}", WorkspaceError::BindingPoisoned));
        *self.snapshot.lock().map_err(|_| poisoned())? = next;
        *self.pending.lock().map_err(|_| poisoned())? = None;
        Ok(kept
            .merged_over
            .as_ref()
            .map(|(ours, _)| (ours.0, candidate.0)))
    }
}

/// Our Commit: our candidate root, published over the head we expected.
fn ours(record: &CommitRecord, intent: &CommitRequest) -> bool {
    record.root == intent.candidate_root
        && record.parent == intent.expected_head
        && record.base_layer == intent.expected_base
}

fn unresolved(published: Option<CommitOutcome>, reason: String) -> Box<Unresolved> {
    Box::new(Unresolved { published, reason })
}
