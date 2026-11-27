//! Logical binding and kernel mount acknowledgements.
use crate::{internal::Control, OperationFailure};
use layerfs_history::{BranchId, BranchSnapshot, WorkspaceId};
use layerfs_protocol::daemon::{MountPlace, ReadyMount, Reply, Request, WorkspaceToken};
use std::fmt;

/// Explicit filesystem operations on one caller-owned sequential control channel.
/// Concurrent callers use separate authenticated channels. This owns no process.
pub struct WorkspaceApi<'a> {
    pub control: &'a mut Control,
    /// The Sandbox and machine a mount through this channel belongs to,
    /// when its caller knows them: the daemon registers the holder with
    /// them. Both absent when the caller knows neither.
    pub place: MountPlace,
}
impl<'a> WorkspaceApi<'a> {
    /// Borrows this Sandbox's original authenticated filesystem control owner.
    #[cfg(unix)]
    pub fn new(sandbox: &'a mut crate::ProviderSandbox) -> Self {
        Self::from_control(&mut sandbox.control)
    }
    /// Borrows an authenticated control channel directly.
    #[cfg(unix)]
    pub fn from_control(control: &'a mut Control) -> Self {
        Self {
            control,
            place: MountPlace::default(),
        }
    }
    /// Gives a mount through this channel the place it belongs to (its
    /// Sandbox, machine and the generation its record named, when known); a
    /// mount then registers them beside its Workspace.
    #[must_use]
    pub fn placed(mut self, place: MountPlace) -> Self {
        self.place = place;
        self
    }
    pub fn exchange(&mut self, request: Request) -> Result<Reply, Box<crate::OperationFailure>> {
        crate::route::daemon::failure::exchange(self.control, request)
    }
}

/// Original logical binding. It carries no mounted location or kernel readiness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundWorkspace {
    /// Exact acknowledged namespace/incarnation.
    pub token: WorkspaceToken,
    /// Coherent selected Branch/root snapshot, without a whole-tree scan.
    pub binding: BranchSnapshot,
}

/// A Workspace that is both bound and kernelly Ready.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountedWorkspace {
    /// First acknowledgement: the Store/engine binding.
    pub bound: BoundWorkspace,
    /// Second acknowledgement: mount directory and negotiated kernel profile.
    pub ready: ReadyMount,
}
/// A mount stopped at its second acknowledgement. The binding is known and
/// stays bound; whether a kernel connection exists is exactly what `attach`
/// records. Nothing was rebound, re-attached or unmounted on the caller's behalf.
#[derive(Debug)]
pub struct MountFailure {
    /// Original acknowledged binding, when the first step completed.
    pub bound: Option<BoundWorkspace>,
    /// Original failure of the step that stopped: bind when `bound` is absent.
    pub failure: Box<OperationFailure>,
}
/// In plain words, never `Debug` text: the failed step's cause, and
/// whether the Workspace stayed bound.
impl fmt::Display for MountFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.failure)?;
        if self.bound.is_some() {
            f.write_str("; the Workspace stays bound, not attached")?;
        }
        Ok(())
    }
}
impl WorkspaceApi<'_> {
    /// Two separately acknowledged attempts: bind, then attach. Each keeps its
    /// own original failure; a failed attach leaves the Workspace bound.
    pub fn mount(
        &mut self,
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> Result<MountedWorkspace, Box<MountFailure>> {
        let bound = self.bind(workspace, branch);
        self.attached(bound)
    }
    /// [`Self::mount`] of Branch `branch` of project `project`, which the
    /// daemon finds by name (#64): the caller reads no Store.
    pub fn mount_named(
        &mut self,
        workspace: WorkspaceId,
        (project, branch): (&str, &str),
    ) -> Result<MountedWorkspace, Box<MountFailure>> {
        let bound = self.bind_named(workspace, project, branch);
        self.attached(bound)
    }
    /// The attach step of a mount whose bind answered `bound`.
    fn attached(
        &mut self,
        bound: Result<BoundWorkspace, Box<OperationFailure>>,
    ) -> Result<MountedWorkspace, Box<MountFailure>> {
        let bound = bound.map_err(|failure| {
            Box::new(MountFailure {
                bound: None,
                failure,
            })
        })?;
        match self.attach(bound.token) {
            Ok(ready) => Ok(MountedWorkspace { bound, ready }),
            Err(failure) => Err(Box::new(MountFailure {
                bound: Some(bound),
                failure,
            })),
        }
    }
}
