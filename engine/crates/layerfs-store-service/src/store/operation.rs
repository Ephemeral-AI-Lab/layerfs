//! Fresh operation custody over shared Workspace identity, base and serial ranges.
use super::{Store, StorePorts};
use crate::OwnerClient;
use layerfs_history::{BranchSnapshot, WorkspaceId};
use layerfs_storage::read::CapturedLocators;
use layerfs_vfs::{CanonicalClient, Workspace, WorkspaceResult};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

pub struct BoundWorkspace {
    pub(crate) store: Arc<Store>,
    pub(super) owner: OwnerClient,
    pub(super) snapshot: Mutex<BranchSnapshot>,
    pub(super) identity: WorkspaceId,
    pub(super) workspace: Arc<Workspace>,
    /// The mount admission intent this Workspace must delete on unmount.
    pub(crate) gc_op: Option<[u8; 16]>,
    /// Remains true until terminal close starts to release admission.
    pub(super) admitted: Option<Arc<AtomicBool>>,
    pub(super) committing: AtomicBool,
    pub(super) producer: super::producer::Producer,
    /// A conflicted merge installed locally and not yet committed.
    pub(super) pending: Mutex<Option<super::commit_merge::PendingMerge>>,
    /// A Commit's retained custody awaiting its one resolution (`resolve.rs`).
    pub(super) retained: Mutex<Option<super::resolve::RetainedCommit>>,
    /// The logical size of one history node, the last one this Workspace
    /// read or published (`statistics.rs`): a Commit's `logical.known`.
    pub(super) logical: Mutex<Option<(layerfs_history::NodeId, u64)>>,
    /// The whiteout facts of one node this Workspace knows without a read
    /// while its mount admission excludes marker changes (`removal.rs`).
    pub(crate) removals: Mutex<Option<crate::removal::KnownRemovals>>,
}
pub struct StoreOperation {
    workspace: Workspace,
    ports: Arc<StorePorts>,
    client: Arc<CanonicalClient>,
    owner: OwnerClient,
}
impl BoundWorkspace {
    /// Terminal local close, then the Workspace's opened Store reader, then
    /// the mount admission intent on the writer. A failed boundary retains
    /// its original cause and permits no replay.
    ///
    /// Once the owner has closed, the mount is no longer live, so its intent
    /// is released (or marked uncertain) even when the reader close fails
    /// (#776 G5, #768 review): no later unmount passes the closed owner
    /// again, so nothing else would release the row, and its keys would
    /// refuse removals and compaction for the daemon's lifetime. The reader
    /// only reads, so a read with an unknown outcome is no reason to keep
    /// them. The reader's failure is still returned (before the intent's,
    /// which a refused release has already marked uncertain), and nothing
    /// with an unknown outcome is sent again.
    pub fn close(&self) -> Result<(), super::CloseError> {
        layerfs_persistence::one_call(|| self.close_once())
    }
    fn close_once(&self) -> Result<(), super::CloseError> {
        if let Err(error) = self.owner.close(self.route()) {
            // The Workspace stays bound; its idle producer handle stays too,
            // but returns its kept ordinals first, as it would at a close
            // that finds the Store's idle slot taken (#768).
            self.producer.release_ordinals();
            return Err(super::CloseError::Owner(error));
        }
        if let Some(admitted) = &self.admitted {
            admitted.store(false, Ordering::Release);
        }
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
        self.operation_with_ports(ports)
    }
    pub(super) fn captured_operation(
        &self,
        reads: Option<Arc<CapturedLocators>>,
    ) -> WorkspaceResult<StoreOperation> {
        let ports = self
            .store
            .captured_ports_for(self.snapshot()?.scope, self.identity, reads);
        self.operation_with_ports(ports)
    }
    fn operation_with_ports(&self, ports: Arc<StorePorts>) -> WorkspaceResult<StoreOperation> {
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
            .map_err(|_| layerfs_vfs::WorkspaceError::BindingPoisoned)
    }
    pub const fn identity(&self) -> WorkspaceId {
        self.identity
    }
    pub fn route(&self) -> layerfs_vfs::Route {
        self.workspace.route()
    }
    /// One lookup in published history on this Workspace's own reader, the
    /// one its bind read the snapshot on, so the mount opens no other
    /// session (#768). It sees what is published when it runs.
    pub(crate) fn published<T>(
        &self,
        lookup: impl FnOnce(&dyn layerfs_history::HistoryCatalog) -> layerfs_history::HistoryResult<T>,
    ) -> Result<T, Arc<super::PortError>> {
        self.store
            .reader(Some(self.identity))
            .map_err(|error| Arc::new(super::PortError::ReadAdmission(error)))?
            .published(lookup)
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
