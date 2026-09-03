use crate::cow_tree::{portable_metadata, Attr, Data, FileData, Kind, NodeId, Workspace, ROOT};
use layerfs_content::file::rope::{self, FileMutationBatch, FileStateRoot, RopeCounters};
use layerfs_content::filesystem::{self, ContentChange, InodeMutation, LogicalCounters};
use layerfs_content::object::access::ObjectRead;
use layerfs_content::object::{ContentDigestWriter, ObjectId};
use layerfs_content::tree::inode::codec::decode_inode_record;
use layerfs_content::tree::inode::{inode_table_lookup, InodeTableCounters};
use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1};
use layerfs_content::CanonicalPath;
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

    fn build_localized_candidate(&mut self) -> Result<Option<BuiltRoot>> {
        let started = Instant::now();
        if self.nodes.values().any(|node| {
            matches!(&node.data, Data::Directory(directory) if !directory.changes.is_empty())
        }) {
            return Ok(None);
        }
        let mut changed = BTreeSet::new();
        for path in self.mutation_paths.keys() {
            let Some(node) = self
                .nodes
                .iter()
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
        let charge = self
            .mutation_paths
            .keys()
            .map(|path| path_charge(path))
            .sum();
        self.policy.check_final_delta(charge)?;
        note_commit_phase(WorkspaceCommitPhase::CandidatePlan, started);

        let captured = self.take_capture();
        if let Some(captured) = &captured {
            layerfs_layerstack_store::note_workspace_capture(1, captured.len);
        }
        let started = Instant::now();
        let reader = CoreReader(&self.reader);
        let mut entries = Vec::with_capacity(changed.len());
        for node in changed {
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
                record.metadata_root = filesystem::build_portable_metadata(
                    &mut objects,
                    record.kind,
                    attr.mode,
                    attr.mtime_seconds,
                    attr.mtime_nanoseconds,
                )?;
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
            filesystem::apply_inode_mutations(&mut objects, self.base_root, mutations)?.root()
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
