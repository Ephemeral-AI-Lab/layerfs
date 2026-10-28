//! One kernel Attach: engine mount owner, the mount's request owner, then a serving session.
use super::{
    kernel::Kernel,
    registry::{bound_mut, idle},
    serving::{ready, stage, Attached, Evidence, Kept, KernelServing},
    types::KernelFailure,
    Failure, Service, Success,
};
use crate::store::BoundWorkspace;
use layerfs_bridge::control::{
    Activity, ControlCode, Reply, TeardownStage, WorkspaceToken, MOUNT_DIRECTORY_LIMIT,
};
use layerfs_fuse::{
    attributes::Identity,
    session::{AttachRemainder, KernelSession, SessionConfig},
    DispatchError,
};
use layerfs_overlay::KernelMount;
use std::sync::Arc;

/// Where a failed Attach leaves the entry.
enum Stop {
    /// No engine, request, kernel or directory owner of this attempt remains.
    Unattached(Failure),
    /// The engine mount outcome is unknown: never a second attachment.
    Unknown(Failure),
    /// A connection or engine owner could not be disposed and is kept.
    Retained(Box<Kept>),
}
impl Service {
    /// Ready is acknowledged only after the kernel mount exists, the handshake
    /// completed, every receive loop entered and the registry records it.
    pub(super) fn attach(
        &self,
        token: WorkspaceToken,
        timing: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    ) -> Result<Success, Failure> {
        let Some(serving) = self.kernel.clone() else {
            return Err(Failure::Rejected(
                ControlCode::Invalid,
                "kernel serving not assembled",
            ));
        };
        let workspace = self.admit_attach(token)?;
        match crate::telemetry::phase(timing, "kernel.attach", || {
            self.attach_admitted(&serving, token, &workspace)
        }) {
            Ok(attached) => {
                let reply = Reply::Ready(Box::new(attached.ready.clone()));
                self.settle(token, Kernel::Ready(Box::new(attached)), Activity::Idle);
                Ok(Success::reply(reply))
            }
            Err(Stop::Unattached(failure)) => {
                self.settle(token, Kernel::Unattached, Activity::Idle);
                Err(failure)
            }
            Err(Stop::Unknown(failure)) => {
                self.settle(token, Kernel::Attaching, Activity::Uncertain);
                Err(failure)
            }
            Err(Stop::Retained(kept)) => {
                let custody = kept.custody(token);
                self.settle(token, Kernel::Retained(kept), Activity::Uncertain);
                Err(Failure::Retained(Box::new(custody)))
            }
        }
    }
    fn admit_attach(&self, token: WorkspaceToken) -> Result<Arc<BoundWorkspace>, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let value = bound_mut(&mut entries, token)?;
        if let Kernel::Retained(kept) = &value.kernel {
            return Err(Failure::Retained(Box::new(kept.custody(token))));
        }
        idle(value)?;
        if !matches!(value.kernel, Kernel::Unattached) {
            return Err(Failure::kernel(
                ControlCode::Invalid,
                "attach:admission",
                "Workspace already owns a kernel connection",
            ));
        }
        value.kernel = Kernel::Attaching;
        value.activity = Activity::Attaching;
        value.epoch = value.epoch.saturating_add(1);
        Ok(value.workspace.clone())
    }
    fn attach_admitted(
        &self,
        serving: &KernelServing,
        token: WorkspaceToken,
        workspace: &Arc<BoundWorkspace>,
    ) -> Result<Attached, Stop> {
        let directory = serving.directory(token);
        let Some(text) = directory
            .to_str()
            .filter(|text| text.len() <= MOUNT_DIRECTORY_LIMIT)
            .map(str::to_owned)
        else {
            return Err(Stop::Unattached(Failure::kernel(
                ControlCode::Invalid,
                "attach:directory",
                "mount directory exceeds the control record bound",
            )));
        };
        let root = workspace
            .operation()
            .and_then(|operation| operation.workspace().base())
            .map(|base| base.root().root_inode().serial())
            .map_err(|error| Stop::Unattached(Failure::Workspace(error)))?;
        let unknown = |failure: Failure| {
            if failure.uncertain() {
                Stop::Unknown(failure)
            } else {
                Stop::Unattached(failure)
            }
        };
        // Prepare the initial authoritative range before any kernel request
        // can create an inode. Bind stays a read-only snapshot operation.
        let operation = workspace
            .operation()
            .map_err(|error| Stop::Unattached(Failure::Workspace(error)))?;
        if let Err(error) = operation.workspace().prepare_serials(operation.ports()) {
            let failure = match operation.ports().failure() {
                Ok(Some(cause)) => match cause.as_ref() {
                    crate::store::PortError::History(error) => Failure::History(error.clone()),
                    _ => Failure::Workspace(error),
                },
                Ok(None) | Err(_) => Failure::Workspace(error),
            };
            return Err(unknown(failure));
        }
        drop(operation);
        let mount = self
            .job(
                workspace.route(),
                |job_client: &crate::OwnerClient, job_route| {
                    job_client.kernel_mount(job_route, root)
                },
            )
            .map_err(unknown)?;
        let failed = |code, phase, detail: String| KernelFailure {
            code,
            phase,
            detail,
            evidence: None,
        };
        let queue = match serving.register(mount) {
            Ok(queue) => queue,
            Err(error) => {
                let code = if error == DispatchError::Capacity {
                    ControlCode::Capacity
                } else {
                    ControlCode::Failed
                };
                let failure = failed(code, "attach:mount", format!("mount lane: {error:?}"));
                return Err(self.unwind(workspace, mount, failure));
            }
        };
        let config = &serving.config;
        let attached = KernelSession::attach(
            SessionConfig {
                directory,
                owner_uid: config.owner_uid,
                owner_gid: config.owner_gid,
                ready_wait: config.ready_wait,
                drain_wait: config.drain_wait,
                control: config.control.clone(),
            },
            queue,
            Arc::new(crate::service::Mounted(workspace.clone())),
            Identity {
                root,
                uid: config.command_uid,
                gid: config.command_gid,
            },
        );
        let failure = match attached {
            Ok(session) => {
                return match ready(token, text, &session) {
                    Some(ready) => Ok(Attached { session, ready }),
                    // A serving session always recorded both receipts; keep the
                    // owner rather than guess if that invariant is ever broken.
                    None => Err(Stop::Retained(Box::new(Kept {
                        ready: None,
                        stage: TeardownStage::Detach,
                        detail: "serving session without negotiation receipt".into(),
                        observer: Some(session.observer()),
                        evidence: Evidence::Other(Box::new(session)),
                        forced: None,
                    }))),
                };
            }
            Err(failure) => failure,
        };
        let detail = failure.to_string();
        let kept = match &failure.remainder {
            AttachRemainder::Retained(undrained) => {
                Some((stage(undrained.stage), Some(undrained.session.observer())))
            }
            AttachRemainder::Unmounted { lane: Some(_), .. } => Some((TeardownStage::Lane, None)),
            AttachRemainder::Unmounted { lane: None, .. } | AttachRemainder::Disposed(_) => None,
        };
        if let Some((stage, observer)) = kept {
            return Err(Stop::Retained(Box::new(Kept {
                ready: None,
                stage,
                detail,
                observer,
                evidence: Evidence::Other(Box::new((failure, mount))),
                forced: None,
            })));
        }
        let phase = if failure.mounted {
            "attach:session"
        } else {
            "attach:mount"
        };
        let mut refusal = failed(ControlCode::Failed, phase, detail);
        refusal.evidence = Some(failure);
        Err(self.unwind(workspace, mount, refusal))
    }
    /// No connection or request owner of this attempt remains: release its engine owner
    /// so the entry returns to Unattached. An unacknowledged release is kept.
    fn unwind(
        &self,
        workspace: &BoundWorkspace,
        mount: KernelMount,
        failure: KernelFailure,
    ) -> Stop {
        match self.job(
            workspace.route(),
            |job_client: &crate::OwnerClient, job_route| job_client.kernel_revoke(job_route, mount),
        ) {
            Ok(()) => Stop::Unattached(Failure::Kernel(Box::new(failure))),
            other => Stop::Retained(Box::new(Kept {
                ready: None,
                stage: TeardownStage::Revoke,
                detail: failure.detail.clone(),
                observer: None,
                evidence: Evidence::Other(Box::new((failure, mount, other))),
                forced: None,
            })),
        }
    }
}
