//! One atomic name removal: tombstone the exact binding, keep other owners.
//!
//! An unlink removes exactly one name. Every other name, open handle, pinned
//! read and frozen generation keeps its own reference to the same inode, and a
//! last-name removal frees no bytes while one of them is still live.
use super::{
    namespace::{check_access, child_path},
    namespace_view::View,
};
use crate::{
    backing::{
        metadata_index::vector,
        metadata_pages::{self, Cell, PageRef},
    },
    overlay::{
        directories::{self, Directory, Origin},
        pieces::CapturedBase,
    },
    runtime::coherence::MutationOrigin,
    *,
};
use layerfs_bridge::contract::Code;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
impl Workspace {
    /// Removes one regular-file or symlink name. The inode, its content and
    /// every other owner survive; a still-open handle keeps reading and writing
    /// the same version after its last name is gone.
    pub fn unlink(
        &self,
        parent: u64,
        name: &[u8],
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        self.remove_name_from(parent, name, false, deadline, MutationOrigin::Local)
    }
    /// Removes one empty directory name. A nonempty directory, the root and a
    /// non-directory name are refused before any publication.
    pub fn rmdir(&self, parent: u64, name: &[u8], deadline: Instant) -> Result<(), WorkspaceError> {
        self.remove_name_from(parent, name, true, deadline, MutationOrigin::Local)
    }
    pub(crate) fn unlink_from(
        &self,
        parent: u64,
        name: &[u8],
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<(), WorkspaceError> {
        self.remove_name_from(parent, name, false, deadline, origin)
    }
    pub(crate) fn rmdir_from(
        &self,
        parent: u64,
        name: &[u8],
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<(), WorkspaceError> {
        self.remove_name_from(parent, name, true, deadline, origin)
    }
    fn remove_name_from(
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
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let (view, path, parent_attr, baseline, revision, generation, frozen) = {
            let state = self.state()?;
            self.available(&state)?;
            self.check_mutation_coherence(&state, origin, false)?;
            let node = state.node(parent)?;
            if node.attr.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            check_access(node.attr, self.inner.root.uid, 3)?;
            if parent == self.inner.root.serial {
                // The root's own name is not removable, but its children are.
            }
            (
                View {
                    base: state.base,
                    root: state.overlay.clone(),
                },
                node.path().to_vec(),
                node.attr,
                state.baseline,
                state.revision,
                state.generation,
                state.submission.clone(),
            )
        };
        child_path(&path, name)?;
        if directory && name.is_empty() {
            return Err(WorkspaceError::InvalidInput);
        }
        let resolved =
            match self.resolve_child(&mut operation, &view, parent, &path, name, deadline) {
                Ok(resolved) => resolved,
                Err(WorkspaceError::NotFound) => return Err(WorkspaceError::NotFound),
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
            if !self.directory_empty(&mut operation, &view, child, &path, name, deadline)? {
                return Err(WorkspaceError::NotEmpty);
            }
        } else if child.kind == NodeKind::Directory {
            return Err(WorkspaceError::IsDirectory);
        }
        let old = self.directory_record(&view, parent, deadline)?;
        let (reanchor, already_dirty) = {
            let host = self
                .host
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let _view = host.writer()?;
            let mut lease = host.payloads.window(1, 3)?;
            self.directory_delta(
                view.root.as_ref(),
                generation,
                parent,
                old.as_ref(),
                lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                deadline,
            )?
        };
        // This publication marks the parent dirty and nothing else: a fresh child
        // already owns its own dirty key, and a canonical child needs none because
        // the tree is derived from names.
        let new_dirty = usize::from(!already_dirty);
        let new_directories = usize::from(!already_dirty);
        let name_bytes = 10 + name.len();
        {
            let state = self.state()?;
            self.check_child_stamp(&state, baseline, revision, generation, &view, child.kind)?;
            self.check_mutation_coherence(&state, origin, false)?;
            state.frontier_bytes(
                state.dirty_inodes + new_dirty,
                state.dirty_directories + new_directories,
                state.fresh_files,
                state.fresh_symlinks,
                state.directory_names + 1,
                state.directory_bytes + name_bytes,
            )?;
        }
        self.maintain_backing(deadline)?;
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let _writer = host.writer()?;
        let needs_completion = {
            let state = self.state()?;
            self.check_child_stamp(&state, baseline, revision, generation, &view, child.kind)?;
            self.check_mutation_coherence(&state, origin, false)?;
            state.completion.is_none()
        };
        let arena = self
            .inner
            .arena
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut lease = host.payloads.window(0, 1)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let capture = frozen
            .as_ref()
            .map(|submission| submission.capture())
            .transpose()?;
        let mut parent_directory =
            old.unwrap_or_else(|| Directory::initial(parent_attr, view.base));
        if reanchor {
            if let (Some(prior), Some(previous)) = (
                old,
                crate::backing::metadata::MetadataHost::anchor(view.root.as_ref()).as_ref(),
            ) {
                // The delta anchors on the exact earlier root this operation's
                // candidate is built on. A frozen submission's captured root is
                // the same root only while no later operation replaced it.
                if let Some(capture) = capture {
                    if prior.generation != capture.generation {
                        return Err(WorkspaceError::Io);
                    }
                }
                parent_directory.origin = Origin::Captured(CapturedBase {
                    root: previous.root()?,
                    inode: parent,
                    generation: prior.generation,
                    revision: prior.revision,
                });
                // The inherited pages now live in the referenced version, so this
                // record keeps exactly what the operation adds. Without an anchor
                // nothing else would hold those names, so the record keeps them.
                parent_directory.entries = PageRef::NULL;
                parent_directory.tombstones = PageRef::NULL;
                parent_directory.count = 0;
                parent_directory.bytes = 0;
            }
        }
        let next = revision.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| WorkspaceError::Io)?;
        let seconds = i64::try_from(time.as_secs()).map_err(|_| WorkspaceError::Capacity)?;
        parent_directory.generation = generation;
        parent_directory.revision = next;
        parent_directory.seconds = seconds;
        parent_directory.nanos = time.subsec_nanos();
        let candidate = host.candidate(
            arena,
            generation,
            needs_completion,
            capture.map(|capture| capture.root.clone()),
        )?;
        let (entries, was_local) =
            directories::drop_entry(&candidate, parent_directory, name, window, deadline)?;
        parent_directory.entries = entries;
        // A directory with no origin to inherit from needs no removal record: an
        // absent binding is already absent. Any other delta must shadow the name
        // its origin still binds, so the local binding becomes a tombstone.
        let shadowed = !matches!(parent_directory.origin, Origin::Empty);
        if was_local {
            // The record's own byte field tracks its entry page only, so dropping
            // the binding always shrinks it, whether or not a removal record takes
            // the name's place in the delta.
            parent_directory.count = parent_directory.count.saturating_sub(1);
            parent_directory.bytes = parent_directory.bytes.saturating_sub(name_bytes as u32);
            if shadowed {
                parent_directory.tombstones =
                    directories::remove_name(&candidate, parent_directory, name, window, deadline)?;
            }
        } else {
            parent_directory.tombstones =
                directories::remove_name(&candidate, parent_directory, name, window, deadline)?;
        }
        let mut updates = vector(2)?;
        updates.push(Cell::new(
            &metadata_pages::dirty_key(generation, parent),
            &[1],
        )?);
        updates.push(Cell::new(
            &metadata_pages::namespace_key(parent),
            &parent_directory.value(),
        )?);
        updates.sort_unstable_by(|a, b| a.key().cmp(b.key()));
        let root = candidate.update(
            view.root
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
        self.check_child_stamp(&state, baseline, revision, generation, &view, child.kind)?;
        self.check_mutation_coherence(&state, origin, true)?;
        if state.completion.is_none() != needs_completion {
            return Err(WorkspaceError::Busy);
        }
        let parent_node = state
            .nodes
            .iter()
            .position(|node| node.attr.serial == parent)
            .ok_or(WorkspaceError::Busy)?;
        state.nodes[parent_node].attr = parent_directory.attributes(state.nodes[parent_node].attr);
        // One name of the child is gone, and the local lookup reference that name
        // owned is gone with it. A regular inode this generation created that no
        // name binds any more has no canonical identity to save.
        if let Some(index) = state
            .nodes
            .iter()
            .position(|node| node.attr.serial == child.serial)
        {
            if child.kind == NodeKind::File {
                state.nodes[index].names = state.nodes[index].names.saturating_sub(1);
            }
            let references = state.nodes[index].references(reference);
            *references = references.saturating_sub(1);
        }
        if child.kind == NodeKind::File {
            state.unlinked(child.serial);
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
        if needs_completion {
            state.completion = candidate.take_completion(generation)?;
            if state.completion.is_none() {
                return Err(WorkspaceError::Io);
            }
        }
        let retired = state.overlay.replace(candidate);
        state.revision = next;
        state.dirty_inodes += new_dirty;
        state.dirty_directories += new_directories;
        // One row per name row this delta still carries. A name with no origin to
        // shadow simply leaves the entry page; any other name the origin binds
        // reappears as its own removal record.
        if was_local {
            if !shadowed {
                state.directory_names = state.directory_names.saturating_sub(1);
                state.directory_bytes = state.directory_bytes.saturating_sub(name_bytes);
            }
        } else {
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
        drop(retired);
        drop(lease);
        drop(_writer);
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
    /// True when the effective namespace of one directory selects no child.
    /// Pending additions and removals are both part of that one selected view.
    fn directory_empty(
        &self,
        operation: &mut crate::runtime::state::OperationGuard,
        view: &View,
        directory: NodeAttributes,
        parent_path: &[u8],
        name: &[u8],
        deadline: Instant,
    ) -> Result<bool, WorkspaceError> {
        let path = child_path(parent_path, name)?;
        let names = self.list_view(operation, view, (directory.serial, &path), &[], 1, deadline)?;
        Ok(names.is_empty())
    }
}
