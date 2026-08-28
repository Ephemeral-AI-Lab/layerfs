use super::super::capture_legacy::put_metadata_observed;
use super::super::session_legacy::{VfsError, VfsResult};
use super::super::{ManagedReplayStep, OperationCounters};
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
use layerfs_materialization::driver::*;
use layerfs_storage::publication::Publication;
use layerfs_storage::refs::RefState;
use layerfs_storage::Engine;
use std::io::{Read, SeekFrom};

use super::spool::load_spooled_metadata;
use super::ManagedEdit;
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
            if matches!(error, layerfs_storage::EngineError::PublicationConflict) {
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

#[allow(clippy::too_many_arguments)]
pub(super) fn replay_replace(
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

pub(super) fn replay_rename(
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

pub(super) fn resolve_parent<S: ObjectRead>(
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

pub(super) fn resolve<S: ObjectRead>(
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

pub(super) fn load_record<S: ObjectRead>(
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
