use crate::cow_tree::{Data, DirectoryData, FileData, Kind, NodeId};
use crate::{
    worker::WorkspaceWorker, CreateWorkspaceSession, EndWorkspaceMode, Workspace,
    WorkspaceCommitResult, WorkspaceCommitStatus, WorkspaceDetail, WorkspaceDiff,
    WorkspaceEndResult, WorkspaceError, WorkspaceFileRangeEdit, WorkspaceId, WorkspacePlacement,
    WorkspaceProjection, WorkspaceResult, WorkspaceSession, WorkspaceSummary, Workspaces,
};
use layerfs_content::object::access::ObjectRead;
use layerfs_content::tree::inode::{InodeId, InodeKind, InodeTableCounters, InodeTableRoot};
use layerfs_layerstack_store::{
    CommitOutcome, Result, StoreError as StorageError, WorkspaceCommitPhase,
};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitTransition {
    Rebased,
    RebasedRefresh,
    InstallationFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceState {
    Active,
    Committed,
    Discarded,
    Ended,
    BrokenCleanup,
}

// Fixed records are held by one anonymous private file. The file is unlinked
// before writing, so failed construction drops its descriptor without leaving a
// scratch path; its full length is reserved against Workspace spool capacity.
const HANDOFF_RECORD_BYTES: u64 = 104;
pub(crate) struct CommitHandoff {
    file: std::fs::File,
    generation: u64,
    base_root: layerfs_content::ObjectId,
    root: layerfs_content::ObjectId,
    inodes: InodeTableRoot,
    records: u64,
    removed: Option<RemovedBindings>,
    selected: bool,
    new_canonical: usize,
}

struct RemovedBindings {
    index: crate::commit_spool::Run<48>,
    names: std::fs::File,
    bytes: u64,
}

impl RemovedBindings {
    fn contains_ancestor(&mut self, path: &str) -> Result<bool> {
        // The sorted digest is only an index. Compare the stored path bytes too,
        // including every equal digest, so collisions cannot remove an alias.
        for end in path
            .bytes()
            .enumerate()
            .filter_map(|(at, byte)| (byte == b'/').then_some(at))
            .chain(std::iter::once(path.len()))
        {
            let prefix = &path.as_bytes()[..end];
            let key = layerfs_content::ObjectId::for_bytes(prefix).to_bytes();
            let mut low = 0;
            let mut high = self.index.count;
            while low < high {
                let mid = low + (high - low) / 2;
                let record = self.index.at(mid)?;
                if record[..32] < key[..] {
                    low = mid + 1;
                } else {
                    high = mid;
                }
            }
            while low < self.index.count {
                let record = self.index.at(low)?;
                if record[..32] != key {
                    break;
                }
                let offset = u64::from_be_bytes(record[32..40].try_into().unwrap());
                let len = u64::from_be_bytes(record[40..48].try_into().unwrap());
                if len == prefix.len() as u64 {
                    self.names.seek(SeekFrom::Start(offset))?;
                    // Canonical path bound, reused for every comparison.
                    let mut bytes = [0_u8; 4096];
                    if prefix.len() > bytes.len() {
                        return Err(StorageError::Integrity("handoff path length"));
                    }
                    self.names.read_exact(&mut bytes[..prefix.len()])?;
                    if bytes[..prefix.len()] == *prefix {
                        return Ok(true);
                    }
                }
                low += 1;
            }
        }
        Ok(false)
    }
}

impl Workspace {
    pub(crate) fn commit(&mut self) -> Result<(CommitOutcome, CommitTransition)> {
        if let Some((outcome, refresh)) = self.pending_publication {
            let transition = self.transition_committed(outcome, refresh)?;
            return Ok((outcome, transition));
        }
        self.ensure_active()?;
        if let Some(mut resolution) = self.resolution.take() {
            resolution.invalidate_if_mutated(self)?;
            if resolution.unresolved() != 0 {
                self.resolution = Some(resolution);
                return Err(StorageError::InvalidInput(
                    "unresolved reconciliation conflict",
                ));
            }
            let choices = resolution.choices()?;
            let candidate = match self.build_candidate() {
                Ok(candidate) => candidate,
                Err(error) => {
                    self.resolution = Some(resolution);
                    return Err(error);
                }
            };
            self.commit_handoff = None;
            let outcome = self.store.clone().commit_reconciliation_checked(
                &resolution.prepared,
                candidate,
                &choices,
                |reader, working_root, final_root| {
                    let objects = layerfs_layerstack_store::CoreReader(reader);
                    self.commit_handoff =
                        Some(self.prepare_selected_handoff(&objects, working_root, final_root)?);
                    Ok(())
                },
            );
            if outcome.is_err() {
                self.resolution = Some(resolution);
            }
            let outcome = outcome?;
            let transition = self.transition_committed(outcome, true)?;
            return Ok((outcome, transition));
        }
        let branch = self
            .store
            .branch(self.branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        if branch.head_commit_id != self.expected_head || branch.base_layer_id != self.expected_base
        {
            return Err(StorageError::CommitHeadMoved {
                expected: self.expected_head,
                actual: branch.head_commit_id,
            });
        }
        if self.mutation_generation == 0 {
            return Ok((
                CommitOutcome::UpToDate {
                    root_id: self.base_root,
                },
                CommitTransition::Rebased,
            ));
        }
        #[cfg(feature = "test-instrumentation")]
        if consume_verification_fault(
            self.branch_id,
            VerificationFault::Candidate,
            self.spool_bytes,
        ) {
            crate::changes::inject_candidate_failure_once();
        }
        let candidate = self.build_candidate()?;
        let outcome =
            self.store
                .commit_candidate(&branch, self.base_root, self.expected_base, candidate)?;
        let transition = self.transition_committed(outcome, false)?;
        Ok((outcome, transition))
    }

    fn transition_committed(
        &mut self,
        outcome: CommitOutcome,
        refresh: bool,
    ) -> Result<CommitTransition> {
        // Publication already succeeded. Keep its identity until installation and
        // cleanup finish; recovery must never construct a second logical Commit.
        self.pending_publication = Some((outcome, refresh));
        let started = Instant::now();
        #[cfg(test)]
        if INJECT_INSTALL_FAILURE.with(|inject| inject.replace(false)) {
            self.presentation_failed = true;
            return Ok(CommitTransition::InstallationFailed);
        }
        let rebased = self.rebase_committed(outcome);
        layerfs_layerstack_store::note_workspace_commit_phase(
            WorkspaceCommitPhase::InPlaceRebase,
            elapsed_ns(started),
        );
        if rebased.is_err() {
            self.presentation_failed = true;
            return Ok(CommitTransition::InstallationFailed);
        }
        self.pending_publication = None;
        Ok(if refresh {
            CommitTransition::RebasedRefresh
        } else {
            CommitTransition::Rebased
        })
    }

    fn prepare_selected_handoff<S: ObjectRead>(
        &self,
        objects: &S,
        working_root: layerfs_content::ObjectId,
        final_root: layerfs_content::ObjectId,
    ) -> Result<CommitHandoff> {
        use layerfs_content::tree::directory::{diff_directory_entries, DirectoryStateRoot};
        use layerfs_content::tree::inode::{codec::decode_inode_record, inode_table_lookup};
        let old_table = InodeTableRoot(
            layerfs_content::filesystem::namespace(objects, working_root)?.inode_table_root,
        );
        let new_table = InodeTableRoot(
            layerfs_content::filesystem::namespace(objects, final_root)?.inode_table_root,
        );
        let handoff_bytes = (self.nodes.len() as u64)
            .checked_mul(HANDOFF_RECORD_BYTES)
            .ok_or(StorageError::Integrity("handoff size"))?;
        let owned = self
            .spool_bytes
            .checked_add(self.references.bytes())
            .and_then(|n| n.checked_add(handoff_bytes))
            .ok_or(StorageError::Integrity("handoff spool"))?;
        self.policy.check(owned)?;
        let limit = self.policy.max_spool_bytes - owned;
        let path = self.spool.join("commit-selected-bindings");
        let mut names = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        std::fs::remove_file(path)?;
        let mut sorter = crate::commit_spool::Sorter::<48>::new(
            &self.spool,
            limit,
            self.policy.max_final_delta_memory_bytes / 4,
        )?;
        let mut count = 0_u64;
        let mut name_bytes = 0_u64;
        for (&node, value) in &self.nodes {
            if !matches!(value.data, Data::Directory(_)) || value.paths.is_empty() {
                continue;
            }
            let inode = self.frontier_inode(node)?;
            let before = inode_table_lookup(
                objects,
                old_table,
                inode,
                &mut InodeTableCounters::default(),
            )?
            .ok_or(StorageError::Integrity("selected old directory"))?;
            let Some(after) = inode_table_lookup(
                objects,
                new_table,
                inode,
                &mut InodeTableCounters::default(),
            )?
            else {
                continue;
            };
            let before = objects.with_authenticated_canonical(before, decode_inode_record)?;
            let after = objects.with_authenticated_canonical(after, decode_inode_record)?;
            if before.kind != InodeKind::Directory || after.kind != InodeKind::Directory {
                continue;
            }
            let parent = value.paths.first().unwrap();
            let mut failure = None;
            let diff = diff_directory_entries(
                objects,
                DirectoryStateRoot(before.content_root),
                DirectoryStateRoot(after.content_root),
                |change| {
                    if change.before.is_none() || change.before == change.after {
                        return Ok(());
                    }
                    let result = (|| -> Result<()> {
                        let name = std::str::from_utf8(change.name.as_bytes())
                            .map_err(|_| StorageError::Integrity("selected binding name"))?;
                        let prefix = if parent.is_empty() {
                            name.to_owned()
                        } else {
                            format!("{parent}/{name}")
                        };
                        if prefix.len() > 4096 {
                            return Err(StorageError::InvalidInput("handoff path length"));
                        }
                        let next_count = count
                            .checked_add(1)
                            .ok_or(StorageError::Integrity("selected binding count"))?;
                        let next_names = name_bytes
                            .checked_add(prefix.len() as u64)
                            .ok_or(StorageError::Integrity("selected binding bytes"))?;
                        if next_count
                            .checked_mul(96)
                            .and_then(|n| n.checked_add(next_names))
                            .is_none_or(|bytes| bytes > limit)
                        {
                            return Err(StorageError::InvalidInput("workspace spool limit"));
                        }
                        let mut record = [0; 48];
                        record[..32].copy_from_slice(
                            layerfs_content::ObjectId::for_bytes(prefix.as_bytes()).as_bytes(),
                        );
                        record[32..40].copy_from_slice(&name_bytes.to_be_bytes());
                        record[40..48].copy_from_slice(&(prefix.len() as u64).to_be_bytes());
                        names.write_all(prefix.as_bytes())?;
                        sorter.push(record)?;
                        count = next_count;
                        name_bytes = next_names;
                        Ok(())
                    })();
                    if let Err(error) = result {
                        failure = Some(error);
                        return Err(layerfs_content::CoreError::Io);
                    }
                    Ok(())
                },
            );
            if let Some(error) = failure {
                return Err(error);
            }
            diff?;
        }
        let index = sorter.finish()?;
        self.prepare_identity_handoff(
            objects,
            final_root,
            Some(RemovedBindings {
                index,
                names,
                bytes: name_bytes + count * 48,
            }),
        )
    }

    pub(crate) fn prepare_handoff<S: ObjectRead>(
        &self,
        objects: &S,
        root: layerfs_content::ObjectId,
    ) -> Result<CommitHandoff> {
        self.prepare_identity_handoff(objects, root, None)
    }

    fn prepare_identity_handoff<S: ObjectRead>(
        &self,
        objects: &S,
        root: layerfs_content::ObjectId,
        removed: Option<RemovedBindings>,
    ) -> Result<CommitHandoff> {
        let selected = removed.is_some();
        let started = Instant::now();
        let mut inode_counters = InodeTableCounters::default();
        let mut binding_checks = 0;
        let mut new_canonical = 0;
        use layerfs_content::tree::directory::{
            directory_lookup, DirectoryStateRoot, NamespaceCounters,
        };
        use layerfs_content::tree::inode::{codec::decode_inode_record, inode_table_lookup_many};
        let records = self
            .nodes
            .iter()
            .filter(|(id, node)| {
                selected
                    || ((!node.paths.is_empty() || node.links != 0)
                        && (node.commit_dirty
                            || self.dirty.contains(id)
                            || node.canonical.is_none()))
            })
            .count() as u64;
        let bytes = records
            .checked_mul(HANDOFF_RECORD_BYTES)
            .ok_or(StorageError::Integrity("handoff size"))?;
        self.policy.check(
            self.spool_bytes
                .checked_add(self.references.bytes())
                .and_then(|n| n.checked_add(bytes))
                .and_then(|n| n.checked_add(removed.as_ref().map_or(0, |removed| removed.bytes)))
                .ok_or(StorageError::Integrity("handoff spool"))?,
        )?;
        let batch_size = (self.policy.max_final_delta_memory_bytes / 4096).clamp(1, 128) as usize;
        let buffer_capacity =
            (self.policy.max_final_delta_memory_bytes / 8).min(64 * 1024) as usize;
        let handoff_capacity =
            buffer_capacity as u64 + batch_size as u64 * 256 + HANDOFF_RECORD_BYTES;
        self.policy.check_final_delta(
            handoff_capacity
                + crate::references::MEMORY.min(self.policy.max_final_delta_memory_bytes / 8),
        )?;
        let path = self.spool.join("commit-handoff");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        std::fs::remove_file(&path)?;
        let mut file = std::io::BufWriter::with_capacity(buffer_capacity, file);
        let namespace = layerfs_content::filesystem::namespace(objects, root)?;
        let inodes = InodeTableRoot(namespace.inode_table_root);
        let mut batch = Vec::with_capacity(batch_size);
        // Omitted materialized nodes remain the authenticated base identities and
        // backing already owned by this frozen Workspace. The common writer has
        // no mutation for them; installation retains them without rediscovery.
        let mut live = self.nodes.iter().filter(|(id, node)| {
            selected
                || ((!node.paths.is_empty() || node.links != 0)
                    && (node.commit_dirty || self.dirty.contains(id) || node.canonical.is_none()))
        });
        loop {
            batch.clear();
            for _ in 0..batch_size {
                let Some((&node, _)) = live.next() else {
                    break;
                };
                let inode = if self.nodes[&node].canonical.is_none()
                    && self.nodes[&node].paths.is_empty()
                {
                    None
                } else {
                    Some(self.frontier_inode(node)?)
                };
                batch.push((node, inode));
            }
            if batch.is_empty() {
                break;
            }
            let keys = batch
                .iter()
                .map(|(_, inode)| inode.unwrap_or(namespace.root_directory_inode))
                .collect::<Vec<_>>();
            let ids = inode_table_lookup_many(objects, inodes, &keys, &mut inode_counters)?;
            for ((node, inode), record_id) in batch.iter().copied().zip(ids) {
                let record_id = record_id.filter(|_| inode.is_some());
                if record_id.is_none() && selected {
                    let mut bytes = [0_u8; HANDOFF_RECORD_BYTES as usize];
                    bytes[..8].copy_from_slice(&node.0.to_be_bytes());
                    file.write_all(&bytes)?;
                    continue;
                }
                let inode = inode.ok_or(StorageError::Integrity("handoff final inode"))?;
                if !self.canonical_nodes.contains_key(&inode) {
                    new_canonical += 1;
                }
                let record_id = record_id.ok_or(StorageError::Integrity("handoff final inode"))?;
                let record =
                    objects.with_authenticated_canonical(record_id, decode_inode_record)?;
                record.validate(node == crate::ROOT)?;
                let old = &self.nodes[&node];
                let attr = self.attr(node)?;
                let metadata =
                    crate::cow_tree::portable_metadata(objects, record.metadata_root, record.kind)?;
                let (kind, len) = match record.kind {
                    InodeKind::RegularFile => (
                        Kind::File,
                        layerfs_content::file::rope::state(
                            objects,
                            layerfs_content::file::rope::FileStateRoot(record.content_root),
                            &mut layerfs_content::file::rope::RopeCounters::default(),
                        )?
                        .logical_len,
                    ),
                    InodeKind::Directory => (Kind::Directory, 0),
                    InodeKind::Symlink => {
                        let target = objects
                            .with_authenticated_canonical(
                                record.content_root,
                                layerfs_content::tree::directory::codec::decode_symlink,
                            )?
                            .target;
                        if !selected
                            && !matches!(&old.data, Data::Symlink(previous) if *previous == target)
                        {
                            return Err(StorageError::Integrity("handoff symlink content"));
                        }
                        (Kind::Symlink, target.len() as u64)
                    }
                };
                let links = if kind == Kind::Directory {
                    2
                } else {
                    u32::try_from(record.namespace_ref_count)
                        .map_err(|_| StorageError::Integrity("handoff links"))?
                };
                if kind != attr.kind
                    || (!selected
                        && (len != attr.size
                            || links != attr.links
                            || metadata.permission_mode != attr.mode
                            || metadata.mtime_seconds != attr.mtime_seconds
                            || metadata.mtime_nanoseconds != attr.mtime_nanoseconds))
                {
                    return Err(StorageError::Integrity("committed Workspace presentation"));
                }
                // Identity membership follows authenticated surviving base
                // bindings plus the common writer's exact final overlays. Check
                // the changed edges against that final directory here; paths and
                // aliases retained by Workspace need no root-to-leaf discovery.
                if let Data::Directory(directory) = &old.data {
                    if !selected {
                        for (name, desired) in &directory.changes {
                            binding_checks += 1;
                            let actual = directory_lookup(
                                objects,
                                DirectoryStateRoot(record.content_root),
                                &layerfs_content::CanonicalName::from_bytes(name)?,
                                &mut NamespaceCounters::default(),
                            )?;
                            if actual
                                != desired
                                    .map(|child| self.frontier_inode(child))
                                    .transpose()?
                            {
                                return Err(StorageError::Integrity("handoff final binding"));
                            }
                        }
                    }
                }
                file.write_all(&node.0.to_be_bytes())?;
                file.write_all(inode.as_bytes())?;
                file.write_all(record.content_root.as_bytes())?;
                file.write_all(&len.to_be_bytes())?;
                file.write_all(&metadata.permission_mode.to_be_bytes())?;
                file.write_all(&links.to_be_bytes())?;
                file.write_all(&metadata.mtime_seconds.to_be_bytes())?;
                file.write_all(&metadata.mtime_nanoseconds.to_be_bytes())?;
                file.write_all(&[record.kind as u8, 0, 0, 0])?;
            }
        }
        if file.stream_position()? != bytes {
            return Err(StorageError::Integrity("handoff length"));
        }
        file.seek(SeekFrom::Start(0))?;
        let file = file.into_inner().map_err(|error| error.into_error())?;
        layerfs_layerstack_store::note_workspace_handoff(
            records,
            bytes,
            handoff_capacity,
            inode_counters.nodes_read,
            binding_checks,
            elapsed_ns(started),
        );
        Ok(CommitHandoff {
            file,
            generation: self.mutation_generation,
            base_root: self.base_root,
            root,
            inodes,
            records,
            removed,
            selected,
            new_canonical,
        })
    }

    fn rebase_committed(&mut self, outcome: CommitOutcome) -> Result<()> {
        let expected_head = match outcome {
            CommitOutcome::Committed { commit_id, .. } => Some(commit_id),
            CommitOutcome::UpToDate { .. } => self.expected_head,
        };
        let expected_root = match outcome {
            CommitOutcome::Committed { root_id, .. } | CommitOutcome::UpToDate { root_id } => {
                root_id
            }
        };
        let pinned = self.store.pin_branch(self.branch_id)?;
        if pinned.branch.head_commit_id != expected_head || pinned.root != expected_root {
            return Err(StorageError::Integrity("committed Workspace head"));
        }
        let handoff = self
            .commit_handoff
            .as_mut()
            .ok_or(StorageError::Integrity("missing Commit handoff"))?;
        if handoff.generation != self.mutation_generation
            || handoff.base_root != self.base_root
            || handoff.root != pinned.root
        {
            return Err(StorageError::Integrity("Commit handoff identity"));
        }
        // Reserve final Workspace identity capacity before installing any node.
        // This becomes retained Workspace state, not a second temporary map.
        self.canonical_nodes
            .try_reserve(handoff.new_canonical)
            .map_err(|_| StorageError::InvalidInput("Workspace identity allocation"))?;
        handoff.file.seek(SeekFrom::Start(0))?;
        for _ in 0..handoff.records {
            let mut bytes = [0; HANDOFF_RECORD_BYTES as usize];
            handoff.file.read_exact(&mut bytes)?;
            let node = NodeId(u64::from_be_bytes(bytes[..8].try_into().unwrap()));
            if bytes[100] == 0 {
                let old = self
                    .nodes
                    .get_mut(&node)
                    .ok_or(StorageError::Integrity("handoff Workspace node"))?;
                old.paths.clear();
                old.links = 0;
                old.commit_dirty = false;
                if matches!(old.data, Data::Directory(_)) {
                    self.directory_parents.remove(&node);
                }
                if let Some(inode) = old.canonical.take() {
                    self.canonical_nodes.remove(&inode);
                }
                continue;
            }
            let inode = InodeId::from_slice(&bytes[8..40])?;
            let content = layerfs_content::ObjectId::from_bytes(&bytes[40..72])?;
            let len = u64::from_be_bytes(bytes[72..80].try_into().unwrap());
            let old = self
                .nodes
                .get_mut(&node)
                .ok_or(StorageError::Integrity("handoff Workspace node"))?;
            if let Some(removed) = &mut handoff.removed {
                let mut failure = None;
                old.paths
                    .retain(|path| match removed.contains_ancestor(path) {
                        Ok(remove) => !remove,
                        Err(error) => {
                            failure = Some(error);
                            true
                        }
                    });
                if let Some(error) = failure {
                    return Err(error);
                }
            }
            if handoff.selected && old.paths.is_empty() && matches!(old.data, Data::Directory(_)) {
                self.directory_parents.remove(&node);
            }
            old.commit_dirty = false;
            old.mode = u32::from_be_bytes(bytes[80..84].try_into().unwrap());
            old.links = u32::from_be_bytes(bytes[84..88].try_into().unwrap());
            old.mtime_seconds = i64::from_be_bytes(bytes[88..96].try_into().unwrap());
            old.mtime_nanoseconds = u32::from_be_bytes(bytes[96..100].try_into().unwrap());
            old.data = match &old.data {
                Data::File(_) => Data::File(FileData::Base {
                    root: layerfs_content::file::rope::FileStateRoot(content),
                    len,
                }),
                Data::Directory(_) => Data::Directory(DirectoryData {
                    base: Some(layerfs_content::tree::directory::DirectoryStateRoot(
                        content,
                    )),
                    changes: Default::default(),
                }),
                Data::Symlink(target) => Data::Symlink(if handoff.selected {
                    layerfs_layerstack_store::CoreReader(&pinned.reader)
                        .with_authenticated_canonical(
                            content,
                            layerfs_content::tree::directory::codec::decode_symlink,
                        )?
                        .target
                } else {
                    target.clone()
                }),
            };
            old.canonical = Some(inode);
            if self.canonical_nodes.insert(inode, node).is_none() {
                handoff.new_canonical = handoff.new_canonical.saturating_sub(1);
            }
        }
        // Retain open-unlinked backing; drop only reachability from the new
        // snapshot. Descriptors for obsolete linked spools stay owned until all
        // cleanup succeeds, so a partial cleanup failure can be retried safely.
        for (&node, value) in &self.nodes {
            if !matches!(value.data, Data::File(FileData::Edited { .. }))
                && self.open_spools.contains_key(&node)
            {
                self.remove_spool_if_exists(node, &self.spool.join(node.0.to_string()))?;
            }
        }
        self.open_spools.retain(|node, _| {
            matches!(
                self.nodes.get(node).map(|node| &node.data),
                Some(Data::File(FileData::Edited { .. }))
            )
        });
        let mut retained_spool = 0_u64;
        let mut retained_inline = 0_u64;
        let mut retained_pieces = 0_u64;
        for value in self
            .nodes
            .values_mut()
            .filter(|node| node.paths.is_empty() && node.links == 0)
        {
            if let Some(inode) = value.canonical.take() {
                self.canonical_nodes.remove(&inode);
            }
            if let Data::File(FileData::Edited {
                spool_high_water,
                pieces,
                ..
            }) = &value.data
            {
                retained_spool += spool_high_water;
                retained_inline += pieces.inline_len();
                retained_pieces += pieces.logical_allocation_charge()?;
            }
        }
        self.references.clear()?;
        if let Some(removed) = self.commit_handoff.as_mut().unwrap().removed.take() {
            removed.index.remove()?;
        }
        let inodes = self.commit_handoff.as_ref().unwrap().inodes;
        self.reader = pinned.reader.with_read_metrics_from(&self.reader);
        self.expected_head = expected_head;
        self.expected_base = pinned.branch.base_layer_id;
        self.base_root = pinned.root;
        self.base_inodes = inodes;
        self.spool_bytes = retained_spool;
        self.spool_bytes_peak = retained_spool;
        self.inline_bytes = retained_inline;
        self.piece_allocation_bytes = retained_pieces;
        self.mutation_generation = 0;
        self.mutation_paths.clear();
        self.dirty.clear();
        self.capture = crate::capture::CaptureState::default();
        self.resolution = None;
        self.state = WorkspaceState::Active;
        self.commit_handoff = None;
        Ok(())
    }

    #[doc(hidden)]
    pub fn discard(&mut self) -> Result<()> {
        if self.state == WorkspaceState::Committed {
            return Err(StorageError::InvalidInput("workspace committed"));
        }
        self.store.retry_candidate_cleanup()?;
        self.clear_spool()?;
        self.references.clear()?;
        self.commit_handoff = None;
        self.state = WorkspaceState::Discarded;
        Ok(())
    }

    pub(crate) fn end_clean(&mut self) -> Result<()> {
        self.store.retry_candidate_cleanup()?;
        self.clear_spool()?;
        self.references.clear()?;
        self.commit_handoff = None;
        self.state = WorkspaceState::Ended;
        Ok(())
    }

    pub(crate) fn ensure_active(&self) -> Result<()> {
        if self.state == WorkspaceState::Active && self.pending_publication.is_none() {
            Ok(())
        } else {
            Err(StorageError::InvalidInput("workspace inactive"))
        }
    }
}

impl Workspaces {
    #[cfg(feature = "test-instrumentation")]
    pub fn verification_workspace_state(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<VerificationWorkspaceState> {
        let worker = self.worker(id)?;
        let workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let (physical_current, physical_peak, physical_errors, physical_observations) =
            workspace.physical_spool.borrow().snapshot();
        Ok(VerificationWorkspaceState {
            spool_bytes: workspace.spool_bytes,
            spool_peak_bytes: workspace.spool_bytes_peak,
            physical_spool_allocated_bytes: physical_current,
            physical_spool_peak_bytes: physical_peak,
            physical_spool_observation_errors: physical_errors,
            physical_spool_observation_count: physical_observations,
            mutation_generation: workspace.mutation_generation,
            open_spool_files: workspace.open_spools.len(),
        })
    }

    pub fn create_workspace_session(
        &self,
        request: CreateWorkspaceSession,
    ) -> WorkspaceResult<WorkspaceSession> {
        self.prune_retained()?;
        if !request.placement.root().is_absolute() || request.placement.root().parent().is_none() {
            return Err(WorkspaceError::InvalidPlacement);
        }
        let lease = self.acquire_lease(request.branch_id)?;
        let pinned = self.store.pin_branch(request.branch_id)?;
        let identity = crate::worker::WorkspaceIdentity {
            layer_stack_id: pinned.layer_stack.id,
            layer_stack_name: pinned.layer_stack.name.clone(),
            branch_name: pinned.branch.name.clone(),
        };
        let id = WorkspaceId::new();
        let state = self.runtime_root.join("workspaces").join(id.to_string());
        std::fs::create_dir_all(&state)?;
        let workspace = Workspace::from_snapshot(
            crate::cow_tree::WorkspaceSnapshot {
                store: self.store.clone(),
                branch_id: request.branch_id,
                expected_head: pinned.branch.head_commit_id,
                expected_base: pinned.branch.base_layer_id,
                root: pinned.root,
                reader: pinned.reader,
            },
            &state.join("spool"),
            crate::ResourcePolicy::default(),
        )?;
        let projection = request.projection.unwrap_or({
            if matches!(
                request.placement,
                crate::WorkspacePlacement::Container { .. }
            ) || cfg!(target_os = "linux")
            {
                WorkspaceProjection::Fuse
            } else {
                WorkspaceProjection::Materialize
            }
        });
        let worker = Arc::new(WorkspaceWorker::new(
            id,
            request.clone(),
            projection,
            identity,
            workspace,
            lease,
        ));
        let handle = match crate::projection::attach(&worker, self.daemon_mount_owner()?) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&state);
                return Err(error);
            }
        };
        #[cfg(debug_assertions)]
        if std::env::var("LAYERFS_WORKSPACE_INJECT_POST_ATTACH_FAILURE").as_deref() == Ok("1") {
            drop(handle);
            std::fs::remove_dir_all(&state)?;
            return Err(WorkspaceError::InvalidPlacement);
        }
        *worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)? = Some(handle);
        let (create_read, cache_rows, cache_bytes) = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .reader
            .take_create_metrics()?;
        if matches!(
            &request.placement,
            crate::WorkspacePlacement::Container { .. }
        ) {
            layerfs_layerstack_store::note_workspace_create_snapshot(
                create_read,
                cache_rows,
                cache_bytes,
            )?;
        }
        let session = session(&worker)?;
        self.sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .insert(id, crate::registry::SessionRecord::Active(worker));
        Ok(session)
    }

    pub fn commit_workspace_session(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<WorkspaceCommitResult> {
        self.commit_workspace_session_with_status(id)
            .map(|status| status.result)
    }

    pub fn commit_workspace_session_with_status(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<WorkspaceCommitStatus> {
        let worker = self.worker(id)?;
        let _timing = layerfs_layerstack_store::begin_workspace_commit(match worker.projection {
            WorkspaceProjection::Fuse => layerfs_layerstack_store::CaptureMode::Live,
            WorkspaceProjection::Materialize => layerfs_layerstack_store::CaptureMode::Materialized,
        })?;
        if worker.has_executions()? {
            return Ok(WorkspaceCommitStatus {
                result: WorkspaceCommitResult::Busy,
                presentation_failed: false,
            });
        }
        if worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .presentation_failed
        {
            return Err(WorkspaceError::InvalidExecution);
        }
        let commit_read_before = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .reader
            .read_metrics_snapshot()?;
        let started = Instant::now();
        let paused = crate::projection::pause(&worker);
        layerfs_layerstack_store::note_workspace_commit_phase(
            WorkspaceCommitPhase::PauseFence,
            elapsed_ns(started),
        );
        paused?;
        if let Err(error) = crate::projection::record_write_metrics(&worker) {
            let _ = crate::projection::resume(&worker);
            return Err(error);
        }
        let started = Instant::now();
        let quiesced = worker.wait_for_writers().and_then(|()| worker.quiesce());
        layerfs_layerstack_store::note_workspace_commit_phase(
            WorkspaceCommitPhase::Quiesce,
            elapsed_ns(started),
        );
        let _quiesced = match quiesced {
            Ok(quiesced) => quiesced,
            Err(WorkspaceError::WorkspaceBusy) => {
                if let Err(error) = crate::projection::resume(&worker) {
                    let _ = crate::projection::end(&worker);
                    worker
                        .workspace
                        .lock()
                        .map_err(|_| WorkspaceError::WorkspaceBusy)?
                        .presentation_failed = true;
                    return Err(error);
                }
                return Ok(WorkspaceCommitStatus {
                    result: WorkspaceCommitResult::Busy,
                    presentation_failed: false,
                });
            }
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        let result = (|| {
            let started = Instant::now();
            let captured = crate::projection::capture(&worker);
            layerfs_layerstack_store::note_workspace_commit_phase(
                WorkspaceCommitPhase::Capture,
                elapsed_ns(started),
            );
            captured?;
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            workspace.note_commit_edit_state()?;
            let previous_head = workspace.expected_head;
            let committed = match workspace.commit() {
                Ok((outcome, transition)) => Ok((
                    WorkspaceCommitResult::from_outcome(outcome, previous_head),
                    transition,
                )),
                Err(error) => WorkspaceError::from_commit(error)
                    .map(|result| (result, CommitTransition::Rebased)),
            };
            let observations = workspace.reader.read_metrics_snapshot().and_then(|after| {
                layerfs_layerstack_store::note_workspace_commit_reads(commit_read_before, after)
            });
            match (committed, observations) {
                (Ok((result @ WorkspaceCommitResult::Created { .. }, _)), Err(_))
                | (Ok((result @ WorkspaceCommitResult::UpToDate { .. }, _)), Err(_)) => {
                    workspace.presentation_failed = true;
                    Ok((result, CommitTransition::InstallationFailed))
                }
                (result, Ok(())) => result,
                (_, Err(error)) => Err(error.into()),
            }
        })();
        #[cfg(feature = "test-instrumentation")]
        if matches!(&result, Ok((WorkspaceCommitResult::Created { .. }, _)))
            && consume_verification_fault(
                worker.request.branch_id,
                VerificationFault::PresentationResume,
                0,
            )
        {
            crate::projection::inject_resume_failure_once();
        }
        let presentation = match &result {
            Ok((
                WorkspaceCommitResult::Created { .. } | WorkspaceCommitResult::UpToDate { .. },
                transition,
            )) => match transition {
                CommitTransition::Rebased => {
                    let started = Instant::now();
                    let resumed = crate::projection::resume(&worker);
                    layerfs_layerstack_store::note_workspace_commit_phase(
                        WorkspaceCommitPhase::Resume,
                        elapsed_ns(started),
                    );
                    resumed
                }
                CommitTransition::InstallationFailed => Err(WorkspaceError::InvalidExecution),
                CommitTransition::RebasedRefresh => {
                    let started = Instant::now();
                    let refreshed = crate::projection::refresh(&worker, self.daemon_mount_owner()?);
                    layerfs_layerstack_store::note_workspace_commit_phase(
                        WorkspaceCommitPhase::Resume,
                        elapsed_ns(started),
                    );
                    refreshed
                }
            },
            _ => {
                let started = Instant::now();
                let resumed = crate::projection::resume(&worker);
                layerfs_layerstack_store::note_workspace_commit_phase(
                    WorkspaceCommitPhase::Resume,
                    elapsed_ns(started),
                );
                resumed
            }
        };
        if let Err(error) = presentation {
            let _ = crate::projection::end(&worker);
            worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .presentation_failed = true;
            return match result {
                Ok((result @ WorkspaceCommitResult::Created { .. }, _))
                | Ok((result @ WorkspaceCommitResult::UpToDate { .. }, _)) => {
                    Ok(WorkspaceCommitStatus {
                        result,
                        presentation_failed: true,
                    })
                }
                _ => Err(error),
            };
        }
        match result {
            Ok((result, _)) => Ok(WorkspaceCommitStatus {
                result,
                presentation_failed: false,
            }),
            Err(WorkspaceError::WorkspaceBusy) => Ok(WorkspaceCommitStatus {
                result: WorkspaceCommitResult::Busy,
                presentation_failed: false,
            }),
            Err(error) => Err(error),
        }
    }

    pub fn recover_workspace_presentation(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<WorkspaceSession> {
        let worker = self.worker(id)?;
        if worker.has_executions()?
            || !worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .presentation_failed
        {
            return Err(WorkspaceError::InvalidExecution);
        }
        crate::projection::pause(&worker)?;
        let _quiesced = worker.quiesce()?;
        crate::projection::end(&worker)?;
        {
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            if let Some((outcome, refresh)) = workspace.pending_publication {
                if workspace.transition_committed(outcome, refresh)?
                    == CommitTransition::InstallationFailed
                {
                    return Err(WorkspaceError::InvalidExecution);
                }
            }
        }
        let handle = crate::projection::attach(&worker, self.daemon_mount_owner()?)?;
        *worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)? = Some(handle);
        let mut workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        workspace.presentation_failed = false;
        Ok(session_locked(&worker, &workspace))
    }

    pub fn edit_workspace_file_range(&self, edit: WorkspaceFileRangeEdit) -> WorkspaceResult<()> {
        self.edit_workspace_file_ranges(vec![edit])
    }

    pub fn start_workspace_resource_sample(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<layerfs_daemon::ResourceSampleClock> {
        let worker = self.worker(id)?;
        let WorkspacePlacement::Container { container_id, .. } = &worker.request.placement else {
            return Err(WorkspaceError::InvalidPlacement);
        };
        let owner = self
            .daemon_mount_owner()?
            .filter(|owner| owner.accepts(container_id))
            .ok_or(WorkspaceError::InvalidPlacement)?;
        owner.start_resource_sample(id).map_err(Into::into)
    }

    pub fn finish_workspace_resource_sample(
        &self,
        id: WorkspaceId,
        t0_unix_ns: u64,
        t3_unix_ns: u64,
        uncertainty_ns: u64,
    ) -> WorkspaceResult<layerfs_daemon::protocol::CgroupResourceSample> {
        let owner = self
            .daemon_mount_owner()?
            .ok_or(WorkspaceError::InvalidPlacement)?;
        owner
            .finish_resource_sample(id, t0_unix_ns, t3_unix_ns, uncertainty_ns)
            .map_err(Into::into)
    }

    pub fn edit_workspace_file_ranges(
        &self,
        edits: Vec<WorkspaceFileRangeEdit>,
    ) -> WorkspaceResult<()> {
        let first = edits.first().ok_or(WorkspaceError::InvalidExecution)?;
        if edits
            .iter()
            .any(|edit| edit.workspace_id != first.workspace_id || edit.path != first.path)
        {
            return Err(WorkspaceError::InvalidExecution);
        }
        let workspace_id = first.workspace_id;
        let path = first.path.clone();
        let worker = self.worker(workspace_id)?;
        if worker.has_executions()? {
            return Err(WorkspaceError::WorkspaceBusy);
        }
        crate::projection::pause(&worker)?;
        let quiesced = worker.wait_for_writers().and_then(|()| worker.quiesce());
        let _quiesced = match quiesced {
            Ok(value) => value,
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        if let Err(error) = crate::projection::capture(&worker) {
            crate::projection::resume(&worker)?;
            return Err(error);
        }
        let result = (|| {
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            let node = lookup_path(&mut workspace, &path)?;
            if workspace.nodes[&node].pins != 0 {
                return Err(WorkspaceError::WorkspaceBusy);
            }
            let checkpoint = workspace.edit_checkpoint(node)?;
            let result = workspace.edit_many(
                node,
                edits
                    .into_iter()
                    .map(|edit| (edit.start, edit.delete_len, edit.replacement))
                    .collect(),
            );
            Ok((result, checkpoint, node))
        })();
        let (result, checkpoint, node) = match result {
            Ok(value) => value,
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        if let Err(error) = result {
            worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .restore_edit(checkpoint)?;
            crate::projection::resume(&worker)?;
            return Err(error.into());
        }
        match crate::projection::refresh_file(&worker, node, self.daemon_mount_owner()?) {
            Ok(()) => Ok(()),
            Err(refresh_error) => {
                let restored = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?
                    .restore_edit(checkpoint);
                if restored.is_err()
                    || crate::projection::refresh_file(&worker, node, self.daemon_mount_owner()?)
                        .is_err()
                {
                    if let Ok(mut workspace) = worker.workspace.lock() {
                        workspace.presentation_failed = true;
                    }
                }
                Err(refresh_error)
            }
        }
    }

    pub fn end_workspace_session(
        &self,
        id: WorkspaceId,
        mode: EndWorkspaceMode,
    ) -> WorkspaceResult<WorkspaceEndResult> {
        let worker = self.worker(id)?;
        if worker.has_executions()? {
            return Err(WorkspaceError::WorkspaceBusy);
        }
        let workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let state = workspace.state;
        let presentation_failed = workspace.presentation_failed;
        drop(workspace);
        if state == WorkspaceState::BrokenCleanup
            || (presentation_failed && mode == EndWorkspaceMode::Clean)
        {
            return Err(WorkspaceError::InvalidPlacement);
        }
        crate::projection::pause(&worker)?;
        let _quiesced = match worker.quiesce() {
            Ok(quiesced) => quiesced,
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        let validated = (|| {
            let active = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .state
                == WorkspaceState::Active;
            if mode == EndWorkspaceMode::Clean && active {
                let has_resolution = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?
                    .resolution
                    .is_some();
                if has_resolution || crate::projection::is_dirty(&worker)? {
                    return Err(WorkspaceError::WorkspaceDirty);
                }
            }
            let workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            let state = workspace
                .spool
                .parent()
                .ok_or(WorkspaceError::InvalidPlacement)?
                .to_owned();
            let discarded = mode == EndWorkspaceMode::Discard;
            Ok((state, discarded))
        })();
        let (state, discarded) = match validated {
            Ok(validated) => validated,
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        crate::projection::record_read_metrics(&worker)?;
        if let Err(error) = crate::projection::end(&worker) {
            if let Ok(mut workspace) = worker.workspace.lock() {
                workspace.state = WorkspaceState::BrokenCleanup;
            }
            return Err(error);
        }
        let finalized = (|| {
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            match mode {
                EndWorkspaceMode::Discard => {
                    workspace.discard()?;
                    workspace.state = WorkspaceState::Ended;
                }
                EndWorkspaceMode::Clean => workspace.end_clean()?,
            }
            drop(workspace);
            if state.exists() {
                std::fs::remove_dir_all(state)?;
            }
            Ok(())
        })();
        if let Err(error) = finalized {
            if let Ok(mut workspace) = worker.workspace.lock() {
                workspace.state = WorkspaceState::BrokenCleanup;
            }
            return Err(error);
        }
        worker
            .lease
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .take();
        let retained = {
            let workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            crate::registry::RetainedSession {
                session: session_locked(&worker, &workspace),
                mutation_generation: workspace.mutation_generation,
                ended_at: SystemTime::now(),
            }
        };
        self.sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .insert(id, crate::registry::SessionRecord::Retained(retained));
        self.prune_retained()?;
        Ok(WorkspaceEndResult {
            session_id: id,
            discarded,
        })
    }

    pub fn sessions(&self) -> WorkspaceResult<Vec<WorkspaceSummary>> {
        self.prune_retained()?;
        self.sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .values()
            .map(|record| match record {
                crate::registry::SessionRecord::Active(worker) => summary(worker),
                crate::registry::SessionRecord::Retained(retained) => {
                    Ok(crate::registry::retained_summary(retained))
                }
            })
            .collect()
    }

    pub fn session(&self, id: WorkspaceId) -> WorkspaceResult<WorkspaceDetail> {
        self.prune_retained()?;
        let record = self
            .sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .get(&id)
            .cloned()
            .ok_or(WorkspaceError::NotFound)?;
        let executions = self.execution_summaries(id)?;
        match record {
            crate::registry::SessionRecord::Active(worker) => {
                let workspace = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?;
                Ok(WorkspaceDetail {
                    session: session_locked(&worker, &workspace),
                    mutation_generation: workspace.mutation_generation,
                    executions,
                })
            }
            crate::registry::SessionRecord::Retained(retained) => Ok(WorkspaceDetail {
                session: retained.session,
                mutation_generation: retained.mutation_generation,
                executions,
            }),
        }
    }

    pub fn diff(&self, id: WorkspaceId) -> WorkspaceResult<WorkspaceDiff> {
        self.prune_retained()?;
        let record = self
            .sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .get(&id)
            .cloned()
            .ok_or(WorkspaceError::NotFound)?;
        match record {
            crate::registry::SessionRecord::Active(worker) => {
                let dirty = crate::projection::is_dirty(&worker)?;
                let workspace = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?;
                Ok(WorkspaceDiff {
                    session_id: id,
                    dirty,
                    mutation_generation: workspace.mutation_generation,
                })
            }
            crate::registry::SessionRecord::Retained(retained) => Ok(WorkspaceDiff {
                session_id: id,
                dirty: false,
                mutation_generation: retained.mutation_generation,
            }),
        }
    }
}

fn lookup_path(workspace: &mut Workspace, path: &str) -> Result<crate::NodeId> {
    let mut node = crate::ROOT;
    for component in path
        .as_bytes()
        .split(|byte| *byte == b'/')
        .filter(|component| !component.is_empty())
    {
        node = workspace.lookup_node(node, component)?;
    }
    Ok(node)
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

pub(crate) fn session(worker: &WorkspaceWorker) -> WorkspaceResult<WorkspaceSession> {
    let workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    Ok(session_locked(worker, &workspace))
}

fn session_locked(worker: &WorkspaceWorker, workspace: &Workspace) -> WorkspaceSession {
    WorkspaceSession {
        id: worker.id,
        branch_id: workspace.branch_id,
        layer_stack_id: worker.identity.layer_stack_id,
        layer_stack_name: worker.identity.layer_stack_name.clone(),
        branch_name: worker.identity.branch_name.clone(),
        pinned_head: workspace.expected_head,
        placement: worker.request.placement.clone(),
        projection: worker.projection,
        state: workspace.state,
    }
}

fn summary(worker: &Arc<WorkspaceWorker>) -> WorkspaceResult<WorkspaceSummary> {
    let dirty = crate::projection::is_dirty(worker)?;
    let workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    Ok(WorkspaceSummary {
        id: worker.id,
        branch_id: workspace.branch_id,
        layer_stack_id: worker.identity.layer_stack_id,
        layer_stack_name: worker.identity.layer_stack_name.clone(),
        branch_name: worker.identity.branch_name.clone(),
        pinned_head: workspace.expected_head,
        state: workspace.state,
        dirty,
    })
}

#[cfg(test)]
thread_local! {
    static INJECT_INSTALL_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_layerstack_store::{
        EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    };

    fn fixture(
        label: &str,
    ) -> (
        std::path::PathBuf,
        Workspaces,
        layerfs_layerstack_store::BranchId,
        LayerStackStore,
    ) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-lifecycle-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("file"), b"abcdef").unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspaces = Workspaces::new(root.join("runtime"), store.clone()).unwrap();
        (root, workspaces, branch, store)
    }

    fn session(
        root: &std::path::Path,
        workspaces: &Workspaces,
        branch: layerfs_layerstack_store::BranchId,
    ) -> WorkspaceSession {
        workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: crate::WorkspacePlacement::Host {
                    root: root.join("mount"),
                },
                projection: Some(WorkspaceProjection::Materialize),
            })
            .unwrap()
    }

    fn prepend(workspaces: &Workspaces, id: WorkspaceId) -> WorkspaceResult<()> {
        workspaces.edit_workspace_file_range(WorkspaceFileRangeEdit {
            workspace_id: id,
            path: "file".into(),
            start: 0,
            delete_len: 0,
            replacement: crate::WorkspaceFileReplacement::Inline(b"P".to_vec()),
        })
    }

    #[test]
    fn no_op_candidate_rechecks_head_before_returning_up_to_date() {
        let (root, _, branch, store) = fixture("no-op-head-check");
        let pinned = store.pin_branch(branch).unwrap();
        let mut workspace = Workspace::open(store.clone(), branch, root.join("spool")).unwrap();
        let file = workspace.lookup_node(crate::ROOT, b"file").unwrap();
        workspace.write(file, 0, b"advanced").unwrap();
        workspace.commit().unwrap();
        let no_op = layerfs_layerstack_store::ObjectBuffer::new(&pinned.reader)
            .unwrap()
            .finish(pinned.root, 0)
            .unwrap();
        assert!(matches!(
            store.commit_candidate(
                &pinned.branch,
                pinned.root,
                pinned.branch.base_layer_id,
                no_op
            ),
            Err(StorageError::CommitHeadMoved { .. })
        ));
        workspace.end_clean().unwrap();
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn handoff_preserves_unseen_alias_and_open_unlinked_handle() {
        let (root, _, branch, store) = fixture("handoff-alias-handles");
        {
            let mut initial =
                Workspace::open(store.clone(), branch, root.join("initial-spool")).unwrap();
            let file = initial.lookup_node(crate::ROOT, b"file").unwrap();
            initial.link(file, crate::ROOT, b"hidden").unwrap();
            assert!(matches!(
                initial.commit().unwrap().1,
                CommitTransition::Rebased
            ));
        }
        let mut workspace = Workspace::open(store.clone(), branch, root.join("spool")).unwrap();
        let file = workspace.lookup_node(crate::ROOT, b"file").unwrap();
        workspace.write(file, 0, b"updated").unwrap();
        workspace.unlink(crate::ROOT, b"file", false).unwrap();
        assert!(workspace.nodes[&file].paths.is_empty());
        let ghost = workspace
            .create_file(crate::ROOT, b"ghost", 0o640)
            .unwrap()
            .node;
        workspace.write(ghost, 0, b"unlinked").unwrap();
        workspace.pin(ghost, false).unwrap();
        workspace.unlink(crate::ROOT, b"ghost", false).unwrap();
        let (outcome, transition) = workspace.commit().unwrap();
        assert!(matches!(outcome, CommitOutcome::Committed { .. }));
        assert_eq!(transition, CommitTransition::Rebased);
        assert_eq!(workspace.lookup_node(crate::ROOT, b"hidden").unwrap(), file);
        assert_eq!(workspace.read(file, 0, 32).unwrap(), b"updated");
        assert_eq!(workspace.attr(file).unwrap().links, 1);
        assert_eq!(workspace.read(ghost, 0, 32).unwrap(), b"unlinked");
        assert_eq!(workspace.attr(ghost).unwrap().links, 0);
        workspace.unpin(ghost).unwrap();
        assert!(workspace.open_spools.is_empty());
        assert!(matches!(
            workspace.commit().unwrap().0,
            CommitOutcome::UpToDate { .. }
        ));
        workspace.end_clean().unwrap();
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selected_root_handoff_preserves_replaced_open_inode_and_stable_survivor() {
        use layerfs_content::filesystem::{self, ContentChange};
        let (root, _, branch, store) = fixture("selected-handoff");
        let mut workspace = Workspace::open(store.clone(), branch, root.join("spool")).unwrap();
        let old = workspace.lookup_node(crate::ROOT, b"file").unwrap();
        workspace.pin(old, false).unwrap();
        let mut objects = layerfs_layerstack_store::ObjectBuffer::new(&workspace.reader).unwrap();
        let selected = filesystem::apply_changes(
            &mut objects,
            workspace.base_root,
            &[
                ContentChange::Remove {
                    path: "file".into(),
                },
                ContentChange::Write {
                    path: "file".into(),
                    bytes: b"replacement".to_vec(),
                    mode: 0o600,
                },
            ],
            [83; 32],
        )
        .unwrap()
        .root_id;
        workspace.commit_handoff = Some(
            workspace
                .prepare_selected_handoff(&objects, workspace.base_root, selected)
                .unwrap(),
        );
        let candidate = objects.finish(selected, 0).unwrap();
        let pinned = store.pin_branch(branch).unwrap();
        let outcome = store
            .commit_candidate(
                &pinned.branch,
                workspace.base_root,
                workspace.expected_base,
                candidate,
            )
            .unwrap();
        assert_eq!(
            workspace.transition_committed(outcome, true).unwrap(),
            CommitTransition::RebasedRefresh
        );
        assert_eq!(workspace.read(old, 0, 32).unwrap(), b"abcdef");
        assert_eq!(workspace.attr(old).unwrap().links, 0);
        let replacement = workspace.lookup_node(crate::ROOT, b"file").unwrap();
        assert_ne!(replacement, old);
        assert_eq!(workspace.read(replacement, 0, 32).unwrap(), b"replacement");
        assert_eq!(workspace.attr(replacement).unwrap().mode, 0o600);
        workspace.unpin(old).unwrap();
        workspace.end_clean().unwrap();
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn published_commit_survives_install_failure_and_recovery_does_not_republish() {
        let (root, workspaces, branch, store) = fixture("published-install-failure");
        let session = session(&root, &workspaces, branch);
        prepend(&workspaces, session.id).unwrap();
        let before = store.branch(branch).unwrap().unwrap().head_commit_id;
        INJECT_INSTALL_FAILURE.with(|inject| inject.set(true));
        let status = workspaces
            .commit_workspace_session_with_status(session.id)
            .unwrap();
        let WorkspaceCommitResult::Created { commit_id, .. } = status.result else {
            panic!("published Commit must be returned");
        };
        assert!(status.presentation_failed);
        assert_ne!(Some(commit_id), before);
        assert_eq!(
            store.branch(branch).unwrap().unwrap().head_commit_id,
            Some(commit_id)
        );
        assert!(workspaces.commit_workspace_session(session.id).is_err());
        workspaces
            .recover_workspace_presentation(session.id)
            .unwrap();
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"Pabcdef");
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::UpToDate { .. }
        ));
        assert_eq!(
            store.branch(branch).unwrap().unwrap().head_commit_id,
            Some(commit_id)
        );
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(all(target_os = "linux", feature = "host-fuse"))]
    fn live_fuse_owner_edit_invalidates_warm_pages_and_rolls_back_resume_failure() {
        if std::env::var_os("LAYERFS_LIVE_FUSE").is_none() {
            return;
        }
        use std::os::unix::fs::MetadataExt;
        let (root, workspaces, branch, store) = fixture("live-invalidation");
        let session = workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: crate::WorkspacePlacement::Host {
                    root: root.join("mount"),
                },
                projection: Some(WorkspaceProjection::Fuse),
            })
            .unwrap();
        let file = root.join("mount/file");
        let inode = std::fs::metadata(&file).unwrap().ino();
        let branch_before = store.pin_branch(branch).unwrap().root;
        assert_eq!(std::fs::read(&file).unwrap(), b"abcdef");
        crate::projection::inject_resume_failure_once();
        assert!(prepend(&workspaces, session.id).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"abcdef");
        assert_eq!(store.pin_branch(branch).unwrap().root, branch_before);
        assert!(
            !workspaces
                .worker(session.id)
                .unwrap()
                .workspace
                .lock()
                .unwrap()
                .presentation_failed
        );
        prepend(&workspaces, session.id).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"Pabcdef");
        assert_eq!(std::fs::metadata(&file).unwrap().ino(), inode);
        assert_eq!(std::fs::metadata(&file).unwrap().len(), 7);
        workspaces
            .edit_workspace_file_range(WorkspaceFileRangeEdit {
                workspace_id: session.id,
                path: "file".into(),
                start: 1,
                delete_len: 6,
                replacement: crate::WorkspaceFileReplacement::Inline(b"Q".to_vec()),
            })
            .unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"PQ");
        assert_eq!(std::fs::metadata(&file).unwrap().ino(), inode);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Discard)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_projection_refresh_restores_exact_state_and_retry_once() {
        let (root, workspaces, branch, store) = fixture("refresh-rollback");
        let session = session(&root, &workspaces, branch);
        let worker = workspaces.worker(session.id).unwrap();
        let before = {
            let workspace = worker.workspace.lock().unwrap();
            (
                workspace.nodes.clone(),
                workspace.dirty.clone(),
                workspace.mutation_generation,
                workspace.mutation_paths.clone(),
                workspace.spool_bytes,
                workspace.inline_bytes,
                workspace.piece_allocation_bytes,
            )
        };
        let branch_before = store.pin_branch(branch).unwrap().root;
        crate::projection::inject_refresh_failure_once();
        assert!(prepend(&workspaces, session.id).is_err());
        {
            let workspace = worker.workspace.lock().unwrap();
            assert_eq!(workspace.nodes, before.0);
            assert_eq!(workspace.dirty, before.1);
            assert_eq!(workspace.mutation_generation, before.2);
            assert_eq!(workspace.mutation_paths, before.3);
            assert_eq!(workspace.spool_bytes, before.4);
            assert_eq!(workspace.inline_bytes, before.5);
            assert_eq!(workspace.piece_allocation_bytes, before.6);
        }
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"abcdef");
        assert_eq!(store.pin_branch(branch).unwrap().root, branch_before);
        assert!(worker.projection_handle.lock().unwrap().is_some());
        prepend(&workspaces, session.id).unwrap();
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"Pabcdef");
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Discard)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pinned_target_rejects_owner_edit_without_state_change() {
        let (root, workspaces, branch, store) = fixture("pinned-edit");
        let session = session(&root, &workspaces, branch);
        let worker = workspaces.worker(session.id).unwrap();
        let node = {
            let mut workspace = worker.workspace.lock().unwrap();
            let node = lookup_path(&mut workspace, "file").unwrap();
            workspace.pin(node, false).unwrap();
            node
        };
        let before = {
            let workspace = worker.workspace.lock().unwrap();
            (
                workspace.nodes.clone(),
                workspace.dirty.clone(),
                workspace.mutation_generation,
                workspace.mutation_paths.clone(),
                workspace.spool_bytes,
                workspace.inline_bytes,
                workspace.piece_allocation_bytes,
                store.pin_branch(branch).unwrap().root,
            )
        };
        assert!(matches!(
            prepend(&workspaces, session.id),
            Err(WorkspaceError::WorkspaceBusy)
        ));
        {
            let workspace = worker.workspace.lock().unwrap();
            assert_eq!(workspace.nodes, before.0);
            assert_eq!(workspace.dirty, before.1);
            assert_eq!(workspace.mutation_generation, before.2);
            assert_eq!(workspace.mutation_paths, before.3);
            assert_eq!(workspace.spool_bytes, before.4);
            assert_eq!(workspace.inline_bytes, before.5);
            assert_eq!(workspace.piece_allocation_bytes, before.6);
        }
        assert_eq!(store.pin_branch(branch).unwrap().root, before.7);
        assert!(worker.projection_handle.lock().unwrap().is_some());
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"abcdef");
        worker.workspace.lock().unwrap().unpin(node).unwrap();
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn busy_commit_with_resume_failure_requires_explicit_presentation_recovery() {
        let (root, workspaces, branch, _) = fixture("busy-resume");
        let session = session(&root, &workspaces, branch);
        let worker = workspaces.worker(session.id).unwrap();
        worker.note_writer(true).unwrap();
        crate::projection::inject_resume_failure_once();
        assert!(matches!(
            workspaces.commit_workspace_session(session.id),
            Err(WorkspaceError::Io(_))
        ));
        assert!(worker.workspace.lock().unwrap().presentation_failed);
        worker.note_writer(false).unwrap();
        assert_eq!(
            workspaces
                .recover_workspace_presentation(session.id)
                .unwrap()
                .state,
            WorkspaceState::Active
        );
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn callback_and_execution_reject_owner_edit_without_state_change() {
        let (root, workspaces, branch, store) = fixture("admission-busy");
        let session = session(&root, &workspaces, branch);
        let worker = workspaces.worker(session.id).unwrap();
        let before = {
            let workspace = worker.workspace.lock().unwrap();
            (
                workspace.nodes.clone(),
                workspace.dirty.clone(),
                workspace.mutation_generation,
                workspace.spool_bytes,
                workspace.inline_bytes,
                workspace.piece_allocation_bytes,
                store.pin_branch(branch).unwrap().root,
            )
        };
        let callback = worker.enter_callback().unwrap();
        assert!(matches!(
            prepend(&workspaces, session.id),
            Err(WorkspaceError::WorkspaceBusy)
        ));
        drop(callback);
        worker.note_execution(true).unwrap();
        assert!(matches!(
            prepend(&workspaces, session.id),
            Err(WorkspaceError::WorkspaceBusy)
        ));
        worker.note_execution(false).unwrap();
        {
            let workspace = worker.workspace.lock().unwrap();
            assert_eq!(workspace.nodes, before.0);
            assert_eq!(workspace.dirty, before.1);
            assert_eq!(workspace.mutation_generation, before.2);
            assert_eq!(workspace.spool_bytes, before.3);
            assert_eq!(workspace.inline_bytes, before.4);
            assert_eq!(workspace.piece_allocation_bytes, before.5);
        }
        assert_eq!(store.pin_branch(branch).unwrap().root, before.6);
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"abcdef");
        assert!(worker.projection_handle.lock().unwrap().is_some());
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(feature = "test-instrumentation")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationFault {
    Candidate,
    PresentationResume,
    ShortAppend,
    NoSpace,
}
#[cfg(feature = "test-instrumentation")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerificationWorkspaceState {
    pub spool_bytes: u64,
    pub spool_peak_bytes: u64,
    pub physical_spool_allocated_bytes: Option<u64>,
    pub physical_spool_peak_bytes: Option<u64>,
    pub physical_spool_observation_errors: u64,
    pub physical_spool_observation_count: u64,
    pub mutation_generation: u64,
    pub open_spool_files: usize,
}
#[cfg(feature = "test-instrumentation")]
#[derive(Clone, Debug)]
pub struct VerificationFaultReceipt {
    pub branch: layerfs_layerstack_store::BranchId,
    pub fault: VerificationFault,
    pub hit_count: u64,
    pub spool_bytes_before: u64,
}
#[cfg(feature = "test-instrumentation")]
static VERIFICATION_FAULT: std::sync::Mutex<Option<VerificationFaultReceipt>> =
    std::sync::Mutex::new(None);
#[cfg(feature = "test-instrumentation")]
static VERIFICATION_FAULT_ARMED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "test-instrumentation")]
pub fn arm_verification_fault(
    branch: layerfs_layerstack_store::BranchId,
    fault: VerificationFault,
) -> WorkspaceResult<()> {
    let mut state = VERIFICATION_FAULT
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    if state.is_some() {
        return Err(WorkspaceError::WorkspaceBusy);
    }
    *state = Some(VerificationFaultReceipt {
        branch,
        fault,
        hit_count: 0,
        spool_bytes_before: 0,
    });
    VERIFICATION_FAULT_ARMED.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}
#[cfg(feature = "test-instrumentation")]
pub fn take_verification_fault_receipt() -> WorkspaceResult<Option<VerificationFaultReceipt>> {
    VERIFICATION_FAULT_ARMED.store(false, std::sync::atomic::Ordering::Release);
    Ok(VERIFICATION_FAULT
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .take())
}
#[cfg(feature = "test-instrumentation")]
pub(crate) fn consume_verification_fault(
    branch: layerfs_layerstack_store::BranchId,
    fault: VerificationFault,
    spool_bytes: u64,
) -> bool {
    if !VERIFICATION_FAULT_ARMED.load(std::sync::atomic::Ordering::Acquire) {
        return false;
    }
    let Ok(mut state) = VERIFICATION_FAULT.lock() else {
        return false;
    };
    let Some(receipt) = state.as_mut() else {
        return false;
    };
    if receipt.branch != branch || receipt.fault != fault || receipt.hit_count != 0 {
        return false;
    }
    receipt.hit_count = 1;
    receipt.spool_bytes_before = spool_bytes;
    VERIFICATION_FAULT_ARMED.store(false, std::sync::atomic::Ordering::Release);
    true
}

#[cfg(all(test, feature = "test-instrumentation"))]
#[test]
fn verification_fault_scope_is_one_shot() {
    let branch = layerfs_layerstack_store::BranchId::new();
    let other = layerfs_layerstack_store::BranchId::new();
    assert!(!consume_verification_fault(
        branch,
        VerificationFault::ShortAppend,
        0
    ));
    arm_verification_fault(branch, VerificationFault::ShortAppend).unwrap();
    assert!(!consume_verification_fault(
        other,
        VerificationFault::ShortAppend,
        0
    ));
    assert!(std::thread::spawn(move || consume_verification_fault(
        branch,
        VerificationFault::ShortAppend,
        4096
    ))
    .join()
    .unwrap());
    assert!(!consume_verification_fault(
        branch,
        VerificationFault::ShortAppend,
        8192
    ));
    let receipt = take_verification_fault_receipt().unwrap().unwrap();
    assert_eq!((receipt.hit_count, receipt.spool_bytes_before), (1, 4096));
    assert!(take_verification_fault_receipt().unwrap().is_none());
}
