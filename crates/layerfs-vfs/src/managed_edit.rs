use crate::capture::put_metadata_observed;
use crate::driver::*;
use crate::workspace::{VfsError, VfsResult};
use crate::{ManagedReplayStep, NativeOperationCounters, NativeRoute, OperationCounters};
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
use std::io::{Read, Seek, SeekFrom, Write};

pub enum ManagedEdit {
    Replace {
        path: CanonicalPath,
        start: u64,
        delete_len: u64,
        spool_offset: u64,
        replacement_len: u64,
        metadata_offset: u64,
        metadata_len: u64,
        sync_required: bool,
        native_identity: Vec<u8>,
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
) -> VfsResult<(NativeMetadata, NativeOperationCounters, bool)> {
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
            return Ok((
                original_metadata,
                NativeOperationCounters {
                    route: Some(NativeRoute::ProtectedExactNoop),
                    bytes_read: replacement.len() as u64,
                    ..NativeOperationCounters::default()
                },
                false,
            ));
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
                return Ok((
                    native.read_metadata_at(parent.as_ref(), name, None)?,
                    NativeOperationCounters {
                        route: Some(NativeRoute::ClonePatch),
                        bytes_written: replacement.len() as u64,
                        patch_bytes: replacement.len() as u64,
                        clone_attempts: 1,
                        clone_successes: 1,
                        ..NativeOperationCounters::default()
                    },
                    false,
                ));
            }
            Err(DriverError::Unsupported) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut counters = replace_native(native, file.as_mut(), start, delete_len, replacement)?;
    if delete_len == replacement.len() as u64 {
        counters.clone_attempts = 1;
        counters.clone_fallbacks = 1;
    }
    Ok((
        native.read_metadata_at(parent.as_ref(), name, None)?,
        counters,
        true,
    ))
}

pub fn sync_pending(native: &dyn ProjectionWorkspace, edits: &[ManagedEdit]) -> VfsResult<()> {
    let mut synced = std::collections::BTreeSet::new();
    for (index, edit) in edits.iter().enumerate().rev() {
        let ManagedEdit::Replace {
            path,
            sync_required: true,
            native_identity,
            ..
        } = edit
        else {
            continue;
        };
        let path = translate_later_renames(path, &edits[index + 1..])?;
        if !synced.insert(path.clone()) {
            continue;
        }
        let root = native.root_directory()?;
        let (parent, name) = native_parent(native, root, &path)?;
        if native.identity_at(parent.as_ref(), name)? != *native_identity {
            return Err(VfsError::Indeterminate);
        }
        let current_token = native.token_at(parent.as_ref(), name)?;
        let mut file = native.open_regular_at(parent.as_ref(), name, Some(&current_token))?;
        native.sync_regular(file.as_mut())?;
        if native.token_at(parent.as_ref(), name)? != current_token
            || native.identity_at(parent.as_ref(), name)? != *native_identity
        {
            return Err(VfsError::Indeterminate);
        }
    }
    Ok(())
}

fn translate_later_renames(
    path: &CanonicalPath,
    edits: &[ManagedEdit],
) -> VfsResult<CanonicalPath> {
    let mut bytes = path.as_bytes().to_vec();
    for edit in edits {
        let ManagedEdit::Rename { from, to, .. } = edit else {
            continue;
        };
        let source = from.as_bytes();
        if bytes == source
            || bytes
                .strip_prefix(source)
                .is_some_and(|suffix| suffix.first() == Some(&b'/'))
        {
            let suffix = &bytes[source.len()..];
            let mut translated = Vec::with_capacity(to.as_bytes().len() + suffix.len());
            translated.extend_from_slice(to.as_bytes());
            translated.extend_from_slice(suffix);
            bytes = translated;
        }
    }
    Ok(CanonicalPath::from_bytes(&bytes)?)
}

pub fn rename_native(
    native: &dyn ProjectionWorkspace,
    from: &CanonicalPath,
    to: &CanonicalPath,
) -> VfsResult<(NativeMetadata, NativeMetadata)> {
    let (source_parent, source) = native_parent(native, native.root_directory()?, from)?;
    let (target_parent, target) = native_parent(native, native.root_directory()?, to)?;
    match native.token_at(target_parent.as_ref(), target) {
        Ok(_) => return Err(VfsError::InvalidState),
        Err(DriverError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
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
    collect_steps: bool,
) -> VfsResult<(RefState, OperationCounters, Vec<ManagedReplayStep>)> {
    let mut counters = OperationCounters::default();
    let mut steps = collect_steps.then(|| Vec::with_capacity(edits.len()));
    let mut namespace =
        engine.with_authenticated_canonical(expected.root, decode_namespace_root)?;
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
        let mut step_counters = OperationCounters::default();
        let edit_counters = if collect_steps {
            &mut step_counters
        } else {
            &mut counters
        };
        let next_namespace = match edit {
            ManagedEdit::Replace {
                path,
                start,
                delete_len,
                spool_offset,
                replacement_len,
                metadata_offset,
                metadata_len,
                ..
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
                edit_counters,
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
                edit_counters,
            )?,
        };
        namespace = next_namespace;
        if let Some(steps) = steps.as_mut() {
            counters = counters.merge(step_counters)?;
            steps.push(ManagedReplayStep {
                tree_level_before: step_counters.rope.tree_level_before,
                counters: step_counters,
            });
        }
    }
    let state = publication
        .publish_namespace(&encode_namespace_root(namespace)?)
        .map_err(VfsError::from)?;
    Ok((state, counters, steps.unwrap_or_default()))
}

pub(crate) fn encode_spooled_metadata(metadata: &NativeMetadata) -> VfsResult<Vec<u8>> {
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
    Ok(bytes)
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
    counters: &mut OperationCounters,
) -> VfsResult<NamespaceRootV1> {
    let (inode, record) = resolve(publication, namespace, path, counters)?;
    if record.kind != InodeKind::RegularFile {
        return Err(VfsError::InvalidState);
    }
    spool.seek(SeekFrom::Start(spool_offset))?;
    let mut replacement = (&mut *spool).take(replacement_len);
    let (content, rope) = replace_rope(
        publication,
        FileStateRoot(record.content_root),
        start,
        delete_len,
        &mut replacement,
    )?;
    counters.add_rope(rope)?;
    if replacement.limit() != 0 {
        return Err(VfsError::InvalidState);
    }
    let metadata_root =
        put_metadata_observed(publication, InodeKind::RegularFile, metadata, counters)?;
    let record_id =
        publication.put_object(&encode_inode_record(layerfs_core::inode::InodeRecordV1 {
            content_root: content.0,
            metadata_root,
            ..record
        })?)?;
    let (table, inode_counters) = inode_table_upsert(
        publication,
        InodeTableRoot(namespace.inode_table_root),
        inode,
        record_id,
    )?;
    counters.add_inode_table(inode_counters)?;
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
    counters: &mut OperationCounters,
) -> VfsResult<NamespaceRootV1> {
    let (source_inode, source_record, source_name) =
        resolve_parent(publication, namespace, from, counters)?;
    let (target_inode, target_record, target_name) =
        resolve_parent(publication, namespace, to, counters)?;
    let mut table = InodeTableRoot(namespace.inode_table_root);
    if source_inode == target_inode {
        let (directory, namespace_counters) = directory_rename(
            publication,
            DirectoryStateRoot(source_record.content_root),
            &source_name,
            target_name,
        )?;
        counters.add_namespace(namespace_counters)?;
        let metadata_root = put_metadata_observed(
            publication,
            InodeKind::Directory,
            source_parent_metadata,
            counters,
        )?;
        let id =
            publication.put_object(&encode_inode_record(layerfs_core::inode::InodeRecordV1 {
                content_root: directory.0,
                metadata_root,
                ..source_record
            })?)?;
        let (next, inode_counters) = inode_table_upsert(publication, table, source_inode, id)?;
        counters.add_inode_table(inode_counters)?;
        table = next;
    } else {
        let (source_directory, moved, namespace_counters) = directory_remove(
            publication,
            DirectoryStateRoot(source_record.content_root),
            &source_name,
        )?;
        counters.add_namespace(namespace_counters)?;
        let (target_directory, namespace_counters) = directory_insert(
            publication,
            DirectoryStateRoot(target_record.content_root),
            target_name,
            moved,
        )?;
        counters.add_namespace(namespace_counters)?;
        let source_metadata_root = put_metadata_observed(
            publication,
            InodeKind::Directory,
            source_parent_metadata,
            counters,
        )?;
        let source_id =
            publication.put_object(&encode_inode_record(layerfs_core::inode::InodeRecordV1 {
                content_root: source_directory.0,
                metadata_root: source_metadata_root,
                ..source_record
            })?)?;
        let (next, inode_counters) =
            inode_table_upsert(publication, table, source_inode, source_id)?;
        counters.add_inode_table(inode_counters)?;
        table = next;
        let target_metadata_root = put_metadata_observed(
            publication,
            InodeKind::Directory,
            target_parent_metadata,
            counters,
        )?;
        let target_id =
            publication.put_object(&encode_inode_record(layerfs_core::inode::InodeRecordV1 {
                content_root: target_directory.0,
                metadata_root: target_metadata_root,
                ..target_record
            })?)?;
        let (next, inode_counters) =
            inode_table_upsert(publication, table, target_inode, target_id)?;
        counters.add_inode_table(inode_counters)?;
        table = next;
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
    counters: &mut OperationCounters,
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
    let (inode, record) = resolve(
        store,
        namespace,
        &CanonicalPath::from_bytes(&parent_bytes)?,
        counters,
    )?;
    if record.kind != InodeKind::Directory {
        return Err(VfsError::InvalidState);
    }
    Ok((inode, record, CanonicalName::from_bytes(name)?))
}

fn resolve<S: ObjectRead>(
    store: &S,
    namespace: NamespaceRootV1,
    path: &CanonicalPath,
    counters: &mut OperationCounters,
) -> VfsResult<(
    layerfs_core::inode::InodeId,
    layerfs_core::inode::InodeRecordV1,
)> {
    let table = InodeTableRoot(namespace.inode_table_root);
    let mut inode = namespace.root_directory_inode;
    let mut record = load_record(store, table, inode, counters)?;
    for component in path.components() {
        if record.kind != InodeKind::Directory {
            return Err(VfsError::InvalidState);
        }
        let name = CanonicalName::from_bytes(component)?;
        let mut namespace_counters = NamespaceCounters::default();
        inode = directory_lookup(
            store,
            DirectoryStateRoot(record.content_root),
            &name,
            &mut namespace_counters,
        )?
        .ok_or(VfsError::InvalidState)?;
        counters.add_namespace(namespace_counters)?;
        record = load_record(store, table, inode, counters)?;
    }
    Ok((inode, record))
}

fn load_record<S: ObjectRead>(
    store: &S,
    table: InodeTableRoot,
    inode: layerfs_core::inode::InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<layerfs_core::inode::InodeRecordV1> {
    let mut inode_counters = InodeTableCounters::default();
    let id = inode_table_lookup(store, table, inode, &mut inode_counters)?
        .ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(inode_counters)?;
    Ok(store.with_authenticated_canonical(id, decode_inode_record)?)
}

pub(crate) fn native_parent<'a>(
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
) -> VfsResult<NativeOperationCounters> {
    let replacement_len = u64::try_from(replacement.len()).map_err(|_| VfsError::InvalidState)?;
    let mut counters = shift_file(file, start, delete_len, replacement_len, |file, len| {
        workspace.set_regular_len(file, len).map_err(Into::into)
    })?;
    file.seek(SeekFrom::Start(start))?;
    file.write_all(replacement)?;
    file.flush()?;
    counters.route = Some(if delete_len == replacement_len {
        NativeRoute::InPlacePatch
    } else {
        NativeRoute::InPlaceShift
    });
    counters.bytes_written = counters
        .bytes_written
        .checked_add(replacement_len)
        .ok_or(VfsError::InvalidState)?;
    counters.patch_bytes = replacement_len;
    Ok(counters)
}

pub(crate) fn shift_regular(
    workspace: &dyn ProjectionWorkspace,
    file: &mut dyn RegularFileHandle,
    start: u64,
    delete_len: u64,
    replacement_len: u64,
) -> VfsResult<NativeOperationCounters> {
    shift_file(file, start, delete_len, replacement_len, |file, len| {
        workspace.set_regular_len(file, len).map_err(Into::into)
    })
}

pub(crate) fn shift_temp(
    file: &mut dyn OwnedTempHandle,
    start: u64,
    delete_len: u64,
    replacement_len: u64,
) -> VfsResult<NativeOperationCounters> {
    shift_file(file, start, delete_len, replacement_len, |file, len| {
        file.set_len(len).map_err(Into::into)
    })
}

fn shift_file<F: Read + Write + Seek + ?Sized>(
    file: &mut F,
    start: u64,
    delete_len: u64,
    replacement_len: u64,
    mut set_len: impl FnMut(&mut F, u64) -> VfsResult<()>,
) -> VfsResult<NativeOperationCounters> {
    let length = file.seek(SeekFrom::End(0))?;
    let end = start
        .checked_add(delete_len)
        .ok_or(VfsError::InvalidState)?;
    if end > length {
        return Err(VfsError::InvalidState);
    }
    let next_len = length
        .checked_sub(delete_len)
        .and_then(|value| value.checked_add(replacement_len))
        .ok_or(VfsError::InvalidState)?;
    let shifted = if next_len == length { 0 } else { length - end };
    if next_len > length {
        set_len(file, next_len)?;
        let mut buffer = vec![0_u8; 1024 * 1024];
        let mut remaining = shifted;
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
        let mut buffer = vec![0_u8; 1024 * 1024];
        let mut source = end;
        while source < length {
            let count = (length - source).min(buffer.len() as u64);
            file.seek(SeekFrom::Start(source))?;
            file.read_exact(&mut buffer[..count as usize])?;
            file.seek(SeekFrom::Start(source - (length - next_len)))?;
            file.write_all(&buffer[..count as usize])?;
            source += count;
        }
        set_len(file, next_len)?;
    }
    Ok(NativeOperationCounters {
        bytes_read: shifted,
        bytes_written: shifted,
        suffix_bytes_shifted: shifted,
        ..NativeOperationCounters::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_core::content::rope::ObjectStore;
    use layerfs_core::inode::{inode_table_from_root, InodeId, InodeRecordV1};
    use layerfs_core::namespace_codec::encode_inode_record;
    use layerfs_core::{CoreError, CoreResult, ObjectId};
    use std::cell::Cell;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct TrackingStore {
        objects: BTreeMap<ObjectId, Vec<u8>>,
        gets: Cell<u64>,
        authenticated: Cell<u64>,
    }

    impl ObjectStore for TrackingStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.gets.set(self.gets.get() + 1);
            self.objects
                .get(&id)
                .cloned()
                .ok_or(CoreError::MissingObject)
        }

        fn put(&mut self, bytes: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(bytes);
            self.objects.entry(id).or_insert_with(|| bytes.to_vec());
            Ok(id)
        }

        fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
        where
            F: FnOnce(&[u8]) -> CoreResult<T>,
        {
            self.authenticated.set(self.authenticated.get() + 1);
            let bytes = self.objects.get(&id).ok_or(CoreError::MissingObject)?;
            if ObjectId::for_bytes(bytes) != id {
                return Err(CoreError::IdentityMismatch);
            }
            callback(bytes)
        }
    }

    #[test]
    fn managed_inode_record_uses_the_borrowed_authenticated_route() {
        let mut store = TrackingStore::default();
        let inode = InodeId::allocate([7; 32], 1);
        let record = InodeRecordV1 {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1,
            content_root: ObjectId::for_bytes(b"content"),
            metadata_root: ObjectId::for_bytes(b"metadata"),
        };
        let record_id = store.put(&encode_inode_record(record).unwrap()).unwrap();
        let table = inode_table_from_root(&mut store, inode, record_id).unwrap();
        store.gets.set(0);
        store.authenticated.set(0);

        assert_eq!(
            load_record(&store, table, inode, &mut OperationCounters::default()).unwrap(),
            record
        );
        assert_eq!(store.gets.get(), 0);
        assert_eq!(store.authenticated.get(), 2);
    }
}
