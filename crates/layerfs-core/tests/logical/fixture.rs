use layerfs_core::content::rope::{build, ObjectStore};
use layerfs_core::inode::{
    inode_table_from_root, inode_table_lookup, inode_table_upsert, merge_inode_tables, InodeId,
    InodeKind, InodeRecordV1, InodeTableCounters,
};
use layerfs_core::logical;
use layerfs_core::metadata::{
    build_metadata_tree, metadata_lookup, metadata_tree_entries, MetadataEntryV1, MetadataKey,
    PortableMetadataV1,
};
use layerfs_core::namespace::{directory_insert, empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{
    encode_inode_record, encode_namespace_root, profile_id as namespace_profile_id,
};
use layerfs_core::{CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};
use std::collections::BTreeMap;
use std::io::Cursor;

#[derive(Default)]
struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

impl ObjectStore for MemoryStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        self.0.entry(id).or_insert_with(|| canonical.to_vec());
        Ok(id)
    }
}

fn metadata(store: &mut MemoryStore, kind: InodeKind) -> ObjectId {
    metadata_at(store, kind, 7)
}

fn metadata_at(store: &mut MemoryStore, kind: InodeKind, mtime_seconds: i64) -> ObjectId {
    let portable = PortableMetadataV1 {
        permission_mode: if kind == InodeKind::Directory {
            0o755
        } else {
            0o644
        },
        mtime_seconds,
        mtime_nanoseconds: 11,
    };
    let (mode, _) = build(store, Cursor::new(portable.mode_bytes(kind).unwrap())).unwrap();
    let (mtime, _) = build(store, Cursor::new(portable.mtime_bytes().unwrap())).unwrap();
    build_metadata_tree(
        store,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".to_owned(), b"mode".to_vec()).unwrap(),
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".to_owned(), b"mtime".to_vec()).unwrap(),
                value_file_root: mtime.0,
            },
        ],
    )
    .unwrap()
}

fn fixture() -> (MemoryStore, ObjectId) {
    let mut store = MemoryStore::default();
    let root_inode = InodeId::allocate([1; 32], 0);
    let file_inode = InodeId::allocate([1; 32], 1);
    let (file, _) = build(&mut store, Cursor::new(b"hello persistent world")).unwrap();
    let file_metadata = metadata(&mut store, InodeKind::RegularFile);
    let file_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: file.0,
                metadata_root: file_metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let directory = empty_directory(&mut store).unwrap();
    let (directory, _) = directory_insert(
        &mut store,
        directory,
        CanonicalName::new("file").unwrap(),
        file_inode,
    )
    .unwrap();
    let directory_metadata = metadata(&mut store, InodeKind::Directory);
    let root_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: directory_metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let table = inode_table_from_root(&mut store, root_inode, root_record).unwrap();
    let (table, _) = inode_table_upsert(&mut store, table, file_inode, file_record).unwrap();
    let root = store
        .put(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: namespace_profile_id(),
                root_directory_inode: root_inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    (store, root)
}

fn rename_fixture() -> (MemoryStore, ObjectId) {
    let mut store = MemoryStore::default();
    let root_inode = InodeId::allocate([2; 32], 0);
    let left_inode = InodeId::allocate([2; 32], 1);
    let right_inode = InodeId::allocate([2; 32], 2);
    let file_inode = InodeId::allocate([2; 32], 3);
    let directory_metadata = metadata(&mut store, InodeKind::Directory);
    let file_metadata = metadata(&mut store, InodeKind::RegularFile);
    let (file, _) = build(&mut store, Cursor::new(b"move me")).unwrap();
    let file_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: file.0,
                metadata_root: file_metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let left = empty_directory(&mut store).unwrap();
    let (left, _) = directory_insert(
        &mut store,
        left,
        CanonicalName::new("file").unwrap(),
        file_inode,
    )
    .unwrap();
    let left_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 1,
                content_root: left.0,
                metadata_root: directory_metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let right = empty_directory(&mut store).unwrap();
    let right_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 1,
                content_root: right.0,
                metadata_root: directory_metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let root_directory = empty_directory(&mut store).unwrap();
    let (root_directory, _) = directory_insert(
        &mut store,
        root_directory,
        CanonicalName::new("left").unwrap(),
        left_inode,
    )
    .unwrap();
    let (root_directory, _) = directory_insert(
        &mut store,
        root_directory,
        CanonicalName::new("right").unwrap(),
        right_inode,
    )
    .unwrap();
    let root_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: root_directory.0,
                metadata_root: directory_metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let table = inode_table_from_root(&mut store, root_inode, root_record).unwrap();
    let (table, _) = inode_table_upsert(&mut store, table, left_inode, left_record).unwrap();
    let (table, _) = inode_table_upsert(&mut store, table, right_inode, right_record).unwrap();
    let (table, _) = inode_table_upsert(&mut store, table, file_inode, file_record).unwrap();
    let root = store
        .put(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: namespace_profile_id(),
                root_directory_inode: root_inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    (store, root)
}
