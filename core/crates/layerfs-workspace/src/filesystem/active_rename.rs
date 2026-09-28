//! Active atomic rename publication.
use super::{
    active_names::{hot, now},
    namespace::{check_access, child_path_active},
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
        let (view, source_parent_path, destination_parent_path, baseline, revision, generation) = {
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
            child_path_active(a.path(), source)?;
            child_path_active(b.path(), destination)?;
            (
                self.selected_view(&state)?,
                a.path().to_vec(),
                b.path().to_vec(),
                state.baseline,
                state.revision,
                state.generation,
            )
        };
        let moved = self.resolve_child(
            &mut operation,
            &view,
            source_parent,
            &source_parent_path,
            source,
            deadline,
        )?;
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
            &destination_parent_path,
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
                let path = child_path_active(&destination_parent_path, destination)?;
                if !self
                    .list_view(
                        &mut operation,
                        &view,
                        (found.attr.serial, &path),
                        &[],
                        1,
                        deadline,
                    )?
                    .is_empty()
                {
                    return Err(WorkspaceError::NotEmpty);
                }
            } else if moved.attr.kind == NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
        }
        let old_path = child_path_active(&source_parent_path, source)?;
        let new_path = child_path_active(&destination_parent_path, destination)?;
        if moved.attr.kind == NodeKind::Directory
            && (destination_parent_path == old_path
                || destination_parent_path.starts_with(&[old_path.as_slice(), b"/"].concat()))
        {
            return Err(WorkspaceError::InvalidInput);
        }
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
            state.dirty_inodes + new_dirty - usize::from(destination_unbound),
            state.dirty_directories + newly_dirty_parents,
            state.fresh_files - usize::from(destination_unbound),
            state.fresh_symlinks,
            state.directory_names + new_rows,
            state.directory_bytes + new_bytes,
        )?;
        // A directory move's path bounds are proven before publication: the
        // moved directory itself, every resident descendant, and - for a
        // growing move - the inherited subtree's never-resident descendants,
        // which only a canonical walk can see. Refusal here changes nothing.
        if moved.attr.kind == NodeKind::Directory {
            drop(state);
            self.preflight_rename_paths(
                &mut operation,
                &view,
                moved.attr.serial,
                &old_path,
                &new_path,
                deadline,
            )?;
            state = self.state()?;
            if state.baseline != baseline
                || state.revision != revision
                || state.generation != generation
            {
                return Err(WorkspaceError::Busy);
            }
        }
        // Prepare every resident replacement path and its charge before the
        // single publication. A failed reservation cannot leave a half-moved
        // namespace or an uncharged extended locator.
        let _paths_charge = self.host.budget.reserve(
            state
                .nodes
                .len()
                .checked_mul(std::mem::size_of::<(usize, crate::runtime::state::NodePath)>())
                .ok_or(WorkspaceError::Capacity)?,
        )?;
        let mut moved_paths = crate::backing::metadata_index::vector(state.nodes.len())?;
        for (index, node) in state.nodes.iter().enumerate() {
            if node.path() == old_path.as_slice()
                || (moved.attr.kind == NodeKind::Directory
                    && node
                        .path()
                        .strip_prefix(old_path.as_slice())
                        .is_some_and(|suffix| suffix.first() == Some(&b'/')))
            {
                let path = crate::runtime::state::NodePath::from_parts(
                    &new_path,
                    &node.path()[old_path.len()..],
                    &self.host.budget,
                )?;
                moved_paths.push((index, path));
            }
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
        if source_record.is_none() && symlink_payload.is_none() {
            let inode = hot(
                moved.attr,
                moved.content,
                moved.metadata,
                generation,
                next,
                false,
            );
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
        if destination_unbound {
            let found = replaced.as_ref().ok_or(WorkspaceError::Io)?;
            updates.push((dirty_key(generation, found.attr.serial).to_vec(), None));
        }
        updates.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let (published, cleanup_error) = if let Some(payload) = &symlink_payload {
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
            (active.publish_records(&updates)?, None)
        };
        if published != next {
            return Err(WorkspaceError::Io);
        }
        for parent in &parents {
            let node = state.node_mut(*parent)?;
            node.attr.mtime_seconds = seconds;
            node.attr.mtime_nanoseconds = nanos;
        }
        for (index, path) in moved_paths {
            let node = &mut state.nodes[index];
            if node.path() == old_path.as_slice() {
                node.parent = destination_parent;
            }
            node.install_path(path);
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
        state.dirty_inodes += new_dirty;
        state.dirty_directories += newly_dirty_parents;
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
