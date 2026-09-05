use crate::cow_tree::{portable_metadata, Attr, Data, FileData, Kind, NodeId, Workspace, ROOT};
use layerfs_content::file::rope::{
    self, FileMutationBatch, FileStateRoot, ObjectStore, RopeCounters,
};
use layerfs_content::filesystem::{
    self, ContentChange, InodeMutation, LogicalCounters, PortableMetadataCache,
};
use layerfs_content::object::access::ObjectRead;
use layerfs_content::object::{ContentDigestWriter, ObjectId};
use layerfs_content::tree::batch::{
    directory_apply_sorted_with_budget, inode_table_apply_sorted_with_budget,
    SORTED_TREE_UPDATE_SCRATCH_BYTES,
};
use layerfs_content::tree::directory::codec::encode_namespace_root;
use layerfs_content::tree::directory::{
    directory_lookup, directory_page_after, empty_directory, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_content::tree::inode::codec::{decode_inode_record, encode_inode_record};
use layerfs_content::tree::inode::{inode_table_lookup, InodeTableCounters};
use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1, InodeTableRoot};
use layerfs_content::tree::NamespaceRootV1;
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
struct FinalEntry {
    node: NodeId,
    attr: Attr,
}

impl Workspace {
    pub(crate) fn build_candidate(&mut self) -> Result<BuiltRoot> {
        #[cfg(any(debug_assertions, feature = "test-instrumentation"))]
        if INJECT_CANDIDATE_FAILURE.with(|inject| inject.replace(false)) {
            return Err(StorageError::Integrity(
                "injected Workspace candidate failure",
            ));
        }
        if let Some(candidate) = self.build_localized_candidate()? {
            return Ok(candidate);
        }
        let captured = self.take_capture();
        if let Some(captured) = &captured {
            layerfs_layerstack_store::note_workspace_capture(1, captured.len);
        }
        let started = Instant::now();
        let base = self.base_manifest()?;
        let final_view = self.final_manifest(manifest_charge(&base))?;
        let base_groups = groups_by_inode(&base);
        let final_groups = groups_by_node(&final_view);
        let mut recreate = BTreeSet::new();
        let mut rewrite = BTreeSet::new();
        let mut renames = BTreeMap::new();
        note_commit_phase(WorkspaceCommitPhase::CandidatePlan, started);

        let started = Instant::now();
        for (node, paths) in &final_groups {
            let entry = final_view
                .get(&paths[0])
                .ok_or(StorageError::Integrity("final manifest"))?;
            if let Some(inode) = self.nodes.get(node).and_then(|node| node.canonical) {
                if let Some(before_paths) = base_groups.get(&inode) {
                    let removed = before_paths
                        .iter()
                        .filter(|path| !paths.contains(path))
                        .collect::<Vec<_>>();
                    let added = paths
                        .iter()
                        .filter(|path| !before_paths.contains(path))
                        .collect::<Vec<_>>();
                    if before_paths.len() == paths.len() && removed.len() == 1 && added.len() == 1 {
                        let before = base
                            .get(removed[0])
                            .ok_or(StorageError::Integrity("rename source"))?;
                        let content_matches = match entry.attr.kind {
                            Kind::File => {
                                !self.file_may_differ(entry.node, before.record.content_root)?
                            }
                            Kind::Symlink => {
                                self.readlink(entry.node)?
                                    == self.base_symlink(before.record.content_root)?
                            }
                            Kind::Directory => false,
                        };
                        if kind(before.record.kind) == entry.attr.kind
                            && before.mode == entry.attr.mode
                            && before.mtime_seconds == entry.attr.mtime_seconds
                            && before.mtime_nanoseconds == entry.attr.mtime_nanoseconds
                            && content_matches
                        {
                            renames.insert(*node, (removed[0].clone(), added[0].clone()));
                            continue;
                        }
                    }
                }
            }
            let exact_base = paths
                .first()
                .and_then(|path| base.get(path))
                .filter(|first| {
                    paths.iter().all(|path| {
                        base.get(path).is_some_and(|candidate| {
                            candidate.inode == first.inode
                                && kind(candidate.record.kind) == entry.attr.kind
                        })
                    })
                })
                .filter(|first| {
                    self.nodes.get(node).and_then(|node| node.canonical) == Some(first.inode)
                })
                .filter(|first| base_groups.get(&first.inode) == Some(paths));
            let Some(base_entry) = exact_base else {
                recreate.insert(*node);
                continue;
            };
            match entry.attr.kind {
                Kind::File
                    if self.file_may_differ(entry.node, base_entry.record.content_root)? =>
                {
                    rewrite.insert(*node);
                }
                Kind::Symlink
                    if self.readlink(entry.node)?
                        != self.base_symlink(base_entry.record.content_root)? =>
                {
                    recreate.insert(*node);
                }
                _ => {}
            }
        }
        note_commit_phase(WorkspaceCommitPhase::DirtyCompare, started);

        let started = Instant::now();
        let seed = *filesystem::namespace(&CoreReader(&self.reader), self.base_root)?
            .root_directory_inode
            .as_bytes();
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
        let mut root = self.base_root;
        let mut cdc_bytes_scanned = 0_u64;
        let renamed_sources = renames
            .values()
            .map(|(source, _)| source.as_str())
            .collect::<BTreeSet<_>>();
        let mut removals = base
            .iter()
            .filter_map(|(path, before)| match final_view.get(path) {
                None if !renamed_sources.contains(path.as_str()) => Some(path.clone()),
                None => None,
                Some(after)
                    if kind(before.record.kind) != after.attr.kind
                        || recreate.contains(&after.node)
                        || (renames.contains_key(&after.node)
                            && self.nodes[&after.node].canonical != Some(before.inode)) =>
                {
                    Some(path.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        removals
            .sort_by(|left, right| depth(right).cmp(&depth(left)).then_with(|| right.cmp(left)));
        for path in removals {
            root = apply_one(
                &mut objects,
                root,
                ContentChange::Remove { path },
                seed,
                &mut cdc_bytes_scanned,
            )?;
        }

        for (source, target) in renames.values() {
            let source = CanonicalPath::new(source)?;
            let target = CanonicalPath::new(target)?;
            let mut counters = LogicalCounters::default();
            let (source_parent, _) =
                filesystem::resolve_parent(&objects, root, &source, &mut counters)?;
            let (target_parent, _) =
                filesystem::resolve_parent(&objects, root, &target, &mut counters)?;
            root = filesystem::rename(
                &mut objects,
                root,
                &source,
                &target,
                source_parent.record.metadata_root,
                target_parent.record.metadata_root,
            )?
            .root();
        }

        let mut directories = final_view
            .iter()
            .filter(|(path, entry)| {
                entry.attr.kind == Kind::Directory
                    && !base
                        .get(*path)
                        .is_some_and(|before| kind(before.record.kind) == Kind::Directory)
            })
            .map(|(path, entry)| (path.clone(), *entry))
            .collect::<Vec<_>>();
        directories.sort_by(|left, right| {
            depth(&left.0)
                .cmp(&depth(&right.0))
                .then_with(|| left.0.cmp(&right.0))
        });
        for (path, entry) in directories {
            root = apply_one(
                &mut objects,
                root,
                ContentChange::Mkdir {
                    path: path.clone(),
                    mode: entry.attr.mode,
                },
                seed,
                &mut cdc_bytes_scanned,
            )?;
            root = set_metadata(&mut objects, root, &path, entry.attr)?;
        }
        note_commit_phase(WorkspaceCommitPhase::Namespace, started);

        let groups = final_groups
            .iter()
            .map(|(node, paths)| (paths[0].clone(), (*node, paths)))
            .collect::<BTreeMap<_, _>>();
        for (representative, (node, paths)) in groups {
            let entry = *final_view
                .get(&representative)
                .ok_or(StorageError::Integrity("final manifest"))?;
            if recreate.contains(&node) {
                let started = Instant::now();
                root = self.create_group(
                    &mut objects,
                    root,
                    &representative,
                    paths,
                    entry,
                    captured,
                    seed,
                    &mut cdc_bytes_scanned,
                )?;
                note_commit_phase(WorkspaceCommitPhase::Content, started);
            } else {
                if rewrite.contains(&node) {
                    let before = base
                        .get(&representative)
                        .ok_or(StorageError::Integrity("base hard-link group"))?;
                    let started = Instant::now();
                    match self.mutate_existing_file(&mut objects, node, *before)? {
                        Some((content, counters)) => {
                            cdc_bytes_scanned = cdc_bytes_scanned
                                .checked_add(counters.cdc_bytes_scanned)
                                .ok_or(StorageError::Integrity("CDC counter"))?;
                            root = filesystem::apply_inode_mutations(
                                &mut objects,
                                root,
                                [InodeMutation::Upsert {
                                    inode: before.inode,
                                    record: InodeRecordV1 {
                                        content_root: content.0,
                                        ..before.record
                                    },
                                }],
                            )?
                            .root();
                        }
                        None if self
                            .incremental_file_supported(node, before.record.content_root) => {}
                        None => {
                            let candidate = filesystem::write_file(
                                &mut objects,
                                root,
                                &CanonicalPath::new(&representative)?,
                                WorkspaceFileReader::new(self, node)?,
                                entry.attr.mode,
                                seed,
                            )?;
                            cdc_bytes_scanned = cdc_bytes_scanned
                                .checked_add(candidate.counters().rope.cdc_bytes_scanned)
                                .ok_or(StorageError::Integrity("CDC counter"))?;
                            root = candidate.root();
                        }
                    }
                    note_commit_phase(WorkspaceCommitPhase::Content, started);
                }
                let before_path = renames
                    .get(&node)
                    .map_or(representative.as_str(), |(source, _)| source.as_str());
                let before = base
                    .get(before_path)
                    .ok_or(StorageError::Integrity("base hard-link group"))?;
                if before.mode != entry.attr.mode
                    || before.mtime_seconds != entry.attr.mtime_seconds
                    || before.mtime_nanoseconds != entry.attr.mtime_nanoseconds
                {
                    let started = Instant::now();
                    root = set_metadata(&mut objects, root, &representative, entry.attr)?;
                    note_commit_phase(WorkspaceCommitPhase::Namespace, started);
                }
            }
        }

        let started = Instant::now();
        let base_root = filesystem::resolve(
            &CoreReader(&self.reader),
            self.base_root,
            &CanonicalPath::root(),
            &mut LogicalCounters::default(),
        )?;
        let base_root_metadata = portable_metadata(
            &CoreReader(&self.reader),
            base_root.record.metadata_root,
            base_root.record.kind,
        )?;
        let final_root = self.attr(ROOT)?;
        if base_root_metadata.permission_mode != final_root.mode
            || base_root_metadata.mtime_seconds != final_root.mtime_seconds
            || base_root_metadata.mtime_nanoseconds != final_root.mtime_nanoseconds
        {
            let path = CanonicalPath::root();
            root = filesystem::set_mode(&mut objects, root, &path, final_root.mode)?.root();
            root = filesystem::set_mtime(
                &mut objects,
                root,
                &path,
                final_root.mtime_seconds,
                final_root.mtime_nanoseconds,
            )?
            .root();
        }
        note_commit_phase(WorkspaceCommitPhase::Namespace, started);

        let started = Instant::now();
        let built = objects.finish(root, cdc_bytes_scanned);
        note_commit_phase(WorkspaceCommitPhase::CandidateFinish, started);
        built
    }

    // Directory overlays already are the final binding delta. Applying their inode
    // edges directly preserves untouched subtrees, including a renamed directory,
    // without building either complete namespace manifest.
    fn build_frontier_candidate(&mut self) -> Result<BuiltRoot> {
        let started = Instant::now();
        self.policy.check_final_delta(4096)?;
        let batch_size = (self.policy.max_final_delta_memory_bytes / 4096).min(128) as usize;
        let tree_scratch = usize::try_from(
            self.policy
                .max_final_delta_memory_bytes
                .saturating_sub(4096),
        )
        .unwrap_or(usize::MAX)
        .min(SORTED_TREE_UPDATE_SCRATCH_BYTES);
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
        let mut inodes = FrontierInodes::new(self.base_root, batch_size, tree_scratch);
        let mut metadata_cache = PortableMetadataCache::default();
        let mut cdc_bytes_scanned = 0_u64;
        note_commit_phase(WorkspaceCommitPhase::CandidatePlan, started);
        let started = Instant::now();
        for (&node, value) in &self.nodes {
            layerfs_layerstack_store::note_workspace_namespace_visits(0, 0, 0, 0, 1);
            if value.paths.is_empty() {
                continue;
            }
            let inode = self.frontier_inode(node)?;
            let before = match value.canonical {
                Some(_) => Some(inodes.record(&objects, inode)?),
                None => None,
            };
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
                    for desired in directory.changes.values() {
                        layerfs_layerstack_store::note_workspace_namespace_visits(
                            0,
                            u64::from(desired.is_some()),
                            0,
                            0,
                            0,
                        );
                    }
                    content = self.apply_frontier_directory(
                        &mut objects,
                        content,
                        &directory.changes,
                        batch_size,
                        tree_scratch,
                    )?;
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
                metadata_cache
                    .get_or_build(
                        &mut objects,
                        inode_kind,
                        attr.mode,
                        attr.mtime_seconds,
                        attr.mtime_nanoseconds,
                    )?
                    .0
            };
            let record = InodeRecordV1 {
                kind: inode_kind,
                content_root,
                metadata_root,
                namespace_ref_count: before.map_or(0, |record| record.namespace_ref_count),
            };
            layerfs_layerstack_store::note_workspace_namespace_visits(
                0,
                0,
                u64::from(before != Some(record)),
                u64::from(before == Some(record)),
                0,
            );
            if before != Some(record) {
                inodes.set(&mut objects, inode, Some(record))?;
            }
        }
        note_commit_phase(WorkspaceCommitPhase::Content, started);
        let started = Instant::now();
        // Add all final edges before releasing old ones. A move therefore never
        // destroys the moved inode, and aliases outside the overlay retain their
        // original references even when they were never materialized in Workspace.
        for additions in [true, false] {
            for value in self.nodes.values().filter(|value| !value.paths.is_empty()) {
                let Data::Directory(directory) = &value.data else {
                    continue;
                };
                for (name, desired) in &directory.changes {
                    let name = CanonicalName::from_bytes(name)?;
                    let before = match directory.base {
                        Some(base) => directory_lookup(
                            &CoreReader(&self.reader),
                            base,
                            &name,
                            &mut NamespaceCounters::default(),
                        )?,
                        None => None,
                    };
                    let after = desired.map(|node| self.frontier_inode(node)).transpose()?;
                    layerfs_layerstack_store::note_workspace_namespace_visits(
                        u64::from(before.is_some()),
                        u64::from(after.is_some()),
                        0,
                        0,
                        0,
                    );
                    if before == after {
                        continue;
                    }
                    if additions {
                        if let Some(inode) = after {
                            let mut record = inodes.record(&objects, inode)?;
                            record.namespace_ref_count = record
                                .namespace_ref_count
                                .checked_add(1)
                                .ok_or(StorageError::Integrity("namespace reference overflow"))?;
                            inodes.set(&mut objects, inode, Some(record))?;
                        }
                    } else if let Some(inode) = before {
                        inodes.release(
                            &mut objects,
                            inode,
                            self.policy.max_final_delta_memory_bytes,
                        )?;
                    }
                }
            }
        }
        inodes.flush(&mut objects)?;
        note_commit_phase(WorkspaceCommitPhase::Namespace, started);
        let started = Instant::now();
        let built = objects.finish(inodes.root, cdc_bytes_scanned);
        note_commit_phase(WorkspaceCommitPhase::CandidateFinish, started);
        built
    }

    fn apply_frontier_directory(
        &self,
        objects: &mut ObjectBuffer<'_>,
        root: DirectoryStateRoot,
        changes: &BTreeMap<Vec<u8>, Option<NodeId>>,
        batch_size: usize,
        scratch_limit: usize,
    ) -> Result<DirectoryStateRoot> {
        let mut source_error = None;
        let deltas = changes.iter().map(|(name, desired)| {
            let result: Result<_> = (|| {
                Ok((
                    CanonicalName::from_bytes(name)?,
                    desired
                        .map(|child| self.frontier_inode(child))
                        .transpose()?,
                ))
            })();
            match result {
                Ok(delta) => Ok(delta),
                Err(error) => {
                    source_error = Some(error);
                    Err(layerfs_content::CoreError::InvalidRecord(
                        "Workspace directory delta",
                    ))
                }
            }
        });
        let sorted = directory_apply_sorted_with_budget(objects, root, deltas, scratch_limit);
        if let Some(error) = source_error {
            return Err(error);
        }
        match sorted {
            Ok((root, _)) => Ok(root),
            Err(
                layerfs_content::CoreError::ObjectLimitExceeded
                | layerfs_content::CoreError::Unsupported,
            ) => {
                let mut root = root;
                let mut batch = Vec::with_capacity(batch_size);
                for (name, desired) in changes {
                    batch.push((
                        CanonicalName::from_bytes(name)?,
                        desired
                            .map(|child| self.frontier_inode(child))
                            .transpose()?,
                    ));
                    if batch.len() == batch_size {
                        root =
                            filesystem::apply_directory_changes(objects, root, batch.drain(..))?.0;
                    }
                }
                if !batch.is_empty() {
                    root = filesystem::apply_directory_changes(objects, root, batch)?.0;
                }
                Ok(root)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn frontier_inode(&self, node: NodeId) -> Result<InodeId> {
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

    fn build_localized_candidate(&mut self) -> Result<Option<BuiltRoot>> {
        let started = Instant::now();
        if self.nodes.values().inspect(|_| layerfs_layerstack_store::note_workspace_namespace_visits(0, 0, 0, 0, 1)).any(|node| {
            matches!(&node.data, Data::Directory(directory) if !directory.changes.is_empty())
        }) {
            return self.build_frontier_candidate().map(Some);
        }
        let charge = self
            .mutation_paths
            .keys()
            .map(|path| path_charge(path))
            .sum();
        if charge > self.policy.max_final_delta_memory_bytes {
            // Large content-only deltas use the same bounded inode batches as
            // structural changes, without materializing an all-path plan first.
            return self.build_frontier_candidate().map(Some);
        }
        self.policy.check_final_delta(charge)?;
        let mut changed = BTreeSet::new();
        for path in self.mutation_paths.keys() {
            let Some(node) = self
                .nodes
                .iter()
                .inspect(|_| {
                    layerfs_layerstack_store::note_workspace_namespace_visits(0, 0, 0, 0, 1)
                })
                .find_map(|(node, value)| value.paths.contains(path).then_some(*node))
            else {
                return Ok(None);
            };
            if self.nodes[&node].canonical.is_none() {
                return Ok(None);
            }
            changed.insert(node);
        }
        if changed.is_empty() || self.dirty.iter().any(|node| !changed.contains(node)) {
            return Ok(None);
        }
        note_commit_phase(WorkspaceCommitPhase::CandidatePlan, started);

        let captured = self.take_capture();
        if let Some(captured) = &captured {
            layerfs_layerstack_store::note_workspace_capture(1, captured.len);
        }
        let started = Instant::now();
        let reader = CoreReader(&self.reader);
        let mut entries = Vec::with_capacity(changed.len());
        for node in changed {
            layerfs_layerstack_store::note_workspace_namespace_visits(
                0,
                0,
                u64::from(self.dirty.contains(&node)),
                u64::from(!self.dirty.contains(&node)),
                0,
            );
            let inode = self.nodes[&node]
                .canonical
                .ok_or(StorageError::Integrity("localized inode"))?;
            let record_id = inode_table_lookup(
                &reader,
                self.base_inodes,
                inode,
                &mut InodeTableCounters::default(),
            )?
            .ok_or(StorageError::Integrity("localized inode record"))?;
            let record = reader.with_authenticated_canonical(record_id, decode_inode_record)?;
            let metadata = portable_metadata(&reader, record.metadata_root, record.kind)?;
            let attr = self.attr(node)?;
            if kind(record.kind) != attr.kind {
                return Err(StorageError::Integrity("localized inode kind"));
            }
            entries.push((
                node,
                attr,
                BaseEntry {
                    inode,
                    record,
                    mode: metadata.permission_mode,
                    mtime_seconds: metadata.mtime_seconds,
                    mtime_nanoseconds: metadata.mtime_nanoseconds,
                },
            ));
        }
        note_commit_phase(WorkspaceCommitPhase::DirtyCompare, started);

        let started = Instant::now();
        let mut objects = ObjectBuffer::new(&self.reader)?;
        let mut mutations = Vec::new();
        let mut metadata_cache = PortableMetadataCache::default();
        let mut cdc_bytes_scanned = 0_u64;
        for (node, attr, base) in entries {
            let mut record = base.record;
            if attr.kind == Kind::File && self.file_may_differ(node, record.content_root)? {
                match self.mutate_existing_file(&mut objects, node, base)? {
                    Some((content, counters)) => {
                        record.content_root = content.0;
                        cdc_bytes_scanned = cdc_bytes_scanned
                            .checked_add(counters.cdc_bytes_scanned)
                            .ok_or(StorageError::Integrity("CDC counter"))?;
                    }
                    None if self.incremental_file_supported(node, record.content_root) => {}
                    None => {
                        let (content, counters) =
                            rope::build(&mut objects, WorkspaceFileReader::new(self, node)?)?;
                        record.content_root = content.0;
                        cdc_bytes_scanned = cdc_bytes_scanned
                            .checked_add(counters.cdc_bytes_scanned)
                            .ok_or(StorageError::Integrity("CDC counter"))?;
                    }
                }
            }
            if base.mode != attr.mode
                || base.mtime_seconds != attr.mtime_seconds
                || base.mtime_nanoseconds != attr.mtime_nanoseconds
            {
                record.metadata_root = metadata_cache
                    .get_or_build(
                        &mut objects,
                        record.kind,
                        attr.mode,
                        attr.mtime_seconds,
                        attr.mtime_nanoseconds,
                    )?
                    .0;
            }
            if record != base.record {
                mutations.push(InodeMutation::Upsert {
                    inode: base.inode,
                    record,
                });
            }
        }
        note_commit_phase(WorkspaceCommitPhase::Content, started);

        let started = Instant::now();
        let root = if mutations.is_empty() {
            self.base_root
        } else {
            apply_sorted_inode_mutations(
                &mut objects,
                self.base_root,
                mutations,
                usize::try_from(self.policy.max_final_delta_memory_bytes)
                    .unwrap_or(usize::MAX)
                    .min(SORTED_TREE_UPDATE_SCRATCH_BYTES),
            )?
        };
        note_commit_phase(WorkspaceCommitPhase::Namespace, started);
        let started = Instant::now();
        let built = objects.finish(root, cdc_bytes_scanned);
        note_commit_phase(WorkspaceCommitPhase::CandidateFinish, started);
        built.map(Some)
    }

    pub(crate) fn resolution_fingerprint(
        &mut self,
        affected_paths: &[CanonicalPath],
    ) -> Result<[u8; 32]> {
        let base = self.base_manifest()?;
        let final_view = self.final_manifest(manifest_charge(&base))?;
        let mut affected = affected_paths.iter().collect::<Vec<_>>();
        affected.sort();
        let mut digest = ContentDigestWriter::new();
        digest.write_all(b"layerfs/workspace-resolution/v2\0")?;
        for affected_path in affected {
            digest.write_all(b"A")?;
            frame(&mut digest, affected_path.as_bytes())?;
            for (path, entry) in final_view
                .iter()
                .filter(|(path, _)| path_intersects(path, affected_path.as_str()))
            {
                digest.write_all(b"E")?;
                frame(&mut digest, path.as_bytes())?;
                digest.write_all(&[match entry.attr.kind {
                    Kind::File => 1,
                    Kind::Directory => 2,
                    Kind::Symlink => 3,
                }])?;
                digest.write_all(&entry.attr.size.to_be_bytes())?;
                digest.write_all(&entry.attr.mode.to_be_bytes())?;
                digest.write_all(&entry.attr.links.to_be_bytes())?;
                digest.write_all(&entry.attr.mtime_seconds.to_be_bytes())?;
                digest.write_all(&entry.attr.mtime_nanoseconds.to_be_bytes())?;
                let node = self
                    .nodes
                    .get(&entry.node)
                    .ok_or(StorageError::Integrity("resolution node"))?;
                digest.write_all(&(node.paths.len() as u64).to_be_bytes())?;
                for alias in &node.paths {
                    frame(&mut digest, alias.as_bytes())?;
                }
                match entry.attr.kind {
                    Kind::File => {
                        let mut reader = WorkspaceFileReader::new(self, entry.node)?;
                        std::io::copy(&mut reader, &mut digest)?;
                    }
                    Kind::Symlink => frame(&mut digest, &self.readlink(entry.node)?)?,
                    Kind::Directory => {}
                }
            }
            digest.write_all(b"Z")?;
        }
        Ok(digest.finish())
    }

    #[allow(clippy::too_many_arguments)]
    fn create_group(
        &self,
        objects: &mut ObjectBuffer<'_>,
        mut root: ObjectId,
        representative: &str,
        paths: &[String],
        entry: FinalEntry,
        captured: Option<(NodeId, u64, FileStateRoot, RopeCounters)>,
        seed: [u8; 32],
        cdc_bytes_scanned: &mut u64,
    ) -> Result<ObjectId> {
        match entry.attr.kind {
            Kind::File => {
                let path = CanonicalPath::new(representative)?;
                if let Some((_, _, content, counters)) = captured
                    .filter(|(node, len, _, _)| *node == entry.node && *len == entry.attr.size)
                {
                    root = filesystem::write_file(
                        objects,
                        root,
                        &path,
                        std::io::empty(),
                        entry.attr.mode,
                        seed,
                    )?
                    .root();
                    let resolved =
                        filesystem::resolve(objects, root, &path, &mut LogicalCounters::default())?;
                    root = filesystem::apply_inode_mutations(
                        objects,
                        root,
                        [InodeMutation::Upsert {
                            inode: resolved.inode,
                            record: InodeRecordV1 {
                                content_root: content.0,
                                ..resolved.record
                            },
                        }],
                    )?
                    .root();
                    *cdc_bytes_scanned = cdc_bytes_scanned
                        .checked_add(counters.cdc_bytes_scanned)
                        .ok_or(StorageError::Integrity("CDC counter"))?;
                } else {
                    let candidate = filesystem::write_file(
                        objects,
                        root,
                        &path,
                        WorkspaceFileReader::new(self, entry.node)?,
                        entry.attr.mode,
                        seed,
                    )?;
                    *cdc_bytes_scanned = cdc_bytes_scanned
                        .checked_add(candidate.counters().rope.cdc_bytes_scanned)
                        .ok_or(StorageError::Integrity("CDC counter"))?;
                    root = candidate.root();
                }
            }
            Kind::Symlink => {
                root = apply_one(
                    objects,
                    root,
                    ContentChange::Symlink {
                        path: representative.to_owned(),
                        target: self.readlink(entry.node)?,
                    },
                    seed,
                    cdc_bytes_scanned,
                )?;
            }
            Kind::Directory => return Err(StorageError::Integrity("directory hard link")),
        }
        for path in &paths[1..] {
            root = apply_one(
                objects,
                root,
                ContentChange::HardLink {
                    source: representative.to_owned(),
                    target: path.clone(),
                },
                seed,
                cdc_bytes_scanned,
            )?;
        }
        set_metadata(objects, root, representative, entry.attr)
    }

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

    fn final_manifest(&mut self, base_charge: u64) -> Result<BTreeMap<String, FinalEntry>> {
        let mut output = BTreeMap::new();
        let mut charge = base_charge;
        let mut pending = vec![(ROOT, String::new())];
        while let Some((directory, prefix)) = pending.pop() {
            for (name, node) in self.directory_entries(directory)? {
                let dirty = self.dirty.contains(&node);
                layerfs_layerstack_store::note_workspace_namespace_visits(
                    0,
                    1,
                    u64::from(dirty),
                    u64::from(!dirty),
                    0,
                );
                let name = std::str::from_utf8(&name)
                    .map_err(|_| StorageError::Integrity("Workspace path"))?;
                let path = if prefix.is_empty() {
                    name.to_owned()
                } else {
                    format!("{prefix}/{name}")
                };
                let attr = self.attr(node)?;
                charge = charge.saturating_add(path_charge(&path));
                output.insert(path.clone(), FinalEntry { node, attr });
                self.policy.check_final_delta(charge)?;
                if attr.kind == Kind::Directory {
                    pending.push((node, path));
                }
            }
        }
        Ok(output)
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

    fn base_symlink(&self, root: ObjectId) -> Result<Vec<u8>> {
        Ok(CoreReader(&self.reader)
            .with_authenticated_canonical(
                root,
                layerfs_content::tree::directory::codec::decode_symlink,
            )?
            .target)
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

// Only this bounded candidate batch is owned by the planner; overlay maps stay
// borrowed. The content layer separately bounds its deferred canonical pages.
struct FrontierInodes {
    root: ObjectId,
    pending: BTreeMap<InodeId, Option<InodeRecordV1>>,
    batch_size: usize,
    scratch_limit: usize,
}

impl FrontierInodes {
    fn new(root: ObjectId, batch_size: usize, scratch_limit: usize) -> Self {
        Self {
            root,
            pending: BTreeMap::new(),
            batch_size,
            scratch_limit,
        }
    }

    fn record(&self, objects: &ObjectBuffer<'_>, inode: InodeId) -> Result<InodeRecordV1> {
        if let Some(record) = self.pending.get(&inode) {
            return record.ok_or(StorageError::Integrity("released frontier inode"));
        }
        let namespace = filesystem::namespace(objects, self.root)?;
        let id = inode_table_lookup(
            objects,
            layerfs_content::tree::inode::InodeTableRoot(namespace.inode_table_root),
            inode,
            &mut InodeTableCounters::default(),
        )?
        .ok_or(StorageError::Integrity("frontier inode record"))?;
        Ok(ObjectStore::with_authenticated_canonical(
            objects,
            id,
            decode_inode_record,
        )?)
    }

    fn set(
        &mut self,
        objects: &mut ObjectBuffer<'_>,
        inode: InodeId,
        record: Option<InodeRecordV1>,
    ) -> Result<()> {
        self.pending.insert(inode, record);
        if self.pending.len() >= self.batch_size {
            self.flush(objects)?;
        }
        Ok(())
    }

    fn flush(&mut self, objects: &mut ObjectBuffer<'_>) -> Result<()> {
        if !self.pending.is_empty() {
            self.root = apply_sorted_inode_mutations(
                objects,
                self.root,
                std::mem::take(&mut self.pending)
                    .into_iter()
                    .map(|(inode, record)| match record {
                        Some(record) => InodeMutation::Upsert { inode, record },
                        None => InodeMutation::Remove { inode },
                    })
                    .collect(),
                self.scratch_limit,
            )?;
        }
        Ok(())
    }

    fn release(
        &mut self,
        objects: &mut ObjectBuffer<'_>,
        inode: InodeId,
        budget: u64,
    ) -> Result<()> {
        // A cursor per deleted-directory ancestor, never a whole deleted subtree.
        let mut directories = Vec::new();
        let mut next = Some(inode);
        loop {
            if let Some(inode) = next.take() {
                let mut record = self.record(objects, inode)?;
                record.namespace_ref_count = record
                    .namespace_ref_count
                    .checked_sub(1)
                    .ok_or(StorageError::Integrity("namespace reference underflow"))?;
                if record.namespace_ref_count != 0 {
                    self.set(objects, inode, Some(record))?;
                } else {
                    self.set(objects, inode, None)?;
                    if record.kind == InodeKind::Directory {
                        // Includes batch storage and the next bounded directory page.
                        if (self.batch_size as u64 + directories.len() as u64 + 2) * 1024 > budget {
                            return Err(StorageError::InvalidInput("workspace final-delta limit"));
                        }
                        directories.push((DirectoryStateRoot(record.content_root), None));
                    }
                }
            }
            let Some((root, after)) = directories.last_mut() else {
                break;
            };
            let page = directory_page_after(
                objects,
                *root,
                after.as_ref(),
                1,
                4096,
                &mut NamespaceCounters::default(),
            )?;
            if let Some((name, child)) = page.entries.into_iter().next() {
                *after = Some(name);
                next = Some(child);
                layerfs_layerstack_store::note_workspace_namespace_visits(1, 0, 0, 0, 0);
            } else {
                directories.pop();
            }
        }
        Ok(())
    }
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

fn apply_one(
    objects: &mut ObjectBuffer<'_>,
    root: ObjectId,
    change: ContentChange,
    seed: [u8; 32],
    cdc_bytes_scanned: &mut u64,
) -> Result<ObjectId> {
    let applied = filesystem::apply_changes(objects, root, &[change], seed)?;
    *cdc_bytes_scanned = cdc_bytes_scanned
        .checked_add(applied.counters.cdc_bytes_scanned)
        .ok_or(StorageError::Integrity("CDC counter"))?;
    Ok(applied.root_id)
}

fn set_metadata(
    objects: &mut ObjectBuffer<'_>,
    root: ObjectId,
    path: &str,
    attr: Attr,
) -> Result<ObjectId> {
    let path = CanonicalPath::new(path)?;
    let root = filesystem::set_mode(objects, root, &path, attr.mode)?.root();
    Ok(filesystem::set_mtime(
        objects,
        root,
        &path,
        attr.mtime_seconds,
        attr.mtime_nanoseconds,
    )?
    .root())
}

fn groups_by_inode(base: &BTreeMap<String, BaseEntry>) -> BTreeMap<InodeId, Vec<String>> {
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for (path, entry) in base {
        if entry.record.kind != InodeKind::Directory {
            groups.entry(entry.inode).or_default().push(path.clone());
        }
    }
    groups
}

fn groups_by_node(final_view: &BTreeMap<String, FinalEntry>) -> BTreeMap<NodeId, Vec<String>> {
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for (path, entry) in final_view {
        if entry.attr.kind != Kind::Directory {
            groups.entry(entry.node).or_default().push(path.clone());
        }
    }
    groups
}

fn kind(kind: InodeKind) -> Kind {
    match kind {
        InodeKind::RegularFile => Kind::File,
        InodeKind::Directory => Kind::Directory,
        InodeKind::Symlink => Kind::Symlink,
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

fn depth(path: &str) -> usize {
    path.bytes().filter(|byte| *byte == b'/').count()
}

fn apply_sorted_inode_mutations(
    objects: &mut ObjectBuffer<'_>,
    root: ObjectId,
    mutations: Vec<InodeMutation>,
    scratch_limit: usize,
) -> Result<ObjectId> {
    let namespace = filesystem::namespace(objects, root)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    let mut deltas = Vec::with_capacity(mutations.len());
    for mutation in &mutations {
        let (inode, record) = match *mutation {
            InodeMutation::Upsert { inode, record } => (
                inode,
                Some(objects.put_owned(encode_inode_record(record)?)?),
            ),
            InodeMutation::Remove { inode } => (inode, None),
        };
        deltas.push((inode, record));
    }
    deltas.sort_unstable_by_key(|(inode, _)| *inode);
    let sorted = inode_table_apply_sorted_with_budget(
        objects,
        table,
        deltas.into_iter().map(Ok),
        scratch_limit,
    );
    match sorted {
        Ok((next, _)) => {
            if next == table {
                Ok(root)
            } else {
                Ok(objects.put_owned(encode_namespace_root(NamespaceRootV1 {
                    inode_table_root: next.0,
                    ..namespace
                })?)?)
            }
        }
        Err(
            layerfs_content::CoreError::ObjectLimitExceeded
            | layerfs_content::CoreError::Unsupported,
        ) => Ok(filesystem::apply_inode_mutations(objects, root, mutations)?.root()),
        Err(error) => Err(error.into()),
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = self.clear_spool();
    }
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
    fn dense_existing_file_delta_uses_bounded_frontier_and_preserves_aliases() {
        let (root, mut workspace) = empty_workspace("dense-frontier");
        let mut files = Vec::new();
        for index in 0..16 {
            let name = format!("file-{index:02}");
            let node = workspace
                .create_file(ROOT, name.as_bytes(), 0o640)
                .unwrap()
                .node;
            workspace.write(node, 0, b"initial").unwrap();
            files.push((name, node));
        }
        workspace.link(files[0].1, ROOT, b"alias").unwrap();
        let sentinel = workspace
            .create_file(ROOT, b"sentinel", 0o600)
            .unwrap()
            .node;
        workspace.write(sentinel, 0, b"unchanged").unwrap();
        workspace.commit().unwrap();
        let original_inodes = files
            .iter()
            .map(|(_, node)| workspace.nodes[node].canonical.unwrap())
            .collect::<Vec<_>>();
        workspace.policy.max_final_delta_memory_bytes = 4096;
        for (index, (_, node)) in files.iter().enumerate() {
            workspace
                .write(*node, 0, format!("changed-{index:02}").as_bytes())
                .unwrap();
            workspace.set_mtime(*node, 1700000001, 123).unwrap();
        }
        assert!(workspace.nodes.values().all(|node| !matches!(&node.data,
            Data::Directory(directory) if !directory.changes.is_empty())));
        assert!(
            workspace
                .mutation_paths
                .keys()
                .map(|path| path_charge(path))
                .sum::<u64>()
                > workspace.policy.max_final_delta_memory_bytes
        );
        workspace.commit().unwrap();
        let reader = workspace.store.snapshot_reader(workspace.base_root);
        let core = CoreReader(&reader);
        for (index, (name, _)) in files.iter().enumerate() {
            let path = CanonicalPath::new(name).unwrap();
            let resolved = filesystem::resolve(
                &core,
                workspace.base_root,
                &path,
                &mut LogicalCounters::default(),
            )
            .unwrap();
            assert_eq!(resolved.inode, original_inodes[index]);
            let metadata =
                portable_metadata(&core, resolved.record.metadata_root, resolved.record.kind)
                    .unwrap();
            assert_eq!(
                (
                    metadata.permission_mode,
                    metadata.mtime_seconds,
                    metadata.mtime_nanoseconds
                ),
                (0o640, 1700000001, 123)
            );
            let mut bytes = Vec::new();
            filesystem::stream(&core, workspace.base_root, &path, &mut bytes).unwrap();
            assert_eq!(bytes, format!("changed-{index:02}").as_bytes());
        }
        for (path, expected) in [
            ("alias", b"changed-00".as_slice()),
            ("sentinel", b"unchanged"),
        ] {
            let mut bytes = Vec::new();
            filesystem::stream(
                &core,
                workspace.base_root,
                &CanonicalPath::new(path).unwrap(),
                &mut bytes,
            )
            .unwrap();
            assert_eq!(bytes, expected);
        }
        let alias = filesystem::resolve(
            &core,
            workspace.base_root,
            &CanonicalPath::new("alias").unwrap(),
            &mut LogicalCounters::default(),
        )
        .unwrap();
        assert_eq!(alias.inode, original_inodes[0]);
        assert_eq!(alias.record.namespace_ref_count, 2);
        drop(workspace);
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
        if let Data::File(FileData::Edited { pieces, .. }) =
            &mut workspace.nodes.get_mut(&file.node).unwrap().data
        {
            *pieces = crate::file_edit::PieceTree::empty()
                .replace(
                    0,
                    0,
                    [
                        crate::file_edit::Piece::Spool { offset: 0, len: 1 },
                        crate::file_edit::Piece::Spool {
                            offset: 1,
                            len: data.len() as u64 - 1,
                        },
                    ],
                )
                .unwrap();
            assert_eq!(pieces.count(), 2);
        }
        let captured = workspace
            .take_capture()
            .expect("sequential capture is reusable");
        let captured_root = captured.root;
        workspace.capture = crate::capture::CaptureState::Ready(Box::new(captured));

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
