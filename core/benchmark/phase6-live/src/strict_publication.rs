//! Thin C5 publication from a ready strict-storage save, with retained uncertain custody.
use crate::{
    reservations::{Book, Owner},
    strict_catalog::{SaveContext, StrictCatalog},
    wire::Snapshot,
};
use layerfs_bridge::contract::*;
use layerfs_content::ObjectId;
use layerfs_history::{
    catalog::HistoryCatalog,
    identity::{BranchId, CommitId, LayerId, WorkspaceId},
    records::*,
    sqlite::SqliteCatalog,
};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Default)]
pub struct Submission {
    pub generation: u64,
    pub revision: u64,
    pub pending: Option<Pending>,
}
#[derive(Clone, Debug)]
pub struct Pending {
    pub owner: Owner,
    pub generation: u64,
    pub revision: u64,
    pub save: i64,
    pub known_stage: Option<StageWire>,
    pub storage_publication: Option<u64>,
    pub quarantined: bool,
}
pub struct Authority {
    pub catalog: Arc<StrictCatalog>,
    pub history: SqliteCatalog,
    pub branch: BranchId,
    pub reservations: Mutex<Book>,
    pub publication: Mutex<Submission>,
}
pub struct Request<'a> {
    pub owner: &'a Owner,
    pub generation: u64,
    pub revision: u64,
    pub base: &'a Snapshot,
    pub save: i64,
    pub root: ObjectId,
}
impl Authority {
    pub fn snapshot(&self) -> Result<Snapshot, Failure> {
        let s = self
            .history
            .branch_snapshot(self.branch)
            .map_err(crate::publication_failure::catalog)?
            .ok_or(Code::NotFound)?;
        Ok(Snapshot {
            stack: s.branch.stack.to_bytes(),
            branch: s.branch.id.to_bytes(),
            base: s.branch.base_layer.to_bytes(),
            head: s.branch.head_commit.map(|id| id.to_bytes()),
            root: *s.effective_root.as_bytes(),
            scope: *s.scope.as_bytes(),
            profile: *s.profile.as_bytes(),
        })
    }
}
struct Refusal(Box<RefusalDetails>);
struct RefusalDetails {
    cause: Failure,
    known_stage: Option<StageWire>,
    observed_outcome: Option<CommitOutcomeWire>,
    known_outcome: Option<CommitOutcomeWire>,
}
impl From<Failure> for Refusal {
    fn from(cause: Failure) -> Self {
        Self(Box::new(RefusalDetails {
            cause,
            known_stage: None,
            observed_outcome: None,
            known_outcome: None,
        }))
    }
}
impl From<Code> for Refusal {
    fn from(code: Code) -> Self {
        Failure::from(code).into()
    }
}
pub fn submit(a: &Authority, r: Request<'_>) -> WorkspaceCommitOutcome {
    match publish(a, &r) {
        Ok(report) => WorkspaceCommitOutcome::Completed(report),
        Err(Refusal(f)) => {
            let f = *f;
            let observed_stage = f.cause.history.as_ref().and_then(|h| match &h.stage {
                StageObservation::Retained(s) | StageObservation::AcknowledgedUnknown(s) => {
                    Some(s.as_ref().clone())
                }
                _ => None,
            });
            WorkspaceCommitOutcome::Failed(Box::new(WorkspaceCommitFailureWire {
                generation: r.generation,
                phase: WorkspaceCommitPhase::CompositeCommit,
                disposition: if f.known_outcome.is_some() {
                    WorkspaceCommitFailureDisposition::KnownCommitLocalFailure
                } else if f.cause.unknown {
                    WorkspaceCommitFailureDisposition::Unknown
                } else {
                    WorkspaceCommitFailureDisposition::KnownBeforeCommit
                },
                cause: f.cause,
                known_stage: f.known_stage,
                observed_stage,
                known_outcome: f.known_outcome,
                observed_outcome: f.observed_outcome,
                installed_revision: None,
            }))
        }
    }
}
fn publish(a: &Authority, r: &Request<'_>) -> Result<WorkspaceCommitReportWire, Refusal> {
    let started = Instant::now();
    if r.save <= 0
        || r.generation == 0
        || r.generation > i64::MAX as u64
        || r.revision == 0
        || r.revision > i64::MAX as u64
    {
        return Err(Code::InvalidInput.into());
    }
    r.owner.check().map_err(|_| Code::InvalidInput)?;
    if r.owner.project != r.base.stack
        || r.owner.branch != r.base.branch
        || r.base.branch != a.branch.to_bytes()
    {
        return Err(Code::Denied.into());
    }
    a.reservations
        .lock()
        .map_err(|_| Code::Ownership)?
        .authorize(r.owner, r.base.scope, r.base.profile)
        .map_err(|_| Code::Denied)?;
    let actual = a.snapshot()?;
    if actual.stack != r.base.stack
        || actual.branch != r.base.branch
        || actual.scope != r.base.scope
        || actual.profile != r.base.profile
    {
        return Err(Code::Denied.into());
    }
    if actual.base != r.base.base || actual.head != r.base.head || actual.root != r.base.root {
        return Err(crate::publication_failure::branch_moved(r.base, &actual).into());
    }
    let mut state = a.publication.lock().map_err(|_| Code::Ownership)?;
    if state.pending.is_some() {
        return Err(Code::Busy.into());
    }
    let next = state.generation.checked_add(1).ok_or(Code::Capacity)?;
    if r.generation != next || r.revision < state.revision {
        return Err(Code::InvalidInput.into());
    }
    let expected_save = SaveContext {
        owner: r.owner.clone(),
        generation: r.generation,
        scope: r.base.scope,
        profile: r.base.profile,
        base_root: r.base.root,
    };
    if !a
        .catalog
        .matches_save_owner(r.save, &expected_save)
        .map_err(|_| Code::Provider)?
    {
        return Err(Code::Denied.into());
    }
    if !a
        .catalog
        .ready_root(r.save, r.root)
        .map_err(|_| Code::Provider)?
    {
        return Err(Code::MissingObject.into());
    }
    let workspace =
        WorkspaceId::from_authority(r.owner.incarnation).map_err(|_| Code::InvalidInput)?;
    let base = LayerId::from_bytes(r.base.base).map_err(|_| Code::InvalidInput)?;
    let head = r
        .base
        .head
        .map(CommitId::from_bytes)
        .transpose()
        .map_err(|_| Code::InvalidInput)?;
    let root = ObjectId::from_bytes(&r.base.root).map_err(|_| Code::InvalidInput)?;
    let profile = ObjectId::from_bytes(&r.base.profile).map_err(|_| Code::InvalidInput)?;
    let scope = ObjectId::from_bytes(&r.base.scope).map_err(|_| Code::InvalidInput)?;
    let context_ns = started.elapsed().as_nanos();
    state.pending = Some(Pending {
        owner: r.owner.clone(),
        generation: r.generation,
        revision: r.revision,
        save: r.save,
        known_stage: None,
        storage_publication: None,
        quarantined: false,
    });
    let stage_started = Instant::now();
    let stage = match a.history.stage_changes(&StageRequest {
        workspace,
        branch: a.branch,
        expected_head: head,
        expected_base: base,
        expected_root: root,
        construction_base_root: root,
        intended_commit_base: base,
        candidate_root: r.root,
        profile,
        scope,
        generation: r.generation,
    }) {
        Ok(stage) => stage,
        Err(error) => {
            let cause = crate::publication_failure::catalog(error);
            state.pending.as_mut().ok_or(Code::Ownership)?.quarantined = cause.unknown;
            return Err(cause.into());
        }
    };
    let known_stage = stage_wire(&stage);
    let pending = state.pending.as_mut().ok_or(Code::Ownership)?;
    pending.known_stage = Some(known_stage.clone());
    let stage_ns = stage_started.elapsed().as_nanos();
    let storage_started = Instant::now();
    let publication = match a.catalog.publish(r.save) {
        Ok(publication) => publication,
        Err(message) => {
            eprintln!("strict storage publication unavailable: {message}");
            pending.quarantined = true;
            // A String cannot establish rollback or commit disposition. The known
            // C5 token and every stored representation remain in owned custody.
            return Err(Refusal(Box::new(RefusalDetails {
                cause: Code::Unknown.into(),
                known_stage: Some(known_stage),
                observed_outcome: None,
                known_outcome: None,
            })));
        }
    };
    pending.storage_publication = Some(publication);
    let storage_ns = storage_started.elapsed().as_nanos();
    let commit_started = Instant::now();
    let committed = match a.history.commit_staged(&CommitStagedRequest {
        workspace,
        token: stage.token,
    }) {
        Ok(outcome) => outcome,
        Err(error) => {
            let cause = crate::publication_failure::catalog(error);
            pending.quarantined = cause.unknown;
            return Err(Refusal(Box::new(RefusalDetails {
                cause,
                known_stage: Some(known_stage),
                observed_outcome: None,
                known_outcome: None,
            })));
        }
    };
    let (outcome, valid) = match committed {
        CommitStagedOutcome::Committed(c) => {
            let valid = c.root == r.root
                && c.stack.to_bytes() == r.base.stack
                && c.base_layer == base
                && c.parent == head;
            (
                CommitOutcomeWire::Committed(CommitWire {
                    commit: c.id.to_bytes(),
                    stack: c.stack.to_bytes(),
                    root: *c.root.as_bytes(),
                    parent: c.parent.map(|p| p.to_bytes()),
                    base_layer: c.base_layer.to_bytes(),
                }),
                valid,
            )
        }
        CommitStagedOutcome::UpToDate {
            head: known,
            root: known_root,
        } => (
            CommitOutcomeWire::UpToDate {
                head: known.map(|h| h.to_bytes()),
                root: *known_root.as_bytes(),
            },
            known == head && known_root == r.root,
        ),
    };
    if !valid {
        pending.quarantined = true;
        return Err(Refusal(Box::new(RefusalDetails {
            cause: Code::Unknown.into(),
            known_stage: Some(known_stage),
            observed_outcome: Some(outcome),
            known_outcome: None,
        })));
    }
    let commit_ns = commit_started.elapsed().as_nanos();
    if let Err(message) = a.catalog.complete_owned_publication(r.save) {
        eprintln!("strict owned publication completion unavailable: {message}");
        pending.quarantined = true;
        return Err(Refusal(Box::new(RefusalDetails {
            cause: Code::Unknown.into(),
            known_stage: Some(known_stage),
            observed_outcome: Some(outcome.clone()),
            known_outcome: Some(outcome),
        })));
    }
    state.generation = r.generation;
    state.revision = r.revision;
    state.pending = None;
    eprintln!("P6_STRICT_PUBLICATION context_ns={context_ns} stage_ns={stage_ns} storage_ns={storage_ns} commit_ns={commit_ns} ready_root_locators=1");
    Ok(WorkspaceCommitReportWire {
        generation: r.generation,
        revision: r.revision,
        stage_token: Some(stage.token.value()),
        outcome,
    })
}
fn stage_wire(r: &StageRecord) -> StageWire {
    StageWire {
        workspace: r.workspace.to_bytes(),
        token: r.token.value(),
        stack: r.stack.to_bytes(),
        branch: r.branch.to_bytes(),
        expected_head: r.expected_head.map(|h| h.to_bytes()),
        expected_base: r.expected_base.to_bytes(),
        expected_root: *r.expected_root.as_bytes(),
        construction_base_root: *r.construction_base_root.as_bytes(),
        intended_commit_base: r.intended_commit_base.to_bytes(),
        candidate_root: *r.candidate_root.as_bytes(),
        profile: *r.profile.as_bytes(),
        scope: *r.scope.as_bytes(),
        generation: r.generation,
    }
}
