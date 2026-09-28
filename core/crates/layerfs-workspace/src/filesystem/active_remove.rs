//! Active unlink and directory removal publication.
use super::{
    active_names::{hot, now},
    namespace::{check_access, child_path_active},
};
use crate::{
    backing::active::{dirty_key, inode_key, namespace_key, HotInode, NamespaceRecord},
    runtime::coherence::MutationOrigin,
    *,
};
use layerfs_bridge::contract::Code;
use std::time::Instant;

impl Workspace {
    pub(super) fn remove_name_active(
        &self,
        parent: u64,
        name: &[u8],
        directory: bool,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<(), WorkspaceError> {
        let reference = if origin.projected() {
            ReferenceScope::Projection
        } else {
            ReferenceScope::Local
        };
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let (view, path, baseline, revision, generation) = {
            let state = self.state()?;
            self.available(&state)?;
            self.check_mutation_coherence(&state, origin, false)?;
            let node = state.node(parent)?;
            if node.attr.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            if !node.attached {
                return Err(WorkspaceError::NotFound);
            }
            check_access(node.attr, self.inner.root.uid, 3)?;
            child_path_active(node.path(), name)?;
            (
                self.selected_view(&state)?,
                node.path().to_vec(),
                state.baseline,
                state.revision,
                state.generation,
            )
        };
        let resolved =
            match self.resolve_child(&mut operation, &view, parent, &path, name, deadline) {
                Ok(resolved) => resolved,
                Err(WorkspaceError::Service(failure))
                    if !failure.unknown
                        && matches!(failure.code, Code::PathNotFound | Code::NotFound) =>
                {
                    return Err(WorkspaceError::NotFound)
                }
                Err(error) => return Err(error),
            };
        let child = resolved.attr;
        if directory {
            if child.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            if child.serial == self.inner.root.serial {
                return Err(WorkspaceError::Unsupported);
            }
            let path = child_path_active(&path, name)?;
            if !self
                .list_view(
                    &mut operation,
                    &view,
                    (child.serial, &path),
                    &[],
                    1,
                    deadline,
                )?
                .is_empty()
            {
                return Err(WorkspaceError::NotEmpty);
            }
        } else if child.kind == NodeKind::Directory {
            return Err(WorkspaceError::IsDirectory);
        }
        let active = self
            .inner
            .active
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut state = self.state()?;
        self.available(&state)?;
        self.check_mutation_coherence(&state, origin, true)?;
        if state.revision != revision
            || state.baseline != baseline
            || state.generation != generation
        {
            return Err(WorkspaceError::Busy);
        }
        let parent_index = *state.node_index.get(&parent).ok_or(WorkspaceError::Busy)?;
        let parent_attr = state.nodes[parent_index].attr;
        check_access(parent_attr, self.inner.root.uid, 3)?;
        let parent_dirty = active.get(&dirty_key(generation, parent))?.is_some();
        let child_record = active.get(&inode_key(child.serial))?;
        let child_unbound = child.kind == NodeKind::File
            && state
                .fresh
                .get(&child.serial)
                .is_some_and(|names| *names == 1);
        let local_name = active.get(&namespace_key(parent, name)?)?.is_some();
        let child_dirty = child.kind == NodeKind::File
            && active.get(&dirty_key(generation, child.serial))?.is_some();
        let new_dirty = usize::from(!parent_dirty)
            + usize::from(child.kind == NodeKind::File && !child_dirty && !child_unbound);
        let new_directories = usize::from(!parent_dirty);
        let name_bytes = 10 + name.len();
        state.frontier_bytes(
            &self.host,
            state.dirty_inodes + new_dirty - usize::from(child_unbound),
            state.dirty_directories + new_directories,
            state.fresh_files - usize::from(child_unbound),
            state.fresh_symlinks,
            state.directory_names + usize::from(!local_name),
            state.directory_bytes + if local_name { 0 } else { name_bytes },
        )?;
        let next = revision.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let (seconds, nanos) = now()?;
        let mut parent_hot = match active.get(&inode_key(parent))? {
            Some(value) => HotInode::parse(&value)?,
            None => hot(
                parent_attr,
                state.nodes[parent_index].content,
                state.nodes[parent_index].metadata,
                generation,
                next,
                state.nodes[parent_index].baseline == 0,
            ),
        };
        parent_hot.revision = next;
        parent_hot.generation = generation;
        parent_hot.seconds = seconds;
        parent_hot.nanos = nanos;
        let mut updates = vec![
            (dirty_key(generation, parent).to_vec(), Some(vec![1])),
            (
                inode_key(parent).to_vec(),
                Some(parent_hot.value()?.to_vec()),
            ),
            (
                namespace_key(parent, name)?,
                Some(
                    NamespaceRecord {
                        serial: child.serial,
                        kind: child.kind,
                        tombstone: true,
                    }
                    .value()?
                    .to_vec(),
                ),
            ),
        ];
        if child.kind == NodeKind::File {
            let mut inode = match child_record {
                Some(value) => HotInode::parse(&value)?,
                None => hot(
                    child,
                    resolved.content,
                    resolved.metadata,
                    generation,
                    next,
                    false,
                ),
            };
            inode.links = inode.links.checked_sub(1).ok_or(WorkspaceError::Io)?;
            inode.revision = next;
            inode.generation = generation;
            updates.push((
                inode_key(child.serial).to_vec(),
                Some(inode.value()?.to_vec()),
            ));
            if !child_unbound {
                updates.push((dirty_key(generation, child.serial).to_vec(), Some(vec![1])));
            }
        }
        if child_unbound {
            updates.push((dirty_key(generation, child.serial).to_vec(), None));
        }
        updates.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let published = active.publish_records(&updates)?;
        if published != next {
            return Err(WorkspaceError::Io);
        }
        let receipt = MutationReceipt {
            incarnation: self.inner.incarnation,
            generation,
            inode: child.serial,
            revision: next,
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
        state.nodes[parent_index].attr.mtime_seconds = seconds;
        state.nodes[parent_index].attr.mtime_nanoseconds = nanos;
        if let Ok(node) = state.node_mut(child.serial) {
            if directory {
                node.attached = false;
            }
            if child.kind == NodeKind::File {
                node.names = node.names.saturating_sub(1);
                node.attr.references = node.names as u64;
            }
            let references = node.references(reference);
            *references = references.saturating_sub(1);
        }
        if child.kind == NodeKind::File {
            state.unlinked(child.serial);
        }
        state.revision = published;
        state.dirty_inodes += new_dirty;
        state.dirty_directories += new_directories;
        if !local_name {
            state.directory_names += 1;
            state.directory_bytes += name_bytes;
        }
        if let (Some(projection), Some(_)) = (&mut state.projection, &delivery) {
            projection.status = CoherenceStatus::Pending {
                receipt,
                published_handle: None,
            };
        }
        drop(state);
        if let Some(delivery) = delivery {
            self.complete_projection_mutation(
                delivery,
                receipt,
                Some((parent, name)),
                None,
                deadline,
            )?;
        }
        Ok(())
    }
}
