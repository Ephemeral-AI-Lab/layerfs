//! Linux half of normal unmount: plain detach once, then the drain barrier.
use super::{
    native::Native,
    registry::bound_mut,
    serving::{stage, Attached, Kept, Leaving},
    unmount::Departure,
    Failure, Service, Success,
};
use crate::{store::BoundWorkspace, Command, NativeJob, NativeReply, Response};
use layerfs_bridge::control::{Activity, ControlCode, NativePhase, TeardownStage, WorkspaceToken};
use layerfs_fuse::session::{Detach, SessionObserver};
use std::{fmt, sync::Arc};

/// Terminal admission over a connection-owning entry. No daemon-side request
/// count refuses here: the kernel's own reversible answer is the fence, because
/// every blocked caller, descriptor, working directory and mapping holds a
/// mount reference. Work the kernel does not count (asynchronous releases and
/// lookup decrements, or a replied request's last bookkeeping) is waited out
/// by the drain barrier after a known detach.
pub(super) fn take(native: &mut Native) -> Result<Departure, Failure> {
    match native {
        Native::Ready(_) => {}
        Native::Leaving(_) => {
            return Err(Failure::native(
                ControlCode::Busy,
                "unmount:admission",
                "terminal unmount already admitted",
            ))
        }
        Native::Unattached | Native::Attaching | Native::Retained(_) => {
            unreachable!("handled by terminal admission")
        }
    }
    let Native::Ready(attached) = std::mem::replace(native, Native::Attaching) else {
        unreachable!("checked serving connection")
    };
    *native = Native::Leaving(Box::new(Leaving {
        ready: attached.ready.clone(),
        observer: attached.session.observer(),
        phase: NativePhase::Probing,
    }));
    Ok(Departure::Session(attached))
}
impl Service {
    /// The session is serviced normally throughout the probe. Only a known
    /// detach is terminal; after it every stop keeps the exact owner.
    pub(super) fn unmount_native(
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
                    Native::Ready(Box::new(Attached { session, ready })),
                    Activity::Idle,
                );
                return Err(Failure::native(
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
                ))
            }
        }
        self.leaving(token, NativePhase::Draining);
        let drained = match session.drain() {
            Ok(drained) => drained,
            Err(undrained) => {
                return Err(self.retain(
                    token,
                    stage(undrained.stage),
                    format!(
                        "connection drain stopped at {:?}: loops {:?}, work {:?}",
                        undrained.stage, undrained.loops, undrained.work
                    ),
                    Some(observer),
                    undrained,
                ))
            }
        };
        // Drain barrier holds: one fixed logical mark, then indexed retirement.
        let revoked = match self.job(workspace.route(), Command::Native(NativeJob::Revoke(mount))) {
            Ok(done) if matches!(done.result(), Ok(Response::Native(NativeReply::Done))) => done,
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
                ));
            }
        };
        self.close(token, &workspace, vec![revoked], Some(drained))
    }
    /// Describes the exact retained native owner for an operator. Observation
    /// only: nothing is transferred, released, retried or settled.
    pub fn retained_native(&self, token: WorkspaceToken) -> Result<Option<String>, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        Ok(match &bound_mut(&mut entries, token)?.native {
            Native::Retained(kept) => Some(format!("{kept:?}")),
            _ => None,
        })
    }
    pub(super) fn leaving(&self, token: WorkspaceToken, phase: NativePhase) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Ok(binding) = bound_mut(&mut entries, token) {
                if let Native::Leaving(leaving) = &mut binding.native {
                    leaving.phase = phase;
                }
            }
        }
    }
    /// Poison recovery stores this exact owner only; a poisoned registry never
    /// authorizes another product operation.
    pub(super) fn settle(&self, token: WorkspaceToken, native: Native, activity: Activity) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(binding) = bound_mut(&mut entries, token) {
            binding.native = native;
            binding.activity = activity;
            binding.epoch = binding.epoch.saturating_add(1);
        }
    }
    /// Stops a terminal operation after effects, keeping its owner in the entry.
    pub(super) fn retain(
        &self,
        token: WorkspaceToken,
        stage: TeardownStage,
        detail: String,
        observer: Option<SessionObserver>,
        evidence: Box<dyn fmt::Debug + Send>,
    ) -> Failure {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut kept = Kept {
            ready: None,
            stage,
            detail,
            observer,
            evidence,
        };
        let custody = kept.custody(token);
        if let Ok(binding) = bound_mut(&mut entries, token) {
            if let Native::Leaving(leaving) = &binding.native {
                kept.ready = Some(leaving.ready.clone());
            }
            binding.native = Native::Retained(Box::new(kept));
            binding.epoch = binding.epoch.saturating_add(1);
        }
        Failure::Retained(Box::new(custody))
    }
}
