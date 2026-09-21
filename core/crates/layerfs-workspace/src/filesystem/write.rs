//! One local splice, prepared outside the state lock and published with an exact head stamp.
use crate::{
    backing::{
        metadata::RootOwner,
        metadata_index::vector,
        metadata_pages::{self, Cell, PageRef},
    },
    overlay::pieces::{self, CapturedBase, Inode, Piece},
    *,
};
use layerfs_bridge::contract::{Inspect, Operation, Root, MAX_FILE};
use std::{
    sync::{atomic::Ordering, Arc},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
impl Workspace {
    pub(crate) fn overlay_inode(
        &self,
        serial: u64,
        root: Option<&Arc<RootOwner>>,
        deadline: Instant,
    ) -> Result<Option<Inode>, WorkspaceError> {
        let Some(root) = root else { return Ok(None) };
        let _view = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?
            .writer()?;
        let host = self
            .host
            .payloads
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut lease = host.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        match root.arena.find(
            root.root()?,
            &metadata_pages::inode_key(serial),
            window,
            deadline,
        )? {
            Some(cell) => Ok(Some(Inode::parse(cell.value())?)),
            None => Ok(None),
        }
    }
    pub(crate) fn overlay_attributes(
        &self,
        attr: NodeAttributes,
        root: Option<&Arc<RootOwner>>,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        Ok(self
            .overlay_inode(attr.serial, root, deadline)?
            .map_or(attr, |inode| inode.attributes(attr)))
    }
    fn edit_original(
        &self,
        path: &WorkspacePath,
        deadline: Instant,
    ) -> Result<(NodeAttributes, Root, Root), WorkspaceError> {
        {
            let state = self.state()?;
            if let Some(node) = state.nodes.iter().find(|n| n.path() == path.as_ref()) {
                return Ok((node.original, node.content, node.metadata));
            }
        }
        let _remote = self.begin(true, deadline)?;
        let mut parent = self.inner.root;
        let mut bytes = vector(4096)?;
        let parts = path.as_ref().split(|b| *b == b'/');
        let total = parts.clone().count();
        let mut result = None;
        for (index, name) in parts.enumerate() {
            super::namespace::check_access(parent, self.inner.root.uid, 1)?;
            if !bytes.is_empty() {
                bytes.push(b'/')
            }
            bytes.extend_from_slice(name);
            let response = self.call(
                Operation::Inspect {
                    root: self.inner.base,
                    query: Inspect::Attributes {
                        path: {
                            let mut path = vector(bytes.len())?;
                            path.extend_from_slice(&bytes);
                            path
                        },
                    },
                },
                0,
                &mut std::io::sink(),
                deadline,
            )?;
            let node = super::namespace::attributes(
                response,
                false,
                self.inner.root.uid,
                self.inner.root.gid,
            )?;
            if index + 1 < total && node.0.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            parent = node.0;
            result = Some(node);
        }
        result.ok_or(WorkspaceError::InvalidInput)
    }
    pub fn edit_file_range(
        &self,
        path: &WorkspacePath,
        edit: &RangeEdit,
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let deadline = Self::callback_deadline(deadline);
        let _operation = self.begin(false, deadline)?;
        if edit.replacement.record.directory.incarnation != self.inner.incarnation
            || !Arc::ptr_eq(
                &edit.replacement.host,
                self.host
                    .payloads
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?,
            )
        {
            return Err(WorkspaceError::InvalidInput);
        }
        if edit.replacement.len() > 8 * 1024 * 1024 {
            return Err(WorkspaceError::Capacity);
        }
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let (original, content, metadata) = self.edit_original(path, deadline)?;
        let _writer = host.writer()?;
        if original.kind == NodeKind::Directory {
            return Err(WorkspaceError::IsDirectory);
        }
        if original.kind != NodeKind::File {
            return Err(WorkspaceError::WrongKind);
        }
        super::namespace::check_access(original, self.inner.root.uid, 2)?;
        let (expected_revision, generation, dirty, old_root, needs_completion, frozen) = {
            let s = self.state()?;
            self.available(&s)?;
            (
                s.revision,
                s.generation,
                s.dirty_inodes,
                s.overlay.clone(),
                s.completion.is_none(),
                s.submission.clone(),
            )
        };
        let arena = self
            .inner
            .arena
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut lease = host.payloads.window(0, 1)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let old = match &old_root {
            Some(root) => arena
                .find(
                    root.root()?,
                    &metadata_pages::inode_key(original.serial),
                    window,
                    deadline,
                )?
                .map(|c| Inode::parse(c.value()))
                .transpose()?,
            None => None,
        };
        let mut inode = old.unwrap_or_else(|| Inode::initial(original, content, metadata));
        if edit.start > edit.end || edit.end > inode.length {
            return Err(WorkspaceError::InvalidInput);
        }
        let length = inode
            .length
            .checked_sub(edit.end - edit.start)
            .and_then(|n| n.checked_add(edit.replacement.len()))
            .ok_or(WorkspaceError::Capacity)?;
        if length > MAX_FILE {
            return Err(WorkspaceError::Capacity);
        }
        let already_dirty = old.is_some_and(|inode| inode.generation == generation);
        if !already_dirty && dirty == 128 {
            return Err(WorkspaceError::Capacity);
        }
        let parent = frozen
            .as_ref()
            .map(|submission| submission.capture().map(|g| g.root.clone()))
            .transpose()?;
        let inherited_capture = match (&old, &frozen) {
            (Some(inode), Some(submission)) => inode.generation == submission.capture()?.generation,
            _ => false,
        };
        let old_pieces = if inherited_capture {
            let frozen = frozen.as_ref().ok_or(WorkspaceError::Io)?.capture()?;
            inode.base = CapturedBase {
                root: frozen.root.root()?,
                inode: original.serial,
                generation: frozen.generation,
                revision: inode.revision,
            }
            .bytes();
            inode.captured = true;
            inode.base_length = inode.length;
            let mut pieces = vector(1024)?;
            if inode.length > 0 {
                pieces.push(Piece {
                    start: 0,
                    length: inode.length,
                    offset: 0,
                    payload: 0,
                    custody: PageRef::NULL,
                });
            }
            pieces
        } else if old.is_some() {
            arena.pieces(inode.pieces, inode.count, inode.length, window, deadline)?
        } else {
            let mut pieces = vector(1024)?;
            if inode.length > 0 {
                pieces.push(Piece {
                    start: 0,
                    length: inode.length,
                    offset: 0,
                    payload: 0,
                    custody: PageRef::NULL,
                });
            }
            pieces
        };
        // Normalize and enforce the shared replay envelope before reserving or touching metadata.
        let mut replacement = Piece {
            start: 0,
            length: edit.replacement.len(),
            offset: 0,
            payload: edit.replacement.record.id,
            custody: {
                let held = edit
                    .replacement
                    .record
                    .state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .custody;
                held.filter(|(id, _)| *id == arena.id)
                    .map_or(PageRef { slot: 1, epoch: 1 }, |(_, r)| r)
            },
        };
        let (mut pieces, edits, replacement_bytes) = pieces::splice(
            &old_pieces,
            edit.start,
            edit.end,
            replacement,
            inode.base_length,
        )?;
        let revision = expected_revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| WorkspaceError::Io)?;
        inode.length = length;
        inode.revision = revision;
        inode.generation = generation;
        inode.seconds = i64::try_from(time.as_secs()).map_err(|_| WorkspaceError::Capacity)?;
        inode.nanos = time.subsec_nanos();
        inode.edits = edits;
        inode.replacement = replacement_bytes;
        inode.count = pieces.len() as u16;
        let candidate = host.candidate(arena, generation, needs_completion, parent)?;
        if !edit.replacement.is_empty() {
            replacement.custody = arena.custody(&candidate, &edit.replacement, window, deadline)?;
            for p in &mut pieces {
                if p.payload == replacement.payload {
                    p.custody = replacement.custody;
                }
            }
        }
        inode.pieces = candidate.build_pieces(&pieces, window, deadline)?;
        let mut updates = vector(2)?;
        updates.push(Cell::new(
            &metadata_pages::dirty_key(generation, original.serial),
            &[1],
        )?);
        updates.push(Cell::new(
            &metadata_pages::inode_key(original.serial),
            &inode.value(),
        )?);
        let root = candidate.update(
            old_root
                .as_ref()
                .map(|r| r.root())
                .transpose()?
                .unwrap_or(PageRef::NULL),
            updates,
            window,
            deadline,
        )?;
        candidate.seal(root, window, deadline)?;
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let mut state = self.state()?;
        self.available(&state)?;
        let same_root = match (&state.overlay, &old_root) {
            (None, None) => true,
            (Some(current), Some(expected)) => Arc::ptr_eq(current, expected),
            _ => false,
        };
        if state.revision != expected_revision
            || !same_root
            || state.generation != generation
            || self.inner.stopping.load(Ordering::Acquire)
        {
            return Err(WorkspaceError::Busy);
        }
        if state.completion.is_none() != needs_completion {
            return Err(WorkspaceError::Busy);
        }
        if needs_completion {
            state.completion = candidate.take_completion(generation)?;
            if state.completion.is_none() {
                return Err(WorkspaceError::Io);
            }
        }
        state.overlay = Some(candidate);
        state.revision = revision;
        if !already_dirty {
            state.dirty_inodes += 1;
        }
        for node in &mut state.nodes {
            if node.attr.serial == original.serial {
                node.attr = inode.attributes(node.original);
            }
        }
        Ok(MutationReceipt {
            incarnation: self.inner.incarnation,
            generation,
            inode: original.serial,
            revision,
            accepted_bytes: edit.replacement.len(),
        })
    }
}
