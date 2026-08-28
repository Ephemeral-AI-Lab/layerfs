use layerfs_core::content::rope::build;
use layerfs_core::inode::{inode_table_from_root, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
use layerfs_core::object::access::ObjectStore;
use layerfs_core::ObjectId;
use layerfs_working_store::WorkingStore;

pub(crate) fn valid_empty_root(working: &mut WorkingStore) -> ObjectId {
    let mut publication = working.begin_candidate_write().unwrap();
    let (mode, _) = build(&mut publication, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = Vec::new();
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut publication, mtime.as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut publication,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_file_root: mtime.0,
            },
        ],
    )
    .unwrap();
    let directory = empty_directory(&mut publication).unwrap();
    let inode = publication.allocate_inode_id().unwrap();
    let record = publication
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let table = inode_table_from_root(&mut publication, inode, record).unwrap();
    let root = publication
        .put(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    publication.commit_candidate(root).unwrap()
}
