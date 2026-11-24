//! One exact resolution of retained Commit custody, never a retry.
//!
//! A Commit whose publication answered with an unknown outcome, or whose
//! install failed after a known publication, keeps its capture and its
//! captured reader and operation owner, and the Workspace refuses later
//! Commits. Its record stays here. The Workspace's next Commit request runs
//! [`BoundWorkspace::resolve_retained`] once before its own admission:
//!
//! - publication known, or our Commit (our candidate root, over the head we
//!   published against) found from the current head: install that root as
//!   the new base;
//! - the head still the one we published against, with another root, or a
//!   first-parent chain from the current head back to it that holds no Commit
//!   of ours: nothing was published, so the capture folds back;
//! - anything else (a chain this walk cannot judge, a failed read, a lane
//!   that does not pass): unresolved, and the custody and its record stay.
//!
//! Before the head is judged the attempt passes the Store's publication lane
//! (`pass_publication_lane`): a lost PostgreSQL `COMMIT` may still be applying
//! on the server, and its row lock or barrier key holds until it ends (#776
//! C2). Nothing is resent: the history is read through the control reader (a
//! connection of its own, unaffected by a writer session the unknown outcome
//! quarantined), and the install or fold-back runs once. A record whose
//! publication outcome this attempt could not determine is put back for the
//! next request, whose attempt only reads again (#776 R1); one whose outcome
//! was determined is taken by the attempt whatever the local step finds.
use super::{BoundWorkspace, CapturedConstruction, CommitFailure, CommitPhase};
use layerfs_content::{filesystem::FilesystemRootId, ObjectId};
use layerfs_history::{
    BranchSnapshot, CommitOutcome, CommitRecord, CommitRequest, HistoryCatalog, HistoryError,
};
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
    /// The outcome of the retained publication, read through the control
    /// reader once the publication lane has passed. `Some`: our Commit (or,
    /// with the head unmoved, a head that already has our root); `None`:
    /// proven unpublished.
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
            .validate_candidate(&target, candidate, kept.capture, &base, None)
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

/// The most Commits [`judge`] reads back from the current head before it
/// leaves the outcome undetermined (the custody stays; nothing is guessed).
const OUTCOME_WALK: usize = 1_024;

/// Judges the retained publication from published history.
///
/// Unmoved head and base: our root means published (UpToDate), any other
/// root means nothing was published. Otherwise the first-parent chain back to
/// the expected head is read. Every head move keeps the previous head on the new
/// head's first-parent chain, except a forced Commit (which names the head it
/// displaced) and a merge fast-forward (which brings in a Commit made on
/// another Branch). So our Commit, or one that displaced it, found on the
/// chain proves publication; reaching the expected head with neither, through
/// Commits all made on this Branch and displacing nothing, over the expected
/// base, proves there was none. Anything else is left undetermined.
fn judge(
    history: &dyn HistoryCatalog,
    intent: &CommitRequest,
) -> Result<Option<CommitOutcome>, Box<Unresolved>> {
    let read = |error: HistoryError| unresolved(None, format!("history read failed: {error:?}"));
    let head = history
        .branch_snapshot(intent.branch)
        .map_err(read)?
        .ok_or_else(|| unresolved(None, "the Branch is absent".into()))?;
    let base_kept = head.branch.base_layer == intent.expected_base;
    if base_kept && head.branch.head_commit == intent.expected_head {
        return Ok((head.effective_root == intent.candidate_root).then_some(
            CommitOutcome::UpToDate {
                head: head.branch.head_commit,
                root: head.effective_root,
            },
        ));
    }
    let undetermined = |why: &str| {
        Err(unresolved(
            None,
            format!("the head is {:?}; {why}", head.branch.head_commit),
        ))
    };
    let mut next = head.branch.head_commit;
    for _ in 0..OUTCOME_WALK {
        if next == intent.expected_head {
            return match base_kept {
                true => Ok(None),
                false => undetermined("the Branch base moved"),
            };
        }
        let Some(id) = next else {
            return undetermined("its history does not reach the head we published against");
        };
        let record = history
            .commit(id)
            .map_err(read)?
            .ok_or_else(|| unresolved(None, "a Commit on the Branch is absent".into()))?;
        if ours(&record, intent) {
            return Ok(Some(committed(record)));
        }
        if let Some(displaced) = record.meta.displaced {
            return match history.commit(displaced).map_err(read)? {
                Some(record) if ours(&record, intent) => Ok(Some(committed(record))),
                _ => undetermined("a forced Commit displaced a head this walk does not follow"),
            };
        }
        if record.meta.branch != Some(intent.branch) {
            return undetermined("a merge fast-forward brought in another Branch's Commits");
        }
        next = record.parent;
    }
    undetermined("too many Commits since the head we published against")
}

fn committed(record: CommitRecord) -> CommitOutcome {
    CommitOutcome::Committed {
        displaced: record.meta.displaced,
        record: Box::new(record),
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
