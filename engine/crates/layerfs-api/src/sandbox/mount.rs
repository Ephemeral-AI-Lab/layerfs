//! Logical binding and kernel mount acknowledgements.
use crate::{internal::Control, OperationFailure};
use layerfs_bridge::control::{ReadyMount, Reply, Request, WorkspaceToken};
use layerfs_history::{BranchId, BranchSnapshot, WorkspaceId};

/// Explicit filesystem operations on one caller-owned sequential control channel.
/// Concurrent callers use separate authenticated channels. This owns no process.
pub struct WorkspaceApi<'a> {
    pub(crate) control: &'a mut Control,
    pub(crate) observation_scope: Option<&'a [u8; 32]>,
}
impl<'a> WorkspaceApi<'a> {
    /// Borrows this Sandbox's original authenticated filesystem control owner.
    #[cfg(unix)]
    pub fn new(sandbox: &'a mut crate::ManagedSandbox) -> Self {
        Self {
            control: &mut sandbox.control,
            observation_scope: None,
        }
    }
    pub(crate) fn exchange(
        &mut self,
        request: Request,
    ) -> Result<Reply, Box<crate::OperationFailure>> {
        let request = match self.observation_scope {
            Some(scope) => Request::Observed {
                scope: *scope,
                request: Box::new(request),
                traced: false,
            },
            None => request,
        };
        crate::base::failure::exchange(self.control, request)
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
impl WorkspaceApi<'_> {
    /// Two separately acknowledged attempts: bind, then attach. Each keeps its
    /// own original failure; a failed attach leaves the Workspace bound.
    pub fn mount(
        &mut self,
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> Result<MountedWorkspace, Box<MountFailure>> {
        let bound = self.bind(workspace, branch).map_err(|failure| {
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
