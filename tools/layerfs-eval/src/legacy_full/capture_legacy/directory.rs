use super::super::session_legacy::{VfsError, VfsResult};
use super::super::OperationCounters;
use super::hard_links::{child_path, existing_inode};
use super::metadata::{put_metadata_observed, put_record};
use super::{capture_regular, SemanticDigestCache};
use layerfs_core::inode::{GeneratedInodeTable, InodeId, InodeKind, InodeRecordV1, InodeTableRoot};
use layerfs_core::namespace::{directory_insert, empty_directory};
use layerfs_core::namespace_codec::encode_symlink;
use layerfs_materialization::driver::*;
use layerfs_storage::publication::Publication;
use layerfs_storage::scratch::{DiskNamespace, DiskTable};
#[allow(clippy::too_many_arguments)]
pub(super) fn capture_directory(
    workspace: &dyn ProjectionWorkspace,
    directory: &dyn DirectoryHandle,
    digest_cache: &SemanticDigestCache,
    inode: InodeId,
    root: bool,
    native_metadata: NativeMetadata,
    publication: &mut Publication<'_>,
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

pub(super) fn encode_entry(entry: &NativeEntry) -> VfsResult<Vec<u8>> {
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

pub(super) fn decode_entry(name: Vec<u8>, bytes: &[u8]) -> VfsResult<NativeEntry> {
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
