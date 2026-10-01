//! Actual C1 trees/SQLite versus independent ordered maps; no candidate root pins.
use layerfs_content::{
    filesystem::{
        inode::codec::{encode_inode_page, InodePage},
        inode::update::apply_inode_values,
    },
    inode_leaf::{InodeKind, InodeValue},
    *,
};
use phase6_live_probe::{tree_diff, tree_facts};
use rusqlite::Connection;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default)]
struct Objects(Arc<Mutex<BTreeMap<ObjectId, Vec<u8>>>>);
impl AuthenticatedObjects for Objects {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let v = self.0.lock().unwrap();
        ids.iter()
            .map(|id| v.get(id).cloned().ok_or(ContentError::MissingObject))
            .collect()
    }
}
struct Sink<'a> {
    objects: Objects,
    db: &'a Connection,
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
fn value(i: u64) -> InodeValue {
    InodeValue {
        kind: if i == 1 {
            InodeKind::Directory
        } else {
            InodeKind::RegularFile
        },
        namespace_ref_count: u64::from(i != 1),
        content_root: ObjectId::for_bytes(&i.to_be_bytes()),
        metadata_root: ObjectId::for_bytes(b"source metadata"),
    }
}
fn certify(db: &Connection, id: ObjectId) {
    tree_facts::certify(db, id, 0, true, 0, &mut Default::default()).unwrap();
}
#[test]
fn actual_tree_split_merge_difference_matches_full_map_and_skips_reuse() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(tree_facts::SCHEMA).unwrap();
    let objects = Objects::default();
    let mut sink = Sink {
        objects: objects.clone(),
        db: &db,
    };
    let old: BTreeMap<_, _> = (1..=1000).map(|i| (i, value(i))).collect();
    let (first, _) = apply_inode_values(
        &mut FilesystemObjects::new(&objects, &mut sink),
        None,
        old.iter().map(|(k, v)| Ok((*k, Some(*v)))),
        FilesystemResources::default().scratch_bytes,
    )
    .unwrap();
    certify(&db, first);
    let mut new = old.clone();
    new.insert(501, value(9999));
    let (second, _) = apply_inode_values(
        &mut FilesystemObjects::new(&objects, &mut sink),
        Some(first),
        std::iter::once(Ok((501, Some(value(9999))))),
        FilesystemResources::default().scratch_bytes,
    )
    .unwrap();
    certify(&db, second);
    let mut got = Vec::new();
    let work = tree_diff::run(&db, Some(first), Some(second), |k, a, b| {
        got.push((
            u64::from_be_bytes(k.try_into().unwrap()),
            a.map(|v| v.to_vec()),
            b.map(|v| v.to_vec()),
        ));
        Ok(())
    })
    .unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, 501);
    assert!(work.skipped > 0);
    assert!(
        work.compared <= 200,
        "must not inspect1000unchanged rows {work:?}"
    );
    let edits: Vec<_> = (100..=950)
        .map(|i| Ok((i, None)))
        .chain((1001..=1300).map(|i| Ok((i, Some(value(i))))))
        .collect();
    for i in 100..=950 {
        new.remove(&i);
    }
    for i in 1001..=1300 {
        new.insert(i, value(i));
    }
    let (third, _) = apply_inode_values(
        &mut FilesystemObjects::new(&objects, &mut sink),
        Some(second),
        edits.into_iter(),
        FilesystemResources::default().scratch_bytes,
    )
    .unwrap();
    certify(&db, third);
    let mut expected = Vec::new();
    for k in old
        .keys()
        .chain(new.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
    {
        if old.get(&k) != new.get(&k) {
            expected.push(k)
        }
    }
    let mut keys = Vec::new();
    tree_diff::run(&db, Some(first), Some(third), |k, _, _| {
        keys.push(u64::from_be_bytes(k.try_into().unwrap()));
        Ok(())
    })
    .unwrap();
    assert_eq!(keys, expected);
}
#[test]
fn missing_and_forged_child_summary_or_role_refused() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(tree_facts::SCHEMA).unwrap();
    let leaf = InodePage::Leaf {
        entries: (1..=50).map(|i| (i, value(i))).collect(),
    };
    let canonical = encode_inode_page(&leaf).unwrap();
    let id = ObjectId::for_bytes(&canonical);
    let branch = InodePage::Branch {
        level: 1,
        subtree_count: 100,
        children: vec![(51, id), (100, id)],
    };
    let bytes = encode_inode_page(&branch).unwrap();
    let parent = ObjectId::for_bytes(&bytes);
    tree_facts::record(&db, parent, ObjectRole::InodeBranch, &bytes).unwrap();
    assert!(tree_facts::certify(&db, parent, 0, true, 0, &mut Default::default()).is_err());
    tree_facts::record(&db, id, ObjectRole::InodeLeaf, &canonical).unwrap();
    assert!(
        tree_facts::certify(&db, parent, 0, true, 0, &mut Default::default())
            .unwrap_err()
            .contains("range/summary")
    );
    assert!(tree_facts::record(&db, id, ObjectRole::DirectoryLeaf, &canonical).is_err());
}
