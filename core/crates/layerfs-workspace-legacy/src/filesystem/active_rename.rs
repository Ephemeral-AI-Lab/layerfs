//! Active atomic rename publication.
use super::{
    active_names::{hot, now},
    namespace::{check_access, check_name},
    rename::RenameRequest,
};
use crate::{
    backing::active::{dirty_key, inode_key, namespace_key, HotInode, NamespaceRecord},
    runtime::coherence::MutationOrigin,
    *,
};
use layerfs_bridge::contract::Code;
use std::time::Instant;

impl Workspace {
    pub(super) fn rename_active(
        &self,
        request: RenameRequest<'_>,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<(), WorkspaceError> {
        let RenameRequest {
            source_parent,
            source,
            destination_parent,
            destination,
            flags,
        } = request;
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let (mut view, baseline, revision, generation) = {
            let state = self.state()?;
            self.available(&state)?;
            self.check_mutation_coherence(&state, origin, false)?;
            let a = state.node(source_parent)?;
            let b = state.node(destination_parent)?;
            for node in [a, b] {
                if node.attr.kind != NodeKind::Directory {
                    return Err(WorkspaceError::NotDirectory);
                }
                if !node.attached {
                    return Err(WorkspaceError::NotFound);
                }
                check_access(node.attr, self.inner.root.uid, 3)?;
            }
            check_name(source)?;
            check_name(destination)?;
            state.live_chain(source_parent, None, self.inner.root.serial)?;
            state.live_chain(destination_parent, None, self.inner.root.serial)?;
            (
                self.selected_view(&state)?,
                state.baseline,
                state.revision,
                state.generation,
            )
        };
        let moved = self.resolve_child(&mut operation, &view, source_parent, source, deadline)?;
        if source_parent == destination_parent && source == destination {
            return Ok(());
        }
        if moved.attr.serial == self.inner.root.serial {
            return Err(WorkspaceError::Unsupported);
        }
        let replaced = match self.resolve_child(
            &mut operation,
            &view,
            destination_parent,
            destination,
            deadline,
        ) {
            Ok(found) => Some(found),
            Err(WorkspaceError::NotFound) => None,
            Err(WorkspaceError::Service(failure))
                if !failure.unknown
                    && matches!(failure.code, Code::PathNotFound | Code::NotFound) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        if let Some(found) = &replaced {
            if found.attr.serial == moved.attr.serial {
                return Ok(());
            }
            if flags.noreplace {
                return Err(WorkspaceError::Exists);
            }
            if found.attr.kind == NodeKind::Directory {
                if moved.attr.kind != NodeKind::Directory {
                    return Err(WorkspaceError::IsDirectory);
                }
                if !self
                    .list_view(&mut operation, &view, found.attr.serial, &[], 1, deadline)?
                    .is_empty()
                {
                    return Err(WorkspaceError::NotEmpty);
                }
            } else if moved.attr.kind == NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
        }
        let moved_name = crate::runtime::state::NodeName::new(destination, &self.host.budget)?;
        let active = self
            .inner
            .active
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let source_record = active.get(&inode_key(moved.attr.serial))?;
        let symlink_payload = if moved.attr.kind == NodeKind::Symlink
            && source_record.is_none()
            && moved.attr.size > 0
        {
            let target = self.readlink_inode_canonical(
                &mut operation,
                moved.base,
                moved.attr.serial,
                deadline,
            )?;
            if target.len() as u64 != moved.attr.size {
                return Err(WorkspaceError::Io);
            }
            let host = self
                .host
                .payloads
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let directory = self
                .inner
                .directory
                .clone()
                .ok_or(WorkspaceError::Unsupported)?;
            let mut source = target.as_slice();
            Some(host.acquire(
                directory,
                target.len() as u64,
                &mut source,
                deadline,
                &self.inner.stopping,
            )?)
        } else {
            None
        };
        let mut state = self.state()?;
        self.available(&state)?;
        self.check_mutation_coherence(&state, origin, true)?;
        if state.baseline != baseline
            || state.revision != revision
            || state.generation != generation
        {
            return Err(WorkspaceError::Busy);
        }
        let same_parent = source_parent == destination_parent;
        let mut parents = vec![source_parent];
        if !same_parent {
            parents.push(destination_parent);
        }
        let mut newly_dirty_parents = 0;
        for serial in &parents {
            newly_dirty_parents +=
                usize::from(active.get(&dirty_key(generation, *serial))?.is_none());
        }
        let source_dirty = moved.attr.kind != NodeKind::Directory
            && active
                .get(&dirty_key(generation, moved.attr.serial))?
                .is_none();
        let destination_record = if let Some(found) = &replaced {
            if found.attr.kind == NodeKind::File {
                active.get(&inode_key(found.attr.serial))?
            } else {
                None
            }
        } else {
            None
        };
        let replaced_nonfile = if let Some(found) = replaced
            .as_ref()
            .filter(|found| found.attr.kind != NodeKind::File)
        {
            active
                .get(&inode_key(found.attr.serial))?
                .map(|value| HotInode::parse(&value))
                .transpose()?
                .is_some_and(|inode| inode.fresh)
                && active
                    .get(&dirty_key(generation, found.attr.serial))?
                    .is_some()
        } else {
            false
        };
        let dropped_directory = replaced_nonfile
            && replaced
                .as_ref()
                .is_some_and(|found| found.attr.kind == NodeKind::Directory);
        let dropped_symlink = replaced_nonfile && !dropped_directory;
        let destination_unbound = replaced.as_ref().is_some_and(|found| {
            found.attr.kind == NodeKind::File
                && state
                    .fresh
                    .get(&found.attr.serial)
                    .is_some_and(|names| *names == 1)
        });
        let source_local = active
            .get(&namespace_key(source_parent, source)?)?
            .is_some();
        let destination_local = active
            .get(&namespace_key(destination_parent, destination)?)?
            .is_some();
        let destination_dirty = if let Some(found) = &replaced {
            found.attr.kind == NodeKind::File
                && !destination_unbound
                && active
                    .get(&dirty_key(generation, found.attr.serial))?
                    .is_none()
        } else {
            false
        };
        let new_dirty =
            newly_dirty_parents + usize::from(source_dirty) + usize::from(destination_dirty);
        let new_rows = usize::from(!source_local) + usize::from(!destination_local);
        let new_bytes = if source_local { 0 } else { 10 + source.len() }
            + if destination_local {
                0
            } else {
                10 + destination.len()
            };
        state.frontier_bytes(
            &self.host,
            state.dirty_inodes + new_dirty
                - usize::from(destination_unbound)
                - usize::from(replaced_nonfile),
            state.dirty_directories + newly_dirty_parents - usize::from(dropped_directory),
            state.fresh_files - usize::from(destination_unbound),
            state.fresh_symlinks - usize::from(dropped_symlink),
            state.directory_names + new_rows,
            state.directory_bytes + new_bytes,
        )?;
        // T0's complete resident ancestry is the live mutation authority.
        // Check every fallible graph condition BEFORE active publication.
        state.live_chain(source_parent, None, self.inner.root.serial)?;
        if state.live_chain(
            destination_parent,
            Some(moved.attr.serial),
            self.inner.root.serial,
        )? && moved.attr.kind == NodeKind::Directory
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let moved_index = state.node_index.get(&moved.attr.serial).copied();
        if let Some(index) = moved_index.filter(|_| moved.attr.kind == NodeKind::Directory) {
            let node = &state.nodes[index];
            if !node.attached || node.parent != source_parent || node.name.bytes.as_ref() != source
            {
                return Err(WorkspaceError::Busy);
            }
            state.live_chain(moved.attr.serial, None, self.inner.root.serial)?;
        }
        let next = revision.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let (seconds, nanos) = now()?;
        let mut updates = vec![
            (
                namespace_key(source_parent, source)?,
                Some(
                    NamespaceRecord {
                        serial: moved.attr.serial,
                        kind: moved.attr.kind,
                        tombstone: true,
                    }
                    .value()?
                    .to_vec(),
                ),
            ),
            (
                namespace_key(destination_parent, destination)?,
                Some(
                    NamespaceRecord {
                        serial: moved.attr.serial,
                        kind: moved.attr.kind,
                        tombstone: false,
                    }
                    .value()?
                    .to_vec(),
                ),
            ),
        ];
        for parent in &parents {
            let node = state.node(*parent)?;
            check_access(node.attr, self.inner.root.uid, 3)?;
            let mut inode = match active.get(&inode_key(*parent))? {
                Some(value) => HotInode::parse(&value)?,
                None => hot(
                    node.attr,
                    node.content,
                    node.metadata,
                    generation,
                    next,
                    node.baseline == 0,
                ),
            };
            inode.revision = next;
            inode.generation = generation;
            inode.seconds = seconds;
            inode.nanos = nanos;
            updates.push((inode_key(*parent).to_vec(), Some(inode.value()?.to_vec())));
            updates.push((dirty_key(generation, *parent).to_vec(), Some(vec![1])));
        }
        if symlink_payload.is_none() {
            let mut inode = match source_record {
                Some(ref value) => HotInode::parse(value)?,
                None => hot(
                    moved.attr,
                    moved.content,
                    moved.metadata,
                    generation,
                    next,
                    false,
                ),
            };
            if moved.attr.kind != NodeKind::Directory {
                inode.generation = generation;
                inode.revision = next;
            }
            updates.push((
                inode_key(moved.attr.serial).to_vec(),
                Some(inode.value()?.to_vec()),
            ));
        }
        if moved.attr.kind != NodeKind::Directory && symlink_payload.is_none() {
            updates.push((
                dirty_key(generation, moved.attr.serial).to_vec(),
                Some(vec![1]),
            ));
        }
        if let Some(found) = replaced
            .as_ref()
            .filter(|found| found.attr.kind == NodeKind::File)
        {
            let mut inode = match destination_record {
                Some(ref value) => HotInode::parse(value)?,
                None => hot(
                    found.attr,
                    found.content,
                    found.metadata,
                    generation,
                    next,
                    false,
                ),
            };
            inode.links = inode.links.checked_sub(1).ok_or(WorkspaceError::Io)?;
            inode.revision = next;
            inode.generation = generation;
            updates.push((
                inode_key(found.attr.serial).to_vec(),
                Some(inode.value()?.to_vec()),
            ));
            if !destination_unbound {
                updates.push((
                    dirty_key(generation, found.attr.serial).to_vec(),
                    Some(vec![1]),
                ));
            }
        }
        if destination_unbound || replaced_nonfile {
            let found = replaced.as_ref().ok_or(WorkspaceError::Io)?;
            updates.push((dirty_key(generation, found.attr.serial).to_vec(), None));
        }
        updates.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let (published, mut cleanup_error) = if let Some(payload) = &symlink_payload {
            let inode = hot(
                NodeAttributes {
                    size: 0,
                    ..moved.attr
                },
                [0; 32],
                [0; 32],
                generation,
                next,
                false,
            );
            let result = active.create_symlink(moved.attr.serial, inode, payload, &updates)?;
            (result.revision, result.cleanup_error)
        } else {
            let result = active.publish_records(&updates)?;
            (result.revision, result.cleanup_error)
        };
        if published != next {
            return Err(WorkspaceError::Io);
        }
        for parent in &parents {
            let node = state.node_mut(*parent)?;
            node.attr.mtime_seconds = seconds;
            node.attr.mtime_nanoseconds = nanos;
        }
        if let Some(index) = moved_index {
            let node = &mut state.nodes[index];
            if node.attr.kind == NodeKind::Directory
                || (node.parent == source_parent && node.name.bytes.as_ref() == source)
            {
                node.name = moved_name;
                node.parent = destination_parent;
            }
        }
        if let Some(found) = &replaced {
            if let Ok(node) = state.node_mut(found.attr.serial) {
                if found.attr.kind == NodeKind::Directory {
                    node.attached = false;
                }
                if found.attr.kind == NodeKind::File {
                    node.names = node.names.saturating_sub(1);
                    node.attr.references = node.names as u64;
                }
                let scope = if origin.projected() {
                    ReferenceScope::Projection
                } else {
                    ReferenceScope::Local
                };
                let references = node.references(scope);
                *references = references.saturating_sub(1);
            }
            if found.attr.kind == NodeKind::File {
                state.unlinked(found.attr.serial);
            }
        }
        state.revision = published;
        state.dirty_inodes = state.dirty_inodes + new_dirty - usize::from(replaced_nonfile);
        state.dirty_directories =
            state.dirty_directories + newly_dirty_parents - usize::from(dropped_directory);
        if let Some(found) = replaced.as_ref().filter(|_| replaced_nonfile) {
            if dropped_directory {
                state.declared.retain(|serial| *serial != found.attr.serial);
            } else {
                state.fresh_symlinks -= 1;
            }
        }
        state.directory_names += new_rows;
        state.directory_bytes += new_bytes;
        let receipt = MutationReceipt {
            incarnation: self.inner.incarnation,
            generation,
            inode: moved.attr.serial,
            revision: published,
            accepted_bytes: 0,
        };
        let delivery = if origin.projected() {
            None
        } else {
            state
                .projection
                .as_ref()
                .and_then(|projection| projection.delivery.clone())
        };
        if let (Some(projection), Some(_)) = (&mut state.projection, &delivery) {
            projection.status = CoherenceStatus::Pending {
                receipt,
                published_handle: None,
            };
        }
        drop(state);
        // This operation's old selection can own the final retirement pin.
        // Release it explicitly while the accepted receipt is still available.
        if let Err(error) = view.release_active() {
            cleanup_error.get_or_insert(error);
        }
        if let Some(delivery) = delivery {
            self.complete_projection_mutation(
                delivery.clone(),
                receipt,
                Some((source_parent, source)),
                None,
                deadline,
            )?;
            if !same_parent {
                self.complete_projection_mutation(
                    delivery,
                    receipt,
                    Some((destination_parent, destination)),
                    None,
                    deadline,
                )?;
            }
        }
        if let Some(cause) = cleanup_error {
            return Err(WorkspaceError::Published {
                receipt,
                published_handle: None,
                cause: Box::new(cause),
            });
        }
        Ok(())
    }
}
