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
    pub dirty_directories: usize,
    pub fresh_files: usize,
    pub fresh_symlinks: usize,
    pub directory_names: usize,
    pub directory_bytes: usize,
    pub handles: Vec<Handle>,
    pub cookies: Vec<Cookie>,
    pub next_handle: u64,
    pub next_cookie: u64,
    pub mounted: bool,
    pub projection: Option<Box<super::coherence::ProjectionState>>,
    /// One entry per regular inode this generation created, with the names the
    /// effective namespace still binds. A fresh identity at zero has no
    /// canonical identity to save, so lowering drops it instead of declaring an
    /// unbound fresh inode. Reset when a capture starts the next generation.
    pub fresh: Vec<FreshName>,
    /// One entry per directory this generation created whose declaration the
    /// canonical state has not accepted yet. A fresh directory serial exists in
    /// the service only after the first Commit that names it, so lowering must
    /// declare it exactly once and never re-declare it in a later generation.
    pub declared: Vec<u64>,
    /// One entry per regular identity this generation created that lost its last
    /// name. A capture resets the per-generation counters but not this set,
    /// because lowering runs after the capture and must still recognise the
    /// identity as unbound. Cleared once the canonical successor is installed.
    pub unbound: Vec<u64>,
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
    /// Names this delta generation knows bind this inode. Exact for an inode
    /// created in the generation, a lower bound for one inherited from a base.
    pub names: u32,
}
/// One regular inode this generation created and its live name count.
#[derive(Clone, Copy)]
pub(crate) struct FreshName {
    pub serial: u64,
    pub names: u32,
}
#[derive(Clone)]
pub(crate) struct Handle {
    pub id: u64,
    pub serial: u64,
    pub directory: bool,
    pub scope: ReferenceScope,
    pub options: FileOpenOptions,
    pub ready: bool,
    // The handle pins this view and its Node. Native namespace edits only add
    // names, so the pinned Node path remains the exact immutable locator.
    pub view: Option<crate::filesystem::namespace_view::View>,
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
    /// This guard holds the one bounded metadata-call slot instead of the
    /// primary admission, because another call already owns that.
    pub metadata_call: bool,
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
            names: 1,
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
            .cloned()
            .ok_or(WorkspaceError::BadHandle)
    }
    pub fn frontier_bytes(
        &self,
        dirty: usize,
        directories: usize,
        fresh_files: usize,
        fresh_symlinks: usize,
        names: usize,
        bytes: usize,
    ) -> Result<usize, WorkspaceError> {
        if dirty > 128
            || directories > dirty
            || fresh_files > dirty - directories
            || fresh_symlinks > dirty - directories - fresh_files
            || names > 128
        {
            return Err(WorkspaceError::Capacity);
        }
        let header = if self.submission.is_some()
            || self
                .branch
                .as_ref()
                .is_some_and(|branch| branch.branch.head_commit.is_some())
        {
            228
        } else {
            195
        };
        let total = header
            + 73 * (dirty - directories)
            + 34 * directories
            + bytes
            + if fresh_symlinks > 0 {
                9 + 8 * (fresh_files + fresh_symlinks)
            } else if fresh_files > 0 {
                7 + 8 * fresh_files
            } else {
                usize::from(directories > 0) * 5
            };
        if total > layerfs_bridge::contract::METADATA_BYTES {
            return Err(WorkspaceError::Capacity);
        }
        Ok(total)
    }
    /// Registers one regular identity this generation created. An identity that
    /// had lost every name becomes a counted dirty identity again.
    pub fn created(&mut self, serial: u64) {
        let mut revived = false;
        if let Some(entry) = self.fresh.iter_mut().find(|e| e.serial == serial) {
            revived = entry.names == 0;
            entry.names = entry.names.saturating_add(1);
        } else {
            self.fresh.push(FreshName { serial, names: 1 });
        }
        if revived {
            self.dirty_inodes = self.dirty_inodes.saturating_add(1);
            self.fresh_files = self.fresh_files.saturating_add(1);
            self.unbound.retain(|bound| *bound != serial);
        }
    }
    /// Registers one more name for an identity this generation created. A base
    /// identity keeps no live-name count here: its other names are not this
    /// delta's, so removing one of them never leaves it unbound.
    pub fn linked(&mut self, serial: u64) {
        if self.fresh.iter().any(|entry| entry.serial == serial) {
            self.created(serial);
        }
    }
    /// Registers one directory identity this generation created and has not
    /// yet declared to the canonical state.
    pub fn declaring(&mut self, serial: u64) {
        if !self.declared.contains(&serial) {
            self.declared.push(serial);
        }
    }
    /// True while the canonical state still has to learn this directory serial.
    pub fn undeclared(&self, serial: u64) -> bool {
        self.declared.contains(&serial)
    }
    /// Forgets the declarations the canonical state has now accepted, and every
    /// identity this generation dropped before lowering reached it. A directory
    /// this Commit did not declare — one a later generation created while the
    /// submission was in flight — keeps its declaration for the next Commit.
    pub fn declared_committed(&mut self, accepted: &[u64]) {
        self.declared.retain(|serial| !accepted.contains(serial));
        self.unbound.clear();
    }
    /// Removes one name; the identity keeps its own live-owner lifetime. Losing
    /// the last name of an identity this generation created removes it from the
    /// captured dirty frontier as well, because lowering then declares nothing
    /// for it: an unbound fresh identity has no canonical identity to name.
    pub fn unlinked(&mut self, serial: u64) {
        let mut unbound = false;
        if let Some(entry) = self.fresh.iter_mut().find(|e| e.serial == serial) {
            unbound = entry.names == 1;
            entry.names = entry.names.saturating_sub(1);
        }
        if unbound {
            self.dirty_inodes = self.dirty_inodes.saturating_sub(1);
            self.fresh_files = self.fresh_files.saturating_sub(1);
            if !self.unbound.contains(&serial) {
                self.unbound.push(serial);
            }
        }
    }
    /// A regular identity this generation created and no name binds any more.
    pub fn unbound(&self, serial: u64) -> bool {
        self.unbound.contains(&serial)
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
            metadata_call: false,
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
        drop(state);
        drop(handle);
        Ok(())
    }
}
impl OperationGuard {
    pub fn local_io(&mut self) -> Result<(), WorkspaceError> {
        self._charge.resize(CALL_SCRATCH)
    }
    pub fn release_remote(&mut self) {
        if self.remote {
            self.remote = false;
            self.workspace.host.remote.store(false, Ordering::Release);
        }
    }
    /// Admits one bounded metadata call: a read-only inspection or one serial
    /// reservation. The primary admission is taken first, so a lone call keeps
    /// the original single-call behaviour; while another call owns it, the one
    /// metadata-call slot admits this one instead and refuses a second.
    /// Construction, save and history Commit never take this path.
    pub fn metadata_call(&mut self) -> Result<(), WorkspaceError> {
        if self.remote || self.metadata_call {
            return Ok(());
        }
        let host = &self.workspace.host;
        if host
            .remote
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            if let Err(error) = self._charge.resize(CALL_SCRATCH) {
                host.remote.store(false, Ordering::Release);
                return Err(error);
            }
            self.remote = true;
            return Ok(());
        }
        // Only a call that is actually in flight makes progress worth
        // overlapping; a retained admission whose call has returned keeps its
        // single-call refusal.
        if host.in_flight.load(Ordering::Acquire) == 0 {
            return Err(WorkspaceError::Busy);
        }
        host.metadata_call
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| WorkspaceError::Busy)?;
        if let Err(error) = self._charge.resize(CALL_SCRATCH) {
            host.metadata_call.store(false, Ordering::Release);
            return Err(error);
        }
        self.metadata_call = true;
        Ok(())
    }
    pub fn release_metadata_call(&mut self) {
        if self.metadata_call {
            self.metadata_call = false;
            self.workspace
                .host
                .metadata_call
                .store(false, Ordering::Release);
        }
        self.release_remote();
    }
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
        if self.metadata_call {
            self._charge
                .resize(0)
                .expect("shrinking a call reservation cannot fail");
            self.workspace
                .host
                .metadata_call
                .store(false, Ordering::Release);
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
