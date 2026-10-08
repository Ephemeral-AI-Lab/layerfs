//! Fresh operation custody over shared Workspace identity, base and serial ranges.
use super::{Store, StorePorts};
use crate::OwnerClient;
use layerfs_history::{BranchSnapshot, WorkspaceId};
use layerfs_workspace::{CanonicalClient, Workspace, WorkspaceResult};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

pub struct BoundWorkspace {
    pub(super) store: Arc<Store>,
    pub(super) owner: OwnerClient,
    pub(super) snapshot: Mutex<BranchSnapshot>,
    pub(super) identity: WorkspaceId,
    pub(super) workspace: Arc<Workspace>,
    pub(super) committing: AtomicBool,
}
pub struct StoreOperation {
    workspace: Workspace,
    ports: Arc<StorePorts>,
    client: Arc<CanonicalClient>,
    owner: OwnerClient,
}
impl BoundWorkspace {
    pub fn operation(&self) -> WorkspaceResult<StoreOperation> {
        let ports = self.store.ports_for(self.snapshot()?.scope, self.identity);
        let client = ports.client();
        let workspace = self.workspace.scoped(client.clone())?;
        Ok(StoreOperation {
            workspace,
            ports,
            client,
            owner: self.owner.clone(),
        })
    }
    pub fn snapshot(&self) -> WorkspaceResult<BranchSnapshot> {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| layerfs_workspace::WorkspaceError::BindingPoisoned)
    }
    /// True until one admitted Commit has a known safe local disposition.
    pub fn commit_in_flight(&self) -> bool {
        self.committing.load(Ordering::Acquire)
    }
    pub const fn identity(&self) -> WorkspaceId {
        self.identity
    }
    pub fn route(&self) -> layerfs_overlay::Route {
        self.workspace.route()
    }
}
impl StoreOperation {
    /// One native bounded step's provider uses an already admitted reader.
    pub fn admitted_client(
        &self,
        reader: super::ReadLease,
    ) -> Result<Arc<CanonicalClient>, Arc<super::PortError>> {
        self.ports.client_on(reader)
    }
    pub const fn workspace(&self) -> &Workspace {
        &self.workspace
    }
    pub fn client(&self) -> &CanonicalClient {
        &self.client
    }
    pub fn ports(&self) -> &StorePorts {
        &self.ports
    }
    pub const fn overlay(&self) -> &OwnerClient {
        &self.owner
    }
}
