use crate::cow_tree::{portable_metadata, Attr, Data, FileData, Kind, NodeId, Workspace, ROOT};
use layerfs_content::file::rope::{self, FileMutationBatch, FileStateRoot, RopeCounters};
use layerfs_content::filesystem::{self, LogicalCounters};
use layerfs_content::object::access::{ObjectRead, ObjectStore};
use layerfs_content::object::{ContentDigestWriter, ObjectId};
use layerfs_content::tree::directory::{
    directory_lookup, directory_page_after, empty_directory, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_content::tree::inode::codec::decode_inode_record;
use layerfs_content::tree::inode::{inode_table_lookup, InodeTableCounters};
use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1};
use layerfs_content::{CanonicalName, CanonicalPath};
use layerfs_layerstack_store::{
    BuiltRoot, CoreReader, ObjectBuffer, Result, StoreError as StorageError, WorkspaceCommitPhase,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::FileExt;
use std::time::Instant;

#[derive(Clone, Copy)]
struct BaseEntry {
    inode: InodeId,
    record: InodeRecordV1,
    mode: u32,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
}

#[derive(Clone, Copy)]
enum FingerprintNode {
    Workspace(NodeId),
    Base(InodeId),
}

impl Workspace {
    pub(crate) fn build_candidate(&mut self) -> Result<BuiltRoot> {
        #[cfg(any(debug_assertions, feature = "test-instrumentation"))]
        if INJECT_CANDIDATE_FAILURE.with(|inject| inject.replace(false)) {
            return Err(StorageError::Integrity(
                "injected Workspace candidate failure",
            ));
        }
        self.commit_handoff = None;
        crate::commit_spool::reset_metrics();
        let result = self.build_frontier_candidate();
        if let Some(stats) = crate::commit_spool::take_metrics() {
            layerfs_layerstack_store::note_workspace_commit_sort(
                stats.sequential_read_calls,
                stats.sequential_read_bytes,
                stats.positional_read_calls,
                stats.positional_read_bytes,
                stats.write_calls,
                stats.write_bytes,
                stats.merge_passes,
            );
        }
        result
    }

    // Directory overlays already are the final binding delta. Applying their inode
    // edges directly preserves untouched subtrees, including a renamed directory,
    // without building either complete namespace manifest.
    fn build_frontier_candidate(&mut self) -> Result<BuiltRoot> {
        let started = Instant::now();
        let memory = self.policy.max_final_delta_memory_bytes;

        let disk = self
            .policy
            .max_spool_bytes
            .checked_sub(self.spool_bytes)
            .and_then(|n| n.checked_sub(self.references.bytes()))
            .ok_or(StorageError::InvalidInput("workspace spool limit"))?;
        let mut references = self.references.sorted(&self.spool, disk / 2, memory / 8)?;
        let mut pairs = crate::commit_spool::Sorter::<65>::new(&self.spool, disk / 2, memory / 8)?;
        let captured = self.take_capture();
        if let Some(captured) = &captured {
            layerfs_layerstack_store::note_workspace_capture(1, captured.len);
        }
        let (mut objects, captured) = match captured {
            Some(crate::capture::CapturedFile {
                node,
                len,
                root,
                counters,
                objects,
            }) => (
                ObjectBuffer::resume_prevalidated(&self.reader, objects),
                Some((node, len, root, counters)),
            ),
            None => (ObjectBuffer::new(&self.reader)?, None),
        };
        let mut metadata_cache = Vec::new();
        let mut tree_counts = layerfs_content::tree::batch::TreeBatchCounters::default();
        let mut cdc_bytes_scanned = 0_u64;
        note_commit_phase(WorkspaceCommitPhase::CandidatePlan, started);
        let started = Instant::now();
        for (&node, value) in &self.nodes {
            if !value.commit_dirty && !self.dirty.contains(&node) {
                continue;
            }
            layerfs_layerstack_store::note_workspace_namespace_visits(0, 0, 0, 0, 1);
            let change =
                reference_delta(&references, crate::references::key(value.canonical, node))?;
            let before = value
                .canonical
                .map(|inode| self.base_record(inode))
                .transpose()?;
            let count = final_count(
                before.map_or(0, |record| record.namespace_ref_count),
                change,
            )?;
            if count == 0 && node != ROOT {
                continue;
            }
            let inode = self.frontier_inode(node)?;
            let attr = self.attr(node)?;
            let inode_kind = match attr.kind {
                Kind::File => InodeKind::RegularFile,
                Kind::Directory => InodeKind::Directory,
                Kind::Symlink => InodeKind::Symlink,
            };
            let content_root = match &value.data {
                Data::Directory(directory) => {
                    let mut content = match directory.base {
                        Some(base) => base,
                        None => empty_directory(&mut objects)?,
                    };
                    let changes = directory.changes.iter().map(|(name, desired)| {
                        Ok((
                            CanonicalName::from_bytes(name)?,
                            desired
                                .map(|child| self.frontier_inode(child))
                                .transpose()
                                .map_err(|_| {
                                    layerfs_content::CoreError::InvalidRecord(
                                        "Workspace binding identity",
                                    )
                                })?,
                        ))
                    });
                    let reserve = self.references.capacity()
                        + pairs.capacity()
                        + (metadata_cache.capacity() * 64) as u64;
                    let budget = memory
                        .checked_sub(reserve)
                        .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?;
                    let (next, stats) =
                        layerfs_content::tree::directory::directory_apply_sorted_with_budget(
                            &mut objects,
                            content,
                            changes,
                            budget as usize,
                        )?;
                    tree_counts.nodes_read += stats.nodes_read;
                    tree_counts.nodes_created += stats.nodes_created;
                    tree_counts.nodes_reused += stats.nodes_reused;
                    tree_counts.peak_scratch_bytes = tree_counts
                        .peak_scratch_bytes
                        .max(stats.peak_scratch_bytes + reserve as usize);
                    content = next;
                    content.0
                }
                Data::Symlink(target) => match before {
                    Some(record) => record.content_root,
                    None => filesystem::symlink_content(&mut objects, target.clone())?,
                },
                Data::File(_) => {
                    if let Some((_, _, root, counters)) =
                        captured.filter(|(id, _, _, _)| *id == node)
                    {
                        cdc_bytes_scanned = cdc_bytes_scanned
                            .checked_add(counters.cdc_bytes_scanned)
                            .ok_or(StorageError::Integrity("CDC counter"))?;
                        root.0
                    } else if let Some(record) = before {
                        if !self.file_may_differ(node, record.content_root)? {
                            record.content_root
                        } else {
                            let metadata = portable_metadata(
                                &CoreReader(&self.reader),
                                record.metadata_root,
                                record.kind,
                            )?;
                            let base = BaseEntry {
                                inode,
                                record,
                                mode: metadata.permission_mode,
                                mtime_seconds: metadata.mtime_seconds,
                                mtime_nanoseconds: metadata.mtime_nanoseconds,
                            };
                            let changed = self.mutate_existing_file(&mut objects, node, base)?;
                            let changed = match changed {
                                Some(changed) => Some(changed),
                                None if self
                                    .incremental_file_supported(node, record.content_root) =>
                                {
                                    None
                                }
                                None => Some(rope::build(
                                    &mut objects,
                                    WorkspaceFileReader::new(self, node)?,
                                )?),
                            };
                            if let Some((root, counters)) = changed {
                                cdc_bytes_scanned = cdc_bytes_scanned
                                    .checked_add(counters.cdc_bytes_scanned)
                                    .ok_or(StorageError::Integrity("CDC counter"))?;
                                root.0
                            } else {
                                record.content_root
                            }
                        }
                    } else {
                        let (root, counters) =
                            rope::build(&mut objects, WorkspaceFileReader::new(self, node)?)?;
                        cdc_bytes_scanned = cdc_bytes_scanned
                            .checked_add(counters.cdc_bytes_scanned)
                            .ok_or(StorageError::Integrity("CDC counter"))?;
                        root.0
                    }
                }
            };
            let old_metadata = before
                .map(|record| {
                    portable_metadata(&CoreReader(&self.reader), record.metadata_root, record.kind)
                })
                .transpose()?;
            let metadata_root = if old_metadata.is_some_and(|metadata| {
                metadata.permission_mode == attr.mode
                    && metadata.mtime_seconds == attr.mtime_seconds
                    && metadata.mtime_nanoseconds == attr.mtime_nanoseconds
            }) {
                before.unwrap().metadata_root
            } else {
                let key = (
                    inode_kind,
                    attr.mode,
                    attr.mtime_seconds,
                    attr.mtime_nanoseconds,
                );
                if let Some((_, root)) = metadata_cache.iter().find(|(stored, _)| *stored == key) {
                    *root
                } else {
                    let root = filesystem::build_portable_metadata(
                        &mut objects,
                        inode_kind,
                        attr.mode,
                        attr.mtime_seconds,
                        attr.mtime_nanoseconds,
                    )?;
                    if metadata_cache.len() == 8 {
                        metadata_cache.remove(0);
                    }
                    if metadata_cache.len() == metadata_cache.capacity() {
                        self.policy.check_final_delta(
                            self.references.capacity()
                                + pairs.capacity()
                                + ((metadata_cache.len() + 1) * 64) as u64,
                        )?;
                        metadata_cache.reserve_exact(1);
                    }
                    metadata_cache.push((key, root));
                    root
                }
            };
            let record = InodeRecordV1 {
                kind: inode_kind,
                content_root,
                metadata_root,
                namespace_ref_count: count,
            };
            layerfs_layerstack_store::note_workspace_namespace_visits(
                0,
                0,
                u64::from(before != Some(record)),
                u64::from(before == Some(record)),
                0,
            );
            if before != Some(record) {
                push_inode(&mut pairs, &mut objects, inode, Some(record))?;
            }
        }
        note_commit_phase(WorkspaceCommitPhase::Content, started);
        let started = Instant::now();
        // Reclaimed and unmaterialized existing inodes use authenticated base
        // counts plus the same net binding input. No deleted-subtree walk.
        references.rewind()?;
        let mut current = references.next()?;
        while let Some(record) = current.take() {
            let key = <[u8; 33]>::try_from(&record[..33]).unwrap();
            let mut change = crate::references::delta(&record);
            loop {
                current = references.next()?;
                if let Some(next) = current.filter(|next| next[..33] == key) {
                    change = change
                        .checked_add(crate::references::delta(&next))
                        .ok_or(StorageError::Integrity("namespace reference overflow"))?;
                } else {
                    break;
                }
            }
            if key[0] != 0 || change == 0 {
                continue;
            }
            let inode = InodeId::from_slice(&key[1..])?;
            let before = self.base_record(inode)?;
            let count = final_count(before.namespace_ref_count, change)?;
            // Live nodes were finalized above; zero-reference pinned nodes and
            // reclaimed nodes still require removal from the new table.
            if count != 0 && self.canonical_nodes.contains_key(&inode) {
                continue;
            }
            push_inode(
                &mut pairs,
                &mut objects,
                inode,
                (count != 0).then_some(InodeRecordV1 {
                    namespace_ref_count: count,
                    ..before
                }),
            )?;
        }
        references.remove()?;
        let mut pairs = pairs.finish()?;
        let input = std::iter::from_fn(|| match pairs.next() {
            Ok(Some(record)) => Some(Ok((
                InodeId::from_slice(&record[..32]).unwrap(),
                (record[32] != 0).then(|| ObjectId::from_bytes(&record[33..]).unwrap()),
            ))),
            Ok(None) => None,
            Err(_) => Some(Err(layerfs_content::CoreError::InvalidRecord(
                "Workspace inode spool",
            ))),
        });
        let namespace = filesystem::namespace(&objects, self.base_root)?;
        drop(metadata_cache);
        let budget = memory
            .checked_sub(self.references.capacity())
            .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?;
        let (table, stats) = layerfs_content::tree::inode::inode_table_apply_sorted_with_budget(
            &mut objects,
            self.base_inodes,
            input,
            budget as usize,
        )?;
        tree_counts.nodes_read += stats.nodes_read;
        tree_counts.nodes_created += stats.nodes_created;
        tree_counts.nodes_reused += stats.nodes_reused;
        tree_counts.peak_scratch_bytes = tree_counts
            .peak_scratch_bytes
            .max(stats.peak_scratch_bytes + self.references.capacity() as usize);
        let root = if table.0 == namespace.inode_table_root {
            self.base_root
        } else {
            objects.put_owned(
                layerfs_content::tree::directory::codec::encode_namespace_root(
                    layerfs_content::tree::NamespaceRootV1 {
                        inode_table_root: table.0,
                        ..namespace
                    },
                )?,
            )?
        };
        pairs.remove()?;
        note_commit_phase(WorkspaceCommitPhase::Namespace, started);
        let started = Instant::now();
        layerfs_layerstack_store::note_workspace_generic_commit(
            self.references.events,
            self.references.bytes(),
            self.references.capacity(),
            self.references.bookkeeping_ns,
            tree_counts.nodes_read,
            tree_counts.nodes_created,
            tree_counts.nodes_reused,
            tree_counts.peak_scratch_bytes as u64,
        );
        self.commit_handoff = Some(self.prepare_handoff(&objects, root)?);
        let built = objects.finish(root, cdc_bytes_scanned);
        note_commit_phase(WorkspaceCommitPhase::CandidateFinish, started);
        built
    }

    pub(crate) fn resolution_fingerprint(
        &mut self,
        affected_paths: &[CanonicalPath],
    ) -> Result<[u8; 32]> {
        self.policy
            .check_final_delta((affected_paths.len() as u64).saturating_mul(8))?;
        let mut affected = affected_paths.iter().collect::<Vec<_>>();
        affected.sort();
        let mut digest = ContentDigestWriter::new();
        digest.write_all(b"layerfs/workspace-resolution/v3\0")?;
        for path in affected {
            digest.write_all(b"A")?;
            frame(&mut digest, path.as_bytes())?;
            let mut current = FingerprintNode::Workspace(ROOT);
            let mut prefix = String::new();
            let mut components = path.components().peekable();
            if components.peek().is_none() {
                self.fingerprint_children(current, &prefix, &mut digest, 0)?;
            } else {
                while let Some(component) = components.next() {
                    let (base, overlay) = self.fingerprint_directory(current)?;
                    let child = if let Some(desired) =
                        overlay.and_then(|id| match &self.nodes[&id].data {
                            Data::Directory(dir) => dir.changes.get(component),
                            _ => None,
                        }) {
                        desired.map(FingerprintNode::Workspace)
                    } else if let Some(base) = base {
                        directory_lookup(
                            &CoreReader(&self.reader),
                            base,
                            &CanonicalName::from_bytes(component)?,
                            &mut NamespaceCounters::default(),
                        )?
                        .map(|inode| self.fingerprint_identity(inode))
                    } else {
                        None
                    };
                    let Some(child) = child else {
                        break;
                    };
                    if !prefix.is_empty() {
                        prefix.push('/');
                    }
                    prefix.push_str(
                        std::str::from_utf8(component)
                            .map_err(|_| StorageError::Integrity("resolution path"))?,
                    );
                    let directory = self.fingerprint_entry(child, &prefix, &mut digest)?;
                    if components.peek().is_none() && directory {
                        self.fingerprint_children(child, &prefix, &mut digest, 0)?;
                    } else if !directory {
                        break;
                    }
                    current = child;
                }
            }
            digest.write_all(b"Z")?;
        }
        Ok(digest.finish())
    }

    fn fingerprint_identity(&self, inode: InodeId) -> FingerprintNode {
        self.canonical_nodes
            .get(&inode)
            .copied()
            .map_or(FingerprintNode::Base(inode), FingerprintNode::Workspace)
    }

    fn fingerprint_directory(
        &self,
        node: FingerprintNode,
    ) -> Result<(Option<DirectoryStateRoot>, Option<NodeId>)> {
        match node {
            FingerprintNode::Workspace(id) => match &self.nodes[&id].data {
                Data::Directory(dir) => Ok((dir.base, Some(id))),
                _ => Err(StorageError::InvalidInput("resolution directory")),
            },
            FingerprintNode::Base(inode) => {
                let record = self.base_record(inode)?;
                if record.kind != InodeKind::Directory {
                    return Err(StorageError::InvalidInput("resolution directory"));
                }
                Ok((Some(DirectoryStateRoot(record.content_root)), None))
            }
        }
    }

    fn fingerprint_children(
        &self,
        node: FingerprintNode,
        prefix: &str,
        digest: &mut ContentDigestWriter,
        stack_bytes: u64,
    ) -> Result<()> {
        let charge = stack_bytes
            .checked_add(512 + prefix.len() as u64 * 2)
            .ok_or(StorageError::Integrity("resolution traversal"))?;
        self.policy
            .check_final_delta(charge + self.references.capacity())?;
        let (base, overlay) = self.fingerprint_directory(node)?;
        let mut after: Option<CanonicalName> = None;
        loop {
            let changes = overlay.and_then(|id| match &self.nodes[&id].data {
                Data::Directory(dir) => Some(&dir.changes),
                _ => None,
            });
            let next_overlay = changes.and_then(|changes| {
                let bound = after.as_ref().map_or(std::ops::Bound::Unbounded, |name| {
                    std::ops::Bound::Excluded(name.as_bytes().to_vec())
                });
                changes
                    .range((bound, std::ops::Bound::Unbounded))
                    .find_map(|(name, child)| child.map(|child| (name, child)))
            });
            let mut base_after = after.clone();
            let next_base = if let Some(base) = base {
                loop {
                    let page = directory_page_after(
                        &CoreReader(&self.reader),
                        base,
                        base_after.as_ref(),
                        1,
                        4096,
                        &mut NamespaceCounters::default(),
                    )?;
                    let Some((name, inode)) = page.entries.into_iter().next() else {
                        break None;
                    };
                    if changes.is_some_and(|changes| changes.contains_key(name.as_bytes())) {
                        base_after = Some(name);
                        continue;
                    }
                    break Some((name, inode));
                }
            } else {
                None
            };
            let next = match (next_overlay, next_base) {
                (Some((name, id)), Some((base_name, _)))
                    if name.as_slice() < base_name.as_bytes() =>
                {
                    Some((
                        CanonicalName::from_bytes(name)?,
                        FingerprintNode::Workspace(id),
                    ))
                }
                (_, Some((name, inode))) => Some((name, self.fingerprint_identity(inode))),
                (Some((name, id)), None) => Some((
                    CanonicalName::from_bytes(name)?,
                    FingerprintNode::Workspace(id),
                )),
                (None, None) => None,
            };
            let Some((name, child)) = next else {
                break;
            };
            let path = if prefix.is_empty() {
                name.as_str().to_owned()
            } else {
                format!("{prefix}/{}", name.as_str())
            };
            self.policy
                .check_final_delta(charge + path.len() as u64 + self.references.capacity())?;
            if self.fingerprint_entry(child, &path, digest)? {
                self.fingerprint_children(child, &path, digest, charge)?;
            }
            after = Some(name);
        }
        Ok(())
    }

    fn fingerprint_entry(
        &self,
        node: FingerprintNode,
        path: &str,
        digest: &mut ContentDigestWriter,
    ) -> Result<bool> {
        let (attr, record) = match node {
            FingerprintNode::Workspace(id) => (self.attr(id)?, None),
            FingerprintNode::Base(inode) => {
                let record = self.base_record(inode)?;
                let portable = portable_metadata(
                    &CoreReader(&self.reader),
                    record.metadata_root,
                    record.kind,
                )?;
                let size = match record.kind {
                    InodeKind::RegularFile => {
                        rope::state(
                            &CoreReader(&self.reader),
                            FileStateRoot(record.content_root),
                            &mut RopeCounters::default(),
                        )?
                        .logical_len
                    }
                    InodeKind::Directory => 0,
                    InodeKind::Symlink => self.base_symlink(record.content_root)?.len() as u64,
                };
                (
                    Attr {
                        node: NodeId(0),
                        kind: match record.kind {
                            InodeKind::RegularFile => Kind::File,
                            InodeKind::Directory => Kind::Directory,
                            InodeKind::Symlink => Kind::Symlink,
                        },
                        size,
                        mode: portable.permission_mode,
                        links: if record.kind == InodeKind::Directory {
                            2
                        } else {
                            u32::try_from(record.namespace_ref_count)
                                .map_err(|_| StorageError::Integrity("resolution links"))?
                        },
                        mtime_seconds: portable.mtime_seconds,
                        mtime_nanoseconds: portable.mtime_nanoseconds,
                    },
                    Some((inode, record)),
                )
            }
        };
        layerfs_layerstack_store::note_workspace_namespace_visits(
            u64::from(record.is_some()),
            1,
            0,
            0,
            0,
        );
        digest.write_all(b"E")?;
        frame(digest, path.as_bytes())?;
        digest.write_all(&[match attr.kind {
            Kind::File => 1,
            Kind::Directory => 2,
            Kind::Symlink => 3,
        }])?;
        digest.write_all(&attr.size.to_be_bytes())?;
        digest.write_all(&attr.mode.to_be_bytes())?;
        digest.write_all(&attr.links.to_be_bytes())?;
        digest.write_all(&attr.mtime_seconds.to_be_bytes())?;
        digest.write_all(&attr.mtime_nanoseconds.to_be_bytes())?;
        if let FingerprintNode::Workspace(id) = node {
            // Use the same identity whether this inode was materialized or is
            // still a borrowed base record. Presentation lookup is not a mutation.
            digest.write_all(self.frontier_inode(id)?.as_bytes())?;
            match attr.kind {
                Kind::File => {
                    std::io::copy(&mut WorkspaceFileReader::new(self, id)?, digest)?;
                }
                Kind::Symlink => frame(digest, &self.readlink(id)?)?,
                Kind::Directory => {}
            }
        } else if let Some((inode, record)) = record {
            // An unmaterialized inode remains authenticated by its base identity;
            // no global alias census or unrelated namespace materialization.
            digest.write_all(inode.as_bytes())?;
            match attr.kind {
                Kind::File => {
                    rope::read_all(
                        &CoreReader(&self.reader),
                        FileStateRoot(record.content_root),
                        digest,
                    )?;
                }
                Kind::Symlink => frame(digest, &self.base_symlink(record.content_root)?)?,
                Kind::Directory => {}
            }
        }
        Ok(attr.kind == Kind::Directory)
    }

    fn base_symlink(&self, root: ObjectId) -> Result<Vec<u8>> {
        Ok(CoreReader(&self.reader)
            .with_authenticated_canonical(
                root,
                layerfs_content::tree::directory::codec::decode_symlink,
            )?
            .target)
    }

    #[cfg(test)]
    fn base_manifest(&self) -> Result<BTreeMap<String, BaseEntry>> {
        let reader = CoreReader(&self.reader);
        let mut output = BTreeMap::new();
        let mut charge = 0_u64;
        let mut pending = vec![CanonicalPath::root()];
        while let Some(directory) = pending.pop() {
            let mut after = None;
            loop {
                let (page, _) = filesystem::list(
                    &reader,
                    self.base_root,
                    &directory,
                    after.as_ref(),
                    128,
                    256 * 1024,
                )?;
                layerfs_layerstack_store::note_workspace_namespace_visits(
                    page.entries.len() as u64,
                    0,
                    0,
                    0,
                    0,
                );
                for (name, _) in page.entries {
                    let path = join(&directory, name.as_str())?;
                    let resolved = filesystem::resolve(
                        &reader,
                        self.base_root,
                        &path,
                        &mut LogicalCounters::default(),
                    )?;
                    let metadata = portable_metadata(
                        &reader,
                        resolved.record.metadata_root,
                        resolved.record.kind,
                    )?;
                    if resolved.record.kind == InodeKind::Directory {
                        pending.push(path.clone());
                    }
                    let path = path.as_str().to_owned();
                    charge = charge.saturating_add(path_charge(&path));
                    output.insert(
                        path,
                        BaseEntry {
                            inode: resolved.inode,
                            record: resolved.record,
                            mode: metadata.permission_mode,
                            mtime_seconds: metadata.mtime_seconds,
                            mtime_nanoseconds: metadata.mtime_nanoseconds,
                        },
                    );
                    self.policy.check_final_delta(charge)?;
                }
                let Some(next) = page.continuation else { break };
                after = Some(next);
            }
        }
        Ok(output)
    }

    fn file_matches(&self, node: NodeId, base: ObjectId) -> Result<bool> {
        match &self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::File(FileData::Base { root, .. }) if root.0 == base => return Ok(true),
            Data::File(FileData::Edited {
                base: Some((root, base_len)),
                pieces,
                ..
            }) if root.0 == base
                && pieces.len() == *base_len
                && matches!(pieces.pieces().as_slice(), [crate::file_edit::Piece::Base { root: piece_root, offset: 0, len }] if *piece_root == *root && *len == *base_len) =>
            {
                return Ok(true)
            }
            Data::File(_) => {}
            _ => return Err(StorageError::InvalidInput("file")),
        }
        let mut final_digest = ContentDigestWriter::new();
        let mut input = WorkspaceFileReader::new(self, node)?;
        std::io::copy(&mut input, &mut final_digest)?;
        let mut base_digest = ContentDigestWriter::new();
        rope::read_all(
            &CoreReader(&self.reader),
            FileStateRoot(base),
            &mut base_digest,
        )?;
        Ok(final_digest.finish() == base_digest.finish())
    }

    fn base_record(&self, inode: InodeId) -> Result<InodeRecordV1> {
        let core = CoreReader(&self.reader);
        let id = inode_table_lookup(
            &core,
            self.base_inodes,
            inode,
            &mut InodeTableCounters::default(),
        )?
        .ok_or(StorageError::Integrity("Workspace base inode"))?;
        Ok(core.with_authenticated_canonical(id, decode_inode_record)?)
    }

    pub(crate) fn frontier_inode(&self, node: NodeId) -> Result<InodeId> {
        let value = self
            .nodes
            .get(&node)
            .ok_or(StorageError::Integrity("frontier node"))?;
        if let Some(inode) = value.canonical {
            return Ok(inode);
        }
        let path = value
            .paths
            .first()
            .ok_or(StorageError::Integrity("frontier path"))?;
        let batch_allowance = (self.policy.max_final_delta_memory_bytes / 4096).min(128) * 1024;
        self.policy
            .check_final_delta(batch_allowance.saturating_add(path_charge(path)))?;
        layerfs_layerstack_store::note_workspace_namespace_visits(0, 1, 0, 0, 0);
        // Bind new identity to this base snapshot: replacing one alias must not
        // accidentally reuse the still-live inode originally allocated at its path.
        Ok(filesystem::allocated_inode(
            self.base_root.to_bytes(),
            &CanonicalPath::new(path)?,
        ))
    }

    fn file_may_differ(&self, node: NodeId, base: ObjectId) -> Result<bool> {
        match &self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::File(FileData::Base { root, .. }) if root.0 == base => Ok(false),
            Data::File(FileData::Edited {
                base: Some((root, base_len)),
                pieces,
                ..
            }) if root.0 == base => Ok(pieces.len() != *base_len
                || !matches!(pieces.pieces().as_slice(), [crate::file_edit::Piece::Base { root: piece_root, offset: 0, len }] if *piece_root == *root && *len == *base_len)),
            Data::File(_) => Ok(!self.file_matches(node, base)?),
            _ => Err(StorageError::InvalidInput("file")),
        }
    }

    fn incremental_file_supported(&self, node: NodeId, base: ObjectId) -> bool {
        matches!(
            self.nodes.get(&node).map(|node| &node.data),
            Some(Data::File(FileData::Edited {
                base: Some((root, _)),
                ..
            })) if root.0 == base
        )
    }

    fn mutate_existing_file(
        &self,
        objects: &mut ObjectBuffer<'_>,
        node: NodeId,
        base: BaseEntry,
    ) -> Result<Option<(FileStateRoot, RopeCounters)>> {
        let Data::File(FileData::Edited {
            base: Some((file_root, _)),
            pieces,
            ..
        }) = &self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        else {
            return Ok(None);
        };
        if file_root.0 != base.record.content_root {
            return Ok(None);
        }
        let (file_root, pieces) = (*file_root, pieces.pieces());
        let mut batch = FileMutationBatch::new(objects, Some(file_root))?;
        let mut changed = false;
        let original_len = layerfs_content::file::rope::state(
            &CoreReader(&self.reader),
            file_root,
            &mut RopeCounters::default(),
        )?
        .logical_len;
        let mut base_cursor = 0_u64;
        let mut final_cursor = 0_u64;
        let mut replacement_len = 0_u64;
        for piece in pieces {
            match piece {
                crate::file_edit::Piece::Base { root, offset, len } => {
                    if root != file_root || offset < base_cursor {
                        return Err(StorageError::Integrity("Workspace base piece order"));
                    }
                    let delete_len = offset - base_cursor;
                    if (delete_len != 0 || replacement_len != 0)
                        && (delete_len != replacement_len
                            || final_cursor != base_cursor
                            || !self.workspace_range_matches_base(
                                node,
                                file_root,
                                final_cursor,
                                final_cursor + replacement_len,
                            )?)
                    {
                        batch.replace(
                            final_cursor,
                            delete_len,
                            WorkspaceRangeReader::new(self, node, final_cursor, replacement_len)?,
                        )?;
                        changed = true;
                    }
                    final_cursor += replacement_len + len;
                    replacement_len = 0;
                    base_cursor = offset + len;
                }
                piece => {
                    replacement_len = replacement_len
                        .checked_add(piece.len())
                        .ok_or(StorageError::InvalidInput("file length"))?
                }
            }
        }
        let delete_len = original_len
            .checked_sub(base_cursor)
            .ok_or(StorageError::Integrity("Workspace base piece order"))?;
        if (delete_len != 0 || replacement_len != 0)
            && (delete_len != replacement_len
                || final_cursor != base_cursor
                || !self.workspace_range_matches_base(
                    node,
                    file_root,
                    final_cursor,
                    final_cursor + replacement_len,
                )?)
        {
            batch.replace(
                final_cursor,
                delete_len,
                WorkspaceRangeReader::new(self, node, final_cursor, replacement_len)?,
            )?;
            changed = true;
        }
        if batch.logical_len()? != self.attr(node)?.size {
            return Err(StorageError::Integrity("Workspace file mutation length"));
        }
        if !changed {
            return Ok(None);
        }
        Ok(Some(batch.finish()?))
    }

    fn workspace_range_matches_base(
        &self,
        node: NodeId,
        base: FileStateRoot,
        start: u64,
        end: u64,
    ) -> Result<bool> {
        let mut offset = start;
        while offset < end {
            let count = (end - offset).min(64 * 1024) as usize;
            let final_bytes = self.read(node, offset, count)?;
            let mut base_bytes = Vec::with_capacity(count);
            rope::read_range(
                &CoreReader(&self.reader),
                base,
                offset..offset + count as u64,
                &mut base_bytes,
            )?;
            if final_bytes != base_bytes {
                return Ok(false);
            }
            offset += count as u64;
        }
        Ok(true)
    }
}

#[cfg(any(debug_assertions, feature = "test-instrumentation"))]
thread_local! {
    static INJECT_CANDIDATE_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(any(debug_assertions, feature = "test-instrumentation"))]
pub(crate) fn inject_candidate_failure_once() {
    INJECT_CANDIDATE_FAILURE.with(|inject| inject.set(true));
}

fn final_count(base: u64, change: i64) -> Result<u64> {
    base.checked_add_signed(change)
        .ok_or(StorageError::Integrity("namespace reference overflow"))
}

fn reference_delta(run: &crate::commit_spool::Run<41>, key: [u8; 33]) -> Result<i64> {
    let (mut low, mut high) = (0, run.count);
    while low < high {
        let mid = low + (high - low) / 2;
        if run.at(mid)?[..33] < key[..] {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    let mut delta = 0i64;
    while low < run.count {
        let record = run.at(low)?;
        if record[..33] != key {
            break;
        }
        delta = delta
            .checked_add(crate::references::delta(&record))
            .ok_or(StorageError::Integrity("namespace reference overflow"))?;
        low += 1;
    }
    Ok(delta)
}

fn push_inode(
    pairs: &mut crate::commit_spool::Sorter<65>,
    objects: &mut ObjectBuffer<'_>,
    inode: InodeId,
    record: Option<InodeRecordV1>,
) -> Result<()> {
    let mut pair = [0; 65];
    pair[..32].copy_from_slice(inode.as_bytes());
    if let Some(record) = record {
        pair[32] = 1;
        let id = objects.put_owned(layerfs_content::tree::inode::codec::encode_inode_record(
            record,
        )?)?;
        pair[33..].copy_from_slice(id.as_bytes());
    }
    pairs.push(pair)
}

fn note_commit_phase(phase: WorkspaceCommitPhase, started: Instant) {
    layerfs_layerstack_store::note_workspace_commit_phase(
        phase,
        started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
    );
}

fn frame(output: &mut impl Write, value: &[u8]) -> Result<()> {
    output.write_all(&(value.len() as u64).to_be_bytes())?;
    output.write_all(value)?;
    Ok(())
}

fn path_intersects(left: &str, right: &str) -> bool {
    left == right || path_is_ancestor(left, right) || path_is_ancestor(right, left)
}

fn path_is_ancestor(parent: &str, child: &str) -> bool {
    parent.is_empty()
        || (child.starts_with(parent) && child.as_bytes().get(parent.len()) == Some(&b'/'))
}

#[cfg(test)]
fn manifest_charge<T>(manifest: &BTreeMap<String, T>) -> u64 {
    manifest.keys().map(|path| path_charge(path)).sum()
}

fn path_charge(path: &str) -> u64 {
    (path.len() as u64).saturating_mul(4).saturating_add(512)
}

struct WorkspaceFileReader<'a> {
    source: WorkspaceFileSource<'a>,
    offset: u64,
    len: u64,
}

#[derive(Clone, Copy)]
enum WorkspaceFileSource<'a> {
    Direct(&'a File),
    Mixed {
        workspace: &'a Workspace,
        node: NodeId,
    },
}

struct WorkspaceRangeReader<'a> {
    workspace: &'a Workspace,
    node: NodeId,
    offset: u64,
    end: u64,
}

impl<'a> WorkspaceRangeReader<'a> {
    fn new(workspace: &'a Workspace, node: NodeId, offset: u64, len: u64) -> Result<Self> {
        let end = offset
            .checked_add(len)
            .ok_or(StorageError::InvalidInput("file range"))?;
        if end > workspace.attr(node)?.size {
            return Err(StorageError::InvalidInput("file range"));
        }
        Ok(Self {
            workspace,
            node,
            offset,
            end,
        })
    }
}

impl Read for WorkspaceRangeReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.offset == self.end || output.is_empty() {
            return Ok(0);
        }
        let bytes = self
            .workspace
            .read(
                self.node,
                self.offset,
                output.len().min((self.end - self.offset) as usize),
            )
            .map_err(std::io::Error::other)?;
        output[..bytes.len()].copy_from_slice(&bytes);
        self.offset += bytes.len() as u64;
        Ok(bytes.len())
    }
}

impl<'a> WorkspaceFileReader<'a> {
    fn new(workspace: &'a Workspace, node: NodeId) -> Result<Self> {
        let len = workspace.attr(node)?.size;
        let source = match &workspace
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::File(FileData::Edited {
                base: None,
                spool,
                spool_high_water,
                pieces,
                ..
            }) if *spool_high_water == len
                && pieces
                    .pieces()
                    .iter()
                    .try_fold(0_u64, |offset, piece| match piece {
                        crate::file_edit::Piece::Spool {
                            offset: source,
                            len,
                        } if *source == offset => offset.checked_add(*len),
                        _ => None,
                    })
                    == Some(len) =>
            {
                WorkspaceFileSource::Direct(workspace.spool_file(node, spool)?)
            }
            Data::File(_) => WorkspaceFileSource::Mixed { workspace, node },
            _ => return Err(StorageError::InvalidInput("file")),
        };
        Ok(Self {
            source,
            offset: 0,
            len,
        })
    }
}

impl Read for WorkspaceFileReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.offset == self.len || output.is_empty() {
            return Ok(0);
        }
        let count = output.len().min((self.len - self.offset) as usize);
        match self.source {
            WorkspaceFileSource::Direct(file) => {
                let mut read = 0;
                while read < count {
                    let next = file.read_at(&mut output[read..count], self.offset + read as u64)?;
                    if next == 0 {
                        return Err(std::io::ErrorKind::UnexpectedEof.into());
                    }
                    read += next;
                }
                self.offset += read as u64;
                Ok(read)
            }
            WorkspaceFileSource::Mixed { workspace, node } => {
                let bytes = workspace
                    .read(node, self.offset, count)
                    .map_err(std::io::Error::other)?;
                output[..bytes.len()].copy_from_slice(&bytes);
                self.offset += bytes.len() as u64;
                Ok(bytes.len())
            }
        }
    }
}

fn join(parent: &CanonicalPath, name: &str) -> Result<CanonicalPath> {
    let value = if parent.is_root() {
        name.to_owned()
    } else {
        format!("{}/{name}", parent.as_str())
    };
    CanonicalPath::new(&value).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_layerstack_store::{
        CommitOutcome, EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    };

    fn empty_workspace(label: &str) -> (std::path::PathBuf, Workspace) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-direct-spool-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new(label).unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspace = Workspace::open(store, branch, root.join("spool")).unwrap();
        (root, workspace)
    }

    #[test]
    fn resolution_fingerprint_is_local_and_materialization_independent() {
        let (root, mut workspace) = empty_workspace("fingerprint-local");
        let focus = workspace.mkdir(ROOT, b"focus", 0o755).unwrap().node;
        let target = workspace.create_file(focus, b"file", 0o644).unwrap().node;
        workspace.write(target, 0, b"target").unwrap();
        let other = workspace.create_file(ROOT, b"other", 0o644).unwrap().node;
        workspace.write(other, 0, b"other").unwrap();
        workspace.commit().unwrap();
        let mut fresh = Workspace::open(
            workspace.store.clone(),
            workspace.branch_id,
            root.join("fresh-spool"),
        )
        .unwrap();
        let paths = [CanonicalPath::new("focus").unwrap()];
        let before = fresh.resolution_fingerprint(&paths).unwrap();
        assert_eq!(
            fresh.nodes.len(),
            1,
            "fingerprint must not materialize a namespace manifest"
        );
        let focus = fresh.lookup_node(ROOT, b"focus").unwrap();
        let target = fresh.lookup_node(focus, b"file").unwrap();
        assert_eq!(fresh.resolution_fingerprint(&paths).unwrap(), before);
        let other = fresh.lookup_node(ROOT, b"other").unwrap();
        fresh.write(other, 0, b"unrelated").unwrap();
        assert_eq!(fresh.resolution_fingerprint(&paths).unwrap(), before);
        fresh.write(target, 0, b"changed").unwrap();
        assert_ne!(fresh.resolution_fingerprint(&paths).unwrap(), before);
        fresh.end_clean().unwrap();
        workspace.end_clean().unwrap();
        drop(fresh);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn structural_frontier_keeps_unrelated_namespace_out_of_budget() {
        let (root, empty) = empty_workspace("frontier");
        let store = empty.store.clone();
        drop(empty);
        let source = root.join("source");
        std::fs::create_dir_all(source.join("background")).unwrap();
        for index in 0..200 {
            std::fs::write(
                source.join(format!("background/file-{index:03}")),
                b"unchanged",
            )
            .unwrap();
        }
        for name in ["tree", "keep"] {
            std::fs::create_dir(source.join(name)).unwrap();
            std::fs::write(source.join(name).join("child"), name.as_bytes()).unwrap();
        }
        std::fs::hard_link(source.join("tree/child"), source.join("outside")).unwrap();
        std::fs::write(source.join("shared"), b"original alias").unwrap();
        std::fs::hard_link(source.join("shared"), source.join("hidden")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("fixture").unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("changed").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let mut workspace = Workspace::open_with_policy(
            store.clone(),
            branch,
            root.join("changed-spool"),
            crate::ResourcePolicy {
                max_final_delta_memory_bytes: 16 * 1024,
                ..crate::ResourcePolicy::default()
            },
        )
        .unwrap();
        // The former structural fallback cannot even hold this small fixture.
        assert!(matches!(
            workspace.base_manifest(),
            Err(StorageError::InvalidInput("workspace final-delta limit"))
        ));
        let before_root = workspace.base_root;
        let reader = store.snapshot_reader(before_root);
        let before = |path: &str| {
            filesystem::resolve(
                &CoreReader(&reader),
                before_root,
                &CanonicalPath::new(path).unwrap(),
                &mut LogicalCounters::default(),
            )
            .unwrap()
        };
        let background = before("background");
        let keep = before("keep");
        let old_shared = before("shared");
        let old_tree = before("tree");
        let old_child = before("tree/child");

        // Neither hidden alias is looked up: releasing only the known binding
        // must still retain its inode and content under the unseen name.
        workspace.unlink(ROOT, b"shared", false).unwrap();
        let replacement = workspace.create_file(ROOT, b"shared", 0o600).unwrap();
        workspace
            .write(replacement.node, 0, b"replacement")
            .unwrap();
        workspace
            .rename(ROOT, b"tree", ROOT, b"moved", false)
            .unwrap();
        let moved = workspace.lookup(ROOT, b"moved").unwrap();
        workspace.unlink(moved.node, b"child", false).unwrap();
        workspace.unlink(ROOT, b"moved", true).unwrap();
        workspace
            .rename(ROOT, b"keep", ROOT, b"kept", false)
            .unwrap();
        let added = workspace.create_file(ROOT, b"added", 0o640).unwrap();
        workspace.write(added.node, 0, b"linked payload").unwrap();
        workspace.link(added.node, ROOT, b"added-alias").unwrap();
        workspace.unlink(ROOT, b"added", false).unwrap();
        for index in 0..120 {
            workspace
                .create_file(ROOT, format!("new-{index:03}").as_bytes(), 0o600)
                .unwrap();
        }
        workspace.commit().unwrap();
        let final_root = workspace.base_root;
        let reader = store.snapshot_reader(final_root);
        let core = CoreReader(&reader);
        let resolve = |path: &str| {
            filesystem::resolve(
                &core,
                final_root,
                &CanonicalPath::new(path).unwrap(),
                &mut LogicalCounters::default(),
            )
            .unwrap()
        };
        assert_eq!(resolve("background").record, background.record);
        assert_eq!(resolve("kept").inode, keep.inode);
        assert_eq!(resolve("kept").record, keep.record);
        assert_eq!(resolve("hidden").inode, old_shared.inode);
        assert_eq!(resolve("hidden").record.namespace_ref_count, 1);
        assert_ne!(resolve("shared").inode, old_shared.inode);
        assert_eq!(resolve("outside").inode, old_child.inode);
        assert_eq!(resolve("outside").record.namespace_ref_count, 1);
        for (path, expected) in [
            ("hidden", b"original alias".as_slice()),
            ("shared", b"replacement"),
            ("outside", b"tree"),
            ("kept/child", b"keep"),
            ("added-alias", b"linked payload"),
        ] {
            let mut bytes = Vec::new();
            filesystem::stream(
                &core,
                final_root,
                &CanonicalPath::new(path).unwrap(),
                &mut bytes,
            )
            .unwrap();
            assert_eq!(bytes, expected, "{path}");
        }
        for index in 0..120 {
            assert_eq!(
                resolve(&format!("new-{index:03}"))
                    .record
                    .namespace_ref_count,
                1
            );
        }
        let namespace = filesystem::namespace(&core, final_root).unwrap();
        let table = layerfs_content::tree::inode::InodeTableRoot(namespace.inode_table_root);
        assert!(inode_table_lookup(
            &core,
            table,
            old_tree.inode,
            &mut InodeTableCounters::default()
        )
        .unwrap()
        .is_none());
        let entries = layerfs_content::tree::inode::inode_table_entries(
            &core,
            table,
            &mut InodeTableCounters::default(),
        )
        .unwrap();
        // root + background directory/files + kept directory/file + outside +
        // hidden + replacement + added-alias + 120 new files; no orphan inodes.
        assert_eq!(entries.len(), 328);
        // A valid long path still must fit the transient planner allocation,
        // together with its pending inode batch, under a custom small policy.
        workspace.policy.max_final_delta_memory_bytes = 4096;
        let name = "x".repeat(250);
        let mut parent = ROOT;
        for _ in 0..4 {
            parent = workspace
                .mkdir(parent, name.as_bytes(), 0o700)
                .unwrap()
                .node;
        }
        let path = workspace.nodes[&parent].paths.first().unwrap();
        assert!(CanonicalPath::new(path).is_ok());
        assert!(matches!(
            workspace.frontier_inode(parent),
            Err(StorageError::InvalidInput("workspace final-delta limit"))
        ));
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clean_workspace_commit_is_immediately_up_to_date() {
        let (root, mut workspace) = empty_workspace("clean-commit");
        let base_root = workspace.base_root;
        let (outcome, transition) = workspace.commit().unwrap();
        assert!(matches!(
            outcome,
            CommitOutcome::UpToDate { root_id } if root_id == base_root
        ));
        assert_eq!(transition, crate::lifecycle::CommitTransition::Rebased);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fully_charged_new_file_reads_directly_from_retained_spool() {
        let (root, mut workspace) = empty_workspace("direct-reader");
        let data = (0..128 * 1024)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let file = workspace.create_file(ROOT, b"full", 0o600).unwrap();
        workspace.write(file.node, 0, &data).unwrap();
        let mut reader = WorkspaceFileReader::new(&workspace, file.node).unwrap();
        assert!(matches!(reader.source, WorkspaceFileSource::Direct(_)));
        let mut actual = Vec::new();
        reader.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, data);

        let sparse = workspace.create_file(ROOT, b"sparse", 0o600).unwrap();
        workspace.write(sparse.node, 4096, b"x").unwrap();
        assert!(matches!(
            WorkspaceFileReader::new(&workspace, sparse.node)
                .unwrap()
                .source,
            WorkspaceFileSource::Mixed { .. }
        ));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sequential_new_file_capture_is_canonical_and_commit_ready() {
        let (root, mut workspace) = empty_workspace("streaming-capture");
        let data = (0..2 * 1024 * 1024)
            .map(|index| ((index * 29 + index / 11) % 251) as u8)
            .collect::<Vec<_>>();
        let file = workspace.create_file(ROOT, b"payload", 0o600).unwrap();
        for (index, chunk) in data.chunks(64 * 1024).enumerate() {
            workspace
                .write(file.node, (index * 64 * 1024) as u64, chunk)
                .unwrap();
        }
        workspace.fsync(Some(file.node)).unwrap();
        let captured_root = match &workspace.capture {
            crate::capture::CaptureState::Ready(captured) => captured.root,
            _ => panic!("sequential capture did not finish"),
        };

        let store = workspace.store.clone();
        let branch = store.branch(workspace.branch_id).unwrap().unwrap();
        let built = workspace.build_candidate().unwrap();
        let outcome = store
            .commit_candidate(&branch, workspace.base_root, workspace.expected_base, built)
            .unwrap();
        let CommitOutcome::Committed { root_id, .. } = outcome else {
            panic!("capture Commit was not created")
        };
        let reader = store.snapshot_reader(root_id);
        let resolved = filesystem::resolve(
            &CoreReader(&reader),
            root_id,
            &CanonicalPath::new("payload").unwrap(),
            &mut LogicalCounters::default(),
        )
        .unwrap();
        assert_eq!(resolved.record.content_root, captured_root.0);
        let mut actual = Vec::new();
        filesystem::stream(
            &CoreReader(&reader),
            root_id,
            &CanonicalPath::new("payload").unwrap(),
            &mut actual,
        )
        .unwrap();
        assert_eq!(actual, data);

        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backward_write_invalidates_capture_and_uses_exact_fallback() {
        let (root, mut workspace) = empty_workspace("capture-fallback");
        let mut expected = vec![17; 128 * 1024];
        let file = workspace.create_file(ROOT, b"payload", 0o600).unwrap();
        workspace.write(file.node, 0, &expected).unwrap();
        workspace.write(file.node, 4096, b"backward10").unwrap();
        expected[4096..4106].copy_from_slice(b"backward10");
        assert!(matches!(
            workspace.capture,
            crate::capture::CaptureState::Invalid
        ));

        let store = workspace.store.clone();
        let branch = store.branch(workspace.branch_id).unwrap().unwrap();
        let built = workspace.build_candidate().unwrap();
        let outcome = store
            .commit_candidate(&branch, workspace.base_root, workspace.expected_base, built)
            .unwrap();
        let CommitOutcome::Committed { root_id, .. } = outcome else {
            panic!("fallback Commit was not created")
        };
        let reader = store.snapshot_reader(root_id);
        let mut actual = Vec::new();
        filesystem::stream(
            &CoreReader(&reader),
            root_id,
            &CanonicalPath::new("payload").unwrap(),
            &mut actual,
        )
        .unwrap();
        assert_eq!(actual, expected);

        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_file_mutations_are_exact_and_bounded() {
        let base = (0..1024 * 1024)
            .map(|index| ((index * 31 + index / 7) % 251) as u8)
            .collect::<Vec<_>>();
        for case in ["overwrite", "noop", "append", "shrink", "grow", "rename"] {
            let root = std::env::temp_dir().join(format!(
                "layerfs-incremental-{case}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let source = root.join("source");
            std::fs::create_dir_all(&source).unwrap();
            std::fs::write(source.join("payload"), &base).unwrap();
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
                    EntityName::new(case).unwrap(),
                    LocalForkSource::Layer { layer_id: layer },
                )
                .unwrap();
            let policy = if case == "overwrite" {
                crate::ResourcePolicy {
                    max_final_delta_memory_bytes: 1024,
                    ..crate::ResourcePolicy::default()
                }
            } else {
                crate::ResourcePolicy::default()
            };
            let mut workspace =
                Workspace::open_with_policy(store.clone(), branch, root.join("spool"), policy)
                    .unwrap();
            let file = workspace.lookup(ROOT, b"payload").unwrap().node;
            let original_inode = workspace.nodes[&file].canonical.unwrap();
            let mut expected = base.clone();
            let (expected_cdc, expected_path) = match case {
                "overwrite" => {
                    let offset = 543_219;
                    workspace.write(file, offset, b"changed-10").unwrap();
                    expected[offset as usize..offset as usize + 10].copy_from_slice(b"changed-10");
                    (10, "payload")
                }
                "noop" => {
                    let offset = 123_457;
                    workspace
                        .write(file, offset, &base[offset as usize..offset as usize + 10])
                        .unwrap();
                    (0, "payload")
                }
                "append" => {
                    workspace
                        .write(file, base.len() as u64, b"append-010")
                        .unwrap();
                    expected.extend_from_slice(b"append-010");
                    (10, "payload")
                }
                "shrink" => {
                    workspace.truncate(file, base.len() as u64 - 4096).unwrap();
                    expected.truncate(base.len() - 4096);
                    (0, "payload")
                }
                "grow" => {
                    workspace.truncate(file, base.len() as u64 + 4096).unwrap();
                    expected.resize(base.len() + 4096, 0);
                    (4096, "payload")
                }
                "rename" => {
                    workspace
                        .rename(ROOT, b"payload", ROOT, b"renamed", false)
                        .unwrap();
                    (0, "renamed")
                }
                _ => unreachable!(),
            };
            let built = workspace.build_candidate().unwrap();
            assert_eq!(built.counters.cdc_bytes_scanned, expected_cdc, "{case}");
            assert!(built.objects.encoded_bytes() < 256 * 1024, "{case}");
            if case == "noop" {
                assert_eq!(built.root_id, workspace.base_root);
                assert!(built.objects.is_empty());
            }
            let candidate_root = built.root_id;
            let record = store.branch(branch).unwrap().unwrap();
            let outcome = store
                .commit_candidate(&record, workspace.base_root, workspace.expected_base, built)
                .unwrap();
            assert_eq!(
                matches!(outcome, CommitOutcome::UpToDate { .. }),
                case == "noop"
            );
            let reader = store.snapshot_reader(candidate_root);
            let mut actual = Vec::new();
            let resolved = filesystem::resolve(
                &CoreReader(&reader),
                candidate_root,
                &CanonicalPath::new(expected_path).unwrap(),
                &mut LogicalCounters::default(),
            )
            .unwrap();
            if case == "rename" {
                assert_eq!(resolved.inode, original_inode);
                assert!(filesystem::resolve(
                    &CoreReader(&reader),
                    candidate_root,
                    &CanonicalPath::new("payload").unwrap(),
                    &mut LogicalCounters::default(),
                )
                .is_err());
            }
            filesystem::stream(
                &CoreReader(&reader),
                candidate_root,
                &CanonicalPath::new(expected_path).unwrap(),
                &mut actual,
            )
            .unwrap();
            assert_eq!(actual, expected, "{case}");

            drop(workspace);
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn owner_prepend_scans_only_replacement_and_retains_every_base_payload() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-owner-prepend-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("payload"), vec![0x5a; 1024 * 1024]).unwrap();
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
        let mut workspace = Workspace::open(store.clone(), branch, root.join("spool")).unwrap();
        let file = workspace.lookup(ROOT, b"payload").unwrap().node;
        let base_root = match workspace.nodes[&file].data {
            Data::File(FileData::Base { root, .. }) => root,
            _ => panic!("base file"),
        };
        let mut base_payloads = BTreeSet::new();
        rope::visit_extents(&CoreReader(&workspace.reader), base_root, |extents| {
            base_payloads.extend(extents.iter().map(|extent| extent.payload_object_id));
            Ok(())
        })
        .unwrap();
        workspace
            .edit_many(
                file,
                vec![(
                    0,
                    0,
                    crate::WorkspaceFileReplacement::Inline(b"PREPEND010".to_vec()),
                )],
            )
            .unwrap();
        let reads_before = workspace.reader.read_metrics_snapshot().unwrap();
        let built = workspace.build_candidate().unwrap();
        let reads_after = workspace.reader.read_metrics_snapshot().unwrap();
        assert_eq!(built.counters.cdc_bytes_scanned, 10);
        assert_eq!(
            reads_after.payload_bytes_read - reads_before.payload_bytes_read,
            0
        );
        let record = store.branch(branch).unwrap().unwrap();
        let outcome = store
            .commit_candidate(&record, workspace.base_root, workspace.expected_base, built)
            .unwrap();
        let CommitOutcome::Committed { root_id, .. } = outcome else {
            panic!("prepend commit")
        };
        let reader = store.snapshot_reader(root_id);
        let resolved = filesystem::resolve(
            &CoreReader(&reader),
            root_id,
            &CanonicalPath::new("payload").unwrap(),
            &mut LogicalCounters::default(),
        )
        .unwrap();
        let mut final_payloads = BTreeSet::new();
        rope::visit_extents(
            &CoreReader(&reader),
            FileStateRoot(resolved.record.content_root),
            |extents| {
                final_payloads.extend(extents.iter().map(|extent| extent.payload_object_id));
                Ok(())
            },
        )
        .unwrap();
        assert!(base_payloads.is_subset(&final_payloads));
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
