//! Fresh failed-demand ownership over the original shared Workspace/cache binding.
use super::Upstream;
use crate::OwnerClient;
use layerfs_sdk::client::{PortFailure, RemoteLengths, RemoteObjects, RemoteSerials};
use layerfs_workspace::{CanonicalClient, Workspace, WorkspaceError};
use std::{
    fmt,
    sync::{Arc, MutexGuard},
};

/// One content/Workspace operation's fresh object failure owner and scoped view.
/// It shares successful immutable cache and current base/serials, not first failure.
pub struct UpstreamOperation {
    objects: Arc<RemoteObjects>,
    client: Arc<CanonicalClient>,
    workspace: Workspace,
    owner: OwnerClient,
    lengths: Arc<RemoteLengths>,
    serials: RemoteSerials,
}
/// Original scope-construction refusal and already-created operation provider.
pub struct OperationRefusal {
    /// Original Workspace scope error.
    pub error: WorkspaceError,
    /// Original operation provider and its first failure/credited result, if any.
    pub objects: Arc<RemoteObjects>,
    /// Original shared-cache client; no error-driven alternate provider is created.
    pub client: Arc<CanonicalClient>,
}
/// Successful operation and retained owner for any returned source/plan/result.
pub struct OperationSuccess<T> {
    /// Original successful value.
    pub value: T,
    /// Exact operation owner, retained until the caller's actual last use.
    pub operation: UpstreamOperation,
}
/// Original Content/Workspace error and its operation's exact provider custody.
pub struct OperationFailure<E> {
    /// Original error, without replacement by a display diagnostic.
    pub error: E,
    /// Original operation provider/view/ports, preserving PortFailure credit.
    pub operation: UpstreamOperation,
}
impl Upstream {
    /// Creates one ordinary saved-base operation. Same-Save pending visibility
    /// is a separate Commit capability and is not inserted in this shared cache.
    pub fn operation(&self) -> Result<UpstreamOperation, OperationRefusal> {
        let objects = Arc::new(RemoteObjects::new(self.calls.clone(), None));
        let lengths = Arc::new(RemoteLengths::new(self.calls.clone()));
        let client = Arc::new(CanonicalClient::with_cache(
            objects.clone(),
            Some(lengths.clone()),
            self.cache.clone(),
        ));
        let workspace = match self.workspace.scoped(client.clone()) {
            Ok(workspace) => workspace,
            Err(error) => {
                return Err(OperationRefusal {
                    error,
                    objects,
                    client,
                })
            }
        };
        Ok(UpstreamOperation {
            objects,
            client,
            workspace,
            owner: self.owner.clone(),
            lengths,
            serials: RemoteSerials::new(self.calls.clone()),
        })
    }
}
impl UpstreamOperation {
    /// Scoped current Workspace; all ordinary reads use this fresh provider.
    pub const fn workspace(&self) -> &Workspace {
        &self.workspace
    }
    /// Existing OverlayRead/OverlayJobs/OverlayFileRead owner ports.
    pub const fn overlay(&self) -> &OwnerClient {
        &self.owner
    }
    /// Shared-cache canonical source for this operation's public Content APIs.
    pub fn client(&self) -> &CanonicalClient {
        &self.client
    }
    /// Existing cheap saved-file length port over the same authenticated Calls.
    pub fn lengths(&self) -> &RemoteLengths {
        &self.lengths
    }
    /// Existing scope allocator; Workspace consumes shared ranges independently.
    pub const fn serials(&self) -> &RemoteSerials {
        &self.serials
    }
    /// Original first object/transport/refusal cause and credited Message, if any.
    pub fn failure(
        &self,
    ) -> Result<MutexGuard<'_, Option<PortFailure>>, layerfs_bridge::contract::FrameError> {
        self.objects.failure()
    }
    /// Runs caller-selected public operations without a provider/registry lock.
    /// Both outcomes retain the operation owner; failures never trigger replay.
    pub fn run<T, E>(
        self,
        f: impl FnOnce(&Self) -> Result<T, E>,
    ) -> Result<OperationSuccess<T>, OperationFailure<E>> {
        match f(&self) {
            Ok(value) => Ok(OperationSuccess {
                value,
                operation: self,
            }),
            Err(error) => Err(OperationFailure {
                error,
                operation: self,
            }),
        }
    }
}
impl fmt::Debug for UpstreamOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpstreamOperation")
            .field("route", &self.workspace.route())
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for OperationRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationRefusal")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl<T: fmt::Debug> fmt::Debug for OperationSuccess<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationSuccess")
            .field("value", &self.value)
            .field("operation", &self.operation)
            .finish()
    }
}
impl<E: fmt::Debug> fmt::Debug for OperationFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationFailure")
            .field("error", &self.error)
            .field("operation", &self.operation)
            .finish()
    }
}
