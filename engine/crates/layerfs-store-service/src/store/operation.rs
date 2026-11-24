//! Fresh operation custody over shared Workspace identity, base and serial ranges.
use super::{Store, StorePorts};
use crate::OwnerClient;
use layerfs_history::{BranchSnapshot, WorkspaceId};
use layerfs_workspace::{CanonicalClient, Workspace, WorkspaceResult};
use std::sync::{atomic::AtomicBool, Arc, Mutex};

pub struct BoundWorkspace {
    pub(crate) store: Arc<Store>,
    pub(super) owner: OwnerClient,
    pub(super) snapshot: Mutex<BranchSnapshot>,
    pub(super) identity: WorkspaceId,
    pub(super) workspace: Arc<Workspace>,
    /// The mount admission intent this Workspace must delete on unmount.
    pub(super) gc_op: Option<[u8; 16]>,
    pub(super) committing: AtomicBool,
    pub(super) producer: super::producer::Producer,
    /// A conflicted merge installed locally and not yet committed.
    pub(super) pending: Mutex<Option<super::commit_merge::PendingMerge>>,
    /// A Commit's retained custody awaiting its one resolution (`resolve.rs`).
    pub(super) retained: Mutex<Option<super::resolve::RetainedCommit>>,
}
pub struct StoreOperation {
    workspace: Workspace,
    ports: Arc<StorePorts>,
    client: Arc<CanonicalClient>,
    owner: OwnerClient,
}
impl BoundWorkspace {
    /// Terminal local close, then the Workspace's opened Store reader.
    /// A failed boundary retains its original cause and permits no replay.
    ///
    /// Once the owner has closed, the mount is no longer live, so its intent
    /// is released (or marked uncertain) even when the reader close fails:
    /// no later unmount passes the closed owner again, so nothing else would
    /// ever release the row, and it would hold the Branch against removal
    /// for the daemon's lifetime (#776 G5). The reader's failure is still the
    /// answer.
    pub fn close(&self) -> Result<(), super::CloseError> {
        self.owner
            .close(self.route())
            .map_err(super::CloseError::Owner)?;
        self.producer.hand_to(&self.store.idle_producer);
        let reader = self.store.close_reader(self.identity);
        let intent = match self.gc_op {
            Some(op) => crate::history::admission::release_or_mark(self.store.history(), &op),
            None => Ok(()),
        };
        reader.map_err(super::CloseError::Reader)?;
        intent.map_err(super::CloseError::Intent)
    }
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
    pub const fn identity(&self) -> WorkspaceId {
        self.identity
    }
    pub fn route(&self) -> layerfs_workspace::Route {
        self.workspace.route()
    }
    /// The `history_serial` of one Commit or Layer, for `remove_file`
    /// visibility in operator reads; absent is 0, as the resolver reports.
    pub fn node_serial(&self, node: layerfs_history::NodeId) -> u64 {
        match self
            .store
            .published(|history| history.node_history_serial(node))
        {
            Ok(Some(serial)) => serial,
            _ => 0,
        }
    }
}
impl StoreOperation {
    pub fn client_handle(&self) -> Arc<CanonicalClient> {
        self.client.clone()
    }
    pub const fn workspace(&self) -> &Workspace {
        &self.workspace
    }
    pub fn client(&self) -> &CanonicalClient {
        &self.client
    }
    /// The Store's memory-only client, for base facts read inside an owner job.
    pub fn resident(&self) -> Arc<CanonicalClient> {
        self.ports.resident()
    }
    pub fn ports(&self) -> &StorePorts {
        &self.ports
    }
    pub const fn overlay(&self) -> &OwnerClient {
        &self.owner
    }
}
