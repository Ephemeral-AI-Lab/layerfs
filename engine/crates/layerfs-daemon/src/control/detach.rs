//! Linux half of normal unmount: plain detach once, then the drain barrier.
//! A drain that stops at `Join` makes the one abort write a forced unmount
//! would and drains again, so a connection that outlived its mount is ended
//! rather than kept (#426, #460).
use super::{
    kernel::Kernel,
    registry::bound_mut,
    serving::{stage, Attached, Evidence, Kept, Leaving},
    Failure, Service, Success,
};
use crate::store::BoundWorkspace;
use layerfs_bridge::control::{
    AbortDisposition, Activity, CommitKnowledge, ControlCode, DetachDisposition, ForcedFacts,
    MountPhase, TeardownStage, WorkspaceToken,
};
use layerfs_fuse::{
    mount::AbortWrite,
    session::{Detach, DrainStage, Forced, SessionObserver},
};
use std::{fmt, sync::Arc};

/// Abort effects after normal admission, which established no Commit custody
/// and completed the one plain detach before attempting the write.
pub(super) fn detached_abort_facts(forced: Option<Forced>) -> Option<ForcedFacts> {
    forced.map(|forced| ForcedFacts {
        abort: match forced.abort {
            AbortWrite::Written => AbortDisposition::Written,
            AbortWrite::Short(_) => AbortDisposition::Short,
            AbortWrite::Failed(_) => AbortDisposition::Failed,
        },
        detach: DetachDisposition::Detached,
        commit: CommitKnowledge::Absent,
        fenced: forced.terminal_replies,
    })
}

/// Terminal admission over a connection-owning entry. No daemon-side request
/// count refuses here: the kernel's own reversible answer is the fence, because
/// every blocked caller, descriptor, working directory and mapping holds a
/// mount reference. Work the kernel does not count (asynchronous releases and
/// lookup decrements, or a replied request's last bookkeeping) is waited out
/// by the drain barrier after a known detach. Forced admission takes the
/// session the same way, into its own first phase.
pub(super) fn take(kernel: &mut Kernel, phase: MountPhase) -> Result<Box<Attached>, Failure> {
    match kernel {
        Kernel::Ready(_) => {}
        Kernel::Leaving(_) => {
            return Err(Failure::kernel(
                ControlCode::Busy,
                "unmount:admission",
                "terminal unmount already admitted",
            ))
        }
        Kernel::Unattached
        | Kernel::Attaching
        | Kernel::Retained(_)
        | Kernel::Exported(_)
        | Kernel::ExportLeaving(_)
        | Kernel::ExportKept(_) => {
            unreachable!("handled by terminal admission")
        }
    }
    let Kernel::Ready(attached) = std::mem::replace(kernel, Kernel::Attaching) else {
        unreachable!("checked serving connection")
    };
    *kernel = Kernel::Leaving(Box::new(Leaving {
        ready: attached.ready.clone(),
        observer: attached.session.observer(),
        phase,
    }));
    Ok(attached)
}
impl Service {
    /// The session is serviced normally throughout the probe. Only a known
    /// detach is terminal; after it every stop keeps the exact owner.
    pub(super) fn unmount_kernel(
        &self,
        token: WorkspaceToken,
        workspace: Arc<BoundWorkspace>,
        attached: Attached,
    ) -> Result<Success, Failure> {
        let Attached { mut session, ready } = attached;
        let mount = session.mount();
        let observer = session.observer();
        match session.detach() {
            Ok(Detach::Detached) => {}
            Ok(Detach::Busy) => {
                // Reversible: the same connection returns to the registry.
                self.settle(
                    token,
                    Kernel::Ready(Box::new(Attached { session, ready })),
                    Activity::Idle,
                );
                return Err(Failure::kernel(
                    ControlCode::Busy,
                    "unmount:kernel",
                    "kernel reported the mount busy; no terminal effect",
                ));
            }
            Err(errno) => {
                return Err(self.retain(
                    token,
                    TeardownStage::Detach,
                    format!("plain detach: {errno}"),
                    Some(observer),
                    Box::new(session),
                    None,
                ))
            }
        }
        self.leaving(token, MountPhase::Draining);
        let drained = match session.drain() {
            Ok(drained) => drained,
            // Detached yet unjoined: the connection outlived its mount because
            // another mount namespace holds a copy of it (an engine or VM agent
            // outside the Sandbox). Make the same one abort write a forced
            // unmount makes, then drain again; a connection whose control is
            // unusable, or one that still stops, keeps its custody (#426, #460).
            Err(undrained) => match undrained.end_stalled() {
                Ok(drained) => drained,
                Err(undrained) => {
                    let facts = detached_abort_facts(undrained.forced);
                    let (stopped, detail) = match undrained.forced.map(|forced| forced.abort) {
                        Some(AbortWrite::Failed(errno)) => (
                            TeardownStage::Abort,
                            format!("abort write: {errno}; nothing further attempted"),
                        ),
                        Some(AbortWrite::Short(count)) => (
                            TeardownStage::Abort,
                            format!("abort write returned {count} of 1 byte; nothing further attempted"),
                        ),
                        abort => (
                            stage(undrained.stage),
                            format!(
                                "connection drain stopped at {:?}: abort {abort:?}, abort_bound {}, loops {:?}, work {:?}",
                                undrained.stage, observer.facts().abort_bound,
                                undrained.loops, undrained.work
                            ),
                        ),
                    };
                    let evidence = match undrained.stage {
                        DrainStage::Join => Evidence::Stalled(undrained),
                        _ => Evidence::Other(undrained),
                    };
                    return Err(self.keep(token, stopped, detail, Some(observer), evidence, facts));
                }
            },
        };
        let facts = detached_abort_facts(drained.forced);
        // Drain barrier holds: one fixed logical mark, then indexed retirement.
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
                    facts,
                ));
            }
        };
        self.close(token, &workspace, Some(drained))
    }
    /// Describes the exact retained kernel owner for an operator. Observation
    /// only: nothing is transferred, released, retried or settled.
    pub fn retained_kernel(&self, token: WorkspaceToken) -> Result<Option<String>, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        Ok(match &bound_mut(&mut entries, token)?.kernel {
            Kernel::Retained(kept) => Some(format!("{kept:?}")),
            _ => None,
        })
    }
    pub(super) fn leaving(&self, token: WorkspaceToken, phase: MountPhase) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Ok(binding) = bound_mut(&mut entries, token) {
                if let Kernel::Leaving(leaving) = &mut binding.kernel {
                    leaving.phase = phase;
                }
            }
        }
    }
    /// Stops a terminal operation after effects, keeping its owner in the entry.
    /// A forced teardown's facts are stored with it, as they stood at the stop.
    pub(super) fn retain(
        &self,
        token: WorkspaceToken,
        stage: TeardownStage,
        detail: String,
        observer: Option<SessionObserver>,
        evidence: Box<dyn fmt::Debug + Send>,
        forced: Option<ForcedFacts>,
    ) -> Failure {
        let evidence = Evidence::Other(evidence);
        self.keep(token, stage, detail, observer, evidence, forced)
    }
    /// The same, with the owner already sorted into its kind.
    pub(super) fn keep(
        &self,
        token: WorkspaceToken,
        stage: TeardownStage,
        detail: String,
        observer: Option<SessionObserver>,
        evidence: Evidence,
        forced: Option<ForcedFacts>,
    ) -> Failure {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut kept = Kept {
            ready: None,
            stage,
            detail,
            observer,
            evidence,
            forced,
        };
        let mut binding = bound_mut(&mut entries, token).ok();
        if let Some(Kernel::Leaving(leaving)) = binding.as_ref().map(|binding| &binding.kernel) {
            kept.ready = Some(leaving.ready.clone());
            // A stop after the session was consumed still reports its
            // connection's own detach and loop facts.
            if kept.observer.is_none() {
                kept.observer = Some(leaving.observer.clone());
            }
        }
        let custody = kept.custody(token);
        if let Some(binding) = binding.as_mut() {
            binding.kernel = Kernel::Retained(Box::new(kept));
            binding.epoch = binding.epoch.saturating_add(1);
        }
        Failure::Retained(Box::new(custody))
    }
}
