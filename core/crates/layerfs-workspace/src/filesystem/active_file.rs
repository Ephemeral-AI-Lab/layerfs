//! Public file mutations publish through the Workspace's active index.
use super::{
    open::OpenReservation,
    original::Original,
    write::{FileMutation, Publication},
};
use crate::{
    backing::active::{Extent, HotInode},
    *,
};
use layerfs_bridge::contract::{Source, MAX_FILE};
use std::time::Instant;

impl Workspace {
    pub(super) fn publish_active_file_mutation(
        &self,
        original: Original,
        mutation: FileMutation<'_>,
        deadline: Instant,
        mut open: Option<&mut OpenReservation>,
    ) -> Result<Publication, WorkspaceError> {
        let (original, content, metadata, baseline) = original;
        if original.kind != NodeKind::File {
            return Err(if original.kind == NodeKind::Directory {
                WorkspaceError::IsDirectory
            } else {
                WorkspaceError::WrongKind
            });
        }
        let active = self
            .inner
            .active
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut state = self.state()?;
        self.available(&state)?;
        self.check_mutation_coherence(&state, mutation.origin(), false)?;
        if state.baseline != baseline {
            return Err(WorkspaceError::Busy);
        }
        let append = self.mutation_handle(&state, mutation, original.serial)?;
        let ready_index = open
            .as_ref()
            .map(|reserved| reserved.validate(&state, original.serial))
            .transpose()?;
        let attr = state.node(original.serial)?.attr;
        let (stored, already_dirty) = active.file_facts(original.serial)?;
        let selected = match stored {
            Some(value) => value,
            None => HotInode {
                revision: state.revision.max(1),
                generation: state.generation,
                length: attr.size,
                kind: NodeKind::File,
                fresh: false,
                storage: u8::from(attr.size > 0),
                mode: attr.mode,
                seconds: attr.mtime_seconds,
                nanos: attr.mtime_nanoseconds,
                links: u32::try_from(attr.references).unwrap_or(u32::MAX),
                base: content,
                metadata,
                inline: if attr.size > 0 {
                    [Some(Extent::base(0, attr.size)), None, None, None]
                } else {
                    [None; 4]
                },
            },
        };
        if selected.kind != NodeKind::File || selected.length != attr.size {
            return Err(WorkspaceError::Io);
        }
        let (accepted, requested) = match mutation {
            FileMutation::Attributes {
                request, handle, ..
            } => {
                request.check(attr.kind, attr.size)?;
                if handle.is_none() && request.size.is_some() {
                    super::namespace::check_access(attr, self.inner.root.uid, 2)?;
                }
                (0, Some(request))
            }
            FileMutation::Write { offset, origin, .. }
            | FileMutation::TinyWrite { offset, origin, .. } => {
                let length = match mutation {
                    FileMutation::Write { replacement, .. } => replacement.len(),
                    FileMutation::TinyWrite { bytes, .. } => bytes.len() as u64,
                    FileMutation::Attributes { .. } => return Err(WorkspaceError::Io),
                };
                if append && origin.projected() && offset != attr.size {
                    return Err(WorkspaceError::InvalidInput);
                }
                let at = if append { attr.size } else { offset };
                if at.checked_add(length).is_none_or(|end| end > MAX_FILE) {
                    return Err(WorkspaceError::Capacity);
                }
                (length, None)
            }
        };
        if accepted == 0 && requested.is_none() {
            return Ok(Publication {
                receipt: MutationReceipt {
                    incarnation: self.inner.incarnation,
                    generation: state.generation,
                    inode: original.serial,
                    revision: state.revision,
                    accepted_bytes: 0,
                },
                attributes: state.presented(attr),
                delivery: None,
                published_handle: None,
                cleanup_error: None,
            });
        }
        state.frontier_bytes(
            &self.host,
            state.dirty_inodes + usize::from(!already_dirty),
            state.dirty_directories,
            state.fresh_files,
            state.fresh_symlinks,
            state.directory_names,
            state.directory_bytes,
        )?;
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let (revision, cleanup_error, updated) = match mutation {
            FileMutation::Attributes { .. } => {
                let result = active.set_attributes_file(
                    original.serial,
                    selected,
                    requested.ok_or(WorkspaceError::Io)?,
                )?;
                (result.revision, result.cleanup_error, result.inode)
            }
            FileMutation::Write {
                offset,
                replacement,
                ..
            } => {
                let at = if append { attr.size } else { offset };
                if let Some(source) =
                    self.read_origin_payload(original.serial, selected.base, replacement, deadline)?
                {
                    let result = active.publish_read_origin(
                        original.serial,
                        selected,
                        at,
                        replacement.len(),
                        source,
                    )?;
                    (result.revision, result.cleanup_error, result.inode)
                } else if replacement.len() <= 128 {
                    let mut reader = replacement.reader(0..replacement.len())?;
                    let mut small_charge = self.host.budget.reserve(replacement.len() as usize)?;
                    let mut bytes = Vec::with_capacity(replacement.len() as usize);
                    small_charge.resize(bytes.capacity())?;
                    bytes.resize(replacement.len() as usize, 0);
                    let mut done = 0;
                    while done < bytes.len() {
                        let read = Source::read(
                            &mut reader,
                            &mut bytes[done..],
                            deadline,
                            &self.inner.stopping,
                        )
                        .map_err(|_| WorkspaceError::Io)?;
                        if read == 0 {
                            return Err(WorkspaceError::Io);
                        }
                        done += read;
                    }
                    let result = active.write_tiny_file(original.serial, selected, at, &bytes)?;
                    (result.revision, result.cleanup_error, result.inode)
                } else {
                    let result =
                        active.write_payload_file(original.serial, selected, at, replacement)?;
                    (result.revision, result.cleanup_error, result.inode)
                }
            }
            FileMutation::TinyWrite { offset, bytes, .. } => {
                let at = if append { attr.size } else { offset };
                if let Some(source) =
                    self.read_origin_bytes(original.serial, selected.base, bytes)?
                {
                    let result = active.publish_read_origin(
                        original.serial,
                        selected,
                        at,
                        bytes.len() as u64,
                        source,
                    )?;
                    (result.revision, result.cleanup_error, result.inode)
                } else {
                    let result = active.write_tiny_file(original.serial, selected, at, bytes)?;
                    (result.revision, result.cleanup_error, result.inode)
                }
            }
        };
        let next = state
            .revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        if revision != next {
            return Err(WorkspaceError::Io);
        }
        let updated = updated.ok_or(WorkspaceError::Io)?;
        let attributes = updated.attributes(attr)?;
        let receipt = MutationReceipt {
            incarnation: self.inner.incarnation,
            generation: state.generation,
            inode: original.serial,
            revision,
            accepted_bytes: accepted,
        };
        let published_handle = open.as_ref().map(|reserved| reserved.id);
        let delivery = if matches!(
            mutation.origin(),
            crate::runtime::coherence::MutationOrigin::ProjectionSize
                | crate::runtime::coherence::MutationOrigin::ProjectionCreate
        ) {
            None
        } else {
            state
                .projection
                .as_ref()
                .and_then(|projection| projection.delivery.clone())
        };
        state.node_mut(original.serial)?.attr = attributes;
        state.revision = revision;
        if !already_dirty {
            state.dirty_inodes += 1;
        }
        if let (Some(projection), Some(_)) = (&mut state.projection, &delivery) {
            projection.status = CoherenceStatus::Pending {
                receipt,
                published_handle,
            };
        }
        if let (Some(reserved), Some(index)) = (open.as_mut(), ready_index) {
            reserved.publish(&mut state, index);
        }
        Ok(Publication {
            receipt,
            attributes: state.presented(attributes),
            delivery,
            published_handle,
            cleanup_error,
        })
    }
}
