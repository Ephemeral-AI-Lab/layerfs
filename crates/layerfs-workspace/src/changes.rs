use crate::cow_tree::{portable_metadata, Attr, Data, FileData, Kind, NodeId, Workspace, ROOT};
use layerfs_content::file::rope::{self, FileMutationBatch, FileStateRoot, RopeCounters};
use layerfs_content::filesystem::{self, ContentChange, InodeMutation, LogicalCounters};
use layerfs_content::object::access::ObjectRead;
use layerfs_content::object::{ContentDigestWriter, ObjectId};
use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1};
use layerfs_content::CanonicalPath;
use layerfs_storage::{BuiltRoot, CoreReader, ObjectBuffer, Result, StorageError};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
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
        let started = Instant::now();
        let base = self.base_manifest()?;
        let final_view = self.final_manifest(manifest_charge(&base))?;
        let base_groups = groups_by_inode(&base);
        let final_groups = groups_by_node(&final_view);
        let mut recreate = BTreeSet::new();
        let mut rewrite = BTreeSet::new();
        let mut renames = BTreeMap::new();
        note_commit_phase(
            layerfs_storage::WorkspaceCommitPhase::CandidatePlan,
            started,
        );

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
        note_commit_phase(layerfs_storage::WorkspaceCommitPhase::DirtyCompare, started);

        let started = Instant::now();
        let seed = *filesystem::namespace(&CoreReader(&self.reader), self.base_root)?
            .root_directory_inode
            .as_bytes();
        let mut objects = ObjectBuffer::new(&self.reader)?;
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
        note_commit_phase(layerfs_storage::WorkspaceCommitPhase::Namespace, started);

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
                    seed,
                    &mut cdc_bytes_scanned,
                )?;
                note_commit_phase(layerfs_storage::WorkspaceCommitPhase::Content, started);
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
                    note_commit_phase(layerfs_storage::WorkspaceCommitPhase::Content, started);
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
                    note_commit_phase(layerfs_storage::WorkspaceCommitPhase::Namespace, started);
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
        note_commit_phase(layerfs_storage::WorkspaceCommitPhase::Namespace, started);

        let started = Instant::now();
        let built = objects.finish(root, cdc_bytes_scanned);
        note_commit_phase(
            layerfs_storage::WorkspaceCommitPhase::CandidateFinish,
            started,
        );
        built
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
        seed: [u8; 32],
        cdc_bytes_scanned: &mut u64,
    ) -> Result<ObjectId> {
        match entry.attr.kind {
            Kind::File => {
                let candidate = filesystem::write_file(
                    objects,
                    root,
                    &CanonicalPath::new(representative)?,
                    WorkspaceFileReader::new(self, entry.node)?,
                    entry.attr.mode,
                    seed,
                )?;
                *cdc_bytes_scanned = cdc_bytes_scanned
                    .checked_add(candidate.counters().rope.cdc_bytes_scanned)
                    .ok_or(StorageError::Integrity("CDC counter"))?;
                root = candidate.root();
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
            Data::File(FileData::Overlay {
                base: Some((root, base_len)),
                len,
                dirty,
                ..
            }) if root.0 == base => Ok(*len != *base_len || !dirty.is_empty()),
            Data::File(_) => Ok(!self.file_matches(node, base)?),
            _ => Err(StorageError::InvalidInput("file")),
        }
    }

    fn incremental_file_supported(&self, node: NodeId, base: ObjectId) -> bool {
        matches!(
            self.nodes.get(&node).map(|node| &node.data),
            Some(Data::File(FileData::Overlay {
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
        let Data::File(FileData::Overlay {
            base: Some((file_root, _)),
            len: final_len,
            dirty,
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
        let (file_root, final_len, dirty) = (*file_root, *final_len, dirty.clone());
        let mut batch = FileMutationBatch::new(objects, Some(file_root))?;
        let mut changed = false;
        for (start, end) in dirty {
            let end = end.min(final_len);
            if start >= end {
                continue;
            }
            let current_len = batch.logical_len()?;
            if start > current_len {
                return Err(StorageError::Integrity("Workspace dirty range gap"));
            }
            let delete_end = end.min(current_len);
            let delete_len = delete_end.saturating_sub(start);
            if delete_len == end - start
                && self.workspace_range_matches_base(node, file_root, start, end)?
            {
                continue;
            }
            batch.replace(
                start,
                delete_len,
                WorkspaceRangeReader::new(self, node, start, end - start)?,
            )?;
            changed = true;
        }
        if final_len < batch.logical_len()? {
            batch.replace(
                final_len,
                batch.logical_len()?.saturating_sub(final_len),
                std::io::empty(),
            )?;
            changed = true;
        }
        if batch.logical_len()? != final_len {
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
            Data::File(FileData::Overlay {
                base: Some((root, base_len)),
                len,
                dirty,
                ..
            }) if root.0 == base && len == base_len && dirty.is_empty() => return Ok(true),
            Data::File(FileData::Overlay {
                base: Some((root, base_len)),
                len,
                dirty,
                ..
            }) if root.0 == base => {
                if len != base_len {
                    return Ok(false);
                }
                for (start, end) in dirty {
                    if !self.workspace_range_matches_base(node, *root, *start, *end)? {
                        return Ok(false);
                    }
                }
                return Ok(true);
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

fn note_commit_phase(phase: layerfs_storage::WorkspaceCommitPhase, started: Instant) {
    layerfs_storage::note_workspace_commit_phase(
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
    workspace: &'a Workspace,
    node: NodeId,
    offset: u64,
    len: u64,
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
        Ok(Self {
            workspace,
            node,
            offset: 0,
            len: workspace.attr(node)?.size,
        })
    }
}

impl Read for WorkspaceFileReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.offset == self.len || output.is_empty() {
            return Ok(0);
        }
        let bytes = self
            .workspace
            .read(
                self.node,
                self.offset,
                output.len().min((self.len - self.offset) as usize),
            )
            .map_err(std::io::Error::other)?;
        output[..bytes.len()].copy_from_slice(&bytes);
        self.offset += bytes.len() as u64;
        Ok(bytes.len())
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
    use layerfs_branch_store::BranchStore;
    use layerfs_layerstack_store::LayerStackStore;
    use layerfs_storage::{EntityName, LayerStackInitialization, LocalForkSource, RemotePlacement};
    use std::sync::Arc;

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
            let authority =
                Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
            let layer = authority
                .initialize_layerstack(
                    EntityName::new("project").unwrap(),
                    LayerStackInitialization::Directory(source),
                )
                .unwrap()
                .genesis_layer_id;
            let branches =
                BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();
            branches
                .pull_layer(authority.clone(), layer, RemotePlacement::Reference)
                .unwrap();
            let branch = branches
                .fork_branch(
                    EntityName::new(case).unwrap(),
                    LocalForkSource::Layer { layer_id: layer },
                )
                .unwrap();
            let mut workspace = Workspace::open(
                branches.clone(),
                authority.clone(),
                branch,
                root.join("spool"),
            )
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
            let outcome = branches
                .commit_candidate(
                    authority.clone(),
                    branch,
                    workspace.expected_head,
                    workspace.expected_base,
                    workspace.base_root,
                    built,
                    workspace.expected_base,
                    false,
                )
                .unwrap();
            assert_eq!(
                matches!(
                    outcome,
                    layerfs_branch_store::CommitOutcome::UpToDate { .. }
                ),
                case == "noop"
            );
            let reader = branches
                .snapshot_reader(authority.clone(), candidate_root)
                .unwrap();
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
            drop(branches);
            drop(authority);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
