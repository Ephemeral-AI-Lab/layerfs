//! Directory creation: one allocation, checked publication, and projection completion.
use super::{
    namespace::{check_access, child_path},
    namespace_view::View,
};
use crate::{
    backing::{
        metadata_index::vector,
        metadata_pages::{self, Cell, PageRef},
    },
    overlay::{
        directories::{self, Directory, Origin},
        pieces::CapturedBase,
    },
    runtime::{
        coherence::MutationOrigin,
        state::{Node, NODE_LIMIT},
    },
    *,
};
use layerfs_bridge::contract::{
    Code, HistoryCommand, HistoryResult, Operation, Response, HISTORY_RESULT_BYTES,
};
use std::{
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
impl Workspace {
    /// Creates one native directory and returns one Local lookup reference.
    /// When mounted, success includes parent-attribute and entry invalidation.
    pub fn mkdir(
        &self,
        parent: u64,
        name: &[u8],
        mode: u32,
        umask: u32,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.mkdir_from(parent, name, mode, umask, deadline, MutationOrigin::Local)
    }
    pub(crate) fn mkdir_from(
        &self,
        parent: u64,
        name: &[u8],
        mode: u32,
        umask: u32,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<NodeAttributes, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        if mode & !0o1777 != 0 || umask & !0o777 != 0 {
            return Err(WorkspaceError::InvalidInput);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let (view, path, attr, baseline, revision, generation, frozen, scope) = {
            let state = self.state()?;
            self.available(&state)?;
            self.check_mutation_coherence(&state, origin, false)?;
            if state.nodes.len() == NODE_LIMIT || state.nodes.len() == state.nodes.capacity() {
                return Err(WorkspaceError::Capacity);
            }
            let node = state.node(parent)?;
            if node.attr.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            check_access(node.attr, self.inner.root.uid, 3)?;
            child_path(node.path(), name)?;
            (
                View {
                    base: state.base,
                    root: state.overlay.clone(),
                },
                node.path().to_vec(),
                node.attr,
                state.baseline,
                state.revision,
                state.generation,
                state.submission.clone(),
                state
                    .branch
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?
                    .scope,
            )
        };
        match self.resolve_child(&mut operation, &view, parent, &path, name, deadline) {
            Ok(_) => return Err(WorkspaceError::Exists),
            Err(WorkspaceError::NotFound) => {}
            Err(WorkspaceError::Service(failure))
                if !failure.unknown
                    && matches!(failure.code, Code::PathNotFound | Code::NotFound) => {}
            Err(error) => return Err(error),
        }
        let old = self.directory_record(&view, parent, deadline)?;
        let already_dirty = old.is_some_and(|directory| directory.generation == generation);
        let new_dirty = if already_dirty { 1 } else { 2 };
        let name_bytes = 10 + name.len();
        {
            let state = self.state()?;
            self.check_mkdir_stamp(&state, baseline, revision, generation, &view)?;
            self.check_mutation_coherence(&state, origin, false)?;
            state.frontier_bytes(
                state.dirty_inodes + new_dirty,
                state.dirty_directories + new_dirty,
                state.directory_names + 1,
                state.directory_bytes + name_bytes,
            )?;
        }
        let serial = {
            operation.remote()?;
            let response = self.host.call_input(
                (self.inner.store, generation),
                Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 1 }),
                &mut &[][..],
                HISTORY_RESULT_BYTES as u64,
                &mut std::io::sink(),
                deadline,
            );
            operation.release_remote();
            match response? {
                Response::History(result) => match *result {
                    HistoryResult::Reservation {
                        scope: actual,
                        start,
                        count: 1,
                    } if actual == scope && start > 0 && start < i64::MAX as u64 => start,
                    _ => return Err(WorkspaceError::Service(Code::Unknown.into())),
                },
                _ => return Err(WorkspaceError::Service(Code::Unknown.into())),
            }
        };
        // Even a later race/failure leaves this real reservation consumed.
        self.maintain_backing(deadline)?;
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let _writer = host.writer()?;
        let needs_completion = {
            let state = self.state()?;
            self.check_mkdir_stamp(&state, baseline, revision, generation, &view)?;
            self.check_mutation_coherence(&state, origin, false)?;
            if state.nodes.iter().any(|node| node.attr.serial == serial) {
                return Err(WorkspaceError::Service(Code::Unknown.into()));
            }
            state.completion.is_none()
        };
        let arena = self
            .inner
            .arena
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut lease = host.payloads.window(0, 1)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        if let Some(root) = &view.root {
            for key in [
                metadata_pages::inode_key(serial),
                metadata_pages::namespace_key(serial),
            ] {
                if arena.find(root.root()?, &key, window, deadline)?.is_some() {
                    return Err(WorkspaceError::Service(Code::Unknown.into()));
                }
            }
        }
        let mut parent_directory = old.unwrap_or_else(|| Directory::initial(attr, view.base));
        let capture = frozen
            .as_ref()
            .map(|submission| submission.capture())
            .transpose()?;
        if !already_dirty {
            if let (Some(prior), Some(capture)) = (old, capture) {
                if prior.generation != capture.generation {
                    return Err(WorkspaceError::Io);
                }
                parent_directory.origin = Origin::Captured(CapturedBase {
                    root: capture.root.root()?,
                    inode: parent,
                    generation: prior.generation,
                    revision: prior.revision,
                });
            }
            parent_directory.entries = PageRef::NULL;
            parent_directory.count = 0;
            parent_directory.bytes = 0;
        }
        let next = revision.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| WorkspaceError::Io)?;
        let seconds = i64::try_from(time.as_secs()).map_err(|_| WorkspaceError::Capacity)?;
        let mut child = Directory::initial(attr, view.base);
        child.origin = Origin::Empty;
        child.generation = generation;
        child.revision = next;
        child.mode = mode & !umask;
        child.seconds = seconds;
        child.nanos = time.subsec_nanos();
        parent_directory.generation = generation;
        parent_directory.revision = next;
        parent_directory.seconds = seconds;
        parent_directory.nanos = time.subsec_nanos();
        parent_directory.count = parent_directory
            .count
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        parent_directory.bytes = parent_directory
            .bytes
            .checked_add(name_bytes as u32)
            .ok_or(WorkspaceError::Capacity)?;
        let candidate = host.candidate(
            arena,
            generation,
            needs_completion,
            capture.map(|g| g.root.clone()),
        )?;
        let mut entry = vector(1)?;
        entry.push(Cell::new(
            &metadata_pages::entry_key(name)?,
            &directories::entry(serial),
        )?);
        parent_directory.entries =
            candidate.update(parent_directory.entries, entry, window, deadline)?;
        let mut updates = vector(4)?;
        for (id, directory) in [(parent, parent_directory), (serial, child)] {
            updates.push(Cell::new(&metadata_pages::dirty_key(generation, id), &[1])?);
            updates.push(Cell::new(
                &metadata_pages::namespace_key(id),
                &directory.value(),
            )?);
        }
        updates.sort_unstable_by(|a, b| a.key().cmp(b.key()));
        let root = candidate.update(
            view.root
                .as_ref()
                .map(|r| r.root())
                .transpose()?
                .unwrap_or(PageRef::NULL),
            updates,
            window,
            deadline,
        )?;
        candidate.seal(root, window, deadline)?;
        let child_path = child_path(&path, name)?;
        let child_attr = child.attributes(NodeAttributes {
            serial,
            kind: NodeKind::Directory,
            size: 0,
            references: 1,
            mode: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            uid: self.inner.root.uid,
            gid: self.inner.root.gid,
        });
        let mut node = Node::new(child_attr, [0; 32], [0; 32], &child_path, parent);
        node.baseline = 0;
        let reference = if origin.projected() {
            ReferenceScope::Projection
        } else {
            ReferenceScope::Local
        };
        *node.references(reference) = 1;
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let mut state = self.state()?;
        self.check_mkdir_stamp(&state, baseline, revision, generation, &view)?;
        self.check_mutation_coherence(&state, origin, true)?;
        if state.completion.is_none() != needs_completion {
            return Err(WorkspaceError::Busy);
        }
        if state.nodes.iter().any(|node| node.attr.serial == serial) {
            return Err(WorkspaceError::Service(Code::Unknown.into()));
        }
        let parent_node = state
            .nodes
            .iter()
            .position(|node| node.attr.serial == parent)
            .ok_or(WorkspaceError::Busy)?;
        check_access(state.nodes[parent_node].attr, self.inner.root.uid, 3)?;
        state.frontier_bytes(
            state.dirty_inodes + new_dirty,
            state.dirty_directories + new_dirty,
            state.directory_names + 1,
            state.directory_bytes + name_bytes,
        )?;
        let receipt = MutationReceipt {
            incarnation: self.inner.incarnation,
            generation,
            inode: serial,
            revision: next,
            accepted_bytes: 0,
        };
        // The kernel's mkdir request holds the parent lock until its reply.
        // Only native callers notify; projected callers let that reply install
        // the entry and invalidate the parent without waiting on themselves.
        let delivery = if origin.projected() {
            None
        } else {
            state
                .projection
                .as_ref()
                .and_then(|projection| projection.delivery.clone())
        };
        if needs_completion {
            state.completion = candidate.take_completion(generation)?;
            if state.completion.is_none() {
                return Err(WorkspaceError::Io);
            }
        }
        state.nodes[parent_node].attr = parent_directory.attributes(state.nodes[parent_node].attr);
        state.nodes.push(node);
        let retired = state.overlay.replace(candidate);
        state.revision = next;
        state.dirty_inodes += new_dirty;
        state.dirty_directories += new_dirty;
        state.directory_names += 1;
        state.directory_bytes += name_bytes;
        if let (Some(projection), Some(_)) = (&mut state.projection, &delivery) {
            projection.status = CoherenceStatus::Pending {
                receipt,
                published_handle: None,
            };
        }
        drop(state);
        drop(retired);
        drop(lease);
        drop(_writer);
        if let Some(delivery) = delivery {
            if let Err(error) = self.complete_projection_mutation(
                delivery,
                receipt,
                Some((parent, name)),
                None,
                deadline,
            ) {
                // The name remains published. An error returns no attributes,
                // so release only this attempt's otherwise unreturned reference.
                self.forget(serial, 1, ReferenceScope::Local);
                return Err(error);
            }
        }
        Ok(child_attr)
    }
    fn check_mkdir_stamp(
        &self,
        state: &crate::runtime::state::State,
        baseline: u64,
        revision: u64,
        generation: u64,
        view: &View,
    ) -> Result<(), WorkspaceError> {
        self.available(state)?;
        if state.nodes.len() == NODE_LIMIT || state.nodes.len() == state.nodes.capacity() {
            return Err(WorkspaceError::Capacity);
        }
        let same = match (&state.overlay, &view.root) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if state.baseline != baseline
            || state.revision != revision
            || state.generation != generation
            || !same
        {
            return Err(WorkspaceError::Busy);
        }
        Ok(())
    }
}
