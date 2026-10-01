//! Structural namespace proof over actual C1 canonical trees and SQLite.
//! Independent logical maps supply expectations; no MinIO/C5/physical claim.
use layerfs_content::{
    filesystem::{
        directory::update::apply_bindings,
        inode::update::apply_inode_values,
        root::{profile_id, scope_for_seed},
        FilesystemRoot,
    },
    inode_leaf::{InodeKind, InodeValue},
    *,
};
use phase6_live_probe::{namespace_index as index, tree_facts, wire::Snapshot};
use rusqlite::Connection;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default)]
struct Objects(Arc<Mutex<BTreeMap<ObjectId, Vec<u8>>>>);
impl AuthenticatedObjects for Objects {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let rows = self.0.lock().unwrap();
        ids.iter()
            .map(|id| rows.get(id).cloned().ok_or(ContentError::MissingObject))
            .collect()
    }
}
struct Sink<'a> {
    db: &'a Connection,
    objects: Objects,
}
impl FinalizedConsumer for Sink<'_> {
    fn accept(&mut self, o: FinalizedObject) -> ContentResult<()> {
        tree_facts::record(self.db, o.id(), o.role(), o.canonical())
            .map_err(|_| ContentError::Io)?;
        self.objects
            .0
            .lock()
            .unwrap()
            .insert(o.id(), o.canonical().to_vec());
        Ok(())
    }
}
struct Fixture {
    db: Connection,
    objects: Objects,
    base: FilesystemRoot,
    snapshot: Snapshot,
}
impl Fixture {
    fn new() -> Self {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(tree_facts::SCHEMA).unwrap();
        db.execute_batch(index::SCHEMA).unwrap();
        let objects = Objects::default();
        let mut f = Self {
            db,
            objects,
            base: FilesystemRoot::new(
                profile_id(),
                scope_for_seed([9; 32]),
                1,
                ObjectId::for_bytes(b"uninitialized"),
            )
            .unwrap(),
            snapshot: Snapshot {
                stack: [1; 17],
                branch: [2; 17],
                base: [3; 33],
                head: None,
                root: [0; 32],
                scope: *scope_for_seed([9; 32]).object().as_bytes(),
                profile: *profile_id().as_bytes(),
            },
        };
        let dir = f.directory(&[]);
        let root = f.build(&BTreeMap::from([(1, dir)]));
        f.base = root;
        f.snapshot.root = f.id(root);
        index::initialize_empty(&f.db, &f.snapshot, root).unwrap();
        f
    }
    fn directory(&self, entries: &[(&str, u64)]) -> InodeValue {
        let mut rows: Vec<_> = entries
            .iter()
            .map(|(n, id)| (PathName::new(n).unwrap(), *id))
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let mut sink = Sink {
            db: &self.db,
            objects: self.objects.clone(),
        };
        let content_root = if rows.is_empty() {
            let bytes = layerfs_content::filesystem::directory::codec::encode_directory_page(
                &layerfs_content::filesystem::directory::codec::DirectoryPage::Leaf {
                    entries: vec![],
                },
            )
            .unwrap();
            let id = ObjectId::for_bytes(&bytes);
            tree_facts::record(&self.db, id, ObjectRole::DirectoryLeaf, &bytes).unwrap();
            self.objects.0.lock().unwrap().insert(id, bytes);
            id
        } else {
            let (root, _) = apply_bindings(
                &mut FilesystemObjects::new(&self.objects, &mut sink),
                None,
                rows.into_iter().map(|(n, id)| Ok((n, Some(id)))),
                FilesystemResources::default().scratch_bytes,
                &mut |_, _| Ok(()),
            )
            .unwrap();
            root.0
        };
        InodeValue {
            kind: InodeKind::Directory,
            namespace_ref_count: 1,
            content_root,
            metadata_root: ObjectId::for_bytes(b"portable placeholder; structural test only"),
        }
    }
    fn build(&self, rows: &BTreeMap<u64, InodeValue>) -> FilesystemRoot {
        let mut sink = Sink {
            db: &self.db,
            objects: self.objects.clone(),
        };
        let (table, _) = apply_inode_values(
            &mut FilesystemObjects::new(&self.objects, &mut sink),
            None,
            rows.iter().map(|(id, v)| {
                let mut v = *v;
                if *id == 1 {
                    v.namespace_ref_count = 0;
                }
                Ok((*id, Some(v)))
            }),
            FilesystemResources::default().scratch_bytes,
        )
        .unwrap();
        FilesystemRoot::new(profile_id(), scope_for_seed([9; 32]), 1, table).unwrap()
    }
    fn id(&self, root: FilesystemRoot) -> [u8; 32] {
        *ObjectId::for_bytes(&root.encode().unwrap()).as_bytes()
    }
    fn prepare(&self, new: FilesystemRoot) -> Result<index::Work, String> {
        index::prepare(&self.db, &self.snapshot, self.base, new)
    }
    fn adopt(&mut self, new: FilesystemRoot) {
        index::seal(
            &self.db,
            &self.snapshot,
            ObjectId::from_bytes(&self.id(new)).unwrap(),
        )
        .unwrap();
        let mut next = self.snapshot.clone();
        next.root = self.id(new);
        next.head = Some([7; 33]);
        index::install_known(&self.db, &self.snapshot, &next).unwrap();
        self.snapshot = next;
        self.base = new;
    }
    fn refusal(&self, new: FilesystemRoot, expected: &str) {
        assert!(self.prepare(new).unwrap_err().contains(expected));
        assert!(index::seal(
            &self.db,
            &self.snapshot,
            ObjectId::from_bytes(&self.id(new)).unwrap()
        )
        .is_err());
        index::check_base(&self.db, &self.snapshot).unwrap();
    }
}
fn file(tag: u64) -> InodeValue {
    InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&tag.to_be_bytes()),
        metadata_root: ObjectId::for_bytes(b"file metadata placeholder"),
    }
}
#[test]
fn one_edit_skips_unchanged_subtrees_and_known_install_requires_seal() {
    let mut f = Fixture::new();
    let names: Vec<_> = (2..=301).map(|id| format!("f{id:04}")).collect();
    let bindings: Vec<_> = names
        .iter()
        .zip(2..=301)
        .map(|(n, id)| (n.as_str(), id))
        .collect();
    let mut rows: BTreeMap<_, _> = (2..=301).map(|id| (id, file(id))).collect();
    rows.insert(1, f.directory(&bindings));
    let populated = f.build(&rows);
    f.prepare(populated).unwrap();
    f.adopt(populated);
    rows.insert(155, file(777));
    let edited = f.build(&rows);
    let w = f.prepare(edited).unwrap();
    assert_eq!(w.inode_diff.changed, 1);
    assert!(w.inode_diff.skipped > 0);
    assert!(w.inode_diff.compared <= 200);
    assert_eq!(w.changed_names, 0);
    assert_eq!(w.parent_steps, 0);
    assert_eq!(w.affected_inodes, 1);
    assert!(w.peak_owned_bytes < 4 * 1024 * 1024);
    assert_eq!(index::committed(&f.db, 155).unwrap(), Some(file(155)));
    let mut next = f.snapshot.clone();
    next.root = f.id(edited);
    assert!(index::install_known(&f.db, &f.snapshot, &next).is_err());
    f.adopt(edited);
    assert_eq!(index::committed(&f.db, 155).unwrap(), Some(file(777)));
    let mut stale = f.snapshot.clone();
    stale.root = f.id(populated);
    assert!(index::check_base(&f.db, &stale)
        .unwrap_err()
        .contains("stale"));
}
#[test]
fn extra_unbound_missing_and_bad_reference_records_refuse() {
    for shape in 0..3 {
        let f = Fixture::new();
        let mut rows = BTreeMap::new();
        rows.insert(
            1,
            f.directory(if shape == 0 { &[] } else { &[("file", 2)] }),
        );
        if shape != 1 {
            let mut v = file(2);
            if shape == 2 {
                v.namespace_ref_count = 2;
            }
            rows.insert(2, v);
        }
        f.refusal(f.build(&rows), "reference count/record mismatch");
    }
}
#[test]
fn omitted_removal_and_new_alias_refuse() {
    for alias in [false, true] {
        let mut f = Fixture::new();
        let mut rows = BTreeMap::from([(1, f.directory(&[("file", 2)])), (2, file(2))]);
        let first = f.build(&rows);
        f.prepare(first).unwrap();
        f.adopt(first);
        rows.insert(
            1,
            f.directory(if alias {
                &[("file", 2), ("alias", 2)]
            } else {
                &[]
            }),
        );
        f.refusal(f.build(&rows), "reference count/record mismatch");
    }
}
#[test]
fn disconnected_cycle_refuses_even_with_exact_single_reference_counts() {
    let mut f = Fixture::new();
    let mut rows = BTreeMap::from([
        (1, f.directory(&[("a", 2)])),
        (2, f.directory(&[("b", 3)])),
        (3, f.directory(&[])),
    ]);
    let first = f.build(&rows);
    f.prepare(first).unwrap();
    f.adopt(first);
    rows.insert(1, f.directory(&[]));
    rows.insert(3, f.directory(&[("a", 2)]));
    f.refusal(f.build(&rows), "parent cycle");
}
#[test]
fn move_updates_parent_without_rewriting_child_and_complete_subtree_removal_passes() {
    let mut f = Fixture::new();
    let mut rows = BTreeMap::from([
        (1, f.directory(&[("a", 2), ("b", 3)])),
        (2, f.directory(&[("file", 4)])),
        (3, f.directory(&[])),
        (4, file(4)),
    ]);
    let first = f.build(&rows);
    f.prepare(first).unwrap();
    f.adopt(first);
    rows.insert(2, f.directory(&[]));
    rows.insert(3, f.directory(&[("moved", 4)]));
    let moved = f.build(&rows);
    let w = f.prepare(moved).unwrap();
    assert_eq!(w.inode_diff.changed, 2);
    assert_eq!(w.changed_names, 2);
    f.adopt(moved);
    let parent: i64 =
        f.db.query_row(
            "SELECT parent FROM namespace_inodes WHERE serial=4",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(parent, 3);
    assert_eq!(index::committed(&f.db, 4).unwrap(), Some(file(4)));
    rows.insert(1, f.directory(&[("a", 2)]));
    rows.remove(&3);
    rows.remove(&4);
    let removed = f.build(&rows);
    f.prepare(removed).unwrap();
    f.adopt(removed);
    assert!(index::committed(&f.db, 3).unwrap().is_none());
    assert!(index::committed(&f.db, 4).unwrap().is_none());
}
#[test]
fn reversed_270_directory_chain_visits_each_parent_once() {
    let mut f = Fixture::new();
    let mut rows = BTreeMap::from([(1, f.directory(&[("deep", 271)])), (272, file(272))]);
    for id in 2..=271 {
        rows.insert(
            id,
            f.directory(&[("next", if id == 2 { 272 } else { id - 1 })]),
        );
    }
    let deep = f.build(&rows);
    let w = f.prepare(deep).unwrap();
    assert_eq!(w.parent_steps, 270);
    assert_eq!(w.changed_names, 271);
    f.adopt(deep);
    rows.insert(272, file(999));
    let edited = f.build(&rows);
    let w = f.prepare(edited).unwrap();
    assert_eq!(w.affected_inodes, 1);
    assert_eq!(w.parent_steps, 0);
    assert_eq!(w.changed_names, 0);
    f.adopt(edited);
}
