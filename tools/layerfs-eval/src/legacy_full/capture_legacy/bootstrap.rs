use super::super::session_legacy::VfsResult;
use super::metadata::put_metadata;
use layerfs_core::inode::{inode_table_from_root, InodeKind, InodeRecordV1};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
use layerfs_materialization::driver::*;
use layerfs_storage::refs::RefState;
use layerfs_storage::Engine;
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
            xattrs: layerfs_materialization::driver::NativeXattrs::new(),
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
