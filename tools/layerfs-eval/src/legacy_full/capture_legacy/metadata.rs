use super::super::session_legacy::VfsResult;
use super::super::OperationCounters;
use layerfs_core::content::rope::build;
use layerfs_core::inode::{
    generated_inode_table_from_root, generated_inode_table_upsert, GeneratedInodeTable, InodeId,
    InodeKind, InodeRecordV1,
};
use layerfs_core::metadata::{
    decode_apple_acl, encode_bsd_flags, MetadataEntryV1, MetadataKey, MetadataTreeBuilder,
    PortableMetadataV1,
};
use layerfs_core::namespace_codec::encode_inode_record;
use layerfs_materialization::driver::*;
use layerfs_storage::publication::Publication;
use std::io::Cursor;
pub(super) fn put_record(
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
    super::super::managed_edit_legacy::spooled_metadata_len(native)?;
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

pub(super) fn metadata_value(
    publication: &mut Publication<'_>,
    key: MetadataKey,
    value: &[u8],
    counters: &mut OperationCounters,
) -> VfsResult<MetadataEntryV1> {
    let (root, rope) = build(publication, Cursor::new(value))?;
    super::super::add_metadata_rope(counters, rope)?;
    Ok(MetadataEntryV1 {
        key,
        value_file_root: root.0,
    })
}
