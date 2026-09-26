//! One local splice, prepared outside the state lock and published with an exact head stamp.
use super::original::Original;
use crate::types::PortableAttributes;
use crate::{
    backing::{
        metadata::RootOwner,
        metadata_index::vector,
        metadata_pages::{self, Cell, PageRef},
        metadata_pieces,
    },
    overlay::pieces::{CapturedBase, Inode, Piece, PieceKind},
    runtime::{
        coherence::MutationOrigin,
        state::{Handle, State},
    },
    *,
};
use layerfs_bridge::contract::MAX_FILE;
use std::{
    sync::{atomic::Ordering, Arc},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
pub(crate) struct Publication {
    pub(crate) receipt: MutationReceipt,
    pub(crate) attributes: NodeAttributes,
    pub(crate) delivery: Option<ProjectionInvalidation>,
    pub(crate) published_handle: Option<HandleId>,
}
#[derive(Clone, Copy)]
pub(crate) enum FileMutation<'a> {
    Range {
        edit: &'a RangeEdit,
        parts: Option<&'a [RangePart]>,
        handle: Option<HandleId>,
        expected: Option<RangeStamp>,
        origin: MutationOrigin,
    },
    Attributes {
        request: PortableAttributes,
        handle: Option<HandleId>,
        origin: MutationOrigin,
    },
    Write {
        handle: HandleId,
        offset: u64,
        replacement: &'a OwnedPayload,
        origin: MutationOrigin,
    },
}
impl FileMutation<'_> {
    pub(crate) fn origin(self) -> MutationOrigin {
        match self {
            Self::Write { origin, .. }
            | Self::Attributes { origin, .. }
            | Self::Range { origin, .. } => origin,
        }
    }
    fn is_metadata_only(self) -> bool {
        matches!(self, Self::Attributes { request, .. } if request.size.is_none())
    }
}
impl Workspace {
    /// Reads a projected descriptor's current stamp and visible metadata together.
    pub fn projected_range_state(
        &self,
        handle: HandleId,
        inode: u64,
    ) -> Result<RangeState, WorkspaceError> {
        let state = self.state()?;
        self.available(&state)?;
        if !state.mounted {
            return Err(WorkspaceError::Closed);
        }
        let opened = state.handle(handle, false)?;
        if opened.scope != ReferenceScope::Projection || opened.serial != inode {
            return Err(WorkspaceError::BadHandle);
        }
        let attr = state.presented(state.node(inode)?.attr);
        if attr.kind != NodeKind::File {
            return Err(WorkspaceError::BadHandle);
        }
        let writable = self.inner.access == WorkspaceAccess::LocalEdit
            && opened.options.access != FileAccess::ReadOnly
            && !opened.options.append
            && state
                .projection
                .as_ref()
                .is_some_and(|projection| projection.status == CoherenceStatus::Ready);
        Ok(RangeState {
            stamp: RangeStamp {
                inode,
                incarnation: self.inner.incarnation,
                generation: state.generation,
                revision: state.revision,
            },
            length: attr.size,
            mtime_seconds: attr.mtime_seconds,
            mtime_nanoseconds: attr.mtime_nanoseconds,
            writable,
        })
    }
    fn check_range_stamp(
        &self,
        state: &State,
        inode: u64,
        expected: RangeStamp,
    ) -> Result<(), WorkspaceError> {
        if expected.inode != inode
            || expected.incarnation != self.inner.incarnation
            || expected.generation != state.generation
            || expected.revision != state.revision
        {
            return Err(WorkspaceError::StaleStamp);
        }
        Ok(())
    }
    pub(super) fn check_payload_owner(
        &self,
        replacement: &OwnedPayload,
    ) -> Result<(), WorkspaceError> {
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
    pub(super) fn write_handle(
        &self,
        state: &State,
        id: HandleId,
        origin: MutationOrigin,
    ) -> Result<Handle, WorkspaceError> {
        let handle = state.handle(id, false)?;
        if handle.options.access == FileAccess::ReadOnly {
            return Err(WorkspaceError::BadHandle);
        }
        let scope = if origin.projected() {
            ReferenceScope::Projection
        } else {
            ReferenceScope::Local
        };
        if handle.scope != scope {
            return Err(WorkspaceError::Unsupported);
        }
        let node = state.node(handle.serial)?;
        if node.attr.kind != NodeKind::File {
            return Err(WorkspaceError::BadHandle);
        }
        Ok(handle)
    }
    pub(super) fn mutation_handle(
        &self,
        state: &State,
        mutation: FileMutation<'_>,
        serial: u64,
    ) -> Result<bool, WorkspaceError> {
        let origin = mutation.origin();
        if origin.projected() {
            self.check_projected_mutation(state, false)?;
        }
        let handle = match mutation {
            FileMutation::Write { handle, .. } => Some(handle),
            FileMutation::Attributes { handle, .. } => handle,
            FileMutation::Range { handle, .. } => handle,
        };
        if let Some(handle) = handle {
            let handle = self.write_handle(state, handle, origin)?;
            if handle.serial != serial {
                return Err(WorkspaceError::BadHandle);
            }
            if matches!(mutation, FileMutation::Range { .. }) && handle.options.append {
                return Err(WorkspaceError::BadHandle);
            }
            if let FileMutation::Range {
                expected: Some(expected),
                ..
            } = mutation
            {
                self.check_range_stamp(state, serial, expected)?;
            }
            return Ok(origin.append(handle.options.append));
        }
        Ok(false)
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
        let mut operation = self.begin(false, deadline)?;
        self.check_payload_owner(replacement)?;
        if replacement.len() > 8 * 1024 * 1024 {
            return Err(WorkspaceError::Capacity);
        }
        let serial = {
            let state = self.state()?;
            self.available(&state)?;
            let selected = self.write_handle(&state, handle, origin)?;
            if origin.projected() {
                self.check_projected_mutation(&state, false)?;
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
        let original = self.serial_original(serial, deadline, &mut operation);
        {
            let state = self.state()?;
            self.available(&state)?;
            self.mutation_handle(&state, mutation, serial)?;
        }
        self.mutate_file(original?, mutation, deadline, None)
            .map(|published| published.receipt)
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
        if attr.kind == NodeKind::Directory {
            return Ok(self
                .directory_record(
                    &super::namespace_view::View {
                        base: [0; 32],
                        root: root.cloned(),
                    },
                    attr.serial,
                    deadline,
                )?
                .map_or(attr, |directory| directory.attributes(attr)));
        }
        match self.overlay_inode(attr.serial, root, deadline)? {
            Some(inode) if inode.kind() == attr.kind => Ok(inode.attributes(attr)),
            Some(_) => Err(WorkspaceError::Io),
            None => Ok(attr),
        }
    }
    /// Changes an existing cached regular inode's length and modification time.
    /// Logical extension owns Zero pieces, without accepting payload bytes.
    pub fn set_len(
        &self,
        serial: u64,
        length: u64,
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        self.resize_file(serial, length, None, deadline, MutationOrigin::Local)
            .map(|published| published.receipt)
    }
    pub(crate) fn set_len_projected(
        &self,
        serial: u64,
        length: u64,
        handle: Option<HandleId>,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.resize_file(
            serial,
            length,
            handle,
            deadline,
            MutationOrigin::ProjectionSize,
        )
        .map(|published| published.attributes)
    }
    fn resize_file(
        &self,
        serial: u64,
        length: u64,
        handle: Option<HandleId>,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<Publication, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        if length > MAX_FILE {
            return Err(WorkspaceError::Capacity);
        }
        let mutation = FileMutation::Attributes {
            request: PortableAttributes {
                size: Some(length),
                ..PortableAttributes::default()
            },
            handle,
            origin,
        };
        if handle.is_none() {
            let state = self.state()?;
            super::namespace::check_access(state.node(serial)?.attr, self.inner.root.uid, 2)?;
        }
        if origin.projected() {
            let state = self.state()?;
            self.available(&state)?;
            self.mutation_handle(&state, mutation, serial)?;
        }
        let original = self.serial_original(serial, deadline, &mut operation);
        if origin.projected() {
            let state = self.state()?;
            self.available(&state)?;
            self.mutation_handle(&state, mutation, serial)?;
        }
        self.mutate_file(original?, mutation, deadline, None)
    }
    pub(super) fn truncate_open(
        &self,
        reserved: &mut super::open::OpenReservation,
        deadline: Instant,
        operation: &mut crate::runtime::state::OperationGuard,
        origin: MutationOrigin,
    ) -> Result<NodeAttributes, WorkspaceError> {
        let original = self.serial_original(reserved.serial, deadline, operation)?;
        self.mutate_file(
            original,
            FileMutation::Attributes {
                request: PortableAttributes {
                    size: Some(0),
                    ..PortableAttributes::default()
                },
                handle: None,
                origin,
            },
            deadline,
            Some(reserved),
        )
        .map(|published| published.attributes)
    }
    pub(super) fn mutate_file(
        &self,
        original: Original,
        mutation: FileMutation<'_>,
        deadline: Instant,
        open: Option<&mut super::open::OpenReservation>,
    ) -> Result<Publication, WorkspaceError> {
        let mut published = self.publish_file_mutation(original, mutation, deadline, open)?;
        if let Some(delivery) = published.delivery.take() {
            self.complete_projection_mutation(
                delivery,
                published.receipt,
                None,
                published.published_handle,
                deadline,
            )?;
        }
        Ok(published)
    }
    fn publish_file_mutation(
        &self,
        original: Original,
        mutation: FileMutation<'_>,
        deadline: Instant,
        open: Option<&mut super::open::OpenReservation>,
    ) -> Result<Publication, WorkspaceError> {
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
        if matches!(
            mutation,
            FileMutation::Range { handle: None, .. }
                | FileMutation::Attributes {
                    handle: None,
                    request: PortableAttributes { size: Some(_), .. },
                    ..
                }
        ) {
            super::namespace::check_access(original, self.inner.root.uid, 2)?;
        }
        {
            let state = self.state()?;
            self.available(&state)?;
            self.mutation_handle(&state, mutation, original.serial)?;
            self.check_mutation_coherence(&state, mutation.origin(), false)?;
            if state.baseline != baseline {
                return Err(WorkspaceError::Busy);
            }
            if let Some(reserved) = open.as_ref() {
                reserved.validate(&state, original.serial)?;
            }
        }
        self.maintain_backing(deadline)?;
        // The mounted publication waits for a current holder instead of
        // refusing: reconciliation holds the gate only for its two short
        // ordering points, and that microsecond overlap must never surface as
        // `EBUSY` to a shell command.
        let _writer = host.writer_until(deadline)?;
        let (expected_revision, generation, dirty, old_root, needs_completion, frozen, append) = {
            let s = self.state()?;
            self.available(&s)?;
            let append = self.mutation_handle(&s, mutation, original.serial)?;
            self.check_mutation_coherence(&s, mutation.origin(), false)?;
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
        if let FileMutation::Attributes { request, .. } = mutation {
            let _ = &request;
        }
        let mut inode = old.unwrap_or_else(|| Inode::initial(original, content, metadata));
        if inode.symlink {
            return Err(WorkspaceError::Io);
        }
        let mut replacement = [Piece {
            kind: PieceKind::Zero,
            length: 0,
            offset: 0,
            payload: 0,
            custody: PageRef::NULL,
        }; 2];
        let payload = match mutation {
            FileMutation::Range { edit, .. } => Some(&edit.replacement),
            FileMutation::Write { replacement, .. } => Some(replacement),
            FileMutation::Attributes { .. } => None,
        };
        let (start, end, accepted_bytes, inserted) = match mutation {
            FileMutation::Range { edit, parts, .. } => {
                if edit.start > edit.end || edit.end > inode.length {
                    return Err(WorkspaceError::InvalidInput);
                }
                (
                    edit.start,
                    edit.end,
                    edit.replacement.len(),
                    super::range::logical_length(edit, parts)?,
                )
            }
            FileMutation::Attributes { request, .. } => {
                let length = request.size.unwrap_or(inode.length);
                replacement[0].length = length.saturating_sub(inode.length);
                (
                    length.min(inode.length),
                    inode.length,
                    0,
                    replacement[0].length,
                )
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
                    replacement[0].length + payload.len(),
                )
            }
        };
        if matches!(mutation, FileMutation::Write { .. }) && accepted_bytes == 0 {
            let state = self.state()?;
            self.available(&state)?;
            self.mutation_handle(&state, mutation, original.serial)?;
            crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
            return Ok(Publication {
                receipt: MutationReceipt {
                    incarnation: self.inner.incarnation,
                    generation: state.generation,
                    inode: original.serial,
                    revision: state.revision,
                    accepted_bytes: 0,
                },
                attributes: state.presented(inode.attributes(original)),
                delivery: None,
                published_handle: None,
            });
        }
        // A metadata-only request never selects replacement extents: the
        // selected content root this generation already is the exact content.
        let metadata_only = mutation.is_metadata_only();
        if metadata_only {
            for piece in &mut replacement {
                piece.length = 0;
            }
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
                length: payload.len(),
                offset: 0,
                payload: payload.record.id,
                custody: held
                    .filter(|(id, _)| *id == arena.id)
                    .map_or(PageRef { slot: 1, epoch: 1 }, |(_, r)| r),
            };
        }
        let stream_pieces = match mutation {
            FileMutation::Range {
                parts: Some(parts), ..
            } => Some(super::range::replacement_pieces(parts, replacement[1])?),
            _ => None,
        };
        let length = inode
            .length
            .checked_sub(end - start)
            .and_then(|n| n.checked_add(inserted))
            .ok_or(WorkspaceError::Capacity)?;
        if length > MAX_FILE {
            return Err(WorkspaceError::Capacity);
        }
        let already_dirty = old.is_some_and(|inode| inode.generation == generation);
        {
            let state = self.state()?;
            if state.revision != expected_revision || state.generation != generation {
                return Err(WorkspaceError::Busy);
            }
            state.frontier_bytes(
                dirty + usize::from(!already_dirty),
                state.dirty_directories,
                state.fresh_files,
                state.fresh_symlinks,
                state.directory_names,
                state.directory_bytes,
            )?;
        }
        let parent = frozen
            .as_ref()
            .map(|submission| submission.capture().map(|g| g.root.clone()))
            .transpose()?;
        let inherited_capture = match (&old, &frozen) {
            (Some(inode), Some(submission)) => inode.generation == submission.capture()?.generation,
            _ => false,
        };
        // The version's own extent sequence. A capture conversion replaces the
        // whole version with one canonical base read of the frozen root, so the
        // local edit list this generation had is exactly empty: a stale edit
        // count would describe extents that no longer exist. A version that has
        // no stored sequence yet is exactly one canonical base read of its
        // selected content, which is the sequence the splice replaces into.
        let mut previous = inode.pieces;
        if inherited_capture {
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
            inode.edits = 0;
            inode.replacement = 0;
            previous = PageRef::NULL;
        }
        // The replacement extents of this mutation, in order. A version whose
        // sequence is one implicit base read folds them into that base inside
        // the splice itself; a stored sequence takes them as the interval's
        // replacement, and a fresh construction takes them as the whole result.
        let mut parts = metadata_pieces::Replacement::new();
        for piece in stream_pieces.as_deref().unwrap_or(&replacement) {
            parts.extend(*piece);
        }
        let candidate = host.candidate(arena, generation, needs_completion, parent)?;
        let mut portions = parts.into_parts();
        if let Some(payload) = payload {
            if !payload.is_empty() {
                let custody = arena.custody(&candidate, payload, window, deadline)?;
                for piece in portions.parts_mut() {
                    if piece.kind == PieceKind::Local && piece.payload == payload.record.id {
                        piece.custody = custody;
                    }
                }
            }
        }
        if !metadata_only {
            let folded = metadata_pieces::replace(
                candidate.as_ref(),
                previous,
                metadata_pieces::Splice {
                    start,
                    end,
                    old_base: inode.base_length,
                    old_replacement: inode.replacement,
                    length,
                },
                &mut portions,
                window,
            )?;
            inode.pieces = folded.root;
            inode.edits = folded.edits;
            inode.replacement = folded.replacement;
        }
        let revision = expected_revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| WorkspaceError::Io)?;
        inode.length = length;
        inode.revision = revision;
        inode.generation = generation;
        let (next_mode, next_seconds, next_nanos) = match mutation {
            // A portable-attribute request changes exactly the fields it names.
            // The fallback for an unnamed field is the selected record's own
            // current value, never the base version's: a mode-only change keeps
            // the live mtime, and an mtime-only change keeps a mode an earlier
            // request in the same generation selected.
            FileMutation::Attributes { request, .. } => {
                let mut current = original;
                current.mode = inode.mode;
                current.mtime_seconds = inode.seconds;
                current.mtime_nanoseconds = inode.nanos;
                request.selected(current)
            }
            _ => (
                inode.mode,
                i64::try_from(time.as_secs()).map_err(|_| WorkspaceError::Capacity)?,
                time.subsec_nanos(),
            ),
        };
        inode.mode = next_mode;
        inode.seconds = next_seconds;
        inode.nanos = next_nanos;
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
        self.check_mutation_coherence(&state, mutation.origin(), true)?;
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
        let attributes = state.presented(inode.attributes(original));
        // SETATTR and CREATE's kernel owners invalidate after their replies and
        // lock boundaries. A synchronous notification here could wait on itself.
        let delivery = if matches!(
            mutation.origin(),
            MutationOrigin::ProjectionSize | MutationOrigin::ProjectionCreate
        ) {
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
        state.overlay = Some(candidate);
        state.revision = revision;
        if let FileMutation::Range {
            edit,
            origin: MutationOrigin::ProjectionRange,
            ..
        } = mutation
        {
            // Piece metadata redirects the suffix; no suffix payload is copied.
            state
                .counters
                .record_range_publication(edit.replacement.len(), 0);
        }
        if !already_dirty {
            state.dirty_inodes += 1;
        }
        for node in &mut state.nodes {
            if node.attr.serial == original.serial {
                node.attr = inode.attributes(node.original);
            }
        }
        if let (Some(projection), Some(_)) = (&mut state.projection, &delivery) {
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
        Ok(Publication {
            receipt,
            attributes,
            delivery,
            published_handle,
        })
    }
}
