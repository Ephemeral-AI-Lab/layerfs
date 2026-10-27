//! Forced terminal unmount: a before-effect guard, one abort write, the local
//! drain with its one plain detach, then Revoke and Close. Nothing is repeated.
//! A normal unmount stalled at Join is forced the same way, its plain detach
//! counting as the one detach.
#[cfg(target_os = "linux")]
use super::serving::{stage, work, Attached, Evidence, Leaving};
use super::{
    kernel::Kernel,
    registry::{bound_mut, Binding},
    types::KernelFailure,
    Failure, Service, Success,
};
#[cfg(target_os = "linux")]
use crate::store::BoundWorkspace;
#[cfg(target_os = "linux")]
use layerfs_bridge::control::{
    AbortDisposition, DetachDisposition, ForceUnmounted, ForcedCleanup, ForcedFacts, ForcedOutcome,
    MountPhase, Reply, TeardownStage,
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
    Failure::Kernel(Box::new(KernelFailure {
        code,
        phase,
        detail: detail.into(),
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
    // Stored custody answers first: no second abort or detach ever follows
    // it. The one exception is a normal unmount stalled at Join, admitted
    // idle with no Commit custody, whose abort control is still unused.
    #[cfg(target_os = "linux")]
    if let Kernel::Retained(kept) = &value.kernel {
        if kept.forceable() {
            return Ok(CommitKnowledge::Absent);
        }
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
    let abort_bound = connection(&value.kernel)?;
    let commit = match (value.activity, &value.published) {
        (Activity::Idle, _) => CommitKnowledge::Absent,
        // A known publication is forced with its original outcome.
        (Activity::LocalFailure, Some(published)) => {
            CommitKnowledge::Published(Box::new(published.clone()))
        }
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
fn connection(kernel: &Kernel) -> Result<bool, Failure> {
    match kernel {
        Kernel::Unattached => Err(refused(
            ControlCode::Invalid,
            ADMISSION,
            "Workspace owns no kernel connection",
        )),
        #[cfg(target_os = "linux")]
        Kernel::Attaching => Err(refused(
            ControlCode::Unknown,
            ADMISSION,
            "original Attach outcome retained",
        )),
        // A terminal operation owns the connection; its activity or stored
        // custody answered above.
        #[cfg(target_os = "linux")]
        Kernel::Leaving(_) | Kernel::Retained(_) => Err(refused(
            ControlCode::Busy,
            ADMISSION,
            "terminal unmount already admitted",
        )),
        #[cfg(target_os = "linux")]
        Kernel::Ready(attached) => Ok(attached.session.abort_bound()),
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
            "kernel serving requires Linux",
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
    /// Taken from a normal unmount stalled at Join: already detached.
    stalled: bool,
}
/// Moves a stalled normal unmount's session out of its retained custody,
/// which `guard` checked, into the forced teardown's own first phase.
#[cfg(target_os = "linux")]
fn take_stalled(kernel: &mut Kernel) -> Box<Attached> {
    let Kernel::Retained(kept) = std::mem::replace(kernel, Kernel::Attaching) else {
        unreachable!("checked retained custody")
    };
    let kept = *kept;
    let (Some(ready), Some(observer), Evidence::Stalled(undrained)) =
        (kept.ready, kept.observer, kept.evidence)
    else {
        unreachable!("checked stalled custody")
    };
    *kernel = Kernel::Leaving(Box::new(Leaving {
        ready: ready.clone(),
        observer,
        phase: MountPhase::Stopping,
    }));
    Box::new(Attached {
        session: undrained.session,
        ready,
    })
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
        let stalled = matches!(value.kernel, Kernel::Retained(_));
        let attached = if stalled {
            take_stalled(&mut value.kernel)
        } else {
            super::detach::take(&mut value.kernel, MountPhase::Stopping)?
        };
        let prior = value.activity;
        value.activity = Activity::Closing;
        value.epoch = value.epoch.saturating_add(1);
        Ok(Admitted {
            workspace: value.workspace.clone(),
            attached,
            prior,
            commit,
            stalled,
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
            stalled,
        } = admitted;
        let Attached { mut session, ready } = *attached;
        let mount = session.mount();
        let observer = session.observer();
        let mut facts = ForcedFacts {
            abort: AbortDisposition::Written,
            detach: if stalled {
                DetachDisposition::Detached
            } else {
                DetachDisposition::NotAttempted
            },
            commit,
            fenced: 0,
        };
        let aborted = if stalled {
            session.abort_detached()
        } else {
            session.abort()
        };
        let written = match aborted {
            Ok(written) => written,
            // Admitted under the lock with its control bound and unused, so
            // this cannot happen; the detached owner is kept, never released.
            Err(refusal) if stalled => {
                return Err(self.retain(
                    token,
                    TeardownStage::Join,
                    format!("abort refused before any effect: {refusal:?}"),
                    Some(observer),
                    Box::new(session),
                    None,
                ))
            }
            Err(refusal) => {
                // Refused before the write: the same connection returns with
                // the activity it had.
                self.settle(
                    token,
                    Kernel::Ready(Box::new(Attached { session, ready })),
                    prior,
                );
                return Err(Failure::kernel(
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
        // The mount is released, so its last counters come from the receipt.
        let work = work(&SessionFacts {
            work: Ok(drained.work),
            opcodes: drained.opcodes,
            ..observer.facts()
        });
        self.leaving(token, MountPhase::Draining);
        // Read and dropped at once: its Lifecycle slot returns before Close.
        match self.job(
            workspace.route(),
            |job_client: &crate::OwnerClient, job_route| job_client.kernel_revoke(job_route, mount),
        ) {
            Ok(()) => {}
            Err(failure) => {
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
        let kernel = self.closed(token, &workspace, Some(drained), Some(facts.clone()))?;
        // One observation of the closed namespace; a failed one claims no
        // state. Its completion is read and dropped here.
        let (cleanup, observation_failure) = match self
            .owner
            .observe_cleanup(None, token.namespace, token.workspace.to_bytes())
            .map_err(Failure::Owner)
        {
            Ok(CleanupState::Held) => (ForcedCleanup::Held, None),
            Ok(CleanupState::Queued) => (ForcedCleanup::Queued, None),
            Ok(CleanupState::Gone) => (ForcedCleanup::Gone, None),
            Ok(CleanupState::Live) => (
                ForcedCleanup::Unobserved,
                Some(Box::new(Failure::Rejected(
                    ControlCode::Unknown,
                    "closed Workspace remains live",
                ))),
            ),
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
                commit: None,
                observation_failure,
                kernel,
            },
        )
    }
}
