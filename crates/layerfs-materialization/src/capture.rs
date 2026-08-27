use crate::driver::*;
use crate::{topology_edge_key, NativeRoute, OperationCounters, VfsError, VfsResult};
use layerfs_core::content::rope::{build, read_all, FileStateRoot, ObjectRead, ObjectStore};
use layerfs_core::inode::{
    generated_inode_table_from_root, generated_inode_table_upsert, inode_table_lookup,
    GeneratedInodeTable, InodeId, InodeKind, InodeRecordV1, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::metadata::{
    decode_apple_acl, encode_bsd_flags, MetadataEntryV1, MetadataKey, MetadataTreeBuilder,
    PortableMetadataV1,
};
use layerfs_core::namespace::{
    directory_insert, empty_directory, visit_directory_entries, DirectoryStateRoot,
    NamespaceCounters, NamespaceRootV1,
};
use layerfs_core::namespace_codec::{
    decode_inode_record, decode_namespace_root, encode_inode_record, encode_namespace_root,
    encode_symlink, profile_id,
};
use layerfs_workspace::{DiskNamespace, DiskTable, WorkingCandidateWrite, WorkingStore};
use std::collections::HashMap;
use std::io::{Cursor, Seek, SeekFrom};
use std::sync::Mutex;

const SEMANTIC_DIGEST_CACHE_LIMIT: usize = 4096;

pub(crate) trait CaptureStore: ObjectStore {
    fn allocate_inode_id(&mut self) -> VfsResult<InodeId>;
}

impl CaptureStore for WorkingCandidateWrite<'_> {
    fn allocate_inode_id(&mut self) -> VfsResult<InodeId> {
        WorkingCandidateWrite::allocate_inode_id(self)
            .map_err(|error| VfsError::Io(std::io::Error::other(error.to_string())))
    }
}

#[derive(Default)]
pub(crate) struct SemanticDigestCache(Mutex<HashMap<layerfs_core::ObjectId, [u8; 32]>>);

impl SemanticDigestCache {
    fn get(&self, root: FileStateRoot) -> VfsResult<Option<[u8; 32]>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| VfsError::InvalidState)?
            .get(&root.0)
            .copied())
    }

    fn insert(&self, root: FileStateRoot, digest: [u8; 32]) -> VfsResult<()> {
        let mut entries = self.0.lock().map_err(|_| VfsError::InvalidState)?;
        if entries.len() == SEMANTIC_DIGEST_CACHE_LIMIT && !entries.contains_key(&root.0) {
            // ponytail: wholesale eviction keeps Store-lifetime memory bounded; add LRU only if
            // measured capture workloads repeatedly exceed 4,096 distinct retained file roots.
            entries.clear();
        }
        entries.insert(root.0, digest);
        Ok(())
    }
}

struct HardLink {
    inode: InodeId,
    record: InodeRecordV1,
    expected: u64,
    observed: u64,
}

impl HardLink {
    fn encode(&self) -> [u8; 121] {
        let mut bytes = [0_u8; 121];
        bytes[..32].copy_from_slice(self.inode.as_bytes());
        bytes[32] = self.record.kind as u8;
        bytes[33..41].copy_from_slice(&self.record.namespace_ref_count.to_be_bytes());
        bytes[41..73].copy_from_slice(self.record.content_root.as_bytes());
        bytes[73..105].copy_from_slice(self.record.metadata_root.as_bytes());
        bytes[105..113].copy_from_slice(&self.expected.to_be_bytes());
        bytes[113..121].copy_from_slice(&self.observed.to_be_bytes());
        bytes
    }

    fn decode(bytes: &[u8]) -> VfsResult<Self> {
        if bytes.len() != 121 {
            return Err(VfsError::InvalidState);
        }
        Ok(Self {
            inode: InodeId::from_slice(&bytes[..32])?,
            record: InodeRecordV1 {
                kind: InodeKind::try_from(bytes[32])?,
                namespace_ref_count: u64::from_be_bytes(bytes[33..41].try_into().unwrap()),
                content_root: layerfs_core::ObjectId::from_bytes(&bytes[41..73])?,
                metadata_root: layerfs_core::ObjectId::from_bytes(&bytes[73..105])?,
            },
            expected: u64::from_be_bytes(bytes[105..113].try_into().unwrap()),
            observed: u64::from_be_bytes(bytes[113..121].try_into().unwrap()),
        })
    }
}

pub(crate) fn capture_workspace_candidate(
    working: &WorkingStore,
    digest_cache: &SemanticDigestCache,
    workspace: &dyn ProjectionWorkspace,
    base_root: layerfs_core::ObjectId,
    operation_id: Option<layerfs_workspace::OperationId>,
) -> VfsResult<(layerfs_core::ObjectId, OperationCounters)> {
    let mut counters = OperationCounters::default();
    counters.native.route = Some(NativeRoute::CaptureStream);
    counters.authority_full_scans = 1;
    workspace.revalidate_root_binding()?;
    let root_handle = workspace.root_directory()?;
    let root_token = workspace.directory_token(root_handle.as_ref())?;
    let existing_table = working
        .create_scratch_table("existing-paths")
        .map_err(working_error)?;
    let existing = existing_table.namespace(b"paths")?;
    let prior_table = seed_existing_paths(working, base_root, &existing, None, &mut counters)?;
    let seeded_links_table = working
        .create_scratch_table("existing-hardlinks")
        .map_err(working_error)?;
    let seeded_links = seeded_links_table.namespace(b"links")?;
    seed_existing_hard_links(
        workspace,
        root_handle.as_ref(),
        &existing,
        &seeded_links,
        &[],
        true,
    )?;
    let mut writer = working.begin_candidate_write().map_err(working_error)?;
    let root_inode = match existing_inode(&existing, b"", InodeKind::Directory)? {
        Some(inode) => inode,
        None => CaptureStore::allocate_inode_id(&mut writer)?,
    };
    let mut table = None;
    let hard_links = working
        .create_scratch_table("hardlinks")
        .map_err(working_error)?;
    let entries = working
        .create_scratch_table("enumeration")
        .map_err(working_error)?;
    let mut next_directory = 0_u64;
    let root_metadata = workspace.read_root_metadata()?;
    capture_directory(
        workspace,
        root_handle.as_ref(),
        digest_cache,
        root_inode,
        true,
        root_metadata,
        &mut writer,
        &mut table,
        &hard_links,
        &entries,
        &existing,
        Some(&seeded_links),
        Some(&seeded_links),
        Some(prior_table),
        &[],
        &mut next_directory,
        &mut counters,
    )?;
    if workspace.directory_token(root_handle.as_ref())? != root_token {
        return Err(DriverError::Conflict.into());
    }
    workspace.revalidate_root_binding()?;
    hard_links
        .for_each(|bytes| {
            let link = HardLink::decode(bytes).map_err(|_| {
                layerfs_workspace::StorageError::InvalidRecord("hard-link scratch value")
            })?;
            if link.observed != link.expected {
                return Err(layerfs_workspace::StorageError::InvalidRecord(
                    "external hard-link boundary",
                ));
            }
            Ok(())
        })
        .map_err(|error| {
            if matches!(
                error,
                layerfs_workspace::StorageError::InvalidRecord("external hard-link boundary")
            ) {
                VfsError::ExternalHardLinkBoundary
            } else {
                error.into()
            }
        })?;
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: table.ok_or(VfsError::InvalidState)?.into_root().0,
    })?;
    workspace.revalidate_root_binding()?;
    let root = writer.put(&namespace)?;
    match operation_id {
        Some(operation_id) => writer
            .commit_operation_candidate(operation_id, root)
            .map_err(working_error)?,
        None => writer.commit_candidate(root).map_err(working_error)?,
    };
    for scratch in [existing_table, seeded_links_table, hard_links, entries] {
        counters.add_scratch(scratch.finish()?)?;
    }
    Ok((root, counters))
}

fn working_error(error: layerfs_workspace::WorkingError) -> VfsError {
    VfsError::Io(std::io::Error::other(error.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn capture_directory(
    workspace: &dyn ProjectionWorkspace,
    directory: &dyn DirectoryHandle,
    digest_cache: &SemanticDigestCache,
    inode: InodeId,
    root: bool,
    native_metadata: NativeMetadata,
    publication: &mut impl CaptureStore,
    table: &mut Option<GeneratedInodeTable>,
    hard_links: &DiskTable,
    entries: &DiskTable,
    existing: &DiskNamespace<'_>,
    existing_links: Option<&DiskNamespace<'_>>,
    prior_links: Option<&DiskNamespace<'_>>,
    prior_table: Option<InodeTableRoot>,
    current_path: &[u8],
    next_directory: &mut u64,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let mut state = empty_directory(publication)?;
    let directory_key = next_directory.to_be_bytes();
    *next_directory = next_directory
        .checked_add(1)
        .ok_or(VfsError::InvalidState)?;
    for entry in workspace.enumerate_at(directory)? {
        let entry = entry?;
        let mut key = directory_key.to_vec();
        key.extend_from_slice(&entry.name);
        entries.enqueue_once(&key, &encode_entry(&entry)?)?;
    }
    while let Some((key, encoded)) = entries.pop_pending_prefix(&directory_key)? {
        let entry = decode_entry(key[8..].to_vec(), &encoded)?;
        if workspace.token_at(directory, &entry.name)? != entry.token {
            return Err(DriverError::Conflict.into());
        }
        let name = layerfs_core::CanonicalName::from_bytes(&entry.name)?;
        let path = child_path(current_path, &entry.name);
        let child_inode = match entry.kind {
            NativeKind::Directory => {
                let child_inode = match existing_inode(existing, &path, InodeKind::Directory)? {
                    Some(inode) => inode,
                    None => publication.allocate_inode_id()?,
                };
                let metadata =
                    workspace.read_metadata_at(directory, &entry.name, Some(&entry.token))?;
                let child =
                    workspace.open_directory_at(directory, &entry.name, Some(&entry.token))?;
                capture_directory(
                    workspace,
                    child.as_ref(),
                    digest_cache,
                    child_inode,
                    false,
                    metadata,
                    publication,
                    table,
                    hard_links,
                    entries,
                    existing,
                    existing_links,
                    prior_links,
                    prior_table,
                    &path,
                    next_directory,
                    counters,
                )?;
                child_inode
            }
            NativeKind::RegularFile => capture_regular(
                workspace,
                directory,
                digest_cache,
                &entry,
                &path,
                publication,
                table,
                hard_links,
                existing,
                existing_links,
                prior_links,
                prior_table,
                counters,
            )?,
            NativeKind::Symlink => {
                let child_inode = match existing_inode(existing, &path, InodeKind::Symlink)? {
                    Some(inode) => inode,
                    None => publication.allocate_inode_id()?,
                };
                let target = workspace.read_link_at(directory, &entry.name, Some(&entry.token))?;
                let content = publication.put(&encode_symlink(
                    &layerfs_core::namespace::SymlinkStateV1::new(target)?,
                )?)?;
                let metadata = put_metadata_observed(
                    publication,
                    InodeKind::Symlink,
                    &workspace.read_metadata_at(directory, &entry.name, Some(&entry.token))?,
                    counters,
                )?;
                put_record(
                    publication,
                    table,
                    child_inode,
                    InodeRecordV1 {
                        kind: InodeKind::Symlink,
                        namespace_ref_count: 1,
                        content_root: content,
                        metadata_root: metadata,
                    },
                    counters,
                )?;
                child_inode
            }
        };
        if workspace.token_at(directory, &entry.name)? != entry.token {
            return Err(DriverError::Conflict.into());
        }
        let (next, namespace_counters) = directory_insert(publication, state, name, child_inode)?;
        counters.add_namespace(namespace_counters)?;
        state = next;
    }
    let metadata = put_metadata_observed(
        publication,
        InodeKind::Directory,
        &native_metadata,
        counters,
    )?;
    put_record(
        publication,
        table,
        inode,
        InodeRecordV1 {
            kind: InodeKind::Directory,
            namespace_ref_count: if root { 0 } else { 1 },
            content_root: state.0,
            metadata_root: metadata,
        },
        counters,
    )
}

fn encode_entry(entry: &NativeEntry) -> VfsResult<Vec<u8>> {
    let token_len = u32::try_from(entry.token.len()).map_err(|_| VfsError::InvalidState)?;
    let hard_len = entry
        .hard_link_key
        .as_ref()
        .map(|key| u32::try_from(key.len()).map_err(|_| VfsError::InvalidState))
        .transpose()?;
    let mut bytes = Vec::with_capacity(
        17 + entry.token.len() + entry.hard_link_key.as_ref().map_or(0, Vec::len),
    );
    bytes.push(match entry.kind {
        NativeKind::Directory => 1,
        NativeKind::RegularFile => 2,
        NativeKind::Symlink => 3,
    });
    bytes.extend_from_slice(&entry.link_count.to_be_bytes());
    bytes.extend_from_slice(&token_len.to_be_bytes());
    bytes.extend_from_slice(&entry.token);
    bytes.extend_from_slice(&hard_len.unwrap_or(u32::MAX).to_be_bytes());
    if let Some(key) = &entry.hard_link_key {
        bytes.extend_from_slice(key);
    }
    Ok(bytes)
}

fn decode_entry(name: Vec<u8>, bytes: &[u8]) -> VfsResult<NativeEntry> {
    if bytes.len() < 17 {
        return Err(VfsError::InvalidState);
    }
    let kind = match bytes[0] {
        1 => NativeKind::Directory,
        2 => NativeKind::RegularFile,
        3 => NativeKind::Symlink,
        _ => return Err(VfsError::InvalidState),
    };
    let link_count = u64::from_be_bytes(bytes[1..9].try_into().unwrap());
    let token_len = usize::try_from(u32::from_be_bytes(bytes[9..13].try_into().unwrap()))
        .map_err(|_| VfsError::InvalidState)?;
    let hard_offset = 13_usize
        .checked_add(token_len)
        .ok_or(VfsError::InvalidState)?;
    if hard_offset
        .checked_add(4)
        .filter(|end| *end <= bytes.len())
        .is_none()
    {
        return Err(VfsError::InvalidState);
    }
    let hard_len = u32::from_be_bytes(bytes[hard_offset..hard_offset + 4].try_into().unwrap());
    let hard_link_key = if hard_len == u32::MAX {
        if bytes.len() != hard_offset + 4 {
            return Err(VfsError::InvalidState);
        }
        None
    } else {
        let end = (hard_offset + 4)
            .checked_add(usize::try_from(hard_len).map_err(|_| VfsError::InvalidState)?)
            .ok_or(VfsError::InvalidState)?;
        if end != bytes.len() {
            return Err(VfsError::InvalidState);
        }
        Some(bytes[hard_offset + 4..end].to_vec())
    };
    Ok(NativeEntry {
        name,
        kind,
        token: bytes[13..hard_offset].to_vec(),
        hard_link_key,
        link_count,
    })
}

#[allow(clippy::too_many_arguments)]
fn capture_regular(
    workspace: &dyn ProjectionWorkspace,
    parent: &dyn DirectoryHandle,
    digest_cache: &SemanticDigestCache,
    entry: &NativeEntry,
    path: &[u8],
    publication: &mut impl CaptureStore,
    table: &mut Option<GeneratedInodeTable>,
    hard_links: &DiskTable,
    existing: &DiskNamespace<'_>,
    existing_links: Option<&DiskNamespace<'_>>,
    prior_links: Option<&DiskNamespace<'_>>,
    prior_table: Option<InodeTableRoot>,
    counters: &mut OperationCounters,
) -> VfsResult<InodeId> {
    let key = entry.hard_link_key.clone().ok_or(VfsError::InvalidState)?;
    if let Some(bytes) = hard_links.get(&key)? {
        let mut link = HardLink::decode(&bytes)?;
        link.observed += 1;
        link.record.namespace_ref_count = link
            .record
            .namespace_ref_count
            .checked_add(1)
            .ok_or(VfsError::InvalidState)?;
        put_record(publication, table, link.inode, link.record, counters)?;
        hard_links.put(&key, &link.encode())?;
        return Ok(link.inode);
    }
    let retained_inode = existing_links
        .map(|links| links.get(&key))
        .transpose()?
        .flatten()
        .map(|bytes| InodeId::from_slice(&bytes))
        .transpose()?;
    let inode = match retained_inode {
        Some(inode) => inode,
        None => publication.allocate_inode_id()?,
    };
    let grouped_prior_inode = prior_links
        .map(|links| links.get(&key))
        .transpose()?
        .flatten()
        .filter(|bytes| bytes.len() == 32)
        .map(|bytes| InodeId::from_slice(&bytes))
        .transpose()?;
    let prior_inode = retained_inode.or(grouped_prior_inode).or(existing_inode(
        existing,
        path,
        InodeKind::RegularFile,
    )?);
    let prior = prior_inode
        .zip(prior_table)
        .map(|(inode, table)| existing_record(&*publication, table, inode, counters))
        .transpose()?;
    let mut file = workspace.open_regular_read_at(parent, &entry.name, Some(&entry.token))?;
    let mut current_digest = layerfs_core::identity::ContentDigestWriter::new();
    let current_bytes = std::io::copy(&mut file, &mut current_digest)?;
    counters.current_digest_bytes = counters
        .current_digest_bytes
        .checked_add(current_bytes)
        .ok_or(VfsError::InvalidState)?;
    counters.native.bytes_read = counters
        .native
        .bytes_read
        .checked_add(current_bytes)
        .ok_or(VfsError::InvalidState)?;
    let current_digest = current_digest.finish();
    let prior_digest = prior
        .map(|record| {
            let root = FileStateRoot(record.content_root);
            if let Some(digest) = digest_cache.get(root)? {
                return Ok(digest);
            }
            let mut digest = layerfs_core::identity::ContentDigestWriter::new();
            let rope = read_all(&*publication, root, &mut digest)?;
            counters.uncached_prior_digest_bytes = counters
                .uncached_prior_digest_bytes
                .checked_add(rope.payload_bytes_read)
                .ok_or(VfsError::InvalidState)?;
            counters.add_rope(rope)?;
            let digest = digest.finish();
            digest_cache.insert(root, digest)?;
            Ok::<_, VfsError>(digest)
        })
        .transpose()?;
    let content = if prior_digest == Some(current_digest) {
        counters.unchanged_file_roots_reused = counters
            .unchanged_file_roots_reused
            .checked_add(1)
            .ok_or(VfsError::InvalidState)?;
        FileStateRoot(prior.ok_or(VfsError::InvalidState)?.content_root)
    } else {
        file.seek(SeekFrom::Start(0))?;
        let (content, rope) = build(publication, &mut file)?;
        counters.changed_current_cdc_bytes = counters
            .changed_current_cdc_bytes
            .checked_add(rope.cdc_bytes_scanned)
            .ok_or(VfsError::InvalidState)?;
        counters.native.bytes_read = counters
            .native
            .bytes_read
            .checked_add(rope.cdc_bytes_scanned)
            .ok_or(VfsError::InvalidState)?;
        counters.add_rope(rope)?;
        content
    };
    let metadata = put_metadata_observed(
        publication,
        InodeKind::RegularFile,
        &workspace.read_metadata_at(parent, &entry.name, Some(&entry.token))?,
        counters,
    )?;
    let record = prior
        .filter(|record| {
            record.kind == InodeKind::RegularFile
                && record.namespace_ref_count == 1
                && record.content_root == content.0
                && record.metadata_root == metadata
        })
        .unwrap_or(InodeRecordV1 {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1,
            content_root: content.0,
            metadata_root: metadata,
        });
    put_record(publication, table, inode, record, counters)?;
    hard_links.put(
        &key,
        &HardLink {
            inode,
            record,
            expected: entry.link_count,
            observed: 1,
        }
        .encode(),
    )?;
    Ok(inode)
}

fn child_path(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = parent.to_vec();
    if !path.is_empty() {
        path.push(b'/');
    }
    path.extend_from_slice(name);
    path
}

fn encode_existing(inode: InodeId, kind: InodeKind) -> [u8; 33] {
    let mut value = [0_u8; 33];
    value[0] = kind as u8;
    value[1..].copy_from_slice(inode.as_bytes());
    value
}

fn existing_inode(
    existing: &DiskNamespace<'_>,
    path: &[u8],
    kind: InodeKind,
) -> VfsResult<Option<InodeId>> {
    existing
        .get(path)?
        .map(|value| {
            if value.len() != 33 || value[0] != kind as u8 {
                return Ok(None);
            }
            Ok(Some(InodeId::from_slice(&value[1..])?))
        })
        .transpose()
        .map(Option::flatten)
}

fn seed_existing_hard_links(
    workspace: &dyn ProjectionWorkspace,
    directory: &dyn DirectoryHandle,
    existing: &DiskNamespace<'_>,
    links: &DiskNamespace<'_>,
    current_path: &[u8],
    allow_ambiguous: bool,
) -> VfsResult<()> {
    for entry in workspace.enumerate_at(directory)? {
        let entry = entry?;
        let path = child_path(current_path, &entry.name);
        match entry.kind {
            NativeKind::Directory => {
                let child =
                    workspace.open_directory_at(directory, &entry.name, Some(&entry.token))?;
                seed_existing_hard_links(
                    workspace,
                    child.as_ref(),
                    existing,
                    links,
                    &path,
                    allow_ambiguous,
                )?;
            }
            NativeKind::RegularFile => {
                if let (Some(key), Some(inode)) = (
                    entry.hard_link_key,
                    existing_inode(existing, &path, InodeKind::RegularFile)?,
                ) {
                    if let Some(prior) = links.get(&key)? {
                        if prior.len() == 32 && prior.as_slice() != inode.as_bytes() {
                            if allow_ambiguous {
                                links.put(&key, &[])?;
                                continue;
                            }
                            return Err(VfsError::InvalidState);
                        }
                    } else {
                        links.put(&key, inode.as_bytes())?;
                    }
                }
            }
            NativeKind::Symlink => {}
        }
    }
    Ok(())
}

pub(crate) fn live_hard_link_authority_working(
    working: &WorkingStore,
    workspace: &dyn ProjectionWorkspace,
    root: layerfs_core::ObjectId,
) -> VfsResult<(DiskTable, OperationCounters)> {
    let mut counters = OperationCounters::default();
    let scratch = working
        .create_scratch_table("live")
        .map_err(working_error)?;
    let paths = scratch.namespace(b"paths")?;
    let topology = scratch.namespace(b"topology")?;
    let links = scratch.namespace(b"authority")?;
    seed_existing_paths(working, root, &paths, Some(&topology), &mut counters)?;
    let directory = workspace.root_directory()?;
    seed_existing_hard_links(workspace, directory.as_ref(), &paths, &links, &[], false)?;
    counters.add_scratch(scratch.observation()?)?;
    Ok((scratch, counters))
}

fn seed_existing_paths<S: ObjectRead>(
    engine: &S,
    root: layerfs_core::ObjectId,
    paths: &DiskNamespace<'_>,
    topology: Option<&DiskNamespace<'_>>,
    counters: &mut OperationCounters,
) -> VfsResult<InodeTableRoot> {
    let namespace = engine.with_authenticated_canonical(root, decode_namespace_root)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    seed_existing_directory(
        engine,
        table,
        namespace.root_directory_inode,
        Vec::new(),
        paths,
        topology,
        counters,
    )?;
    Ok(table)
}

fn seed_existing_directory<S: ObjectRead>(
    engine: &S,
    table: InodeTableRoot,
    inode: InodeId,
    path: Vec<u8>,
    paths: &DiskNamespace<'_>,
    topology: Option<&DiskNamespace<'_>>,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let record = existing_record(engine, table, inode, counters)?;
    if record.kind != InodeKind::Directory {
        return Err(VfsError::InvalidState);
    }
    paths.put(&path, &encode_existing(inode, record.kind))?;
    let mut callback_error = None;
    let mut namespace_counters = NamespaceCounters::default();
    let visited = visit_directory_entries(
        engine,
        DirectoryStateRoot(record.content_root),
        &mut namespace_counters,
        |entries| {
            for (name, child) in entries {
                if let Some(topology) = topology {
                    if let Err(error) =
                        topology.put(&topology_edge_key(*child, inode, name.as_bytes()), &[])
                    {
                        callback_error = Some(error.into());
                        return Err(layerfs_core::CoreError::Io);
                    }
                }
                let child_record = match existing_record(engine, table, *child, counters) {
                    Ok(record) => record,
                    Err(error) => {
                        callback_error = Some(error);
                        return Err(layerfs_core::CoreError::Io);
                    }
                };
                let child_path = child_path(&path, name.as_bytes());
                if child_record.kind == InodeKind::Directory {
                    if let Err(error) = seed_existing_directory(
                        engine, table, *child, child_path, paths, topology, counters,
                    ) {
                        callback_error = Some(error);
                        return Err(layerfs_core::CoreError::Io);
                    }
                } else if let Err(error) =
                    paths.put(&child_path, &encode_existing(*child, child_record.kind))
                {
                    callback_error = Some(error.into());
                    return Err(layerfs_core::CoreError::Io);
                }
            }
            Ok(())
        },
    );
    if let Some(error) = callback_error {
        return Err(error);
    }
    visited?;
    counters.add_namespace(namespace_counters)?;
    Ok(())
}

fn existing_record<S: ObjectRead>(
    source: &S,
    table: InodeTableRoot,
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<InodeRecordV1> {
    let mut inode_counters = InodeTableCounters::default();
    let id = inode_table_lookup(source, table, inode, &mut inode_counters)?
        .ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(inode_counters)?;
    Ok(source.with_authenticated_canonical(id, decode_inode_record)?)
}

fn put_record(
    publication: &mut impl CaptureStore,
    table: &mut Option<GeneratedInodeTable>,
    inode: InodeId,
    record: InodeRecordV1,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let id = publication.put(&encode_inode_record(record)?)?;
    *table = Some(match table.take() {
        Some(root) => {
            let (root, inode_counters) =
                generated_inode_table_upsert(publication, root, inode, id)?;
            counters.add_inode_table(inode_counters)?;
            root
        }
        None => generated_inode_table_from_root(publication, inode, id)?,
    });
    Ok(())
}

pub(crate) fn put_metadata_observed(
    publication: &mut impl CaptureStore,
    kind: InodeKind,
    native: &NativeMetadata,
    counters: &mut OperationCounters,
) -> VfsResult<layerfs_core::ObjectId> {
    spooled_metadata_len(native)?;
    let portable = PortableMetadataV1 {
        permission_mode: native.mode,
        mtime_seconds: native.mtime_seconds,
        mtime_nanoseconds: native.mtime_nanoseconds,
    };
    portable.validate(kind)?;
    let mut tree = MetadataTreeBuilder::new();
    if let Some(acl) = &native.acl {
        decode_apple_acl(acl)?;
        let entry = metadata_value(
            publication,
            MetadataKey::new("apple.acl".into(), Vec::new())?,
            acl,
            counters,
        )?;
        tree.push(publication, entry)?;
    }
    if let Some(flags) = encode_bsd_flags(native.bsd_flags)? {
        let entry = metadata_value(
            publication,
            MetadataKey::new("apple.bsd-flags".into(), Vec::new())?,
            &flags,
            counters,
        )?;
        tree.push(publication, entry)?;
    }
    for (name, value) in &native.xattrs {
        let entry = metadata_value(
            publication,
            MetadataKey::new("apple.xattr".into(), name)?,
            &value,
            counters,
        )?;
        tree.push(publication, entry)?;
    }
    let mode = portable.mode_bytes(kind)?;
    let entry = metadata_value(
        publication,
        MetadataKey::new("portable".into(), b"mode".to_vec())?,
        &mode,
        counters,
    )?;
    tree.push(publication, entry)?;
    let mtime = portable.mtime_bytes()?;
    let entry = metadata_value(
        publication,
        MetadataKey::new("portable".into(), b"mtime".to_vec())?,
        &mtime,
        counters,
    )?;
    tree.push(publication, entry)?;
    Ok(tree.finish(publication)?)
}

fn metadata_value(
    publication: &mut impl CaptureStore,
    key: MetadataKey,
    value: &[u8],
    counters: &mut OperationCounters,
) -> VfsResult<MetadataEntryV1> {
    let (root, rope) = build(publication, Cursor::new(value))?;
    counters.add_metadata_rope(rope)?;
    Ok(MetadataEntryV1 {
        key,
        value_file_root: root.0,
    })
}

pub(crate) fn spooled_metadata_len(metadata: &NativeMetadata) -> VfsResult<u64> {
    let acl = metadata
        .acl
        .as_deref()
        .map(|acl| {
            decode_apple_acl(acl)?;
            if acl.len() > 4_620 {
                return Err(VfsError::InvalidState);
            }
            Ok(acl.len() as u64)
        })
        .transpose()?
        .unwrap_or(0);
    let mut len = 36_u64.checked_add(acl).ok_or(VfsError::InvalidState)?;
    for (name, value) in &metadata.xattrs {
        len = len
            .checked_add(6)
            .and_then(|total| total.checked_add(name.len() as u64))
            .and_then(|total| total.checked_add(value.len() as u64))
            .ok_or(VfsError::InvalidState)?;
    }
    let maximum = 36_u64 + 4_620 + 7 * MAX_NATIVE_XATTR_BYTES as u64;
    if len > maximum {
        return Err(VfsError::InvalidState);
    }
    Ok(len)
}
