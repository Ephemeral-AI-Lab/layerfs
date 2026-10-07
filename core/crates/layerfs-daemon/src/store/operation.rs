//! Fresh operation custody over shared Workspace identity, base and serial ranges.
use super::{Store, StorePorts};
use crate::OwnerClient;
use layerfs_history::{BranchSnapshot, WorkspaceId};
use layerfs_workspace::{CanonicalClient, Workspace, WorkspaceResult};
use std::sync::Arc;

pub struct BoundWorkspace {
    pub(super) store: Arc<Store>,
    pub(super) owner: OwnerClient,
    pub(super) snapshot: BranchSnapshot,
    pub(super) identity: WorkspaceId,
    pub(super) workspace: Workspace,
}
pub struct StoreOperation {
    workspace: Workspace,
    ports: Arc<StorePorts>,
    client: Arc<CanonicalClient>,
    owner: OwnerClient,
}
impl BoundWorkspace {
    pub fn operation(&self) -> WorkspaceResult<StoreOperation> {
        let ports = self.store.ports(self.snapshot.scope);
        let client = ports.client();
        let workspace = self.workspace.scoped(client.clone())?;
        Ok(StoreOperation {
            workspace,
            ports,
            client,
            owner: self.owner.clone(),
        })
    }
    pub const fn snapshot(&self) -> &BranchSnapshot {
        &self.snapshot
    }
    pub const fn identity(&self) -> WorkspaceId {
        self.identity
    }
    pub fn route(&self) -> layerfs_overlay::Route {
        self.workspace.route()
    }
}
impl StoreOperation {
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
