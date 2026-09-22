//! One checked portable-attribute request applied to the selected inode version.
use super::namespace_view::View;
use crate::{
    backing::{
        metadata_index::vector,
        metadata_pages::{self, Cell, PageRef},
    },
    overlay::{
        directories::{Directory, Origin},
        pieces::CapturedBase,
    },
    runtime::coherence::MutationOrigin,
    *,
};
use std::{
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
impl Workspace {
    /// Changes portable mode and mtime with an optional length change. Every
    /// field is checked against the selected kind and version before any page is
    /// written; an unsupported or invalid field changes nothing. A metadata-only
    /// change keeps the selected content and republishes only attributes.
    pub fn set_attributes(
        &self,
        serial: u64,
        request: PortableAttributes,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.set_attributes_from(serial, request, deadline, MutationOrigin::Local)
    }
    pub(crate) fn set_attributes_from(
        &self,
        serial: u64,
        request: PortableAttributes,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<NodeAttributes, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        if request.is_empty() {
            return Err(WorkspaceError::InvalidInput);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        let attr = {
            let state = self.state()?;
            self.available(&state)?;
            state.node(serial)?.attr
        };
        request.check(attr.kind, attr.size)?;
        if origin.projected() {
            let state = self.state()?;
            self.available(&state)?;
            self.check_projected_mutation(&state, false)?;
        }
        if attr.kind == NodeKind::Directory {
            if request.size.is_some() {
                return Err(WorkspaceError::Unsupported);
            }
            return self
                .mutate_directory(attr, request, deadline, origin)
                .map(|published| published.attributes);
        }
        let original = self.serial_original(serial, deadline, &mut operation)?;
        if original.0.kind != attr.kind {
            return Err(WorkspaceError::WrongKind);
        }
        if attr.kind != NodeKind::File {
            return Err(WorkspaceError::WrongKind);
        }
        self.mutate_file(
            original,
            super::write::FileMutation::Attributes {
                request,
                handle: None,
                origin,
            },
            deadline,
            None,
        )
        .map(|published| published.attributes)
    }
}
impl Workspace {
    /// Publishes one metadata-only change of a directory's maintained delta.
    /// Entries and tombstones are preserved exactly; a directory that has no
    /// delta this generation keeps the canonical origin it already selected.
    fn mutate_directory(
        &self,
        attr: NodeAttributes,
        request: PortableAttributes,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<super::write::Publication, WorkspaceError> {
        if attr.kind != NodeKind::Directory || request.size.is_some() {
            return Err(WorkspaceError::Unsupported);
        }
        let (mode, seconds, nanos) = request.selected(attr);
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        self.maintain_backing(deadline)?;
        let (view, expected_revision, generation, old_root, needs_completion, frozen) = {
            let state = self.state()?;
            self.available(&state)?;
            self.check_mutation_coherence(&state, origin, false)?;
            (
                View {
                    base: state.base,
                    root: state.overlay.clone(),
                },
                state.revision,
                state.generation,
                state.overlay.clone(),
                state.completion.is_none(),
                state.submission.clone(),
            )
        };
        // The delta is read under its own writer window; the publication below
        // takes the writer only after this read has released it.
        let current = self.directory_record(&view, attr.serial, deadline)?;
        let (reanchor, already_dirty) = {
            let _view = host.writer()?;
            let mut lease = host.payloads.window(1, 3)?;
            self.directory_delta(
                view.root.as_ref(),
                generation,
                attr.serial,
                current.as_ref(),
                lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                deadline,
            )?
        };
        let _writer = host.writer()?;
        let arena = self
            .inner
            .arena
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut lease = host.payloads.window(0, 1)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let mut directory = current.unwrap_or_else(|| Directory::initial(attr, view.base));
        if reanchor {
            // This operation changes no binding, so the delta keeps the exact
            // entry and tombstone pages it inherited from its own origin.
            if let (Some(prior), Some(previous)) =
                (current, crate::backing::metadata::MetadataHost::anchor(view.root.as_ref()).as_ref())
            {
                // The delta anchors on the exact earlier root this operation's
                // candidate is built on; a frozen submission's captured root is
                // the same root only while nothing replaced it.
                if let Some(submission) = &frozen {
                    let capture = submission.capture()?;
                    if prior.generation != capture.generation {
                        return Err(WorkspaceError::Io);
                    }
                }
                directory.origin = Origin::Captured(CapturedBase {
                    root: previous.root()?,
                    inode: attr.serial,
                    generation: prior.generation,
                    revision: prior.revision,
                });
            }
        }
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| WorkspaceError::Io)?;
        let _ = time;
        directory.generation = generation;
        directory.revision = expected_revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        directory.mode = mode;
        directory.seconds = seconds;
        directory.nanos = nanos;
        let parent = frozen
            .as_ref()
            .map(|submission| submission.capture().map(|capture| capture.root.clone()))
            .transpose()?;
        let candidate = host.candidate(arena, generation, needs_completion, parent)?;
        let mut updates = vector(2)?;
        updates.push(Cell::new(
            &metadata_pages::dirty_key(generation, attr.serial),
            &[1],
        )?);
        updates.push(Cell::new(
            &metadata_pages::namespace_key(attr.serial),
            &directory.value(),
        )?);
        updates.sort_unstable_by(|a, b| a.key().cmp(b.key()));
        let root = candidate.update(
            old_root
                .as_ref()
                .map(|owner| owner.root())
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
        self.check_mutation_coherence(&state, origin, true)?;
        let same_root = match (&state.overlay, &old_root) {
            (None, None) => true,
            (Some(current), Some(expected)) => Arc::ptr_eq(current, expected),
            _ => false,
        };
        if state.revision != expected_revision
            || !same_root
            || state.generation != generation
            || state.completion.is_none() != needs_completion
        {
            return Err(WorkspaceError::Busy);
        }
        let receipt = MutationReceipt {
            incarnation: self.inner.incarnation,
            generation,
            inode: attr.serial,
            revision: directory.revision,
            accepted_bytes: 0,
        };
        if needs_completion {
            state.completion = candidate.take_completion(generation)?;
            if state.completion.is_none() {
                return Err(WorkspaceError::Io);
            }
        }
        let attributes = directory.attributes(attr);
        for node in &mut state.nodes {
            if node.attr.serial == attr.serial {
                node.attr = attributes;
            }
        }
        let retired = state.overlay.replace(candidate);
        state.revision = directory.revision;
        if !already_dirty {
            state.dirty_inodes += 1;
            state.dirty_directories += 1;
        }
        drop(state);
        drop(retired);
        let _ = directory;
        Ok(super::write::Publication {
            receipt,
            attributes,
            delivery: None,
            published_handle: None,
        })
    }
}
