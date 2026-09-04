use layerfs_content::filesystem::{
    self as fs, ContentChange as Change, ReconcileBudget, ReconcileChoice, ReconcileConflict,
    ReconcileConflictKind,
};
use layerfs_content::object::access::ObjectStore;
use layerfs_content::{CanonicalPath, CoreError, CoreResult, ObjectId};
use std::collections::BTreeMap;

#[derive(Default)]
struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);
impl ObjectStore for MemoryStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
    }
    fn put(&mut self, bytes: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(bytes);
        self.0.insert(id, bytes.to_vec());
        Ok(id)
    }
}
fn write(path: &str, bytes: &[u8]) -> Change {
    Change::Write {
        path: path.into(),
        bytes: bytes.to_vec(),
        mode: 0o644,
    }
}
fn apply(store: &mut MemoryStore, root: ObjectId, changes: &[Change]) -> ObjectId {
    fs::apply_changes(store, root, changes, [9; 32])
        .unwrap()
        .root_id
}
fn conflict(store: &MemoryStore, root: ObjectId, path: &str) -> ReconcileConflict {
    let path = CanonicalPath::new(path).unwrap();
    let node = fs::resolve(store, root, &path, &mut fs::LogicalCounters::default()).unwrap();
    ReconcileConflict {
        inode: node.inode,
        kind: ReconcileConflictKind::Content,
        affected_paths: vec![path],
        base: None,
        branch: None,
        layer: None,
    }
}
fn bytes(store: &MemoryStore, root: ObjectId, path: &str) -> Vec<u8> {
    let mut out = Vec::new();
    fs::stream(store, root, &CanonicalPath::new(path).unwrap(), &mut out).unwrap();
    out
}

#[test]
fn choices_expand_missing_parent_then_apply_later_child_without_losing_siblings() {
    let mut store = MemoryStore::default();
    let empty = fs::empty_root(&mut store, [1; 32]).unwrap();
    let base = apply(
        &mut store,
        empty,
        &[
            Change::Mkdir {
                path: "dir".into(),
                mode: 0o755,
            },
            write("dir/a", b"a"),
            write("dir/b", b"b"),
            write("dir/c", b"c"),
        ],
    );
    let branch = apply(
        &mut store,
        base,
        &[
            write("dir/a", b"branch-a"),
            write("dir/b", b"branch-b"),
            write("dir/c", b"branch-c"),
        ],
    );
    let layer = apply(
        &mut store,
        base,
        &[
            write("dir/a", b"layer-a"),
            write("dir/b", b"layer-b"),
            write("dir/c", b"layer-c"),
        ],
    );
    let working = apply(
        &mut store,
        base,
        &[
            Change::Remove {
                path: "dir/a".into(),
            },
            Change::Remove {
                path: "dir/b".into(),
            },
            Change::Remove {
                path: "dir/c".into(),
            },
            Change::Remove { path: "dir".into() },
        ],
    );
    let conflicts = [
        conflict(&store, base, "dir/a"),
        conflict(&store, base, "dir/b"),
    ];
    let dir = std::env::temp_dir();
    let budget = ReconcileBudget {
        scratch_dir: &dir,
        memory_bytes: 8 * 1024 * 1024,
        spool_bytes: 1024 * 1024,
    };
    let root = fs::replace_choices_from_snapshots_bounded(
        &mut store,
        working,
        branch,
        layer,
        &conflicts,
        &[ReconcileChoice::Branch, ReconcileChoice::Layer],
        budget,
    )
    .unwrap();
    assert_eq!(bytes(&store, root, "dir/a"), b"branch-a");
    assert_eq!(bytes(&store, root, "dir/b"), b"layer-b");
    assert_eq!(bytes(&store, root, "dir/c"), b"branch-c");
}

#[test]
fn selected_inode_content_preserves_unselected_alias_reference_count() {
    let mut store = MemoryStore::default();
    let empty = fs::empty_root(&mut store, [2; 32]).unwrap();
    let base = apply(
        &mut store,
        empty,
        &[
            write("file", b"old"),
            Change::HardLink {
                source: "file".into(),
                target: "outside".into(),
            },
        ],
    );
    let source = apply(
        &mut store,
        base,
        &[
            Change::Remove {
                path: "outside".into(),
            },
            write("file", b"updated"),
        ],
    );
    let conflicts = [conflict(&store, base, "file")];
    let dir = std::env::temp_dir();
    let budget = ReconcileBudget {
        scratch_dir: &dir,
        memory_bytes: 8 * 1024 * 1024,
        spool_bytes: 1024 * 1024,
    };
    let root = fs::replace_choices_from_snapshots_bounded(
        &mut store,
        base,
        source,
        base,
        &conflicts,
        &[ReconcileChoice::Branch],
        budget,
    )
    .unwrap();
    let a = fs::resolve(
        &store,
        root,
        &CanonicalPath::new("file").unwrap(),
        &mut fs::LogicalCounters::default(),
    )
    .unwrap();
    let b = fs::resolve(
        &store,
        root,
        &CanonicalPath::new("outside").unwrap(),
        &mut fs::LogicalCounters::default(),
    )
    .unwrap();
    assert_eq!(a.inode, b.inode);
    assert_eq!(a.record.namespace_ref_count, 2);
    assert_eq!(bytes(&store, root, "outside"), b"updated");
}
