//! One atomic rename: both parent deltas publish through a single candidate root.
//!
//! The source name becomes a tombstone and the destination name becomes the
//! binding of the same inode in the same publication, so no observer sees an
//! intermediate unlink or link. A replaced destination keeps its inode for its
//! existing handles and readers; only its name is gone.
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
/// One rename request: both parents, both names and the selected flags.
#[derive(Clone, Copy)]
pub(crate) struct RenameRequest<'a> {
    pub source_parent: u64,
    pub source: &'a [u8],
    pub destination_parent: u64,
    pub destination: &'a [u8],
    pub flags: RenameFlags,
}
/// One parent whose entry set this operation edits.
struct Parent {
    serial: u64,
    path: Vec<u8>,
    directory: Directory,
    /// This generation's ledger already carries the parent's dirty key.
    dirty: bool,
    /// The maintained delta must be re-anchored on the current root.
    reanchor: bool,
    remove: Option<Vec<u8>>,
    bind: Option<(Vec<u8>, u64, NodeKind)>,
    mtime: (i64, u32),
}
impl Workspace {
    /// Moves or renames one name. Ordinary rename replaces an existing
    /// destination; `RenameFlags::noreplace` refuses one instead.
    pub fn rename(
        &self,
        source_parent: u64,
        source: &[u8],
        destination_parent: u64,
        destination: &[u8],
        flags: RenameFlags,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        self.rename_from(
            RenameRequest {
                source_parent,
                source,
                destination_parent,
                destination,
                flags,
            },
            deadline,
            MutationOrigin::Local,
        )
    }
    pub(crate) fn rename_from(
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
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let (view, baseline, revision, generation, frozen) = {
            let state = self.state()?;
            self.available(&state)?;
            self.check_mutation_coherence(&state, origin, false)?;
            (
                View {
                    base: state.base,
                    root: state.overlay.clone(),
                },
                state.baseline,
                state.revision,
                state.generation,
                state.submission.clone(),
            )
        };
        let mut parents = self.rename_parents(
            &mut operation,
            &view,
            (source_parent, source),
            (destination_parent, destination),
            deadline,
        )?;
        let source_attr = self.resolve_child(
            &mut operation,
            &view,
            source_parent,
            &parents[0].path.clone(),
            source,
            deadline,
        )?;
        if source_attr.attr.serial == self.inner.root.serial {
            return Err(WorkspaceError::Unsupported);
        }
        let same_parent = source_parent == destination_parent;
        let destination_attr = self.resolve_child(
            &mut operation,
            &view,
            destination_parent,
            &parents[if same_parent { 0 } else { 1 }].path.clone(),
            destination,
            deadline,
        );
        let destination_attr = match destination_attr {
            Ok(resolved) => Some(resolved),
            Err(WorkspaceError::NotFound) => None,
            Err(WorkspaceError::Service(failure))
                if !failure.unknown
                    && matches!(failure.code, Code::PathNotFound | Code::NotFound) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        if let Some(replaced) = &destination_attr {
            if replaced.attr.serial == source_attr.attr.serial {
                // Ordinary same-inode aliases are already the requested state.
                return Ok(());
            }
            if flags.noreplace {
                return Err(WorkspaceError::Exists);
            }
            if replaced.attr.kind == NodeKind::Directory {
                if source_attr.attr.kind != NodeKind::Directory {
                    return Err(WorkspaceError::IsDirectory);
                }
                let path = child_path(&parents[1].path, destination)?;
                let entries = self.list_view(
                    &mut operation,
                    &view,
                    (replaced.attr.serial, &path),
                    &[],
                    1,
                    deadline,
                )?;
                if !entries.is_empty() {
                    return Err(WorkspaceError::NotEmpty);
                }
            } else if source_attr.attr.kind == NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
        }
        if source_attr.attr.kind == NodeKind::Directory
            && self.descends_from(destination_parent, source_attr.attr.serial)?
        {
            // Moving a directory beneath its own descendant would create a cycle.
            return Err(WorkspaceError::InvalidInput);
        }
        let (seconds, nanos) = now()?;
        parents[0].mtime = (seconds, nanos);
        parents[0].remove = Some(source.to_vec());
        if same_parent {
            parents[0].bind = Some((
                destination.to_vec(),
                source_attr.attr.serial,
                source_attr.attr.kind,
            ));
        } else {
            parents[1].mtime = (seconds, nanos);
            parents[1].bind = Some((
                destination.to_vec(),
                source_attr.attr.serial,
                source_attr.attr.kind,
            ));
        }
        // Exactly one dirty key per edited parent this generation has not marked
        // yet; the publication below writes no other dirty record.
        let new_dirty = parents.iter().filter(|parent| !parent.dirty).count();
        let name_bytes = 10 + source.len() + 10 + destination.len();
        {
            let state = self.state()?;
            self.check_child_stamp(
                &state,
                baseline,
                revision,
                generation,
                &view,
                source_attr.attr.kind,
            )?;
            self.check_mutation_coherence(&state, origin, false)?;
            state.frontier_bytes(
                state.dirty_inodes + new_dirty,
                state.dirty_directories + new_dirty,
                state.fresh_files,
                state.fresh_symlinks,
                state.directory_names + 2,
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
            self.check_child_stamp(
                &state,
                baseline,
                revision,
                generation,
                &view,
                source_attr.attr.kind,
            )?;
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
        let candidate = host.candidate(
            arena,
            generation,
            needs_completion,
            capture.map(|capture| capture.root.clone()),
        )?;
        let mut updates = vector(6)?;
        let mut changed = 0usize;
        // Rows this publication adds to the two parents it edits. A locally bound
        // source trades its entry row for its removal record, and a replaced
        // destination rewrites a row that already existed, so neither adds one.
        // Signed: dropping a locally bound name without a removal record takes a
        // row away, and the generation's counters must follow it down. A
        // saturating unsigned count would keep the row and refuse the Commit.
        let mut rows = 0isize;
        let mut row_bytes = 0isize;
        for parent in &mut parents {
            if parent.remove.is_none() && parent.bind.is_none() {
                continue;
            }
            changed += 1;
            if parent.reanchor {
                // The delta anchors on the root this generation started from: the
                // frozen capture when one exists, otherwise the exact previous root
                // the record's own revision still names. It is never the root about
                // to be published.
                let previous = capture
                    .as_ref()
                    .map(|capture| capture.root.clone())
                    .or_else(|| crate::backing::metadata::MetadataHost::anchor(view.root.as_ref()));
                if let (Some(prior), Some(previous)) = (loaded(&parent.directory), previous) {
                    // Only a version the frozen root itself holds becomes a delta
                    // against it. A record the successor generation created keeps its
                    // own rows, which are exactly what this generation counted.
                    if capture.is_none_or(|capture| prior.generation == capture.generation) {
                        parent.directory.origin = Origin::Captured(CapturedBase {
                            root: previous.root()?,
                            inode: parent.serial,
                            generation: prior.generation,
                            revision: prior.revision,
                        });
                        // The inherited pages now live in the referenced version, so
                        // this record keeps exactly what the operation adds. Without an
                        // anchor nothing else would hold those names, so it keeps them.
                        parent.directory.entries = PageRef::NULL;
                        parent.directory.tombstones = PageRef::NULL;
                        parent.directory.count = 0;
                        parent.directory.bytes = 0;
                    }
                }
            }
            parent.directory.generation = generation;
            parent.directory.revision = revision.checked_add(1).ok_or(WorkspaceError::Capacity)?;
            parent.directory.seconds = parent.mtime.0;
            parent.directory.nanos = parent.mtime.1;
            if let Some(name) = &parent.remove {
                let removed = (10 + name.len()) as u32;
                let (entries, was_local) =
                    directories::drop_entry(&candidate, parent.directory, name, window, deadline)?;
                parent.directory.entries = entries;
                // Only a delta with an origin to shadow keeps a removal record.
                let shadowed = !matches!(parent.directory.origin, Origin::Empty);
                if was_local {
                    parent.directory.count = parent.directory.count.saturating_sub(1);
                    parent.directory.bytes = parent.directory.bytes.saturating_sub(removed);
                    if !shadowed {
                        rows -= 1;
                        row_bytes -= removed as isize;
                    }
                } else {
                    rows += 1;
                    row_bytes += removed as isize;
                }
                if shadowed {
                    parent.directory.tombstones = directories::remove_name(
                        &candidate,
                        parent.directory,
                        name,
                        window,
                        deadline,
                    )?;
                }
            }
            if let Some((name, serial, kind)) = &parent.bind {
                let added = (10 + name.len()) as u32;
                // The destination name is bound by this publication whatever the
                // origin held: an inherited binding for it is shadowed by the new
                // entry, and a removal record this operation wrote for the same
                // name is cleared, because one name owns a binding or a removal.
                let local = directories::has_entry(
                    &candidate,
                    parent.directory.entries,
                    name,
                    window,
                    deadline,
                )?;
                if !local {
                    parent.directory.count = parent
                        .directory
                        .count
                        .checked_add(1)
                        .ok_or(WorkspaceError::Capacity)?;
                    parent.directory.bytes = parent
                        .directory
                        .bytes
                        .checked_add(added)
                        .ok_or(WorkspaceError::Capacity)?;
                    rows += 1;
                    row_bytes += added as isize;
                }
                if parent.directory.count > 128 {
                    return Err(WorkspaceError::Capacity);
                }
                parent.directory.tombstones =
                    directories::keep_name(&candidate, parent.directory, name, window, deadline)?;
                let mut entry = vector(1)?;
                entry.push(Cell::new(
                    &metadata_pages::entry_key(name)?,
                    &directories::entry(*serial, *kind)?,
                )?);
                parent.directory.entries =
                    candidate.update(parent.directory.entries, entry, window, deadline)?;
            }
            updates.push(Cell::new(
                &metadata_pages::dirty_key(generation, parent.serial),
                &[1],
            )?);
            updates.push(Cell::new(
                &metadata_pages::namespace_key(parent.serial),
                &parent.directory.value(),
            )?);
        }
        if changed == 0 {
            return Err(WorkspaceError::Io);
        }
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
        self.check_child_stamp(
            &state,
            baseline,
            revision,
            generation,
            &view,
            source_attr.attr.kind,
        )?;
        self.check_mutation_coherence(&state, origin, true)?;
        if state.completion.is_none() != needs_completion {
            return Err(WorkspaceError::Busy);
        }
        for parent in &parents {
            let index = state
                .nodes
                .iter()
                .position(|node| node.attr.serial == parent.serial)
                .ok_or(WorkspaceError::Busy)?;
            state.nodes[index].attr = parent.directory.attributes(state.nodes[index].attr);
        }
        // The moved name is the destination name from here on, for the moved
        // identity and for every descendant a moved directory carries. A cached
        // path is the locator a later service read uses, so a stale one would ask
        // for a name this operation just removed.
        let new_path = child_path(&parents[if same_parent { 0 } else { 1 }].path, destination)?;
        let old_path = child_path(&parents[0].path, source)?;
        if source_attr.attr.kind == NodeKind::Directory {
            for node in &mut state.nodes {
                if node.path() == old_path.as_slice() {
                    node.parent = destination_parent;
                }
                if node.path().starts_with(&old_path) {
                    let suffix = node.path()[old_path.len()..].to_vec();
                    let mut replaced = new_path.clone();
                    replaced.extend_from_slice(&suffix);
                    if replaced.len() <= crate::runtime::state::PATH_BYTES {
                        let len = replaced.len();
                        node.path[..len].copy_from_slice(&replaced);
                        node.path_len = len;
                    }
                }
            }
        } else {
            for node in &mut state.nodes {
                if node.path() == old_path.as_slice() {
                    let len = new_path.len();
                    node.path[..len].copy_from_slice(&new_path);
                    node.path_len = len;
                    node.parent = destination_parent;
                }
            }
        }
        if let Some(replaced) = &destination_attr {
            // The replaced name is gone exactly like an unlinked one: it releases
            // the lookup reference it owned, and an identity this generation
            // created that no name binds any more has no canonical identity to
            // declare. Its record still moves to the successor root, so its open
            // handle keeps reading its own version.
            if let Some(index) = state
                .nodes
                .iter()
                .position(|node| node.attr.serial == replaced.attr.serial)
            {
                state.nodes[index].names = state.nodes[index].names.saturating_sub(1);
                let reference = state.nodes[index].references(if origin.projected() {
                    ReferenceScope::Projection
                } else {
                    ReferenceScope::Local
                });
                *reference = reference.saturating_sub(1);
            }
            if replaced.attr.kind == NodeKind::File {
                state.unlinked(replaced.attr.serial);
            }
        }
        let receipt = MutationReceipt {
            incarnation: self.inner.incarnation,
            generation,
            inode: source_attr.attr.serial,
            revision: revision.checked_add(1).ok_or(WorkspaceError::Capacity)?,
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
        state.revision = receipt.revision;
        state.dirty_inodes += new_dirty;
        state.dirty_directories += new_dirty;
        state.directory_names = state.directory_names.saturating_add_signed(rows);
        state.directory_bytes = state.directory_bytes.saturating_add_signed(row_bytes);
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
            // One small fixed notification set: the source parent's entry, the
            // destination parent's entry and the moved inode's own attributes.
            // A failure retains the published mutation and reports Coherence.
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
        Ok(())
    }
    /// Loads both edited parents and resolves the destination's prior binding.
    fn rename_parents(
        &self,
        operation: &mut crate::runtime::state::OperationGuard,
        view: &View,
        source: (u64, &[u8]),
        destination: (u64, &[u8]),
        deadline: Instant,
    ) -> Result<Vec<Parent>, WorkspaceError> {
        let (source_parent, source_name) = source;
        let (destination_parent, destination_name) = destination;
        if source_parent == self.inner.root.serial && source_name.is_empty() {
            return Err(WorkspaceError::Unsupported);
        }
        let generation = self.state()?.generation;
        let mut parents = Vec::new();
        parents
            .try_reserve_exact(if source_parent == destination_parent {
                1
            } else {
                2
            })
            .map_err(|_| WorkspaceError::Capacity)?;
        for (serial, name) in [
            (source_parent, source_name),
            (destination_parent, destination_name),
        ] {
            if parents
                .iter()
                .any(|parent: &Parent| parent.serial == serial)
            {
                continue;
            }
            let (path, attr) = {
                let state = self.state()?;
                let node = state.node(serial)?;
                if node.attr.kind != NodeKind::Directory {
                    return Err(WorkspaceError::NotDirectory);
                }
                check_access(node.attr, self.inner.root.uid, 3)?;
                (node.path().to_vec(), node.attr)
            };
            child_path(&path, name)?;
            let loaded = self.directory_record(view, serial, deadline)?;
            let directory = loaded.unwrap_or_else(|| Directory::initial(attr, view.base));
            let (reanchor, dirty) = {
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
                    serial,
                    loaded.as_ref(),
                    lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                    deadline,
                )?
            };
            parents.push(Parent {
                serial,
                path,
                directory,
                dirty,
                reanchor,
                remove: None,
                bind: None,
                mtime: (attr.mtime_seconds, attr.mtime_nanoseconds),
            });
        }
        let _ = operation;
        Ok(parents)
    }
    /// True when `serial` is `ancestor` or one of its cached descendants.
    fn descends_from(&self, serial: u64, ancestor: u64) -> Result<bool, WorkspaceError> {
        let state = self.state()?;
        let mut current = serial;
        let mut steps = 0;
        loop {
            if current == ancestor {
                return Ok(true);
            }
            let node = match state.nodes.iter().find(|node| node.attr.serial == current) {
                Some(node) => node,
                None => return Ok(false),
            };
            if node.attr.serial == self.inner.root.serial {
                return Ok(false);
            }
            current = node.parent;
            steps += 1;
            if steps > crate::runtime::state::NODE_LIMIT {
                // A bounded refusal, distinct from a detected cycle.
                return Err(WorkspaceError::Unsupported);
            }
        }
    }
}
/// The captured record one parent delta was loaded from, if it had one.
fn loaded(directory: &Directory) -> Option<Directory> {
    (directory.revision != 0).then_some(*directory)
}
fn now() -> Result<(i64, u32), WorkspaceError> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WorkspaceError::Io)?;
    Ok((
        i64::try_from(time.as_secs()).map_err(|_| WorkspaceError::Capacity)?,
        time.subsec_nanos(),
    ))
}
