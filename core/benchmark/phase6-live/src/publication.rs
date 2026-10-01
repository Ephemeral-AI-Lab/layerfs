//! Trusted construction publication: constant context/locator checks and actual C5.
use crate::{metadata::Authority, objects::Locators, reservations::Owner, wire::Snapshot};
use layerfs_bridge::contract::*;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_history::{
    catalog::HistoryCatalog,
    identity::{CommitId, LayerId, WorkspaceId},
    records::*,
};
use std::time::Instant;
#[derive(Default)]
pub struct Submission {
    generation: u64,
    revision: u64,
    pending: bool,
}
pub struct Request<'a> {
    pub owner: &'a Owner,
    pub generation: u64,
    pub revision: u64,
    pub base: &'a Snapshot,
    pub root: ObjectId,
}
fn outcome_failure(generation: u64, cause: Failure) -> WorkspaceCommitOutcome {
    let disposition = if cause.unknown {
        WorkspaceCommitFailureDisposition::Unknown
    } else {
        WorkspaceCommitFailureDisposition::KnownBeforeCommit
    };
    WorkspaceCommitOutcome::Failed(Box::new(WorkspaceCommitFailureWire {
        generation,
        phase: WorkspaceCommitPhase::CompositeCommit,
        disposition,
        cause,
        known_stage: None,
        observed_stage: None,
        known_outcome: None,
        observed_outcome: None,
        installed_revision: None,
    }))
}
pub fn submit(a: &Authority, request: Request<'_>) -> WorkspaceCommitOutcome {
    match publish(a, &request) {
        Ok(report) => WorkspaceCommitOutcome::Completed(report),
        Err(cause) => outcome_failure(request.generation, cause),
    }
}
fn publish(a: &Authority, r: &Request<'_>) -> Result<WorkspaceCommitReportWire, Failure> {
    let started = Instant::now();
    a.reservations
        .lock()
        .map_err(|_| Code::Ownership)?
        .authorize(r.owner, r.base.scope, r.base.profile)
        .map_err(|_| Code::Denied)?;
    let actual = a.snapshot().map_err(|_| Code::Provider)?;
    if actual.stack != r.base.stack
        || actual.branch != r.base.branch
        || actual.scope != r.base.scope
        || actual.profile != r.base.profile
    {
        return Err(Code::Denied.into());
    }
    if actual.base != r.base.base || actual.head != r.base.head || actual.root != r.base.root {
        return Err(crate::publication_failure::branch_moved(r.base, &actual));
    }
    let locator = a
        .locators
        .lookup(r.root)
        .map_err(|_| Code::Provider)?
        .ok_or(Code::MissingObject)?;
    if locator.role != ObjectRole::FilesystemRoot {
        return Err(Code::Integrity.into());
    }
    let mut state = a.publication.lock().map_err(|_| Code::Ownership)?;
    if state.pending {
        return Err(Code::Busy.into());
    }
    let next = state
        .generation
        .checked_add(1)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(Code::Capacity)?;
    if r.generation != next
        || r.revision == 0
        || r.revision > i64::MAX as u64
        || r.revision < state.revision
    {
        return Err(Code::InvalidInput.into());
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
    state.pending = true;
    let stage_started = Instant::now();
    let stage = a
        .history
        .stage_changes(&StageRequest {
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
        })
        .map_err(crate::publication_failure::catalog)?;
    let stage_ns = stage_started.elapsed().as_nanos();
    let commit_started = Instant::now();
    let outcome = match a
        .history
        .commit_staged(&CommitStagedRequest {
            workspace,
            token: stage.token,
        })
        .map_err(crate::publication_failure::catalog)?
    {
        CommitStagedOutcome::Committed(c) => {
            if c.root != r.root
                || c.stack.to_bytes() != r.base.stack
                || c.base_layer != base
                || c.parent != head
            {
                return Err(Code::Unknown.into());
            }
            CommitOutcomeWire::Committed(CommitWire {
                commit: c.id.to_bytes(),
                stack: c.stack.to_bytes(),
                root: *c.root.as_bytes(),
                parent: c.parent.map(|p| p.to_bytes()),
                base_layer: c.base_layer.to_bytes(),
            })
        }
        CommitStagedOutcome::UpToDate {
            head: known,
            root: known_root,
        } => {
            if known != head || known_root != r.root {
                return Err(Code::Unknown.into());
            }
            CommitOutcomeWire::UpToDate {
                head: known.map(|h| h.to_bytes()),
                root: *known_root.as_bytes(),
            }
        }
    };
    let commit_ns = commit_started.elapsed().as_nanos();
    state.generation = r.generation;
    state.revision = r.revision;
    state.pending = false;
    eprintln!("P6_PUBLICATION context_ns={context_ns} stage_ns={stage_ns} commit_ns={commit_ns} candidate_locators=1");
    Ok(WorkspaceCommitReportWire {
        generation: r.generation,
        revision: r.revision,
        stage_token: Some(stage.token.value()),
        outcome,
    })
}
