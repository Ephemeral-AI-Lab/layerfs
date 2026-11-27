//! Displace prepared inode values through the mounted reader's ordinary ports.
//! No namespace is changed and the fixture keeps the default Store policy.
use layerfs_content::{
    filesystem::attributes::PortableMetadata,
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeMetadata, InodeValue,
        INODE_VALUE_BYTES, LEAF_ROW_BYTES, MAXIMUM_LEAF_ROWS,
    },
    AuthenticatedObjects, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_daemon::store::Store;
use layerfs_history::WorkspaceId;
use layerfs_storage::{policy::POOLED_VALUE_CACHE_BYTES, Storage};
use std::sync::Arc;

/// More distinct stored values than one reader can retain. Each leaf fits the
/// canonical grammar and each value references a real saved payload. Inline
/// mtimes make the values distinct, so persistence cannot deduplicate them.
pub fn prepare(storage: &Storage) -> Vec<FinalizedObject> {
    let payload = FinalizedObject::new(
        ObjectRole::WholeFile,
        layerfs_content::encode_whole_file_payload(b"FP-21 pressure").unwrap(),
    )
    .unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(payload.clone()).unwrap();
    let leaves = (POOLED_VALUE_CACHE_BYTES / INODE_VALUE_BYTES).div_ceil(MAXIMUM_LEAF_ROWS) + 1;
    let objects = (0..leaves)
        .map(|page| {
            let rows = (0..MAXIMUM_LEAF_ROWS)
                .map(|row| {
                    let serial = (page * MAXIMUM_LEAF_ROWS + row + 1) as u64;
                    InodeLeafRow {
                        serial,
                        value: encode_inode_value(InodeValue {
                            kind: InodeKind::RegularFile,
                            namespace_ref_count: 1,
                            content_root: payload.id(),
                            metadata_root: InodeMetadata::Inline(PortableMetadata {
                                mode: 0o644,
                                mtime_seconds: serial as i64,
                                mtime_nanoseconds: 0,
                            }),
                        }),
                    }
                })
                .collect();
            let object = FinalizedObject::new(
                ObjectRole::InodeLeaf,
                InodeLeaf {
                    subtree_bytes: (MAXIMUM_LEAF_ROWS * LEAF_ROW_BYTES) as u64,
                    rows,
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
            save.accept(object.clone()).unwrap();
            object
        })
        .collect::<Vec<_>>();
    save.finish().unwrap();
    objects
}

/// Use this Workspace's reader, not the control reader or a new Storage handle.
/// The ordinary pooled-value cache clears on overflow. Subsequent reconstruction
/// of a prepared target must acquire its stored values again, even if its pack
/// body and locator remain resident. The actual gate hold proves that miss.
pub fn displace(
    store: &Arc<Store>,
    scope: ObjectId,
    workspace: WorkspaceId,
    objects: &[FinalizedObject],
) {
    let ports = store.ports_for(scope, workspace);
    for object in objects {
        assert_eq!(
            ports.read_canonical(object.id()).unwrap(),
            object.canonical()
        );
    }
    println!(
        "FP-21 displaced pooled values: {} leaves, {} distinct values, allowance={} bytes",
        objects.len(),
        objects.len() * MAXIMUM_LEAF_ROWS,
        POOLED_VALUE_CACHE_BYTES
    );
}
