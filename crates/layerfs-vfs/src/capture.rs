use crate::driver::*;
use crate::workspace::{VfsError, VfsResult};
use crate::{NativeRoute, OperationCounters};
use layerfs_core::content::rope::build;
use layerfs_core::inode::{
    generated_inode_table_from_root, generated_inode_table_upsert, inode_table_from_root,
    inode_table_lookup, GeneratedInodeTable, InodeId, InodeKind, InodeRecordV1, InodeTableCounters,
    InodeTableRoot,
};
use layerfs_core::metadata::{
    build_metadata_tree, decode_apple_acl, encode_bsd_flags, MetadataEntryV1, MetadataKey,
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
use layerfs_engine::publication::Publication;
use layerfs_engine::refs::RefState;
use layerfs_engine::scratch::DiskTable;
use layerfs_engine::Engine;
use std::io::Cursor;

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

pub(crate) fn capture_workspace(
    engine: &Engine,
    workspace: &dyn ProjectionWorkspace,
    expected: Option<&RefState>,
    live_hard_links: Option<&DiskTable>,
    seed_live_hard_links: bool,
    require_same_root: bool,
) -> VfsResult<(RefState, OperationCounters)> {
    let mut counters = OperationCounters::default();
    counters.native.route = Some(NativeRoute::CaptureStream);
    workspace.revalidate_root_binding()?;
    let root_handle = workspace.root_directory()?;
    let root_token = workspace.directory_token(root_handle.as_ref())?;
    let existing = DiskTable::create_near(engine.path(), "existing-paths")?;
    if let Some(expected) = expected {
        seed_existing_paths(engine, expected.root, &existing, &mut counters)?;
    }
    let seeded_links = DiskTable::create_near(engine.path(), "existing-hardlinks")?;
    if seed_live_hard_links {
        seed_existing_hard_links(
            workspace,
            root_handle.as_ref(),
            &existing,
            &seeded_links,
            &[],
        )?;
    }
    let existing_links = live_hard_links.or(seed_live_hard_links.then_some(&seeded_links));
    let mut publication = engine.begin_publication(expected, "main")?;
    let root_inode = match existing_inode(&existing, b"", InodeKind::Directory)? {
        Some(inode) => inode,
        None => publication.allocate_inode_id()?,
    };
    let mut table = None;
    let hard_links = DiskTable::create_near(engine.path(), "hardlinks")?;
    let entries = DiskTable::create_near(engine.path(), "enumeration")?;
    let mut next_directory = 0_u64;
    let root_metadata = workspace.read_root_metadata()?;
    capture_directory(
        workspace,
        root_handle.as_ref(),
        root_inode,
        true,
        root_metadata,
        &mut publication,
        &mut table,
        &hard_links,
        &entries,
        &existing,
        existing_links,
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
                layerfs_engine::EngineError::InvalidRecord("hard-link scratch value")
            })?;
            if link.observed != link.expected {
                return Err(layerfs_engine::EngineError::InvalidRecord(
                    "external hard-link boundary",
                ));
            }
            Ok(())
        })
        .map_err(|error| {
            if matches!(
                error,
                layerfs_engine::EngineError::InvalidRecord("external hard-link boundary")
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
    if require_same_root
        && expected.is_none_or(|state| layerfs_core::ObjectId::for_bytes(&namespace) != state.root)
    {
        return Err(VfsError::ExternalDirtyConflict);
    }
    Ok((publication.publish_namespace(&namespace)?, counters))
}

pub fn initialize_empty(engine: &Engine) -> VfsResult<RefState> {
    let mut publication = engine.begin_publication(None, "main")?;
    let root_inode = publication.allocate_inode_id()?;
    let directory = empty_directory(&mut publication)?;
    let metadata = put_metadata(
        &mut publication,
        InodeKind::Directory,
        &NativeMetadata {
            mode: 0o755,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            xattrs: Vec::new(),
            acl: None,
            bsd_flags: 0,
        },
    )?;
    let record = InodeRecordV1 {
        kind: InodeKind::Directory,
        namespace_ref_count: 0,
        content_root: directory.0,
        metadata_root: metadata,
    };
    let record_id = publication.put_object(&encode_inode_record(record)?)?;
    let table = inode_table_from_root(&mut publication, root_inode, record_id)?;
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: table.0,
    })?;
    Ok(publication.publish_namespace(&namespace)?)
}

#[allow(clippy::too_many_arguments)]
fn capture_directory(
    workspace: &dyn ProjectionWorkspace,
    directory: &dyn DirectoryHandle,
    inode: InodeId,
    root: bool,
    native_metadata: NativeMetadata,
    publication: &mut Publication<'_>,
    table: &mut Option<GeneratedInodeTable>,
    hard_links: &DiskTable,
    entries: &DiskTable,
    existing: &DiskTable,
    existing_links: Option<&DiskTable>,
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
                    child_inode,
                    false,
                    metadata,
                    publication,
                    table,
                    hard_links,
                    entries,
                    existing,
                    existing_links,
                    &path,
                    next_directory,
                    counters,
                )?;
                child_inode
            }
            NativeKind::RegularFile => capture_regular(
                workspace,
                directory,
                &entry,
                publication,
                table,
                hard_links,
                existing_links,
                counters,
            )?,
            NativeKind::Symlink => {
                let child_inode = match existing_inode(existing, &path, InodeKind::Symlink)? {
                    Some(inode) => inode,
                    None => publication.allocate_inode_id()?,
                };
                let target = workspace.read_link_at(directory, &entry.name, Some(&entry.token))?;
                let content = publication.put_object(&encode_symlink(
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
    entry: &NativeEntry,
    publication: &mut Publication<'_>,
    table: &mut Option<GeneratedInodeTable>,
    hard_links: &DiskTable,
    existing_links: Option<&DiskTable>,
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
    let inode = match existing_links
        .map(|links| links.get(&key))
        .transpose()?
        .flatten()
    {
        Some(bytes) => InodeId::from_slice(&bytes)?,
        None => publication.allocate_inode_id()?,
    };
    let mut file = workspace.open_regular_read_at(parent, &entry.name, Some(&entry.token))?;
    let (content, rope) = build(publication, &mut file)?;
    counters.native.bytes_read = counters
        .native
        .bytes_read
        .checked_add(rope.cdc_bytes_scanned)
        .ok_or(VfsError::InvalidState)?;
    counters.add_rope(rope)?;
    let metadata = put_metadata_observed(
        publication,
        InodeKind::RegularFile,
        &workspace.read_metadata_at(parent, &entry.name, Some(&entry.token))?,
        counters,
    )?;
    let record = InodeRecordV1 {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: content.0,
        metadata_root: metadata,
    };
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
    existing: &DiskTable,
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
    existing: &DiskTable,
    links: &DiskTable,
    current_path: &[u8],
) -> VfsResult<()> {
    for entry in workspace.enumerate_at(directory)? {
        let entry = entry?;
        let path = child_path(current_path, &entry.name);
        match entry.kind {
            NativeKind::Directory => {
                let child =
                    workspace.open_directory_at(directory, &entry.name, Some(&entry.token))?;
                seed_existing_hard_links(workspace, child.as_ref(), existing, links, &path)?;
            }
            NativeKind::RegularFile => {
                if let (Some(key), Some(inode)) = (
                    entry.hard_link_key,
                    existing_inode(existing, &path, InodeKind::RegularFile)?,
                ) {
                    if let Some(prior) = links.get(&key)? {
                        if prior.as_slice() != inode.as_bytes() {
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

pub(crate) fn live_hard_link_authority(
    engine: &Engine,
    workspace: &dyn ProjectionWorkspace,
    root: layerfs_core::ObjectId,
) -> VfsResult<(DiskTable, OperationCounters)> {
    let mut counters = OperationCounters::default();
    let paths = DiskTable::create_near(engine.path(), "live-hardlink-paths")?;
    seed_existing_paths(engine, root, &paths, &mut counters)?;
    let links = DiskTable::create_near(engine.path(), "live-hardlink-authority")?;
    let directory = workspace.root_directory()?;
    seed_existing_hard_links(workspace, directory.as_ref(), &paths, &links, &[])?;
    Ok((links, counters))
}

fn seed_existing_paths(
    engine: &Engine,
    root: layerfs_core::ObjectId,
    paths: &DiskTable,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let namespace = decode_namespace_root(&engine.load_object(root)?.canonical_bytes)?;
    seed_existing_directory(
        engine,
        InodeTableRoot(namespace.inode_table_root),
        namespace.root_directory_inode,
        Vec::new(),
        paths,
        counters,
    )
}

fn seed_existing_directory(
    engine: &Engine,
    table: InodeTableRoot,
    inode: InodeId,
    path: Vec<u8>,
    paths: &DiskTable,
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
                let child_record = match existing_record(engine, table, *child, counters) {
                    Ok(record) => record,
                    Err(error) => {
                        callback_error = Some(error);
                        return Err(layerfs_core::CoreError::Io);
                    }
                };
                let child_path = child_path(&path, name.as_bytes());
                if child_record.kind == InodeKind::Directory {
                    if let Err(error) =
                        seed_existing_directory(engine, table, *child, child_path, paths, counters)
                    {
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

fn existing_record(
    engine: &Engine,
    table: InodeTableRoot,
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<InodeRecordV1> {
    let mut inode_counters = InodeTableCounters::default();
    let id = inode_table_lookup(engine, table, inode, &mut inode_counters)?
        .ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(inode_counters)?;
    Ok(decode_inode_record(
        &engine.load_object(id)?.canonical_bytes,
    )?)
}

fn put_record(
    publication: &mut Publication<'_>,
    table: &mut Option<GeneratedInodeTable>,
    inode: InodeId,
    record: InodeRecordV1,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let id = publication.put_object(&encode_inode_record(record)?)?;
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

pub(crate) fn put_metadata(
    publication: &mut Publication<'_>,
    kind: InodeKind,
    native: &NativeMetadata,
) -> VfsResult<layerfs_core::ObjectId> {
    put_metadata_observed(publication, kind, native, &mut OperationCounters::default())
}

pub(crate) fn put_metadata_observed(
    publication: &mut Publication<'_>,
    kind: InodeKind,
    native: &NativeMetadata,
    counters: &mut OperationCounters,
) -> VfsResult<layerfs_core::ObjectId> {
    let portable = PortableMetadataV1 {
        permission_mode: native.mode,
        mtime_seconds: native.mtime_seconds,
        mtime_nanoseconds: native.mtime_nanoseconds,
    };
    portable.validate(kind)?;
    let mut values = vec![
        (
            MetadataKey::new("portable".into(), b"mode".to_vec())?,
            portable.mode_bytes(kind)?.to_vec(),
        ),
        (
            MetadataKey::new("portable".into(), b"mtime".to_vec())?,
            portable.mtime_bytes()?.to_vec(),
        ),
    ];
    for (name, value) in &native.xattrs {
        values.push((
            MetadataKey::new("apple.xattr".into(), name.clone())?,
            value.clone(),
        ));
    }
    if let Some(acl) = &native.acl {
        decode_apple_acl(acl)?;
        values.push((
            MetadataKey::new("apple.acl".into(), Vec::new())?,
            acl.clone(),
        ));
    }
    if let Some(flags) = encode_bsd_flags(native.bsd_flags)? {
        values.push((
            MetadataKey::new("apple.bsd-flags".into(), Vec::new())?,
            flags.to_vec(),
        ));
    }
    values.sort_by(|left, right| left.0.cmp(&right.0));
    let mut entries = Vec::with_capacity(values.len());
    for (key, value) in values {
        let (root, rope) = build(publication, Cursor::new(value))?;
        counters.add_rope(rope)?;
        entries.push(MetadataEntryV1 {
            key,
            value_file_root: root.0,
        });
    }
    Ok(build_metadata_tree(publication, &entries)?)
}
