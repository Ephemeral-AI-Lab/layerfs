use layerfs_content::file::rope::{build, ObjectStore};
use layerfs_content::filesystem as logical;
use layerfs_content::tree::directory::codec::{
    encode_namespace_root, profile_id as namespace_profile_id,
};
use layerfs_content::tree::directory::{directory_insert, empty_directory};
use layerfs_content::tree::inode::codec::encode_inode_record;
use layerfs_content::tree::inode::{
    inode_table_from_root, inode_table_lookup, inode_table_upsert, reconcile_inode_tables, InodeId,
    InodeKind, InodeRecordV1, InodeTableCounters,
};
use layerfs_content::tree::metadata::{
    build_metadata_tree, metadata_lookup, metadata_tree_entries, MetadataEntryV1, MetadataKey,
    PortableMetadataV1,
};
use layerfs_content::tree::NamespaceRootV1;
use layerfs_content::{CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};
use std::cell::RefCell;
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

#[test]
fn three_root_reconciliation_combines_content_and_metadata_on_one_inode() {
    let (mut store, base) = fixture();
    let path = CanonicalPath::new("file").unwrap();
    let source =
        logical::replace_range(&mut store, base, &path, 0, 5, Cursor::new(b"HELLO")).unwrap();
    let destination_metadata = metadata_at(&mut store, InodeKind::RegularFile, 99);
    let destination = logical::replace_range_with_metadata(
        &mut store,
        base,
        &path,
        0,
        0,
        Cursor::new(Vec::<u8>::new()),
        Some(destination_metadata),
    )
    .unwrap();
    let merged = logical::reconcile_roots(&mut store, base, source.root(), destination.root())
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    logical::stream(&store, merged.root(), &path, &mut bytes).unwrap();
    assert_eq!(bytes, b"HELLO persistent world");
    assert_eq!(
        logical::resolve(
            &store,
            merged.root(),
            &path,
            &mut logical::LogicalCounters::default(),
        )
        .unwrap()
        .record
        .metadata_root,
        destination_metadata
    );
}

#[test]
fn three_root_reconciliation_combines_disjoint_names_in_one_directory() {
    let (mut store, base) = fixture();
    let source_metadata = metadata(&mut store, InodeKind::RegularFile);
    let source = logical::replace_file(
        &mut store,
        base,
        &CanonicalPath::new("source").unwrap(),
        Cursor::new(b"source"),
        |_| Ok((InodeId::allocate([3; 32], 1), source_metadata)),
    )
    .unwrap();
    let destination_metadata = metadata(&mut store, InodeKind::RegularFile);
    let destination = logical::replace_file(
        &mut store,
        base,
        &CanonicalPath::new("destination").unwrap(),
        Cursor::new(b"destination"),
        |_| Ok((InodeId::allocate([3; 32], 2), destination_metadata)),
    )
    .unwrap();
    let merged = logical::reconcile_roots(&mut store, base, source.root(), destination.root())
        .unwrap()
        .unwrap();
    let mut source_bytes = Vec::new();
    logical::stream(
        &store,
        merged.root(),
        &CanonicalPath::new("source").unwrap(),
        &mut source_bytes,
    )
    .unwrap();
    let mut destination_bytes = Vec::new();
    logical::stream(
        &store,
        merged.root(),
        &CanonicalPath::new("destination").unwrap(),
        &mut destination_bytes,
    )
    .unwrap();
    assert_eq!(source_bytes, b"source");
    assert_eq!(destination_bytes, b"destination");
    assert!(merged.counters().namespace.nodes_created > 0);
}

#[test]
fn three_root_reconciliation_never_publishes_an_undercounted_parallel_hard_link() {
    let (mut store, base) = fixture();
    let source = logical::hard_link(
        &mut store,
        base,
        &CanonicalPath::new("file").unwrap(),
        &CanonicalPath::new("source-link").unwrap(),
    )
    .unwrap();
    let destination = logical::hard_link(
        &mut store,
        base,
        &CanonicalPath::new("file").unwrap(),
        &CanonicalPath::new("destination-link").unwrap(),
    )
    .unwrap();

    assert!(
        logical::reconcile_roots(&mut store, base, source.root(), destination.root())
            .unwrap()
            .is_err()
    );
}

#[test]
fn three_root_reconciliation_combines_disjoint_metadata_keys() {
    let (mut store, base) = fixture();
    let path = CanonicalPath::new("file").unwrap();
    let base_metadata = logical::resolve(
        &store,
        base,
        &path,
        &mut logical::LogicalCounters::default(),
    )
    .unwrap()
    .record
    .metadata_root;
    let source_metadata = metadata_at(&mut store, InodeKind::RegularFile, 99);
    let (xattr, _) = build(&mut store, Cursor::new(b"value")).unwrap();
    let mut destination_entries = metadata_tree_entries(&store, base_metadata).unwrap();
    let xattr_key = MetadataKey::new("apple.xattr".to_owned(), b"user.test".to_vec()).unwrap();
    destination_entries.push(MetadataEntryV1 {
        key: xattr_key.clone(),
        value_file_root: xattr.0,
    });
    destination_entries.sort_by(|left, right| left.key.cmp(&right.key));
    let destination_metadata = build_metadata_tree(&mut store, &destination_entries).unwrap();
    let source = logical::replace_range_with_metadata(
        &mut store,
        base,
        &path,
        0,
        0,
        Cursor::new(Vec::<u8>::new()),
        Some(source_metadata),
    )
    .unwrap();
    let destination = logical::replace_range_with_metadata(
        &mut store,
        base,
        &path,
        0,
        0,
        Cursor::new(Vec::<u8>::new()),
        Some(destination_metadata),
    )
    .unwrap();
    let merged = logical::reconcile_roots(&mut store, base, source.root(), destination.root())
        .unwrap()
        .unwrap();
    let merged_metadata = logical::resolve(
        &store,
        merged.root(),
        &path,
        &mut logical::LogicalCounters::default(),
    )
    .unwrap()
    .record
    .metadata_root;
    assert_eq!(
        metadata_lookup(&store, merged_metadata, &xattr_key)
            .unwrap()
            .unwrap()
            .value_file_root,
        xattr.0
    );
    assert_eq!(
        metadata_lookup(
            &store,
            merged_metadata,
            &MetadataKey::new("portable".to_owned(), b"mtime".to_vec()).unwrap(),
        )
        .unwrap()
        .unwrap()
        .value_file_root,
        metadata_lookup(
            &store,
            source_metadata,
            &MetadataKey::new("portable".to_owned(), b"mtime".to_vec()).unwrap(),
        )
        .unwrap()
        .unwrap()
        .value_file_root
    );
}

#[test]
fn inode_reconciliation_prunes_equal_persistent_subtrees() {
    let mut store = MemoryStore::default();
    let common = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(b"common-content"),
                metadata_root: ObjectId::for_bytes(b"common-metadata"),
            })
            .unwrap(),
        )
        .unwrap();
    let first = InodeId::allocate([9; 32], 0);
    let mut base = inode_table_from_root(&mut store, first, common).unwrap();
    let mut keys = vec![first];
    for serial in 1..512 {
        let key = InodeId::allocate([9; 32], serial);
        keys.push(key);
        base = inode_table_upsert(&mut store, base, key, common).unwrap().0;
    }
    keys.sort();
    let source_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                content_root: ObjectId::for_bytes(b"source-content"),
                ..store
                    .with_authenticated_canonical(
                        common,
                        layerfs_content::tree::inode::codec::decode_inode_record,
                    )
                    .unwrap()
            })
            .unwrap(),
        )
        .unwrap();
    let destination_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                metadata_root: ObjectId::for_bytes(b"destination-metadata"),
                ..store
                    .with_authenticated_canonical(
                        common,
                        layerfs_content::tree::inode::codec::decode_inode_record,
                    )
                    .unwrap()
            })
            .unwrap(),
        )
        .unwrap();
    let source = inode_table_upsert(&mut store, base, keys[1], source_record)
        .unwrap()
        .0;
    let destination = inode_table_upsert(&mut store, base, keys[510], destination_record)
        .unwrap()
        .0;
    let (merged, counters, _) = reconcile_inode_tables(&mut store, base, source, destination)
        .unwrap()
        .unwrap();
    assert!(counters.nodes_read < 12, "{counters:?}");
    assert_eq!(
        inode_table_lookup(&store, merged, keys[1], &mut InodeTableCounters::default(),).unwrap(),
        Some(source_record)
    );
    assert_eq!(
        inode_table_lookup(
            &store,
            merged,
            keys[510],
            &mut InodeTableCounters::default(),
        )
        .unwrap(),
        Some(destination_record)
    );
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

struct CountingRead<'a> {
    store: &'a MemoryStore,
    reads: RefCell<BTreeMap<ObjectId, u64>>,
}

impl layerfs_content::object::access::ObjectRead for CountingRead<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        *self.reads.borrow_mut().entry(id).or_default() += 1;
        ObjectStore::get(self.store, id)
    }
}

#[test]
fn path_diff_reuses_equal_persistent_objects_and_emits_incrementally() {
    let (mut store, initial) = fixture();
    let base = logical::apply_changes(
        &mut store,
        initial,
        &[logical::ContentChange::Write {
            path: "stable".into(),
            bytes: b"unchanged".to_vec(),
            mode: 0o644,
        }],
        [1; 32],
    )
    .unwrap()
    .root_id;
    let changed = logical::apply_changes(
        &mut store,
        base,
        &[logical::ContentChange::Write {
            path: "file".into(),
            bytes: b"changed".to_vec(),
            mode: 0o644,
        }],
        [1; 32],
    )
    .unwrap()
    .root_id;

    let namespace = logical::namespace(&store, base).unwrap();
    let table = layerfs_content::tree::inode::InodeTableRoot(namespace.inode_table_root);
    let root_record = inode_table_lookup(
        &store,
        table,
        namespace.root_directory_inode,
        &mut InodeTableCounters::default(),
    )
    .unwrap()
    .unwrap();
    let root_record = store.0.get(&root_record).unwrap();
    let shared_directory = layerfs_content::tree::inode::codec::decode_inode_record(root_record)
        .unwrap()
        .content_root;
    let stable = logical::resolve(
        &store,
        base,
        &CanonicalPath::new("stable").unwrap(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap();
    let stable_record = inode_table_lookup(
        &store,
        table,
        stable.inode,
        &mut InodeTableCounters::default(),
    )
    .unwrap()
    .unwrap();

    let reader = CountingRead {
        store: &store,
        reads: RefCell::new(BTreeMap::new()),
    };
    let mut entries = Vec::new();
    logical::diff_roots(&reader, base, changed, |entry| {
        entries.push(entry);
        Ok(())
    })
    .unwrap();
    assert_eq!(entries.len(), 1);
    assert!(matches!(
        &entries[0],
        logical::DiffEntry::Modify { path, aspects, .. }
            if path == &CanonicalPath::new("file").unwrap()
                && aspects.content
                && !aspects.directory_membership
    ));
    let reads = reader.reads.borrow();
    assert_eq!(reads.get(&shared_directory), Some(&1));
    assert_eq!(reads.get(&stable_record), Some(&1));

    let equal = CountingRead {
        store: &store,
        reads: RefCell::new(BTreeMap::new()),
    };
    logical::diff_roots(&equal, base, base, |_| Ok(())).unwrap();
    assert!(equal.reads.borrow().is_empty());
}

#[test]
fn path_diff_preserves_order_types_metadata_symlinks_directories_and_hard_links() {
    let (mut store, base) = fixture();
    let seed = [1; 32];

    let hard_link = logical::apply_changes(
        &mut store,
        base,
        &[logical::ContentChange::HardLink {
            source: "file".into(),
            target: "alias".into(),
        }],
        seed,
    )
    .unwrap()
    .root_id;
    let mut entries = Vec::new();
    logical::diff_roots(&store, base, hard_link, |entry| {
        entries.push(entry);
        Ok(())
    })
    .unwrap();
    assert_eq!(paths(&entries), ["", "alias", "file"]);
    assert!(matches!(
        &entries[0],
        logical::DiffEntry::Modify { aspects, .. }
            if aspects.directory_membership && aspects.content
    ));
    assert!(matches!(&entries[1], logical::DiffEntry::Add { .. }));
    assert!(matches!(
        &entries[2],
        logical::DiffEntry::Modify { aspects, .. }
            if aspects.hard_links && !aspects.node_type && !aspects.content && !aspects.metadata
    ));

    let metadata = logical::apply_changes(
        &mut store,
        base,
        &[logical::ContentChange::SetMode {
            path: "file".into(),
            mode: 0o600,
        }],
        seed,
    )
    .unwrap()
    .root_id;
    let mut entries = Vec::new();
    logical::diff_roots(&store, base, metadata, |entry| {
        entries.push(entry);
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        entries.as_slice(),
        [logical::DiffEntry::Modify { path, aspects, .. }]
            if path == &CanonicalPath::new("file").unwrap()
                && aspects.metadata
                && !aspects.node_type
                && !aspects.content
                && !aspects.directory_membership
                && !aspects.hard_links
    ));

    let type_change = logical::apply_changes(
        &mut store,
        base,
        &[
            logical::ContentChange::Remove {
                path: "file".into(),
            },
            logical::ContentChange::Mkdir {
                path: "file".into(),
                mode: 0o755,
            },
            logical::ContentChange::Write {
                path: "file/child".into(),
                bytes: b"child".to_vec(),
                mode: 0o644,
            },
        ],
        seed,
    )
    .unwrap()
    .root_id;
    let mut entries = Vec::new();
    logical::diff_roots(&store, base, type_change, |entry| {
        entries.push(entry);
        Ok(())
    })
    .unwrap();
    assert_eq!(paths(&entries), ["", "file", "file/child"]);
    assert!(matches!(
        &entries[1],
        logical::DiffEntry::Modify { aspects, .. }
            if aspects.node_type && aspects.content && aspects.metadata
    ));
    assert!(matches!(&entries[2], logical::DiffEntry::Add { .. }));

    let symlink_base = logical::apply_changes(
        &mut store,
        base,
        &[logical::ContentChange::Symlink {
            path: "link".into(),
            target: b"one".to_vec(),
        }],
        seed,
    )
    .unwrap()
    .root_id;
    let symlink_changed = logical::apply_changes(
        &mut store,
        symlink_base,
        &[
            logical::ContentChange::Remove {
                path: "link".into(),
            },
            logical::ContentChange::Symlink {
                path: "link".into(),
                target: b"two".to_vec(),
            },
        ],
        seed,
    )
    .unwrap()
    .root_id;
    let mut entries = Vec::new();
    logical::diff_roots(&store, symlink_base, symlink_changed, |entry| {
        entries.push(entry);
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        entries.as_slice(),
        [logical::DiffEntry::Modify { path, aspects, .. }]
            if path == &CanonicalPath::new("link").unwrap()
                && aspects.content
                && !aspects.node_type
    ));
}

fn paths(entries: &[logical::DiffEntry]) -> Vec<&str> {
    entries
        .iter()
        .map(|entry| match entry {
            logical::DiffEntry::Add { path, .. }
            | logical::DiffEntry::Remove { path, .. }
            | logical::DiffEntry::Modify { path, .. } => path.as_str(),
        })
        .collect()
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

#[test]
fn exact_reads_and_local_mutation_share_one_logical_owner() {
    let (mut store, root) = fixture();
    let path = CanonicalPath::new("file").unwrap();
    let (stat, _) = logical::stat(&store, root, &path).unwrap();
    assert_eq!(stat.kind, InodeKind::RegularFile);
    let (page, _) =
        logical::list(&store, root, &CanonicalPath::new("").unwrap(), None, 1, 128).unwrap();
    assert_eq!(page.entries[0].0.as_bytes(), b"file");
    let mut before = Vec::new();
    logical::read_range(&store, root, &path, 6..16, &mut before).unwrap();
    assert_eq!(before, b"persistent");

    let candidate =
        logical::replace_range(&mut store, root, &path, 6, 10, Cursor::new(b"logical")).unwrap();
    assert_eq!(candidate.parent_root(), root);
    assert_eq!(candidate.counters().rope.cdc_bytes_scanned, 7);
    let mut old = Vec::new();
    logical::stream(&store, root, &path, &mut old).unwrap();
    assert_eq!(old, b"hello persistent world");
    let mut new = Vec::new();
    logical::stream(&store, candidate.root(), &path, &mut new).unwrap();
    assert_eq!(new, b"hello logical world");
}

#[test]
fn rename_keeps_old_roots_readable_and_handles_same_and_cross_directory_moves() {
    let (mut store, root) = rename_fixture();
    let metadata = logical::resolve(
        &store,
        root,
        &CanonicalPath::new("left").unwrap(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap()
    .record
    .metadata_root;
    let moved = logical::rename(
        &mut store,
        root,
        &CanonicalPath::new("left/file").unwrap(),
        &CanonicalPath::new("right/moved").unwrap(),
        metadata,
        metadata,
    )
    .unwrap();
    let mut old = Vec::new();
    logical::stream(
        &store,
        root,
        &CanonicalPath::new("left/file").unwrap(),
        &mut old,
    )
    .unwrap();
    assert_eq!(old, b"move me");
    let mut current = Vec::new();
    logical::stream(
        &store,
        moved.root(),
        &CanonicalPath::new("right/moved").unwrap(),
        &mut current,
    )
    .unwrap();
    assert_eq!(current, old);
    assert!(logical::stat(
        &store,
        moved.root(),
        &CanonicalPath::new("left/file").unwrap()
    )
    .is_err());

    let renamed = logical::rename(
        &mut store,
        moved.root(),
        &CanonicalPath::new("right/moved").unwrap(),
        &CanonicalPath::new("right/final").unwrap(),
        metadata,
        metadata,
    )
    .unwrap();
    let mut final_bytes = Vec::new();
    logical::stream(
        &store,
        renamed.root(),
        &CanonicalPath::new("right/final").unwrap(),
        &mut final_bytes,
    )
    .unwrap();
    assert_eq!(final_bytes, b"move me");
}

#[test]
fn three_root_reconciliation_streams_independent_inode_changes_and_reports_overlap() {
    let (mut store, base) = rename_fixture();
    let source = logical::replace_range(
        &mut store,
        base,
        &CanonicalPath::new("left/file").unwrap(),
        0,
        7,
        Cursor::new(b"changed"),
    )
    .unwrap();
    let metadata = logical::resolve(
        &store,
        base,
        &CanonicalPath::new("left").unwrap(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap()
    .record
    .metadata_root;
    let destination = logical::rename(
        &mut store,
        base,
        &CanonicalPath::new("left/file").unwrap(),
        &CanonicalPath::new("right/moved").unwrap(),
        metadata,
        metadata,
    )
    .unwrap();
    let merged = logical::reconcile_roots(&mut store, base, source.root(), destination.root())
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    logical::stream(
        &store,
        merged.root(),
        &CanonicalPath::new("right/moved").unwrap(),
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"changed");
    assert!(merged.counters().inode_table.nodes_created > 0);

    let conflicting = logical::replace_range(
        &mut store,
        base,
        &CanonicalPath::new("left/file").unwrap(),
        0,
        7,
        Cursor::new(b"other!!"),
    )
    .unwrap();
    assert!(
        logical::reconcile_roots(&mut store, base, source.root(), conflicting.root())
            .unwrap()
            .is_err()
    );
}

#[test]
fn typed_conflicts_report_canonical_paths_and_exact_choice_roots() {
    let (mut store, base) = fixture();
    let root = |store: &mut MemoryStore, changes: &[logical::ContentChange], seed: [u8; 32]| {
        logical::apply_changes(store, base, changes, seed)
            .unwrap()
            .root_id
    };

    let branch = root(
        &mut store,
        &[logical::ContentChange::Write {
            path: "file".into(),
            bytes: b"branch".to_vec(),
            mode: 0o644,
        }],
        [3; 32],
    );
    let layer = root(
        &mut store,
        &[logical::ContentChange::Write {
            path: "file".into(),
            bytes: b"layer".to_vec(),
            mode: 0o644,
        }],
        [4; 32],
    );
    let content = logical::reconcile(&mut store, base, branch, layer).unwrap();
    assert_eq!(content.conflicts.len(), 1);
    assert_eq!(
        content.conflicts[0].kind,
        logical::ReconcileConflictKind::Content
    );
    assert_eq!(
        content.conflicts[0].affected_paths,
        vec![CanonicalPath::new("file").unwrap()]
    );
    assert_eq!(
        logical::reconcile_with(&mut store, base, branch, layer, |_| {
            Some(logical::ReconcileChoice::Branch)
        })
        .unwrap()
        .root_id,
        branch
    );
    let layer_choice = logical::reconcile_with(&mut store, base, branch, layer, |_| {
        Some(logical::ReconcileChoice::Layer)
    })
    .unwrap()
    .root_id;
    assert_eq!(layer_choice, layer);

    let branch_type = root(
        &mut store,
        &[
            logical::ContentChange::Remove {
                path: "file".into(),
            },
            logical::ContentChange::Mkdir {
                path: "file".into(),
                mode: 0o755,
            },
        ],
        [5; 32],
    );
    let layer_type = root(
        &mut store,
        &[
            logical::ContentChange::Remove {
                path: "file".into(),
            },
            logical::ContentChange::Symlink {
                path: "file".into(),
                target: b"target".to_vec(),
            },
        ],
        [6; 32],
    );
    let type_conflict = logical::reconcile(&mut store, base, branch_type, layer_type).unwrap();
    assert!(type_conflict.conflicts.iter().any(|conflict| {
        conflict.kind == logical::ReconcileConflictKind::Type
            && conflict.affected_paths == vec![CanonicalPath::new("file").unwrap()]
    }));

    let removed = root(
        &mut store,
        &[logical::ContentChange::Remove {
            path: "file".into(),
        }],
        [7; 32],
    );
    let directory = logical::reconcile(&mut store, base, removed, layer).unwrap();
    assert!(directory.conflicts.iter().any(|conflict| {
        conflict.kind == logical::ReconcileConflictKind::Directory
            && conflict.affected_paths == vec![CanonicalPath::new("file").unwrap()]
    }));

    let linked = root(
        &mut store,
        &[logical::ContentChange::HardLink {
            source: "file".into(),
            target: "branch-link".into(),
        }],
        [8; 32],
    );
    let layer_linked = root(
        &mut store,
        &[logical::ContentChange::HardLink {
            source: "file".into(),
            target: "layer-link".into(),
        }],
        [9; 32],
    );
    let hard_link = logical::reconcile(&mut store, base, linked, layer_linked).unwrap();
    assert!(hard_link.conflicts.iter().any(|conflict| {
        conflict.kind == logical::ReconcileConflictKind::HardLink
            && conflict
                .affected_paths
                .contains(&CanonicalPath::new("file").unwrap())
    }));

    let nested_link = root(
        &mut store,
        &[
            logical::ContentChange::Mkdir {
                path: "dir".into(),
                mode: 0o755,
            },
            logical::ContentChange::HardLink {
                source: "file".into(),
                target: "dir/branch-link".into(),
            },
        ],
        [10; 32],
    );
    let root_link = root(
        &mut store,
        &[logical::ContentChange::HardLink {
            source: "file".into(),
            target: "layer-link".into(),
        }],
        [11; 32],
    );
    assert_eq!(
        logical::reconcile_with(&mut store, base, nested_link, root_link, |_| {
            Some(logical::ReconcileChoice::Branch)
        })
        .unwrap()
        .root_id,
        nested_link
    );
}
