//! Hidden original-channel access for recovery, diagnostics and external proofs.
pub use super::exchange::{Control, ControlCause, ControlFailure, ControlPhase};
use crate::{BoundWorkspace, OperationFailure, WorkspaceApi};
use layerfs_history::{BranchId, WorkspaceId};
pub use layerfs_project::store::install;
use layerfs_protocol::{
    daemon::{CleanupObservation, MountPlace, ReadyMount, WorkspaceStatus, WorkspaceToken},
    duplex::Connection,
};
use std::ops::{Deref, DerefMut};

/// Diagnostic facade over the same operation owner, without channel policy changes.
pub struct InternalWorkspaceApi<'a>(WorkspaceApi<'a>);
/// Borrows an original channel without reconnecting or resetting correlation.
pub fn workspace(control: &mut Control) -> InternalWorkspaceApi<'_> {
    InternalWorkspaceApi(WorkspaceApi {
        control,
        place: MountPlace::default(),
    })
}
impl<'a> Deref for InternalWorkspaceApi<'a> {
    type Target = WorkspaceApi<'a>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for InternalWorkspaceApi<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl InternalWorkspaceApi<'_> {
    /// Gives a mount through this channel the place it belongs to (its
    /// Sandbox, machine and the generation its record named, when known); a
    /// mount then registers them beside its Workspace.
    #[must_use]
    pub fn placed(mut self, place: MountPlace) -> Self {
        self.0.place = place;
        self
    }
    /// Original logical bind for component proofs; kernel mount still uses both steps.
    pub fn bind(
        &mut self,
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> Result<BoundWorkspace, Box<OperationFailure>> {
        self.0.bind(workspace, branch)
    }
    /// Original attachment for separately acknowledged ownership proofs.
    pub fn attach(&mut self, token: WorkspaceToken) -> Result<ReadyMount, Box<OperationFailure>> {
        self.0.attach(token)
    }
    /// Observes a lost mount acknowledgement without replay.
    pub fn locate(
        &mut self,
        workspace: WorkspaceId,
    ) -> Result<WorkspaceStatus, Box<OperationFailure>> {
        self.0.locate(workspace)
    }
    /// Observes automatic local physical reclamation once.
    pub fn cleanup(
        &mut self,
        token: WorkspaceToken,
    ) -> Result<CleanupObservation, Box<OperationFailure>> {
        self.0.cleanup(token)
    }
}
/// Transfers the original terminal connection only for its fence proof.
pub fn into_connection(control: Control) -> Connection {
    control.into_connection()
}

/// Transfers provider runtime, original channel and admitted command identity.
#[cfg(unix)]
pub fn split_provider(
    sandbox: crate::ProviderSandbox,
) -> (Box<crate::Sandbox>, Control, crate::CommandIdentity) {
    (sandbox.runtime, sandbox.control, sandbox.command_identity)
}
