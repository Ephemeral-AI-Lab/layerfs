//! One native Attach: engine mount owner, shared lane, then a serving session.
use super::{
    native::Native,
    registry::{bound_mut, idle},
    serving::{ready, stage, Attached, Kept, NativeServing},
    types::NativeFailure,
    Failure, Service, Success,
};
use crate::{store::BoundWorkspace, Command, Completion, NativeJob, NativeReply, Response};
use layerfs_bridge::control::{
    Activity, ControlCode, Reply, TeardownStage, WorkspaceToken, MOUNT_DIRECTORY_LIMIT,
};
use layerfs_fuse::{
    attributes::Identity,
    session::{AttachRemainder, NativeSession, SessionConfig},
    DispatchError,
};
use layerfs_overlay::NativeMount;
use std::sync::Arc;

/// Where a failed Attach leaves the entry.
enum Stop {
    /// No engine, lane, kernel or directory owner of this attempt remains.
    Unattached(Failure),
    /// The engine mount outcome is unknown: never a second attachment.
    Unknown(Failure),
    /// A connection or engine owner could not be disposed and is kept.
    Retained(Box<Kept>),
}
impl Service {
    /// Ready is acknowledged only after the kernel mount exists, the handshake
    /// completed, every receive loop entered and the registry records it.
    pub(super) fn attach(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        let Some(serving) = self.native.clone() else {
            return Err(Failure::Rejected(
                ControlCode::Invalid,
                "native serving not assembled",
            ));
        };
        let workspace = self.admit_attach(token)?;
        match self.attach_admitted(&serving, token, &workspace) {
            Ok((attached, created)) => {
                let reply = Reply::Ready(Box::new(attached.ready.clone()));
                self.settle(token, Native::Ready(Box::new(attached)), Activity::Idle);
                Ok(Success {
                    completion: Some(created),
                    ..Success::reply(reply)
                })
            }
            Err(Stop::Unattached(failure)) => {
                self.settle(token, Native::Unattached, Activity::Idle);
                Err(failure)
            }
            Err(Stop::Unknown(failure)) => {
                self.settle(token, Native::Attaching, Activity::Uncertain);
                Err(failure)
            }
            Err(Stop::Retained(kept)) => {
                let custody = kept.custody(token);
                self.settle(token, Native::Retained(kept), Activity::Uncertain);
                Err(Failure::Retained(Box::new(custody)))
            }
        }
    }
    fn admit_attach(&self, token: WorkspaceToken) -> Result<Arc<BoundWorkspace>, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let value = bound_mut(&mut entries, token)?;
        if let Native::Retained(kept) = &value.native {
            return Err(Failure::Retained(Box::new(kept.custody(token))));
        }
        idle(value)?;
        if !matches!(value.native, Native::Unattached) {
            return Err(Failure::native(
                ControlCode::Invalid,
                "attach:admission",
                "Workspace already owns a native connection",
            ));
        }
        value.native = Native::Attaching;
        value.activity = Activity::Attaching;
        value.epoch = value.epoch.saturating_add(1);
        Ok(value.workspace.clone())
    }
    fn attach_admitted(
        &self,
        serving: &NativeServing,
        token: WorkspaceToken,
        workspace: &Arc<BoundWorkspace>,
    ) -> Result<(Attached, Completion), Stop> {
        let directory = serving.directory(token);
        let Some(text) = directory
            .to_str()
            .filter(|text| text.len() <= MOUNT_DIRECTORY_LIMIT)
            .map(str::to_owned)
        else {
            return Err(Stop::Unattached(Failure::native(
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
        let created = self
            .job(
                workspace.route(),
                Command::Native(NativeJob::Mount { root }),
            )
            .map_err(unknown)?;
        let mount = match created.result() {
            Ok(Response::Native(NativeReply::Mount(mount))) => *mount,
            _ => return Err(unknown(Failure::Completion(Box::new(created)))),
        };
        let failed = |code, phase, detail: String| NativeFailure {
            code,
            phase,
            detail,
            completions: Vec::new(),
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
                return Err(self.unwind(workspace, mount, created, failure));
            }
        };
        let config = &serving.config;
        let attached = NativeSession::attach(
            SessionConfig {
                directory,
                owner_uid: config.owner_uid,
                owner_gid: config.owner_gid,
                ready_wait: config.ready_wait,
                drain_wait: config.drain_wait,
            },
            queue,
            workspace.clone(),
            Identity {
                root,
                uid: config.command_uid,
                gid: config.command_gid,
            },
        );
        let failure = match attached {
            Ok(session) => {
                return match ready(token, text, &session) {
                    Some(ready) => Ok((Attached { session, ready }, created)),
                    // A serving session always recorded both receipts; keep the
                    // owner rather than guess if that invariant is ever broken.
                    None => Err(Stop::Retained(Box::new(Kept {
                        ready: None,
                        stage: TeardownStage::Detach,
                        detail: "serving session without negotiation receipt".into(),
                        observer: Some(session.observer()),
                        evidence: Box::new((session, created)),
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
                evidence: Box::new((failure, created)),
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
        Err(self.unwind(workspace, mount, created, refusal))
    }
    /// No connection or lane of this attempt remains: release its engine owner
    /// so the entry returns to Unattached. An unacknowledged release is kept.
    fn unwind(
        &self,
        workspace: &BoundWorkspace,
        mount: NativeMount,
        created: Completion,
        mut failure: NativeFailure,
    ) -> Stop {
        match self.job(workspace.route(), Command::Native(NativeJob::Revoke(mount))) {
            Ok(done) if matches!(done.result(), Ok(Response::Native(NativeReply::Done))) => {
                failure.completions = vec![created, done];
                Stop::Unattached(Failure::Native(Box::new(failure)))
            }
            other => Stop::Retained(Box::new(Kept {
                ready: None,
                stage: TeardownStage::Revoke,
                detail: failure.detail.clone(),
                observer: None,
                evidence: Box::new((failure, created, other)),
                forced: None,
            })),
        }
    }
}
