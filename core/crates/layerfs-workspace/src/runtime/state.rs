use super::host::Host;
use crate::{backing::budget::Charge, *};
use layerfs_bridge::contract::{Operation, Response, Root};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

pub(crate) const NODE_LIMIT: usize = 256;
pub(crate) const HANDLE_LIMIT: usize = 128;
pub(crate) const COOKIE_LIMIT: usize = 1024;
pub(crate) const PATH_BYTES: usize = 4096;
pub(crate) const CALL_SCRATCH: usize = 128 * 1024;

#[derive(Clone)]
pub struct Workspace {
    pub(crate) inner: Arc<Inner>,
    pub(crate) host: Arc<Host>,
}
pub(crate) struct Inner {
    pub id: String,
    pub incarnation: Root,
    pub store: u32,
    pub access: WorkspaceAccess,
    pub arena: Option<Arc<crate::backing::metadata::Arena>>,
    pub root: NodeAttributes,
    pub mount_path: PathBuf,
    pub directory: Option<Arc<crate::backing::directory::Directory>>,
    pub stopping: AtomicBool,
    pub state: Mutex<State>,
    pub _charge: Charge,
}
pub(crate) struct State {
    pub base: Root,
    pub branch: Option<Arc<BranchContext>>,
    pub baseline: u64,
    pub nodes: Vec<Node>,
    pub overlay: Option<Arc<crate::backing::metadata::RootOwner>>,
    pub completion: Option<crate::backing::metadata::CompletionReserve>,
    pub submission: Option<Arc<crate::overlay::snapshot::Submission>>,
    pub generation: u64,
    pub revision: u64,
    pub dirty_inodes: usize,
    pub handles: Vec<Handle>,
    pub cookies: Vec<Cookie>,
    pub next_handle: u64,
    pub next_cookie: u64,
    pub mounted: bool,
    pub projection: Option<Box<super::coherence::ProjectionState>>,
    pub closed: bool,
    pub active: usize,
    pub tables: Option<Charge>,
}
pub(crate) struct Node {
    pub attr: NodeAttributes,
    pub original: NodeAttributes,
    pub baseline: u64,
    pub metadata: Root,
    pub content: Root,
    pub path: [u8; PATH_BYTES],
    pub path_len: usize,
    pub parent: u64,
    pub lookups: u64,
    pub projection_lookups: u64,
    pub handles: usize,
}
#[derive(Clone, Copy)]
pub(crate) struct Handle {
    pub id: u64,
    pub serial: u64,
    pub directory: bool,
    pub scope: ReferenceScope,
    pub options: FileOpenOptions,
    pub ready: bool,
}
pub(crate) struct Cookie {
    pub id: u64,
    pub handle: u64,
    pub after: [u8; 255],
    pub len: usize,
    pub dots: u8,
}
pub(crate) struct OperationGuard {
    pub workspace: Workspace,
    pub remote: bool,
    pub _charge: Charge,
}

impl Node {
    pub fn new(
        attr: NodeAttributes,
        content: Root,
        metadata: Root,
        path: &[u8],
        parent: u64,
    ) -> Self {
        let mut stored = [0; PATH_BYTES];
        stored[..path.len()].copy_from_slice(path);
        Self {
            attr,
            original: attr,
            baseline: 1,
            metadata,
            content,
            path: stored,
            path_len: path.len(),
            parent,
            lookups: 0,
            projection_lookups: 0,
            handles: 0,
        }
    }
    pub fn path(&self) -> &[u8] {
        &self.path[..self.path_len]
    }
    pub fn references(&mut self, scope: ReferenceScope) -> &mut u64 {
        match scope {
            ReferenceScope::Local => &mut self.lookups,
            ReferenceScope::Projection => &mut self.projection_lookups,
        }
    }
}
impl State {
    // ponytail: scans are bounded by 256 nodes; use an index if that profile grows.
    pub fn node(&self, serial: u64) -> Result<&Node, WorkspaceError> {
        self.nodes
            .iter()
            .find(|node| node.attr.serial == serial)
            .ok_or(WorkspaceError::NotFound)
    }
    pub fn node_mut(&mut self, serial: u64) -> Result<&mut Node, WorkspaceError> {
        self.nodes
            .iter_mut()
            .find(|node| node.attr.serial == serial)
            .ok_or(WorkspaceError::NotFound)
    }
    pub fn handle(&self, id: HandleId, directory: bool) -> Result<Handle, WorkspaceError> {
        self.handles
            .iter()
            .find(|handle| handle.id == id && handle.directory == directory && handle.ready)
            .copied()
            .ok_or(WorkspaceError::BadHandle)
    }
    pub fn collect(&mut self, root: u64) {
        self.nodes.retain(|node| {
            node.attr.serial == root
                || node.lookups > 0
                || node.projection_lookups > 0
                || node.handles > 0
        });
    }
}
impl Workspace {
    pub fn id(&self) -> &str {
        &self.inner.id
    }
    /// The immutable semantic capability selected at attach.
    pub fn access_mode(&self) -> WorkspaceAccess {
        self.inner.access
    }
    pub fn root(&self) -> NodeAttributes {
        self.inner.root
    }
    pub fn mount_path(&self) -> &Path {
        &self.inner.mount_path
    }
    pub(crate) fn state(&self) -> Result<MutexGuard<'_, State>, WorkspaceError> {
        self.inner.state.lock().map_err(|_| WorkspaceError::Io)
    }
    pub(crate) fn available(&self, state: &State) -> Result<(), WorkspaceError> {
        if state.closed {
            return Err(WorkspaceError::Closed);
        }
        if self.inner.stopping.load(Ordering::Acquire) {
            return Err(WorkspaceError::Busy);
        }
        Ok(())
    }
    pub(crate) fn begin(
        &self,
        remote: bool,
        deadline: Instant,
    ) -> Result<OperationGuard, WorkspaceError> {
        if deadline <= Instant::now() {
            return Err(WorkspaceError::Deadline);
        }
        let charge = self
            .host
            .budget
            .reserve(if remote { CALL_SCRATCH } else { 0 })?;
        let mut state = self.state()?;
        self.available(&state)?;
        if remote
            && self
                .host
                .remote
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Err(WorkspaceError::Busy);
        }
        state.active += 1;
        Ok(OperationGuard {
            workspace: self.clone(),
            remote,
            _charge: charge,
        })
    }
    pub(crate) fn call(
        &self,
        operation: Operation,
        bytes: u64,
        output: &mut dyn Write,
        deadline: Instant,
    ) -> Result<Response, WorkspaceError> {
        self.host
            .call(self.inner.store, operation, bytes, output, deadline)
    }
    pub(crate) fn deliver(
        &self,
        generation: u64,
        operation: Operation,
        input: &mut dyn layerfs_bridge::contract::Source,
        bytes: u64,
        output: &mut dyn Write,
        deadline: Instant,
    ) -> Result<Response, WorkspaceError> {
        let _remote = self.begin(true, deadline)?;
        self.host.call_input(
            (self.inner.store, generation),
            operation,
            input,
            bytes,
            output,
            deadline,
        )
    }
    pub(crate) fn callback_deadline(deadline: Instant) -> Instant {
        deadline.min(Instant::now() + Duration::from_secs(10))
    }
    pub fn handle_attributes(&self, handle: HandleId) -> Result<NodeAttributes, WorkspaceError> {
        let state = self.state()?;
        if state.closed {
            return Err(WorkspaceError::Closed);
        }
        let handle = state
            .handles
            .iter()
            .find(|entry| entry.id == handle && entry.ready)
            .ok_or(WorkspaceError::BadHandle)?;
        Ok(state.node(handle.serial)?.attr)
    }
    pub(crate) fn release_handle(
        &self,
        id: HandleId,
        directory: bool,
    ) -> Result<(), WorkspaceError> {
        let mut state = self.state()?;
        let index = state
            .handles
            .iter()
            .position(|handle| handle.id == id && handle.directory == directory && handle.ready)
            .ok_or(WorkspaceError::BadHandle)?;
        let handle = state.handles.swap_remove(index);
        state.node_mut(handle.serial)?.handles -= 1;
        state.cookies.retain(|cookie| cookie.handle != id);
        state.collect(self.inner.root.serial);
        Ok(())
    }
}
impl OperationGuard {
    pub fn remote(&mut self) -> Result<(), WorkspaceError> {
        if self.remote {
            return Ok(());
        }
        self.workspace
            .host
            .remote
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| WorkspaceError::Busy)?;
        if let Err(error) = self._charge.resize(CALL_SCRATCH) {
            self.workspace.host.remote.store(false, Ordering::Release);
            return Err(error);
        }
        self.remote = true;
        Ok(())
    }
}
impl Drop for OperationGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.workspace.inner.state.lock() {
            state.active -= 1;
        }
        if self.remote {
            self._charge
                .resize(0)
                .expect("shrinking a call reservation cannot fail");
            self.workspace.host.remote.store(false, Ordering::Release);
        }
    }
}

/// Charged immutable Branch context; selected captures keep their exact view.
pub(crate) struct BranchContext {
    pub snapshot: layerfs_bridge::contract::BranchSnapshotWire,
    pub _charge: Charge,
}
impl std::ops::Deref for BranchContext {
    type Target = layerfs_bridge::contract::BranchSnapshotWire;
    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}
impl BranchContext {
    pub fn new(
        snapshot: layerfs_bridge::contract::BranchSnapshotWire,
        budget: &Arc<crate::backing::budget::Budget>,
    ) -> Result<Self, WorkspaceError> {
        let charge =
            budget.reserve(std::mem::size_of::<Self>() + snapshot.branch.name.capacity() + 64)?;
        Ok(Self {
            snapshot,
            _charge: charge,
        })
    }
}
