//! One local splice, prepared outside the state lock and published with an exact head stamp.
use crate::{
    backing::{
        metadata::RootOwner,
        metadata_index::vector,
        metadata_pages::{self, Cell, PageRef},
    },
    overlay::pieces::{self, CapturedBase, Inode, Piece, PieceKind},
    runtime::{
        coherence::MutationOrigin,
        state::{Handle, State},
    },
    *,
};
use layerfs_bridge::contract::{Inspect, Operation, Root, MAX_FILE};
use std::{
    sync::{atomic::Ordering, Arc},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
type Original = (NodeAttributes, Root, Root, u64);
#[derive(Clone, Copy)]
enum FileMutation<'a> {
    Range(&'a RangeEdit),
    SetLen(u64),
    Write {
        handle: HandleId,
        offset: u64,
        replacement: &'a OwnedPayload,
        origin: MutationOrigin,
    },
}
impl FileMutation<'_> {
    fn origin(self) -> MutationOrigin {
        match self {
            Self::Write { origin, .. } => origin,
            _ => MutationOrigin::Local,
        }
    }
}
impl Workspace {
    fn check_payload_owner(&self, replacement: &OwnedPayload) -> Result<(), WorkspaceError> {
        if replacement.record.directory.incarnation != self.inner.incarnation
            || !Arc::ptr_eq(
                &replacement.host,
                self.host
                    .payloads
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?,
            )
        {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok(())
    }
    fn write_handle(
        &self,
        state: &State,
        id: HandleId,
        origin: MutationOrigin,
    ) -> Result<Handle, WorkspaceError> {
        let handle = state.handle(id, false)?;
        if handle.options.access == FileAccess::ReadOnly {
            return Err(WorkspaceError::BadHandle);
        }
        let scope = match origin {
            MutationOrigin::Local => ReferenceScope::Local,
            MutationOrigin::Projection { .. } => ReferenceScope::Projection,
        };
        if handle.scope != scope {
            return Err(WorkspaceError::Unsupported);
        }
        let node = state.node(handle.serial)?;
        if node.attr.kind != NodeKind::File {
            return Err(WorkspaceError::BadHandle);
        }
        super::namespace::check_access(node.attr, self.inner.root.uid, 2)?;
        Ok(handle)
    }
    fn mutation_handle(
        &self,
        state: &State,
        mutation: FileMutation<'_>,
        serial: u64,
    ) -> Result<bool, WorkspaceError> {
        if let FileMutation::Write { handle, origin, .. } = mutation {
            if origin.projected() {
                self.check_projected_write(state, false)?;
            }
            let handle = self.write_handle(state, handle, origin)?;
            if handle.serial != serial {
                return Err(WorkspaceError::BadHandle);
            }
            return Ok(origin.append(handle.options.append));
        }
        Ok(false)
    }
    fn check_mutation_coherence(
        &self,
        state: &State,
        mutation: FileMutation<'_>,
        publication: bool,
    ) -> Result<(), WorkspaceError> {
        match mutation.origin() {
            MutationOrigin::Local => self.check_projection_mutation(state),
            MutationOrigin::Projection { .. } => self.check_projected_write(state, publication),
        }
    }
    /// Atomically overwrites through a writable local handle, filling any gap
    /// with zeros. Append handles select live EOF and ignore the supplied offset.
    /// Empty input validates the request without changing content or timestamps.
    pub fn write_file(
        &self,
        handle: HandleId,
        offset: u64,
        replacement: &OwnedPayload,
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        self.write_file_from(handle, offset, replacement, deadline, MutationOrigin::Local)
    }
    pub(crate) fn write_file_from(
        &self,
        handle: HandleId,
        offset: u64,
        replacement: &OwnedPayload,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<MutationReceipt, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let deadline = Self::callback_deadline(deadline);
        let _operation = self.begin(false, deadline)?;
        self.check_payload_owner(replacement)?;
        if replacement.len() > 8 * 1024 * 1024 {
            return Err(WorkspaceError::Capacity);
        }
        let serial = {
            let state = self.state()?;
            self.available(&state)?;
            let selected = self.write_handle(&state, handle, origin)?;
            if origin.projected() {
                self.check_projected_write(&state, false)?;
            }
            let append = origin.append(selected.options.append);
            if !append
                && offset
                    .checked_add(replacement.len())
                    .is_none_or(|end| end > MAX_FILE)
            {
                return Err(WorkspaceError::Capacity);
            }
            crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
            if replacement.is_empty() && !(origin.projected() && append) {
                return Ok(MutationReceipt {
                    incarnation: self.inner.incarnation,
                    generation: state.generation,
                    inode: selected.serial,
                    revision: state.revision,
                    accepted_bytes: 0,
                });
            }
            selected.serial
        };
        let mutation = FileMutation::Write {
            handle,
            offset,
            replacement,
            origin,
        };
        let original = self.serial_original(serial, deadline);
        {
            let state = self.state()?;
            self.available(&state)?;
            self.mutation_handle(&state, mutation, serial)?;
        }
        self.mutate_file(original?, mutation, deadline, None)
    }
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
    ) -> Result<Original, WorkspaceError> {
        let (base, baseline) = {
            let state = self.state()?;
            if let Some(node) = state
                .nodes
                .iter()
                .find(|n| n.path() == path.as_ref() && n.baseline == state.baseline)
            {
                return Ok((node.original, node.content, node.metadata, state.baseline));
            }
            (state.base, state.baseline)
        };
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
                    root: base,
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
        result
            .map(|(attr, content, metadata)| (attr, content, metadata, baseline))
            .ok_or(WorkspaceError::InvalidInput)
    }
    fn serial_original(&self, serial: u64, deadline: Instant) -> Result<Original, WorkspaceError> {
        let _path = self
            .host
            .budget
            .reserve(crate::runtime::state::PATH_BYTES)?;
        let (base, baseline, path, path_len, selected) = {
            let state = self.state()?;
            self.available(&state)?;
            let node = state.node(serial)?;
            if node.attr.kind == NodeKind::Directory {
                return Err(WorkspaceError::IsDirectory);
            }
            if node.attr.kind != NodeKind::File {
                return Err(WorkspaceError::WrongKind);
            }
            super::namespace::check_access(node.attr, self.inner.root.uid, 2)?;
            if node.baseline == state.baseline {
                return Ok((node.original, node.content, node.metadata, state.baseline));
            }
            (
                state.base,
                state.baseline,
                node.path,
                node.path_len,
                node.attr,
            )
        };
        let _remote = self.begin(true, deadline)?;
        let mut bytes = vector(path_len)?;
        bytes.extend_from_slice(&path[..path_len]);
        let response = self.call(
            Operation::Inspect {
                root: base,
                query: Inspect::Attributes { path: bytes },
            },
            0,
            &mut std::io::sink(),
            deadline,
        )?;
        let (attr, content, metadata) = super::namespace::attributes(
            response,
            false,
            self.inner.root.uid,
            self.inner.root.gid,
        )?;
        if attr.serial != serial
            || attr.kind != selected.kind
            || attr.references != selected.references
        {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok((attr, content, metadata, baseline))
    }
    /// Changes an existing cached regular inode's length and modification time.
    /// Logical extension owns Zero pieces, without accepting payload bytes.
    pub fn set_len(
        &self,
        serial: u64,
        length: u64,
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let deadline = Self::callback_deadline(deadline);
        let _operation = self.begin(false, deadline)?;
        if length > MAX_FILE {
            return Err(WorkspaceError::Capacity);
        }
        let original = self.serial_original(serial, deadline)?;
        self.mutate_file(original, FileMutation::SetLen(length), deadline, None)
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
        self.check_payload_owner(&edit.replacement)?;
        if edit.start > edit.end {
            return Err(WorkspaceError::InvalidInput);
        }
        if edit.replacement.len() > 8 * 1024 * 1024 {
            return Err(WorkspaceError::Capacity);
        }
        let original = self.edit_original(path, deadline)?;
        self.mutate_file(original, FileMutation::Range(edit), deadline, None)
    }
    pub(super) fn truncate_open(
        &self,
        reserved: &mut super::open::OpenReservation,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        let original = self.serial_original(reserved.serial, deadline)?;
        self.mutate_file(original, FileMutation::SetLen(0), deadline, Some(reserved))?;
        Ok(())
    }
    fn mutate_file(
        &self,
        original: Original,
        mutation: FileMutation<'_>,
        deadline: Instant,
        open: Option<&mut super::open::OpenReservation>,
    ) -> Result<MutationReceipt, WorkspaceError> {
        let (receipt, delivery, published_handle) =
            self.publish_file_mutation(original, mutation, deadline, open)?;
        if let Some(delivery) = delivery {
            self.complete_projection_mutation(delivery, receipt, published_handle, deadline)?;
        }
        Ok(receipt)
    }
    fn publish_file_mutation(
        &self,
        original: Original,
        mutation: FileMutation<'_>,
        deadline: Instant,
        open: Option<&mut super::open::OpenReservation>,
    ) -> Result<
        (
            MutationReceipt,
            Option<ProjectionInvalidation>,
            Option<HandleId>,
        ),
        WorkspaceError,
    > {
        let (original, content, metadata, baseline) = original;
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        if original.kind == NodeKind::Directory {
            return Err(WorkspaceError::IsDirectory);
        }
        if original.kind != NodeKind::File {
            return Err(WorkspaceError::WrongKind);
        }
        super::namespace::check_access(original, self.inner.root.uid, 2)?;
        {
            let state = self.state()?;
            self.available(&state)?;
            self.mutation_handle(&state, mutation, original.serial)?;
            self.check_mutation_coherence(&state, mutation, false)?;
            if state.baseline != baseline {
                return Err(WorkspaceError::Busy);
            }
            if let Some(reserved) = open.as_ref() {
                reserved.validate(&state, original.serial)?;
            }
        }
        self.maintain_backing(deadline)?;
        let _writer = host.writer()?;
        let (expected_revision, generation, dirty, old_root, needs_completion, frozen, append) = {
            let s = self.state()?;
            self.available(&s)?;
            let append = self.mutation_handle(&s, mutation, original.serial)?;
            self.check_mutation_coherence(&s, mutation, false)?;
            if s.baseline != baseline {
                return Err(WorkspaceError::Busy);
            }
            (
                s.revision,
                s.generation,
                s.dirty_inodes,
                s.overlay.clone(),
                s.completion.is_none(),
                s.submission.clone(),
                append,
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
        let mut replacement = [Piece {
            kind: PieceKind::Zero,
            start: 0,
            length: 0,
            offset: 0,
            payload: 0,
            custody: PageRef::NULL,
        }; 2];
        let payload = match mutation {
            FileMutation::Range(edit) => Some(&edit.replacement),
            FileMutation::Write { replacement, .. } => Some(replacement),
            FileMutation::SetLen(_) => None,
        };
        let (start, end, accepted_bytes) = match mutation {
            FileMutation::Range(edit) => {
                if edit.start > edit.end || edit.end > inode.length {
                    return Err(WorkspaceError::InvalidInput);
                }
                (edit.start, edit.end, edit.replacement.len())
            }
            FileMutation::SetLen(length) => {
                replacement[0].length = length.saturating_sub(inode.length);
                (length.min(inode.length), inode.length, 0)
            }
            FileMutation::Write {
                offset,
                replacement: payload,
                origin,
                ..
            } => {
                if append && origin.projected() && offset != inode.length {
                    return Err(WorkspaceError::InvalidInput);
                }
                let offset = if append { inode.length } else { offset };
                let end = offset
                    .checked_add(payload.len())
                    .filter(|end| *end <= MAX_FILE)
                    .ok_or(WorkspaceError::Capacity)?;
                replacement[0].length = offset.saturating_sub(inode.length);
                (
                    offset.min(inode.length),
                    end.min(inode.length),
                    payload.len(),
                )
            }
        };
        if matches!(mutation, FileMutation::Write { .. }) && accepted_bytes == 0 {
            let state = self.state()?;
            self.available(&state)?;
            self.mutation_handle(&state, mutation, original.serial)?;
            crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
            return Ok((
                MutationReceipt {
                    incarnation: self.inner.incarnation,
                    generation: state.generation,
                    inode: original.serial,
                    revision: state.revision,
                    accepted_bytes: 0,
                },
                None,
                None,
            ));
        }
        if let Some(payload) = payload {
            let held = payload
                .record
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .custody;
            replacement[1] = Piece {
                kind: PieceKind::Local,
                start: 0,
                length: payload.len(),
                offset: 0,
                payload: payload.record.id,
                custody: held
                    .filter(|(id, _)| *id == arena.id)
                    .map_or(PageRef { slot: 1, epoch: 1 }, |(_, r)| r),
            };
        }
        let length = inode
            .length
            .checked_sub(end - start)
            .and_then(|n| n.checked_add(replacement[0].length))
            .and_then(|n| n.checked_add(replacement[1].length))
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
                    kind: PieceKind::Base,
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
                    kind: PieceKind::Base,
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
        let (mut pieces, edits, replacement_bytes) =
            pieces::splice(&old_pieces, start, end, &replacement, inode.base_length)?;
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
        if let Some(payload) = payload {
            if !payload.is_empty() {
                let custody = arena.custody(&candidate, payload, window, deadline)?;
                for p in &mut pieces {
                    if p.kind == PieceKind::Local && p.payload == payload.record.id {
                        p.custody = custody;
                    }
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
        self.mutation_handle(&state, mutation, original.serial)?;
        self.check_mutation_coherence(&state, mutation, true)?;
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
        let ready_index = open
            .as_ref()
            .map(|reserved| reserved.validate(&state, original.serial))
            .transpose()?;
        let receipt = MutationReceipt {
            incarnation: self.inner.incarnation,
            generation,
            inode: original.serial,
            revision,
            accepted_bytes,
        };
        let published_handle = open.as_ref().map(|reserved| reserved.id);
        let delivery = state
            .projection
            .as_ref()
            .and_then(|projection| projection.delivery.clone());
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
        if let Some(projection) = &mut state.projection {
            projection.status = CoherenceStatus::Pending {
                receipt,
                published_handle,
            };
        }
        if let (Some(reserved), Some(index)) = (open, ready_index) {
            reserved.publish(&mut state, index);
        }
        // Returning ends temporary piece vectors, state/window guards and finally
        // the writer's working reservation before the outer notification call.
        Ok((receipt, delivery, published_handle))
    }
}
