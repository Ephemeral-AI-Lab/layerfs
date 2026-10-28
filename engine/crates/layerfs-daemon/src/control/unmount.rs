//! Normal terminal unmount: reversible probe, complete drain, then Close.
use super::{
    kernel::Kernel,
    registry::{bound_mut, idle},
    Failure, KernelEvidence, Service, Success,
};
use crate::store::BoundWorkspace;
use layerfs_bridge::control::{Activity, ForcedFacts, Reply, UnmountControl, WorkspaceToken};
use std::sync::Arc;

/// Original kernel connection receipt carried to Close.
type Receipt = Option<Box<KernelEvidence>>;

/// What terminal admission took out of the registry entry.
pub(super) enum Departure {
    /// No kernel mount is owned: logical Close is the whole operation.
    Unattached,
    #[cfg(target_os = "linux")]
    Session(Box<super::serving::Attached>),
}
impl Service {
    /// Refuses uncommitted changes with their paths unless `discard`.
    pub(crate) fn unmount(&self, control: UnmountControl) -> Result<Success, Failure> {
        let token = control.token;
        if !control.discard {
            if let Some(report) = self.dirty(token)? {
                return Ok(Success::reply(Reply::Dirty(Box::new(report))));
            }
        }
        let (workspace, departure) = self.admit_unmount(token)?;
        match departure {
            Departure::Unattached => {}
            #[cfg(target_os = "linux")]
            Departure::Session(attached) => {
                return self.unmount_kernel(token, workspace, *attached)
            }
        };
        self.close(token, &workspace, None)
    }
    /// One short registry step: no effect unless the entry is idle and its
    /// kernel half is either absent or a serving connection with no request.
    fn admit_unmount(
        &self,
        token: WorkspaceToken,
    ) -> Result<(Arc<BoundWorkspace>, Departure), Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let value = bound_mut(&mut entries, token)?;
        // Retained custody answers before ordinary activity admission.
        #[cfg(target_os = "linux")]
        if let Kernel::Retained(kept) = &value.kernel {
            return Err(Failure::Retained(Box::new(kept.custody(token))));
        }
        idle(value)?;
        let departure = match value.kernel {
            Kernel::Unattached => Departure::Unattached,
            #[cfg(target_os = "linux")]
            Kernel::Attaching => {
                return Err(Failure::kernel(
                    layerfs_bridge::control::ControlCode::Unknown,
                    "unmount:admission",
                    "original Attach outcome retained",
                ))
            }
            #[cfg(target_os = "linux")]
            _ => Departure::Session(super::detach::take(
                &mut value.kernel,
                layerfs_bridge::control::MountPhase::Probing,
            )?),
        };
        value.activity = Activity::Closing;
        value.epoch = value.epoch.saturating_add(1);
        Ok((value.workspace.clone(), departure))
    }
    /// Logical Close and routing removal, after every kernel owner is revoked.
    pub(super) fn close(
        &self,
        token: WorkspaceToken,
        workspace: &BoundWorkspace,
        kernel: Receipt,
    ) -> Result<Success, Failure> {
        let kernel = self.closed(token, workspace, kernel, None)?;
        self.depart(
            token,
            Success {
                reply: Reply::Unmounted(token),
                commit: None,
                observation_failure: None,
                kernel,
            },
        )
    }
    /// The one logical Close. Acknowledged, it hands back its completion with
    /// the receipts it was given; otherwise the entry keeps them.
    pub(super) fn closed(
        &self,
        token: WorkspaceToken,
        workspace: &BoundWorkspace,
        kernel: Receipt,
        forced: Option<ForcedFacts>,
    ) -> Result<Receipt, Failure> {
        match workspace.close() {
            Ok(()) => Ok(kernel),
            Err(error) => {
                let failure = match error {
                    crate::store::CloseError::Owner(cause) => Failure::Owner(cause),
                    crate::store::CloseError::Reader(cause) => Failure::Workspace(
                        layerfs_workspace::WorkspaceError::Service(Box::new(cause)),
                    ),
                };
                Err(self.unclosed(token, failure, kernel, forced))
            }
        }
    }
    /// Routing removal after an acknowledged Close; the reply is already known.
    pub(super) fn depart(
        &self,
        token: WorkspaceToken,
        success: Success,
    ) -> Result<Success, Failure> {
        if self
            .entries
            .lock()
            .map(|mut entries| entries.remove(&token.workspace))
            .is_err()
        {
            return Err(Failure::After {
                cause: Box::new(Failure::Poisoned),
                original: Box::new(success),
            });
        }
        if let Err(cause) = self.owner.retire_namespace(token.namespace) {
            return Err(Failure::After {
                cause: Box::new(Failure::Owner(cause)),
                original: Box::new(success),
            });
        }
        Ok(success)
    }
    /// Close was not acknowledged. Without a connection the entry returns to
    /// its prior usable state; after a detach it keeps the exact custody.
    fn unclosed(
        &self,
        token: WorkspaceToken,
        failure: Failure,
        kernel: Receipt,
        forced: Option<ForcedFacts>,
    ) -> Failure {
        #[cfg(target_os = "linux")]
        if kernel.is_some() {
            return self.retain(
                token,
                layerfs_bridge::control::TeardownStage::Close,
                failure.wire().detail,
                None,
                Box::new((failure, kernel)),
                forced,
            );
        }
        #[cfg(not(target_os = "linux"))]
        let _ = (kernel, forced);
        if let Ok(mut entries) = self.entries.lock() {
            if let Ok(binding) = bound_mut(&mut entries, token) {
                binding.activity = if failure.uncertain() {
                    Activity::Uncertain
                } else {
                    Activity::Idle
                };
                binding.epoch = binding.epoch.saturating_add(1);
            }
        }
        failure
    }
    /// Attach is a Linux capability; elsewhere it is refused before effects.
    #[cfg(not(target_os = "linux"))]
    pub(super) fn attach(
        &self,
        _token: WorkspaceToken,
        _timing: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    ) -> Result<Success, Failure> {
        Err(Failure::Rejected(
            layerfs_bridge::control::ControlCode::Invalid,
            "kernel serving requires Linux",
        ))
    }
}
