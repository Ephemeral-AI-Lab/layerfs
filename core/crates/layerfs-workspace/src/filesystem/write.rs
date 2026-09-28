//! One local splice, prepared outside the state lock and published with an exact head stamp.
use super::original::Original;
use crate::types::PortableAttributes;
use crate::{
    backing::{metadata::RootOwner, metadata_pages},
    overlay::pieces::Inode,
    runtime::{
        coherence::MutationOrigin,
        state::{Handle, State},
    },
    *,
};
use layerfs_bridge::contract::MAX_FILE;
use std::{sync::Arc, time::Instant};
pub(crate) struct Publication {
    pub(crate) receipt: MutationReceipt,
    pub(crate) attributes: NodeAttributes,
    pub(crate) delivery: Option<ProjectionInvalidation>,
    pub(crate) published_handle: Option<HandleId>,
    pub(crate) cleanup_error: Option<WorkspaceError>,
}
#[derive(Clone, Copy)]
pub(crate) enum FileMutation<'a> {
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
    TinyWrite {
        handle: HandleId,
        offset: u64,
        bytes: &'a [u8],
        origin: MutationOrigin,
    },
}
impl FileMutation<'_> {
    pub(crate) fn origin(self) -> MutationOrigin {
        match self {
            Self::Write { origin, .. }
            | Self::TinyWrite { origin, .. }
            | Self::Attributes { origin, .. } => origin,
        }
    }
}
impl Workspace {
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
            FileMutation::Write { handle, .. } | FileMutation::TinyWrite { handle, .. } => {
                Some(handle)
            }
            FileMutation::Attributes { handle, .. } => handle,
        };
        if let Some(handle) = handle {
            let handle = self.write_handle(state, handle, origin)?;
            if handle.serial != serial {
                return Err(WorkspaceError::BadHandle);
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
        self.write_file_mutation_from(
            handle,
            offset,
            FileMutation::Write {
                handle,
                offset,
                replacement,
                origin,
            },
            deadline,
        )
    }

    /// A FUSE callback's <=128-byte input is copied and Budget-charged before
    /// any mutation can publish or reply. Larger and ordinary local inputs
    /// retain their existing immutable OwnedPayload route.
    pub(crate) fn write_tiny_file_from(
        &self,
        handle: HandleId,
        offset: u64,
        input: &[u8],
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<MutationReceipt, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        if input.len() > 128 {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut charge = self.host.budget.reserve(input.len())?;
        let mut bytes = Vec::with_capacity(input.len());
        charge.resize(bytes.capacity())?;
        bytes.extend_from_slice(input);
        let result = self.write_file_mutation_from(
            handle,
            offset,
            FileMutation::TinyWrite {
                handle,
                offset,
                bytes: &bytes,
                origin,
            },
            deadline,
        );
        drop(charge);
        result
    }

    fn write_file_mutation_from(
        &self,
        handle: HandleId,
        offset: u64,
        mutation: FileMutation<'_>,
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let length = match mutation {
            FileMutation::Write { replacement, .. } => replacement.len(),
            FileMutation::TinyWrite { bytes, .. } => bytes.len() as u64,
            FileMutation::Attributes { .. } => return Err(WorkspaceError::InvalidInput),
        };
        let origin = mutation.origin();
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        if let FileMutation::Write { replacement, .. } = mutation {
            self.check_payload_owner(replacement)?;
        }
        if length > 8 * 1024 * 1024 {
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
            if !append && offset.checked_add(length).is_none_or(|end| end > MAX_FILE) {
                return Err(WorkspaceError::Capacity);
            }
            crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
            if length == 0 && !(origin.projected() && append) {
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
                        active: None,
                        origins: None,
                        directory_path: None,
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
        let mut published =
            self.publish_active_file_mutation(original, mutation, deadline, open)?;
        if let Some(delivery) = published.delivery.take() {
            self.complete_projection_mutation(
                delivery,
                published.receipt,
                None,
                published.published_handle,
                deadline,
            )?;
        }
        if let Some(cause) = published.cleanup_error.take() {
            return Err(WorkspaceError::Published {
                receipt: published.receipt,
                published_handle: published.published_handle,
                cause: Box::new(cause),
            });
        }
        Ok(published)
    }
}
