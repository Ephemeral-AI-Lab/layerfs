//! Native attachment: Ready is acknowledged separately from the logical binding.
use super::{BoundWorkspace, WorkspaceApi};
use crate::OperationFailure;
use layerfs_bridge::control::{ReadyMount, Reply, Request, WorkspaceStatus, WorkspaceToken};
use layerfs_history::{BranchId, WorkspaceId};

/// A Workspace that is both bound and natively Ready.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountedWorkspace {
    /// First acknowledgement: the Store/engine binding.
    pub bound: BoundWorkspace,
    /// Second acknowledgement: mount directory and negotiated kernel profile.
    pub ready: ReadyMount,
}
/// A mount stopped at its second acknowledgement. The binding is known and
/// stays bound; whether a native connection exists is exactly what `attach`
/// records. Nothing was rebound, re-attached or unmounted on the caller's behalf.
#[derive(Debug)]
pub struct MountFailure {
    /// Original acknowledged binding, when the first step completed.
    pub bound: Option<BoundWorkspace>,
    /// Original failure of the step that stopped: bind when `bound` is absent.
    pub failure: Box<OperationFailure>,
}
impl WorkspaceApi<'_> {
    /// Attaches one bound Workspace once. Ready means the kernel mount exists,
    /// the handshake completed and every receive loop is serving. A lost reply
    /// leaves the outcome unknown; observe it with `locate`, never by replay.
    pub fn attach(&mut self, token: WorkspaceToken) -> Result<ReadyMount, Box<OperationFailure>> {
        let request = Request::Attach(token);
        match self.exchange(request.clone())? {
            Reply::Ready(ready) => Ok(*ready),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
    /// Observes one incarnation by identity alone, for a caller whose binding
    /// or attachment acknowledgement was lost. It binds and attaches nothing.
    pub fn locate(
        &mut self,
        workspace: WorkspaceId,
    ) -> Result<WorkspaceStatus, Box<OperationFailure>> {
        let request = Request::Locate(workspace);
        match self.exchange(request.clone())? {
            Reply::Located(status) => Ok(*status),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
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
