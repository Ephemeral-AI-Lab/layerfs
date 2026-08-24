use crate::capture::put_metadata;
use crate::driver::*;
use crate::workspace::{VfsError, VfsResult};
use layerfs_core::content::rope::{replace as replace_rope, FileStateRoot, ObjectRead};
use layerfs_core::inode::{
    inode_table_lookup, inode_table_upsert, InodeKind, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::namespace::{
    directory_insert, directory_lookup, directory_remove, directory_rename, DirectoryStateRoot,
    NamespaceCounters, NamespaceRootV1,
};
use layerfs_core::namespace_codec::{
    decode_inode_record, decode_namespace_root, encode_inode_record, encode_namespace_root,
};
use layerfs_core::{CanonicalName, CanonicalPath};
use layerfs_engine::publication::Publication;
use layerfs_engine::refs::RefState;
use layerfs_engine::Engine;
use std::io::{Read, SeekFrom};

pub enum ManagedEdit {
    Replace {
        path: CanonicalPath,
        start: u64,
        delete_len: u64,
        spool_offset: u64,
        replacement_len: u64,
        metadata_offset: u64,
        metadata_len: u64,
    },
    Rename {
        from: CanonicalPath,
        to: CanonicalPath,
        source_metadata_offset: u64,
        source_metadata_len: u64,
        target_metadata_offset: u64,
        target_metadata_len: u64,
    },
}

pub fn native_hard_link_key(
    native: &dyn ProjectionWorkspace,
    path: &CanonicalPath,
) -> VfsResult<Vec<u8>> {
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    native
        .identity_at(parent.as_ref(), name)
        .map_err(Into::into)
}

pub fn mutate_native(
    native: &dyn ProjectionWorkspace,
    path: &CanonicalPath,
    start: u64,
    delete_len: u64,
    replacement: &[u8],
) -> VfsResult<NativeMetadata> {
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    let original_metadata = native.read_metadata_at(parent.as_ref(), name, None)?;
    let protected = original_metadata.bsd_flags & 0x0000_0006 != 0;
    let mut file = if protected {
        native.open_regular_read_at(parent.as_ref(), name, None)?
    } else {
        native.open_regular_at(parent.as_ref(), name, None)?
    };
    let length = file.seek(SeekFrom::End(0))?;
    let end = start
        .checked_add(delete_len)
        .ok_or(VfsError::InvalidState)?;
    if end > length {
        return Err(VfsError::InvalidState);
    }
    if protected {
        if delete_len == replacement.len() as u64 {
            file.seek(SeekFrom::Start(start))?;
            let mut offset = 0;
            let mut buffer = [0_u8; 64 * 1024];
            while offset < replacement.len() {
                let count = (replacement.len() - offset).min(buffer.len());
                file.read_exact(&mut buffer[..count])?;
                if buffer[..count] != replacement[offset..offset + count] {
                    return Err(VfsError::NativeProtected);
                }
                offset += count;
            }
            return Ok(original_metadata);
        }
        return Err(VfsError::NativeProtected);
    }
    if delete_len == replacement.len() as u64 {
        match native.clone_temp_from_regular(file.as_ref()) {
            Ok(mut temp) => {
                temp.seek(SeekFrom::Start(start))?;
                temp.write_all(replacement)?;
                temp.flush()?;
                let metadata = native.read_temp_metadata(temp.as_ref())?;
                native.set_temp_metadata(temp.as_mut(), &metadata)?;
                native.atomic_replace(temp, parent.as_ref(), name)?;
                return native
                    .read_metadata_at(parent.as_ref(), name, None)
                    .map_err(Into::into);
            }
            Err(DriverError::Unsupported) => {}
            Err(error) => return Err(error.into()),
        }
    }
    replace_native(native, file.as_mut(), start, delete_len, replacement)?;
    native
        .read_metadata_at(parent.as_ref(), name, None)
        .map_err(Into::into)
}

pub fn rename_native(
    native: &dyn ProjectionWorkspace,
    from: &CanonicalPath,
    to: &CanonicalPath,
) -> VfsResult<(NativeMetadata, NativeMetadata)> {
    let (source_parent, source) = native_parent(native, native.root_directory()?, from)?;
    let (target_parent, target) = native_parent(native, native.root_directory()?, to)?;
    if native
        .read_metadata_at(source_parent.as_ref(), source, None)?
        .bsd_flags
        & 0x0000_0006
        != 0
        || native
            .read_directory_metadata(source_parent.as_ref())?
            .bsd_flags
            & 0x0000_0006
            != 0
        || native
            .read_directory_metadata(target_parent.as_ref())?
            .bsd_flags
            & 0x0000_0006
            != 0
    {
        return Err(VfsError::NativeProtected);
    }
    native.rename_at(
        source_parent.as_ref(),
        source,
        target_parent.as_ref(),
        target,
    )?;
    Ok((
        native.read_directory_metadata(source_parent.as_ref())?,
        native.read_directory_metadata(target_parent.as_ref())?,
    ))
}

pub fn replay(
    engine: &Engine,
    expected: &RefState,
    edits: &[ManagedEdit],
    spool: &mut dyn OwnedTempHandle,
) -> VfsResult<RefState> {
    let mut namespace = decode_namespace_root(&engine.load_object(expected.root)?.canonical_bytes)?;
    let mut publication = engine
        .begin_publication(Some(expected), &expected.name)
        .map_err(|error| {
            if matches!(error, layerfs_engine::EngineError::PublicationConflict) {
                VfsError::ExternalDirtyConflict
            } else {
                error.into()
            }
        })?;
    for edit in edits {
        namespace = match edit {
            ManagedEdit::Replace {
                path,
                start,
                delete_len,
                spool_offset,
                replacement_len,
                metadata_offset,
                metadata_len,
            } => replay_replace(
                &mut publication,
                namespace,
                path,
                *start,
                *delete_len,
                *spool_offset,
                *replacement_len,
                &load_spooled_metadata(spool, *metadata_offset, *metadata_len)?,
                spool,
            )?,
            ManagedEdit::Rename {
                from,
                to,
                source_metadata_offset,
                source_metadata_len,
                target_metadata_offset,
                target_metadata_len,
            } => replay_rename(
                &mut publication,
                namespace,
                from,
                to,
                &load_spooled_metadata(spool, *source_metadata_offset, *source_metadata_len)?,
                &load_spooled_metadata(spool, *target_metadata_offset, *target_metadata_len)?,
            )?,
        };
    }
    publication
        .publish_namespace(&encode_namespace_root(namespace)?)
        .map_err(Into::into)
}

pub(crate) fn spool_metadata(
    spool: &mut dyn OwnedTempHandle,
    metadata: &NativeMetadata,
) -> VfsResult<(u64, u64)> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"LFSMETA1");
    bytes.extend_from_slice(&metadata.mode.to_be_bytes());
    bytes.extend_from_slice(&metadata.mtime_seconds.to_be_bytes());
    bytes.extend_from_slice(&metadata.mtime_nanoseconds.to_be_bytes());
    bytes.extend_from_slice(&metadata.bsd_flags.to_be_bytes());
    bytes.extend_from_slice(
        &metadata
            .acl
            .as_ref()
            .map(|acl| u32::try_from(acl.len()).map_err(|_| VfsError::InvalidState))
            .transpose()?
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    bytes.extend_from_slice(
        &u32::try_from(metadata.xattrs.len())
            .map_err(|_| VfsError::InvalidState)?
            .to_be_bytes(),
    );
    if let Some(acl) = &metadata.acl {
        bytes.extend_from_slice(acl);
    }
    for (name, value) in &metadata.xattrs {
        bytes.extend_from_slice(
            &u16::try_from(name.len())
                .map_err(|_| VfsError::InvalidState)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(
            &u32::try_from(value.len())
                .map_err(|_| VfsError::InvalidState)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(value);
    }
    if bytes.len() > MAX_NATIVE_XATTR_BYTES + 128 * 1024 {
        return Err(VfsError::InvalidState);
    }
    let offset = spool.seek(SeekFrom::End(0))?;
    spool.write_all(&bytes)?;
    Ok((
        offset,
        u64::try_from(bytes.len()).map_err(|_| VfsError::InvalidState)?,
    ))
}

fn load_spooled_metadata(
    spool: &mut dyn OwnedTempHandle,
    offset: u64,
    len: u64,
) -> VfsResult<NativeMetadata> {
    let len = usize::try_from(len).map_err(|_| VfsError::InvalidState)?;
    if len > MAX_NATIVE_XATTR_BYTES + 128 * 1024 {
        return Err(VfsError::InvalidState);
    }
    spool.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0_u8; len];
    spool.read_exact(&mut bytes)?;
    let mut cursor = 0_usize;
    let mut take = |len: usize| -> VfsResult<&[u8]> {
        let end = cursor.checked_add(len).ok_or(VfsError::InvalidState)?;
        let value = bytes.get(cursor..end).ok_or(VfsError::InvalidState)?;
        cursor = end;
        Ok(value)
    };
    if take(8)? != b"LFSMETA1" {
        return Err(VfsError::InvalidState);
    }
    let mode = u32::from_be_bytes(take(4)?.try_into().unwrap());
    let mtime_seconds = i64::from_be_bytes(take(8)?.try_into().unwrap());
    let mtime_nanoseconds = u32::from_be_bytes(take(4)?.try_into().unwrap());
    let bsd_flags = u32::from_be_bytes(take(4)?.try_into().unwrap());
    let acl_len = u32::from_be_bytes(take(4)?.try_into().unwrap());
    let count = u32::from_be_bytes(take(4)?.try_into().unwrap());
    let acl = (acl_len != u32::MAX)
        .then(|| take(acl_len as usize).map(<[u8]>::to_vec))
        .transpose()?;
    let mut xattrs = Vec::new();
    for _ in 0..count {
        let name_len = u16::from_be_bytes(take(2)?.try_into().unwrap()) as usize;
        let value_len = u32::from_be_bytes(take(4)?.try_into().unwrap()) as usize;
        let name = take(name_len)?.to_vec();
        let value = take(value_len)?.to_vec();
        xattrs.push((name, value));
    }
    if cursor != bytes.len() {
        return Err(VfsError::InvalidState);
    }
    Ok(NativeMetadata {
        mode,
        mtime_seconds,
        mtime_nanoseconds,
        xattrs,
        acl,
        bsd_flags,
    })
}

#[allow(clippy::too_many_arguments)]
fn replay_replace(
    publication: &mut Publication<'_>,
    namespace: NamespaceRootV1,
    path: &CanonicalPath,
    start: u64,
    delete_len: u64,
    spool_offset: u64,
    replacement_len: u64,
    metadata: &NativeMetadata,
    spool: &mut dyn OwnedTempHandle,
) -> VfsResult<NamespaceRootV1> {
    let (inode, record) = resolve(publication, namespace, path)?;
    if record.kind != InodeKind::RegularFile {
        return Err(VfsError::InvalidState);
    }
    spool.seek(SeekFrom::Start(spool_offset))?;
    let mut replacement = (&mut *spool).take(replacement_len);
    let (content, _) = replace_rope(
        publication,
        FileStateRoot(record.content_root),
        start,
        delete_len,
        &mut replacement,
    )?;
    if replacement.limit() != 0 {
        return Err(VfsError::InvalidState);
    }
    let metadata_root = put_metadata(publication, InodeKind::RegularFile, metadata)?;
    let record_id =
        publication.put_object(&encode_inode_record(layerfs_core::inode::InodeRecordV1 {
            content_root: content.0,
            metadata_root,
            ..record
        })?)?;
    let table = inode_table_upsert(
        publication,
        InodeTableRoot(namespace.inode_table_root),
        inode,
        record_id,
    )?
    .0;
    Ok(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })
}

fn replay_rename(
    publication: &mut Publication<'_>,
    namespace: NamespaceRootV1,
    from: &CanonicalPath,
    to: &CanonicalPath,
    source_parent_metadata: &NativeMetadata,
    target_parent_metadata: &NativeMetadata,
) -> VfsResult<NamespaceRootV1> {
    let (source_inode, source_record, source_name) = resolve_parent(publication, namespace, from)?;
    let (target_inode, target_record, target_name) = resolve_parent(publication, namespace, to)?;
    let mut table = InodeTableRoot(namespace.inode_table_root);
    if source_inode == target_inode {
        let directory = directory_rename(
            publication,
            DirectoryStateRoot(source_record.content_root),
            &source_name,
            target_name,
        )?
        .0;
        let metadata_root =
            put_metadata(publication, InodeKind::Directory, source_parent_metadata)?;
        let id =
            publication.put_object(&encode_inode_record(layerfs_core::inode::InodeRecordV1 {
                content_root: directory.0,
                metadata_root,
                ..source_record
            })?)?;
        table = inode_table_upsert(publication, table, source_inode, id)?.0;
    } else {
        let (source_directory, moved, _) = directory_remove(
            publication,
            DirectoryStateRoot(source_record.content_root),
            &source_name,
        )?;
        let target_directory = directory_insert(
            publication,
            DirectoryStateRoot(target_record.content_root),
            target_name,
            moved,
        )?
        .0;
        let source_metadata_root =
            put_metadata(publication, InodeKind::Directory, source_parent_metadata)?;
        let source_id =
            publication.put_object(&encode_inode_record(layerfs_core::inode::InodeRecordV1 {
                content_root: source_directory.0,
                metadata_root: source_metadata_root,
                ..source_record
            })?)?;
        table = inode_table_upsert(publication, table, source_inode, source_id)?.0;
        let target_metadata_root =
            put_metadata(publication, InodeKind::Directory, target_parent_metadata)?;
        let target_id =
            publication.put_object(&encode_inode_record(layerfs_core::inode::InodeRecordV1 {
                content_root: target_directory.0,
                metadata_root: target_metadata_root,
                ..target_record
            })?)?;
        table = inode_table_upsert(publication, table, target_inode, target_id)?.0;
    }
    Ok(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })
}

fn resolve_parent<S: ObjectRead>(
    store: &S,
    namespace: NamespaceRootV1,
    path: &CanonicalPath,
) -> VfsResult<(
    layerfs_core::inode::InodeId,
    layerfs_core::inode::InodeRecordV1,
    CanonicalName,
)> {
    let components = path.components().collect::<Vec<_>>();
    let (name, parents) = components.split_last().ok_or(VfsError::InvalidState)?;
    let parent_bytes =
        parents
            .iter()
            .enumerate()
            .fold(Vec::new(), |mut bytes, (index, component)| {
                if index != 0 {
                    bytes.push(b'/');
                }
                bytes.extend_from_slice(component);
                bytes
            });
    let (inode, record) = resolve(store, namespace, &CanonicalPath::from_bytes(&parent_bytes)?)?;
    if record.kind != InodeKind::Directory {
        return Err(VfsError::InvalidState);
    }
    Ok((inode, record, CanonicalName::from_bytes(name)?))
}

fn resolve<S: ObjectRead>(
    store: &S,
    namespace: NamespaceRootV1,
    path: &CanonicalPath,
) -> VfsResult<(
    layerfs_core::inode::InodeId,
    layerfs_core::inode::InodeRecordV1,
)> {
    let table = InodeTableRoot(namespace.inode_table_root);
    let mut inode = namespace.root_directory_inode;
    let mut record = load_record(store, table, inode)?;
    for component in path.components() {
        if record.kind != InodeKind::Directory {
            return Err(VfsError::InvalidState);
        }
        let name = CanonicalName::from_bytes(component)?;
        inode = directory_lookup(
            store,
            DirectoryStateRoot(record.content_root),
            &name,
            &mut NamespaceCounters::default(),
        )?
        .ok_or(VfsError::InvalidState)?;
        record = load_record(store, table, inode)?;
    }
    Ok((inode, record))
}

fn load_record<S: ObjectRead>(
    store: &S,
    table: InodeTableRoot,
    inode: layerfs_core::inode::InodeId,
) -> VfsResult<layerfs_core::inode::InodeRecordV1> {
    let id = inode_table_lookup(store, table, inode, &mut InodeTableCounters::default())?
        .ok_or(VfsError::InvalidState)?;
    Ok(decode_inode_record(&store.get(id)?)?)
}

fn native_parent<'a>(
    workspace: &dyn ProjectionWorkspace,
    mut directory: Box<dyn DirectoryHandle>,
    path: &'a CanonicalPath,
) -> VfsResult<(Box<dyn DirectoryHandle>, &'a [u8])> {
    let components = path.components().collect::<Vec<_>>();
    let (name, parents) = components.split_last().ok_or(VfsError::InvalidState)?;
    for component in parents {
        directory = workspace.open_directory_at(directory.as_ref(), component, None)?;
    }
    Ok((directory, name))
}

fn replace_native(
    workspace: &dyn ProjectionWorkspace,
    file: &mut dyn RegularFileHandle,
    start: u64,
    delete_len: u64,
    replacement: &[u8],
) -> VfsResult<()> {
    let length = file.seek(SeekFrom::End(0))?;
    let end = start
        .checked_add(delete_len)
        .ok_or(VfsError::InvalidState)?;
    if end > length {
        return Err(VfsError::InvalidState);
    }
    let replacement_len = u64::try_from(replacement.len()).map_err(|_| VfsError::InvalidState)?;
    let next_len = length
        .checked_sub(delete_len)
        .and_then(|value| value.checked_add(replacement_len))
        .ok_or(VfsError::InvalidState)?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    if next_len > length {
        workspace.set_regular_len(file, next_len)?;
        let mut remaining = length - end;
        while remaining > 0 {
            let count = remaining.min(buffer.len() as u64);
            let source = end + remaining - count;
            file.seek(SeekFrom::Start(source))?;
            file.read_exact(&mut buffer[..count as usize])?;
            file.seek(SeekFrom::Start(source + (next_len - length)))?;
            file.write_all(&buffer[..count as usize])?;
            remaining -= count;
        }
    } else if next_len < length {
        let mut source = end;
        while source < length {
            let count = (length - source).min(buffer.len() as u64);
            file.seek(SeekFrom::Start(source))?;
            file.read_exact(&mut buffer[..count as usize])?;
            file.seek(SeekFrom::Start(source - (length - next_len)))?;
            file.write_all(&buffer[..count as usize])?;
            source += count;
        }
        workspace.set_regular_len(file, next_len)?;
    }
    file.seek(SeekFrom::Start(start))?;
    file.write_all(replacement)?;
    file.flush()?;
    Ok(())
}
