use super::super::capture_legacy::put_metadata_observed;
use super::super::session_legacy::{VfsError, VfsResult};
use super::super::OperationCounters;
use super::resolution::{namespace, resolve};
use layerfs_core::content::rope::{replace, FileStateRoot};
use layerfs_core::inode::{inode_table_upsert, InodeId, InodeKind, InodeRecordV1, InodeTableRoot};
use layerfs_core::namespace::{directory_insert, DirectoryStateRoot, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root};
use layerfs_core::{CanonicalName, CanonicalPath};
use layerfs_materialization::driver::NativeMetadata;
use layerfs_storage::publication::Publication;
use layerfs_storage::refs::RefState;
use layerfs_storage::Engine;
use std::io::Read;
pub(crate) fn replace_range_at_ref<R: Read>(
    engine: &Engine,
    expected: &RefState,
    path: &CanonicalPath,
    start: u64,
    delete_len: u64,
    input: R,
) -> VfsResult<(RefState, OperationCounters)> {
    let mut counters = OperationCounters::default();
    let mut publication = engine.begin_publication(Some(expected), &expected.name)?;
    let namespace = namespace(&publication, expected.root)?;
    let (inode, record) = resolve(&publication, namespace, path, &mut counters)?;
    if record.kind != InodeKind::RegularFile {
        return Err(VfsError::InvalidState);
    }
    let (content, rope) = replace(
        &mut publication,
        FileStateRoot(record.content_root),
        start,
        delete_len,
        input,
    )?;
    counters.add_rope(rope)?;
    let namespace = update_record(
        &mut publication,
        namespace,
        inode,
        InodeRecordV1 {
            content_root: content.0,
            ..record
        },
        &mut counters,
    )?;
    let state = publication.publish_namespace(&encode_namespace_root(namespace)?)?;
    Ok((state, counters))
}

pub(super) fn require_main(expected: &RefState) -> VfsResult<()> {
    if expected.name == "main" {
        Ok(())
    } else {
        Err(VfsError::InvalidState)
    }
}

pub(super) fn update_record(
    publication: &mut Publication<'_>,
    namespace: NamespaceRootV1,
    inode: InodeId,
    record: InodeRecordV1,
    counters: &mut OperationCounters,
) -> VfsResult<NamespaceRootV1> {
    let record = publication.put_object(&encode_inode_record(record)?)?;
    let (table, visits) = inode_table_upsert(
        publication,
        InodeTableRoot(namespace.inode_table_root),
        inode,
        record,
    )?;
    counters.add_inode_table(visits)?;
    Ok(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn create_file(
    publication: &mut Publication<'_>,
    namespace: NamespaceRootV1,
    parent_inode: InodeId,
    parent_record: InodeRecordV1,
    name: CanonicalName,
    content: FileStateRoot,
    counters: &mut OperationCounters,
) -> VfsResult<NamespaceRootV1> {
    let inode = publication.allocate_inode_id()?;
    let metadata = put_metadata_observed(
        publication,
        InodeKind::RegularFile,
        &NativeMetadata {
            mode: 0o644,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            xattrs: layerfs_materialization::driver::NativeXattrs::new(),
            acl: None,
            bsd_flags: 0,
        },
        counters,
    )?;
    let file_record = publication.put_object(&encode_inode_record(InodeRecordV1 {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: content.0,
        metadata_root: metadata,
    })?)?;
    let (directory, visits) = directory_insert(
        publication,
        DirectoryStateRoot(parent_record.content_root),
        name,
        inode,
    )?;
    counters.add_namespace(visits)?;
    let parent_record = publication.put_object(&encode_inode_record(InodeRecordV1 {
        content_root: directory.0,
        ..parent_record
    })?)?;
    let (table, visits) = inode_table_upsert(
        publication,
        InodeTableRoot(namespace.inode_table_root),
        inode,
        file_record,
    )?;
    counters.add_inode_table(visits)?;
    let (table, visits) = inode_table_upsert(publication, table, parent_inode, parent_record)?;
    counters.add_inode_table(visits)?;
    Ok(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })
}
