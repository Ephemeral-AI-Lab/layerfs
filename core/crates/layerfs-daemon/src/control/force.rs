//! Forced terminal unmount: a before-effect guard, one abort write, the local
//! drain with its one plain detach, then Revoke and Close. Nothing is repeated.
#[cfg(target_os = "linux")]
use super::serving::{stage, work, Attached};
use super::{
    native::Native,
    registry::{bound_mut, Binding},
    types::NativeFailure,
    Failure, Service, Success,
};
#[cfg(target_os = "linux")]
use crate::{store::BoundWorkspace, Command, NativeJob, NativeReply, Response};
#[cfg(target_os = "linux")]
use layerfs_bridge::control::{
    AbortDisposition, DetachDisposition, ForceUnmounted, ForcedCleanup, ForcedFacts, ForcedOutcome,
    NativePhase, Reply, TeardownStage,
};
use layerfs_bridge::control::{Activity, CommitKnowledge, ControlCode, WorkspaceToken};
#[cfg(target_os = "linux")]
use layerfs_fuse::{
    mount::AbortWrite,
    session::{DetachAttempt, SessionFacts},
};
#[cfg(target_os = "linux")]
use layerfs_overlay::CleanupState;
#[cfg(target_os = "linux")]
use nix::errno::Errno;
#[cfg(target_os = "linux")]
use std::sync::Arc;

const ADMISSION: &str = "force:admission";
const CUSTODY: &str = "force:custody";
const CAPABILITY: &str = "force:capability";

fn refused(code: ControlCode, phase: &'static str, detail: &'static str) -> Failure {
    Failure::Native(Box::new(NativeFailure {
        code,
        phase,
        detail: detail.into(),
        completions: Vec::new(),
        evidence: None,
    }))
}
/// The whole before-effect decision, made under the registry lock and in this
/// order. A refusal changes nothing. Admitted, it returns the Commit knowledge
/// the receipt will carry: copied here, never settled by a later read.
fn guard(
    value: &Binding,
    token: WorkspaceToken,
    relinquish_unknown: bool,
) -> Result<CommitKnowledge, Failure> {
    // Stored custody answers first: no second abort or detach ever follows it.
    #[cfg(target_os = "linux")]
    if let Native::Retained(kept) = &value.native {
        return Err(Failure::Retained(Box::new(kept.custody(token))));
    }
    #[cfg(not(target_os = "linux"))]
    let _ = token;
    if matches!(
        value.activity,
        Activity::Committing | Activity::Attaching | Activity::Closing
    ) {
        return Err(refused(
            ControlCode::Busy,
            ADMISSION,
            "Workspace control operation active",
        ));
    }
    let abort_bound = connection(&value.native)?;
    let commit = match (value.activity, &value.published) {
        (Activity::Idle, _) => CommitKnowledge::Absent,
        // A known publication is forced with its original outcome.
        (Activity::LocalFailure, Some(published)) => CommitKnowledge::Published(published.clone()),
        (Activity::Uncertain, _) if relinquish_unknown => CommitKnowledge::Unknown,
        (Activity::LocalFailure, None) => {
            return Err(refused(
                ControlCode::Unknown,
                CUSTODY,
                "retained Commit custody without its publication",
            ))
        }
        _ => {
            return Err(refused(
                ControlCode::Unknown,
                CUSTODY,
                "original Commit custody retained and not relinquished",
            ))
        }
    };
    if !abort_bound {
        return Err(refused(
            ControlCode::Failed,
            CAPABILITY,
            "no abort control is bound to this connection",
        ));
    }
    Ok(commit)
}
/// Only a serving connection can be forced; answers whether its abort control
/// is bound. Outside Linux no entry ever owns a connection.
fn connection(native: &Native) -> Result<bool, Failure> {
    match native {
        Native::Unattached => Err(refused(
            ControlCode::Invalid,
            ADMISSION,
            "Workspace owns no native connection",
        )),
        #[cfg(target_os = "linux")]
        Native::Attaching => Err(refused(
            ControlCode::Unknown,
            ADMISSION,
            "original Attach outcome retained",
        )),
        // A terminal operation owns the connection; its activity or stored
        // custody answered above.
        #[cfg(target_os = "linux")]
        Native::Leaving(_) | Native::Retained(_) => Err(refused(
            ControlCode::Busy,
            ADMISSION,
            "terminal unmount already admitted",
        )),
        #[cfg(target_os = "linux")]
        Native::Ready(attached) => Ok(attached.session.abort_bound()),
    }
}
#[cfg(not(target_os = "linux"))]
impl Service {
    /// The same guard as on Linux; no entry here can pass it.
    pub(super) fn force_unmount(
        &self,
        token: WorkspaceToken,
        relinquish_unknown: bool,
    ) -> Result<Success, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        guard(bound_mut(&mut entries, token)?, token, relinquish_unknown)?;
        Err(refused(
            ControlCode::Invalid,
            ADMISSION,
            "native serving requires Linux",
        ))
    }
}
/// What forced admission took out of the registry entry.
#[cfg(target_os = "linux")]
struct Admitted {
    workspace: Arc<BoundWorkspace>,
    attached: Box<Attached>,
    /// Restored only if the abort is refused before any effect.
    prior: Activity,
    commit: CommitKnowledge,
}
#[cfg(target_os = "linux")]
fn detach(attempt: DetachAttempt) -> DetachDisposition {
    match attempt {
        DetachAttempt::NotAttempted => DetachDisposition::NotAttempted,
        DetachAttempt::Detached => DetachDisposition::Detached,
        // Aborted and still mounted: never the reversible normal answer.
        DetachAttempt::Failed(Errno::EBUSY) => DetachDisposition::Busy,
        DetachAttempt::Failed(_) => DetachDisposition::Failed,
    }
}
#[cfg(target_os = "linux")]
impl Service {
    pub(super) fn force_unmount(
        &self,
        token: WorkspaceToken,
        relinquish_unknown: bool,
    ) -> Result<Success, Failure> {
        let admitted = self.admit_force(token, relinquish_unknown)?;
        self.force_admitted(token, admitted)
    }
    /// One short registry step. Admission is the first effect: the entry is
    /// `Stopping` and closing before the lock is released.
    fn admit_force(
        &self,
        token: WorkspaceToken,
        relinquish_unknown: bool,
    ) -> Result<Admitted, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let value = bound_mut(&mut entries, token)?;
        let commit = guard(value, token, relinquish_unknown)?;
        let attached = super::detach::take(&mut value.native, NativePhase::Stopping)?;
        let prior = value.activity;
        value.activity = Activity::Closing;
        value.epoch = value.epoch.saturating_add(1);
        Ok(Admitted {
            workspace: value.workspace.clone(),
            attached,
            prior,
            commit,
        })
    }
    /// Each step is made once. A step that is not established keeps the exact
    /// owner in the entry with the facts as they stood; no fallback, second
    /// write, second detach or wait on a caller process follows.
    fn force_admitted(
        &self,
        token: WorkspaceToken,
        admitted: Admitted,
    ) -> Result<Success, Failure> {
        let Admitted {
            workspace,
            attached,
            prior,
            commit,
        } = admitted;
        let Attached { mut session, ready } = *attached;
        let mount = session.mount();
        let observer = session.observer();
        let mut facts = ForcedFacts {
            abort: AbortDisposition::Written,
            detach: DetachDisposition::NotAttempted,
            commit,
            fenced: 0,
        };
        let written = match session.abort() {
            Ok(written) => written,
            Err(refusal) => {
                // Refused before the write: the same connection returns with
                // the activity it had.
                self.settle(
                    token,
                    Native::Ready(Box::new(Attached { session, ready })),
                    prior,
                );
                return Err(Failure::native(
                    ControlCode::Failed,
                    CAPABILITY,
                    format!("abort refused before any effect: {refusal:?}"),
                ));
            }
        };
        // A write that was not accepted whole attempts nothing further and
        // stops no request service, so no terminal reply was made.
        let unwritten = match written {
            AbortWrite::Written => None,
            AbortWrite::Short(count) => Some((
                AbortDisposition::Short,
                format!("abort write returned {count} of 1 byte; nothing further attempted"),
            )),
            AbortWrite::Failed(errno) => Some((
                AbortDisposition::Failed,
                format!("abort write: {errno}; nothing further attempted"),
            )),
        };
        if let Some((abort, detail)) = unwritten {
            facts.abort = abort;
            return Err(self.retain(
                token,
                TeardownStage::Abort,
                detail,
                Some(observer),
                Box::new(session),
                Some(facts),
            ));
        }
        let drained = match session.force_drain() {
            Ok(drained) => drained,
            Err(undrained) => {
                let attempt = undrained
                    .forced
                    .map_or(DetachAttempt::NotAttempted, |forced| forced.detach);
                facts.detach = detach(attempt);
                facts.fenced = undrained.forced.map_or(0, |forced| forced.terminal_replies);
                return Err(self.retain(
                    token,
                    stage(undrained.stage),
                    format!(
                        "forced drain stopped at {:?}: detach {attempt:?}, loops {:?}, work {:?}",
                        undrained.stage, undrained.loops, undrained.work
                    ),
                    Some(observer),
                    undrained,
                    Some(facts),
                ));
            }
        };
        facts.detach = DetachDisposition::Detached;
        facts.fenced = drained.forced.map_or(0, |forced| forced.terminal_replies);
        // The lane is released, so its last counters come from the receipt.
        let work = work(&SessionFacts {
            work: Ok(drained.work),
            opcodes: drained.opcodes,
            ..observer.facts()
        });
        self.leaving(token, NativePhase::Draining);
        // Read and dropped at once: its Lifecycle slot returns before Close.
        match self.job(workspace.route(), Command::Native(NativeJob::Revoke(mount))) {
            Ok(done) if matches!(done.result(), Ok(Response::Native(NativeReply::Done))) => {}
            other => {
                let failure = match other {
                    Ok(done) => Failure::Completion(Box::new(done)),
                    Err(error) => error,
                };
                return Err(self.retain(
                    token,
                    TeardownStage::Revoke,
                    failure.wire().detail,
                    Some(observer),
                    Box::new((failure, drained)),
                    Some(facts),
                ));
            }
        }
        let (completion, earlier, native) = self.closed(
            token,
            &workspace,
            Vec::new(),
            Some(drained),
            Some(facts.clone()),
        )?;
        // One observation of the closed namespace; a failed one claims no
        // state. Its completion is read and dropped here.
        let (cleanup, observation_failure) = match self
            .job(workspace.route(), Command::CleanupState)
        {
            Ok(done) => match done.result() {
                Ok(Response::CleanupState(CleanupState::Held)) => (ForcedCleanup::Held, None),
                Ok(Response::CleanupState(CleanupState::Queued)) => (ForcedCleanup::Queued, None),
                Ok(Response::CleanupState(CleanupState::Gone)) => (ForcedCleanup::Gone, None),
                _ => (
                    ForcedCleanup::Unobserved,
                    Some(Box::new(Failure::Completion(Box::new(done)))),
                ),
            },
            Err(error) => (ForcedCleanup::Unobserved, Some(Box::new(error))),
        };
        self.depart(
            token,
            Success {
                reply: Reply::ForceUnmounted(Box::new(ForceUnmounted {
                    token,
                    outcome: ForcedOutcome {
                        facts,
                        work,
                        cleanup,
                    },
                })),
                completion: Some(completion),
                earlier,
                commit: None,
                observation_failure,
                native,
            },
        )
    }
}
