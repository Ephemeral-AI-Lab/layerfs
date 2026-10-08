//! Additive forced-teardown records: dispositions, Commit knowledge and receipt.
use crate::{
    control::{ControlError, WorkspaceToken},
    control_history::{outcome, put_outcome, put_token, token},
    control_native::{put_work, work, NativeWork},
    wire::{Reader, Writer},
};
use layerfs_history::CommitStagedOutcome;
/// Result of the one abort write on the connection's own control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AbortDisposition {
    /// The one-byte write was accepted whole.
    Written = 1,
    /// The write returned another count; nothing further was attempted.
    Short = 2,
    /// The write returned an error; nothing further was attempted.
    Failed = 3,
}
/// Result of the one plain detach that follows a complete local drain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DetachDisposition {
    /// The local drain did not hold, so no detach was attempted.
    NotAttempted = 1,
    /// Kernel detach is known.
    Detached = 2,
    /// The kernel kept the aborted mount for an external reference.
    Busy = 3,
    /// The detach returned another error; the mount state is not established.
    Failed = 4,
}
/// Commit custody copied at forced admission; never settled by a later read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitKnowledge {
    /// No Commit custody was retained; this is not a not-published verdict.
    Absent,
    /// Original known publication, kept even when local installation failed.
    Published(CommitStagedOutcome),
    /// Original unknown custody, explicitly relinquished and still unknown.
    Unknown,
}
/// Effects of one admitted forced teardown, as far as each was established.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForcedFacts {
    /// The abort write.
    pub abort: AbortDisposition,
    /// The detach attempt.
    pub detach: DetachDisposition,
    /// Commit knowledge at admission.
    pub commit: CommitKnowledge,
    /// Terminal replies made to requests fenced before their attempt.
    pub fenced: u64,
}
/// Engine cleanup state of the closed namespace, observed once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ForcedCleanup {
    /// A retained owner still holds the closed namespace; no cleanup is queued.
    Held = 1,
    /// Physical cleanup is queued with the owner.
    Queued = 2,
    /// No local row remains.
    Gone = 3,
    /// The one observation failed; no state is claimed.
    Unobserved = 4,
}
/// Bounded result of a completed forced teardown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForcedOutcome {
    /// Abort, detach, Commit knowledge and terminal replies.
    pub facts: ForcedFacts,
    /// Maintained counters observed at the end of the teardown.
    pub work: NativeWork,
    /// Cleanup state after logical Close.
    pub cleanup: ForcedCleanup,
}
/// Original acknowledgement of a forced terminal close.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForceUnmounted {
    /// Exact incarnation that was closed.
    pub token: WorkspaceToken,
    /// What the teardown established.
    pub outcome: ForcedOutcome,
}
pub(crate) fn put_facts(out: &mut Writer, value: &ForcedFacts) -> Result<(), ControlError> {
    out.put(&[value.abort as u8, value.detach as u8])?;
    match &value.commit {
        CommitKnowledge::Absent => out.byte(1)?,
        CommitKnowledge::Published(outcome) => {
            out.byte(2)?;
            put_outcome(out, outcome)?;
        }
        CommitKnowledge::Unknown => out.byte(3)?,
    }
    out.put(&value.fenced.to_be_bytes())
}
pub(crate) fn facts(input: &mut Reader<'_>) -> Result<ForcedFacts, ControlError> {
    let abort = match input.byte()? {
        1 => AbortDisposition::Written,
        2 => AbortDisposition::Short,
        3 => AbortDisposition::Failed,
        _ => return Err(ControlError("abort disposition")),
    };
    let detach = match input.byte()? {
        1 => DetachDisposition::NotAttempted,
        2 => DetachDisposition::Detached,
        3 => DetachDisposition::Busy,
        4 => DetachDisposition::Failed,
        _ => return Err(ControlError("detach disposition")),
    };
    let commit = match input.byte()? {
        1 => CommitKnowledge::Absent,
        2 => CommitKnowledge::Published(outcome(input)?),
        3 => CommitKnowledge::Unknown,
        _ => return Err(ControlError("Commit knowledge")),
    };
    Ok(ForcedFacts {
        abort,
        detach,
        commit,
        fenced: u64::from_be_bytes(input.array()?),
    })
}
pub(crate) fn put_forced(out: &mut Writer, value: &ForceUnmounted) -> Result<(), ControlError> {
    put_token(out, value.token)?;
    put_facts(out, &value.outcome.facts)?;
    put_work(out, &value.outcome.work)?;
    out.byte(value.outcome.cleanup as u8)
}
pub(crate) fn forced(input: &mut Reader<'_>) -> Result<ForceUnmounted, ControlError> {
    let token = token(input)?;
    let facts = facts(input)?;
    let work = work(input)?;
    let cleanup = match input.byte()? {
        1 => ForcedCleanup::Held,
        2 => ForcedCleanup::Queued,
        3 => ForcedCleanup::Gone,
        4 => ForcedCleanup::Unobserved,
        _ => return Err(ControlError("forced cleanup")),
    };
    Ok(ForceUnmounted {
        token,
        outcome: ForcedOutcome {
            facts,
            work,
            cleanup,
        },
    })
}
