//! Active creation and binding publication.
use super::{
    active_names::{hot, now},
    create::Creation,
    namespace::{check_access, check_name},
};
use crate::{
    backing::active::{dirty_key, inode_key, namespace_key, HotInode, NamespaceRecord},
    runtime::state::{Handle, Node},
    *,
};
use layerfs_bridge::contract::{
    Code, HistoryCommand, HistoryResult, Operation, Response, HISTORY_RESULT_BYTES,
};
use std::time::Instant;

impl Workspace {
    pub(super) fn create_child_active(
        &self,
        parent: u64,
        name: &[u8],
        creation: Creation<'_>,
        deadline: Instant,
    ) -> Result<(NodeAttributes, Option<HandleId>), WorkspaceError> {
        let (kind, mode, umask, origin, open) = match creation {
            Creation::Directory {
                mode,
                umask,
                origin,
            } => (NodeKind::Directory, mode, umask, origin, None),
            Creation::File {
                options,
                open,
                origin,
            } => (NodeKind::File, options.mode, options.umask, origin, open),
            Creation::Symlink { origin, .. } => (NodeKind::Symlink, 0o777, 0, origin, None),
            Creation::Link { origin, .. } => (NodeKind::File, 0, 0, origin, None),
        };
        let reference = if origin.projected() {
            ReferenceScope::Projection
        } else {
            ReferenceScope::Local
        };
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let (mut view, baseline, revision, generation, scope) = {
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
            check_name(name)?;
            state.live_chain(parent, None, self.inner.root.serial)?;
            (
                self.selected_view(&state)?,
                state.baseline,
                state.revision,
                state.generation,
                state
                    .branch
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?
                    .scope,
            )
        };
        match self.resolve_child(&mut operation, &view, parent, name, deadline) {
            Ok(resolved) => {
                if let Creation::File { options, .. } = creation {
                    if !options.exclusive {
                        let serial = resolved.attr.serial;
                        {
                            let mut state = self.state()?;
                            if state.revision != revision || state.baseline != baseline {
                                return Err(WorkspaceError::Busy);
                            }
                            self.cache_lookup(
                                &mut state, resolved, name, parent, reference, baseline,
                            )?;
                        }
                        return match self.open_file_admitted(
                            serial,
                            options.open,
                            reference,
                            deadline,
                            &mut operation,
                            origin,
                        ) {
                            Ok((attr, handle)) => Ok((attr, Some(handle))),
                            Err(error) => {
                                self.forget(serial, 1, reference);
                                Err(error)
                            }
                        };
                    }
                }
                return Err(WorkspaceError::Exists);
            }
            Err(WorkspaceError::NotFound) => {}
            Err(WorkspaceError::Service(failure))
                if !failure.unknown
                    && matches!(failure.code, Code::PathNotFound | Code::NotFound) => {}
            Err(error) => return Err(error),
        }
        let link_target = if let Creation::Link { serial, .. } = creation {
            Some(self.link_target(serial, &mut operation, deadline)?)
        } else {
            None
        };
        let serial = if let Creation::Link { serial, .. } = creation {
            serial
        } else {
            operation.metadata_call()?;
            let result = self.host.call_input(
                (self.inner.store, generation),
                Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 1 }),
                &mut &[][..],
                HISTORY_RESULT_BYTES as u64,
                &mut std::io::sink(),
                deadline,
            );
            operation.release_metadata_call();
            match result? {
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
        let payload = if let Creation::Symlink { target, .. } = creation {
            if target.is_empty() {
                None
            } else {
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
                let mut source = target;
                Some(host.acquire(
                    directory,
                    target.len() as u64,
                    &mut source,
                    deadline,
                    &self.inner.stopping,
                )?)
            }
        } else {
            None
        };
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
        state.live_chain(parent, None, self.inner.root.serial)?;
        let parent_node = *state.node_index.get(&parent).ok_or(WorkspaceError::Busy)?;
        let parent_attr = state.nodes[parent_node].attr;
        check_access(parent_attr, self.inner.root.uid, 3)?;
        if !matches!(creation, Creation::Link { .. }) && state.node_index.contains_key(&serial) {
            return Err(WorkspaceError::Service(Code::Unknown.into()));
        }
        let parent_dirty = active.get(&dirty_key(generation, parent))?.is_some();
        let child_dirty = active.get(&dirty_key(generation, serial))?.is_some();
        let new_dirty = usize::from(!parent_dirty) + usize::from(!child_dirty);
        let new_directories =
            usize::from(!parent_dirty) + usize::from(kind == NodeKind::Directory && !child_dirty);
        let file = kind == NodeKind::File && link_target.is_none();
        let symlink = kind == NodeKind::Symlink;
        let name_bytes = 10 + name.len();
        let prior_binding = active
            .get(&namespace_key(parent, name)?)?
            .map(|bytes| NamespaceRecord::parse(&bytes))
            .transpose()?;
        let replacing_tombstone = prior_binding.is_some_and(|record| record.tombstone);
        state.frontier_bytes(
            &self.host,
            state.dirty_inodes + new_dirty,
            state.dirty_directories + new_directories,
            state.fresh_files + usize::from(file),
            state.fresh_symlinks + usize::from(symlink),
            state.directory_names + usize::from(!replacing_tombstone),
            state.directory_bytes + if replacing_tombstone { 0 } else { name_bytes },
        )?;
        if link_target.is_none() {
            state.reserve_nodes()?;
        }
        if file {
            state.reserve_fresh()?;
        }
        let handle = if let Some(options) = open {
            let (id, next) = super::open::handle_slot(&state)?;
            Some((
                Handle {
                    id,
                    serial,
                    directory: false,
                    scope: reference,
                    options,
                    ready: true,
                    view: None,
                },
                next,
            ))
        } else {
            None
        };
        let prepared_name = if link_target.is_none() {
            Some(crate::runtime::state::NodeName::new(
                name,
                &self.host.budget,
            )?)
        } else {
            None
        };
        let next = revision.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let (seconds, nanos) = now()?;
        let target_size = match creation {
            Creation::Symlink { target, .. } => target.len() as u64,
            _ => 0,
        };
        let mut child_attr =
            link_target
                .as_ref()
                .map(|(attr, _)| *attr)
                .unwrap_or(NodeAttributes {
                    serial,
                    kind,
                    size: target_size,
                    references: 1,
                    mode: mode & !umask,
                    mtime_seconds: seconds,
                    mtime_nanoseconds: nanos,
                    uid: self.inner.root.uid,
                    gid: self.inner.root.gid,
                });
        let mut parent_hot = match active.get(&inode_key(parent))? {
            Some(value) => HotInode::parse(&value)?,
            None => hot(
                parent_attr,
                state.nodes[parent_node].content,
                state.nodes[parent_node].metadata,
                generation,
                next,
                state.nodes[parent_node].baseline == 0,
            ),
        };
        parent_hot.generation = generation;
        parent_hot.revision = next;
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
                        serial,
                        kind,
                        tombstone: false,
                    }
                    .value()?
                    .to_vec(),
                ),
            ),
        ];
        if let Some((target, (_, content, metadata, _))) = link_target {
            let mut inode = match active.get(&inode_key(serial))? {
                Some(value) => HotInode::parse(&value)?,
                None => hot(target, content, metadata, generation, next, false),
            };
            inode.links = inode.links.checked_add(1).ok_or(WorkspaceError::Capacity)?;
            inode.revision = next;
            inode.generation = generation;
            updates.push((inode_key(serial).to_vec(), Some(inode.value()?.to_vec())));
            updates.push((dirty_key(generation, serial).to_vec(), Some(vec![1])));
        } else if payload.is_none() {
            let inode = hot(
                NodeAttributes {
                    size: 0,
                    ..child_attr
                },
                [0; 32],
                [0; 32],
                generation,
                next,
                true,
            );
            updates.push((inode_key(serial).to_vec(), Some(inode.value()?.to_vec())));
            updates.push((dirty_key(generation, serial).to_vec(), Some(vec![1])));
        }
        updates.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let (published, mut cleanup_error) = if let Some(payload) = &payload {
            let inode = hot(
                NodeAttributes {
                    size: 0,
                    ..child_attr
                },
                [0; 32],
                [0; 32],
                generation,
                next,
                true,
            );
            let result = active.create_symlink(serial, inode, payload, &updates)?;
            (result.revision, result.cleanup_error)
        } else {
            let result = active.publish_records(&updates)?;
            (result.revision, result.cleanup_error)
        };
        if published != next {
            return Err(WorkspaceError::Io);
        }
        let receipt = MutationReceipt {
            incarnation: self.inner.incarnation,
            generation,
            inode: serial,
            revision: next,
            accepted_bytes: 0,
        };
        let returned_handle = handle.as_ref().map(|(handle, _)| handle.id);
        let delivery = if origin.projected() {
            None
        } else {
            state
                .projection
                .as_ref()
                .and_then(|projection| projection.delivery.clone())
        };
        state.nodes[parent_node].attr.mtime_seconds = seconds;
        state.nodes[parent_node].attr.mtime_nanoseconds = nanos;
        if link_target.is_some() {
            let node = state.node_mut(serial)?;
            node.names = node.names.checked_add(1).ok_or(WorkspaceError::Capacity)?;
            node.attr.references = node.names as u64;
            let references = node.references(reference);
            *references = references.checked_add(1).ok_or(WorkspaceError::Capacity)?;
            child_attr.references = node.names as u64;
            state.linked(serial);
        } else {
            let mut node = Node::new(
                child_attr,
                [0; 32],
                [0; 32],
                prepared_name.ok_or(WorkspaceError::Io)?,
                parent,
            );
            node.baseline = 0;
            *node.references(reference) = 1;
            node.handles = usize::from(open.is_some());
            state.push_node(node);
            if file {
                state.created(serial);
            }
            if kind == NodeKind::Directory {
                state.declaring(serial);
            }
        }
        if let Some((handle, next)) = handle {
            state.next_handle = next;
            state.handles.push(handle);
        }
        state.revision = published;
        state.dirty_inodes += new_dirty;
        state.dirty_directories += new_directories;
        state.fresh_files += usize::from(file);
        state.fresh_symlinks += usize::from(symlink);
        if !replacing_tombstone {
            state.directory_names += 1;
            state.directory_bytes += name_bytes;
        }
        if let (Some(projection), Some(_)) = (&mut state.projection, &delivery) {
            projection.status = CoherenceStatus::Pending {
                receipt,
                published_handle: returned_handle,
            };
        }
        drop(state);
        // This operation's old selection can own the final retirement pin.
        // Release it explicitly while the accepted receipt is still available.
        if let Err(error) = view.release_active() {
            cleanup_error.get_or_insert(error);
        }
        if let Some(delivery) = delivery {
            if let Err(error) = self.complete_projection_mutation(
                delivery,
                receipt,
                Some((parent, name)),
                returned_handle,
                deadline,
            ) {
                self.forget(serial, 1, ReferenceScope::Local);
                return Err(error);
            }
        }
        if let Some(cause) = cleanup_error {
            return Err(WorkspaceError::Published {
                receipt,
                published_handle: returned_handle,
                cause: Box::new(cause),
            });
        }
        Ok((child_attr, returned_handle))
    }
}
