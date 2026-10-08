//! Normal terminal unmount: reversible probe, complete drain, then Close.
use super::{
    native::Native,
    registry::{bound_mut, idle},
    Failure, Service, Success,
};
use crate::{store::BoundWorkspace, Command, Completion, Response};
use layerfs_bridge::control::{Activity, ForcedFacts, Reply, WorkspaceToken};
use std::{fmt, sync::Arc};

/// Original native connection receipt carried to Close.
type Receipt = Option<Box<dyn fmt::Debug + Send>>;

/// What terminal admission took out of the registry entry.
pub(super) enum Departure {
    /// No kernel mount is owned: logical Close is the whole operation.
    Unattached,
    #[cfg(target_os = "linux")]
    Session(Box<super::serving::Attached>),
}
impl Service {
    pub(super) fn unmount(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        let (workspace, departure) = self.admit_unmount(token)?;
        let earlier = match departure {
            Departure::Unattached => Vec::new(),
            #[cfg(target_os = "linux")]
            Departure::Session(attached) => {
                return self.unmount_native(token, workspace, *attached)
            }
        };
        self.close(token, &workspace, earlier, None)
    }
    /// One short registry step: no effect unless the entry is idle and its
    /// native half is either absent or a serving connection with no request.
    fn admit_unmount(
        &self,
        token: WorkspaceToken,
    ) -> Result<(Arc<BoundWorkspace>, Departure), Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let value = bound_mut(&mut entries, token)?;
        // Retained custody answers before ordinary activity admission.
        #[cfg(target_os = "linux")]
        if let Native::Retained(kept) = &value.native {
            return Err(Failure::Retained(Box::new(kept.custody(token))));
        }
        idle(value)?;
        let departure = match value.native {
            Native::Unattached => Departure::Unattached,
            #[cfg(target_os = "linux")]
            Native::Attaching => {
                return Err(Failure::native(
                    layerfs_bridge::control::ControlCode::Unknown,
                    "unmount:admission",
                    "original Attach outcome retained",
                ))
            }
            #[cfg(target_os = "linux")]
            _ => Departure::Session(super::detach::take(
                &mut value.native,
                layerfs_bridge::control::NativePhase::Probing,
            )?),
        };
        value.activity = Activity::Closing;
        value.epoch = value.epoch.saturating_add(1);
        Ok((value.workspace.clone(), departure))
    }
    /// Logical Close and routing removal, after every native owner is revoked.
    pub(super) fn close(
        &self,
        token: WorkspaceToken,
        workspace: &BoundWorkspace,
        earlier: Vec<Completion>,
        native: Receipt,
    ) -> Result<Success, Failure> {
        let (completion, earlier, native) = self.closed(token, workspace, earlier, native, None)?;
        self.depart(
            token,
            Success {
                reply: Reply::Unmounted(token),
                completion: Some(completion),
                earlier,
                commit: None,
                observation_failure: None,
                native,
            },
        )
    }
    /// The one logical Close. Acknowledged, it hands back its completion with
    /// the receipts it was given; otherwise the entry keeps them.
    pub(super) fn closed(
        &self,
        token: WorkspaceToken,
        workspace: &BoundWorkspace,
        earlier: Vec<Completion>,
        native: Receipt,
        forced: Option<ForcedFacts>,
    ) -> Result<(Completion, Vec<Completion>, Receipt), Failure> {
        match self.job(workspace.route(), Command::Close) {
            Ok(done) if matches!(done.result(), Ok(Response::Done)) => Ok((done, earlier, native)),
            other => {
                let failure = match other {
                    Ok(done) => Failure::Completion(Box::new(done)),
                    Err(error) => error,
                };
                Err(self.unclosed(token, failure, earlier, native, forced))
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
        Ok(success)
    }
    /// Close was not acknowledged. Without a connection the entry returns to
    /// its prior usable state; after a detach it keeps the exact custody.
    fn unclosed(
        &self,
        token: WorkspaceToken,
        failure: Failure,
        earlier: Vec<Completion>,
        native: Receipt,
        forced: Option<ForcedFacts>,
    ) -> Failure {
        #[cfg(target_os = "linux")]
        if native.is_some() || !earlier.is_empty() {
            return self.retain(
                token,
                layerfs_bridge::control::TeardownStage::Close,
                failure.wire().detail,
                None,
                Box::new((failure, earlier, native)),
                forced,
            );
        }
        #[cfg(not(target_os = "linux"))]
        let _ = (earlier, native, forced);
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
    pub(super) fn attach(&self, _token: WorkspaceToken) -> Result<Success, Failure> {
        Err(Failure::Rejected(
            layerfs_bridge::control::ControlCode::Invalid,
            "native serving requires Linux",
        ))
    }
}
