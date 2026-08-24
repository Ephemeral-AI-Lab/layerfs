use layerfs_core::content::rope::ObjectStore;
use layerfs_core::inode::{
    diff_inode_table_entries, generated_inode_table_from_root, generated_inode_table_upsert,
    inode_table_lookup, inode_table_remove, inode_table_upsert, visit_inode_table_entries, InodeId,
    InodeTableCounters, InodeTableRoot,
};
use layerfs_core::namespace::{
    diff_directory_entries, directory_insert, directory_lookup, directory_remove, directory_rename,
    empty_directory, visit_directory_entries, DirectoryStateRoot, DirectoryStateV1,
    NamespaceCounters,
};
use layerfs_core::namespace_codec::{
    decode_directory_node, decode_directory_state, decode_inode_table_node, encode_directory_node,
    encode_directory_state, encode_inode_table_node, profile_id, DirectoryNodeV1, InodeTableNodeV1,
};
use layerfs_core::{CanonicalName, CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

impl ObjectStore for MemoryStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
    }
    fn put(&mut self, bytes: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(bytes);
        self.0.entry(id).or_insert_with(|| bytes.to_vec());
        Ok(id)
    }
}

fn new_ids(store: &MemoryStore, before: &BTreeSet<ObjectId>) -> BTreeSet<ObjectId> {
    store
        .0
        .keys()
        .filter(|id| !before.contains(id))
        .copied()
        .collect()
}

fn directory_reachable(store: &MemoryStore, root: DirectoryStateRoot) -> BTreeSet<ObjectId> {
    fn visit(store: &MemoryStore, id: ObjectId, reachable: &mut BTreeSet<ObjectId>) {
        if !reachable.insert(id) {
            return;
        }
        if let DirectoryNodeV1::Branch { children, .. } =
            decode_directory_node(store.0.get(&id).unwrap()).unwrap()
        {
            for (_, child) in children {
                visit(store, child, reachable);
            }
        }
    }
    let mut reachable = BTreeSet::from([root.0]);
    let state = decode_directory_state(store.0.get(&root.0).unwrap()).unwrap();
    visit(store, state.mapping_root, &mut reachable);
    reachable
}

fn inode_reachable(
    store: &MemoryStore,
    root: layerfs_core::inode::InodeTableRoot,
) -> BTreeSet<ObjectId> {
    fn visit(store: &MemoryStore, id: ObjectId, reachable: &mut BTreeSet<ObjectId>) {
        if !reachable.insert(id) {
            return;
        }
        if let InodeTableNodeV1::Branch { children, .. } =
            decode_inode_table_node(store.0.get(&id).unwrap()).unwrap()
        {
            for (_, child) in children {
                visit(store, child, reachable);
            }
        }
    }
    let mut reachable = BTreeSet::new();
    visit(store, root.0, &mut reachable);
    reachable
}

fn directory_leaf_counts(store: &MemoryStore, root: DirectoryStateRoot) -> Vec<usize> {
    let state = decode_directory_state(store.0.get(&root.0).unwrap()).unwrap();
    let DirectoryNodeV1::Branch { children, .. } =
        decode_directory_node(store.0.get(&state.mapping_root).unwrap()).unwrap()
    else {
        panic!("fixture is not a directory branch")
    };
    children
        .into_iter()
        .map(
            |(_, id)| match decode_directory_node(store.0.get(&id).unwrap()).unwrap() {
                DirectoryNodeV1::Leaf { entries, .. } => entries.len(),
                _ => panic!("fixture child is not a leaf"),
            },
        )
        .collect()
}

fn inode_leaf_counts(store: &MemoryStore, root: layerfs_core::inode::InodeTableRoot) -> Vec<usize> {
    let InodeTableNodeV1::Branch { children, .. } =
        decode_inode_table_node(store.0.get(&root.0).unwrap()).unwrap()
    else {
        panic!("fixture is not an inode branch")
    };
    children
        .into_iter()
        .map(
            |(_, id)| match decode_inode_table_node(store.0.get(&id).unwrap()).unwrap() {
                InodeTableNodeV1::Leaf(entries) => entries.len(),
                _ => panic!("fixture child is not a leaf"),
            },
        )
        .collect()
}

#[test]
fn paired_persistent_diffs_prune_unchanged_directory_and_inode_subtrees() {
    let mut store = MemoryStore::default();
    let store_id = [0x44; 32];
    let root_inode = InodeId::allocate(store_id, 0);
    let mut inodes =
        generated_inode_table_from_root(&mut store, root_inode, ObjectId::for_bytes(b"root"))
            .unwrap();
    let mut directory = empty_directory(&mut store).unwrap();
    for serial in 1..=300_u64 {
        let inode = InodeId::allocate(store_id, serial);
        inodes = generated_inode_table_upsert(
            &mut store,
            inodes,
            inode,
            ObjectId::for_bytes(&serial.to_be_bytes()),
        )
        .unwrap()
        .0;
        directory = directory_insert(
            &mut store,
            directory,
            CanonicalName::new(&format!("entry-{serial:04}")).unwrap(),
            inode,
        )
        .unwrap()
        .0;
    }
    let old_inodes = inodes.into_root();
    let old_directory = directory;
    let changed_inode = InodeId::allocate(store_id, 150);
    let new_inodes = inode_table_upsert(
        &mut store,
        old_inodes,
        changed_inode,
        ObjectId::for_bytes(b"replacement"),
    )
    .unwrap()
    .0;
    let old_name = CanonicalName::new("entry-0150").unwrap();
    let new_name = CanonicalName::new("renamed-0150").unwrap();
    let new_directory = directory_rename(&mut store, old_directory, &old_name, new_name.clone())
        .unwrap()
        .0;

    let mut inode_diffs = Vec::new();
    let inode_counters = diff_inode_table_entries(&store, old_inodes, new_inodes, |diff| {
        inode_diffs.push(diff);
        Ok(())
    })
    .unwrap();
    assert_eq!(inode_diffs.len(), 1);
    assert_eq!(inode_diffs[0].inode, changed_inode);
    assert!(inode_counters.nodes_read < 10);

    let mut directory_diffs = Vec::new();
    let directory_counters = diff_directory_entries(&store, old_directory, new_directory, |diff| {
        directory_diffs.push(diff);
        Ok(())
    })
    .unwrap();
    assert_eq!(directory_diffs.len(), 2);
    assert_eq!(directory_diffs[0].name, old_name);
    assert_eq!(directory_diffs[1].name, new_name);
    assert!(directory_counters.nodes_read < 20, "{directory_counters:?}");

    assert_eq!(
        diff_inode_table_entries(&store, old_inodes, old_inodes, |_| Ok(())).unwrap(),
        InodeTableCounters::default()
    );
    assert_eq!(
        diff_directory_entries(&store, old_directory, old_directory, |_| Ok(())).unwrap(),
        NamespaceCounters::default()
    );
}

#[test]
fn inode_diff_merges_unequal_canonical_heights_without_collecting_entries() {
    let mut store = MemoryStore::default();
    let mut entries = (0..128_u64)
        .map(|serial| {
            (
                InodeId::allocate([0x91; 32], serial),
                ObjectId::for_bytes(&serial.to_be_bytes()),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.0);
    let old = InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Leaf(entries[..127].to_vec())).unwrap(),
            )
            .unwrap(),
    );
    let left = store
        .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(entries[..64].to_vec())).unwrap())
        .unwrap();
    let right = store
        .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(entries[64..].to_vec())).unwrap())
        .unwrap();
    let new = InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 1,
                    subtree_entry_count: 128,
                    children: vec![(entries[63].0, left), (entries[127].0, right)],
                })
                .unwrap(),
            )
            .unwrap(),
    );
    let mut diffs = Vec::new();
    let counters = diff_inode_table_entries(&store, old, new, |diff| {
        diffs.push(diff);
        Ok(())
    })
    .unwrap();
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].inode, entries[127].0);
    assert_eq!(diffs[0].before, None);
    assert_eq!(diffs[0].after, Some(entries[127].1));
    assert!(counters.nodes_read <= 8);
}

#[test]
fn directory_diff_merges_unequal_canonical_heights_without_collecting_entries() {
    let mut store = MemoryStore::default();
    let entries = (0..128_u64)
        .map(|serial| {
            (
                CanonicalName::new(&format!("entry-{serial:014}")).unwrap(),
                InodeId::allocate([0x73; 32], serial),
            )
        })
        .collect::<Vec<_>>();
    let leaf = |entries: &[(CanonicalName, InodeId)]| DirectoryNodeV1::Leaf {
        subtree_encoded_bytes: entries
            .iter()
            .map(|entry| 34 + entry.0.as_bytes().len() as u64)
            .sum(),
        entries: entries.to_vec(),
    };
    let old_mapping = store
        .put(&encode_directory_node(&leaf(&entries)).unwrap())
        .unwrap();
    let left = store
        .put(&encode_directory_node(&leaf(&entries[..64])).unwrap())
        .unwrap();
    let right = store
        .put(&encode_directory_node(&leaf(&entries[64..])).unwrap())
        .unwrap();
    let bytes = entries
        .iter()
        .map(|entry| 34 + entry.0.as_bytes().len() as u64)
        .sum();
    let new_mapping = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 1,
                subtree_entry_count: 128,
                subtree_encoded_bytes: bytes,
                children: vec![
                    (entries[63].0.clone(), left),
                    (entries[127].0.clone(), right),
                ],
            })
            .unwrap(),
        )
        .unwrap();
    let state = |mapping_root, tree_level| DirectoryStateV1 {
        entry_count: 128,
        tree_level,
        profile_id: profile_id(),
        mapping_root,
    };
    let old = DirectoryStateRoot(
        store
            .put(&encode_directory_state(state(old_mapping, 0)).unwrap())
            .unwrap(),
    );
    let new = DirectoryStateRoot(
        store
            .put(&encode_directory_state(state(new_mapping, 1)).unwrap())
            .unwrap(),
    );
    let mut diffs = Vec::new();
    let counters = diff_directory_entries(&store, old, new, |diff| {
        diffs.push(diff);
        Ok(())
    })
    .unwrap();
    assert!(diffs.is_empty());
    assert!(counters.nodes_read <= 8);
}

#[test]
fn directory_diff_streams_valid_unmatched_prefixes_tails_and_renames() {
    fn leaf(store: &mut MemoryStore, prefix: &str, inode_seed: u8) -> (CanonicalName, ObjectId) {
        let entries = (0..64_u64)
            .map(|serial| {
                (
                    CanonicalName::new(&format!("{prefix}-{serial:03}-{}", "x".repeat(48)))
                        .unwrap(),
                    InodeId::allocate([inode_seed; 32], serial),
                )
            })
            .collect::<Vec<_>>();
        let bytes = entries
            .iter()
            .map(|entry| 34 + entry.0.as_bytes().len() as u64)
            .sum();
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: bytes,
                    entries: entries.clone(),
                })
                .unwrap(),
            )
            .unwrap();
        (entries.last().unwrap().0.clone(), id)
    }

    fn root(
        store: &mut MemoryStore,
        children: Vec<(CanonicalName, ObjectId)>,
    ) -> DirectoryStateRoot {
        let mut entry_count = 0_u64;
        let mut encoded_bytes = 0_u64;
        for (_, child) in &children {
            match decode_directory_node(store.0.get(child).unwrap()).unwrap() {
                DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes,
                    entries,
                } => {
                    entry_count += entries.len() as u64;
                    encoded_bytes += subtree_encoded_bytes;
                }
                _ => panic!("fixture child is not a leaf"),
            }
        }
        let mapping_root = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Branch {
                    level: 1,
                    subtree_entry_count: entry_count,
                    subtree_encoded_bytes: encoded_bytes,
                    children,
                })
                .unwrap(),
            )
            .unwrap();
        DirectoryStateRoot(
            store
                .put(
                    &encode_directory_state(DirectoryStateV1 {
                        entry_count,
                        tree_level: 1,
                        profile_id: profile_id(),
                        mapping_root,
                    })
                    .unwrap(),
                )
                .unwrap(),
        )
    }

    let mut store = MemoryStore::default();
    let prefix = leaf(&mut store, "a", 1);
    let middle = leaf(&mut store, "m", 2);
    let tail = leaf(&mut store, "z", 3);
    let middle_tail = root(&mut store, vec![middle.clone(), tail.clone()]);
    let all = root(
        &mut store,
        vec![prefix.clone(), middle.clone(), tail.clone()],
    );
    let prefix_middle = root(&mut store, vec![prefix.clone(), middle.clone()]);

    for (old, new, before, after) in [
        (middle_tail, all, 0, 64),
        (all, middle_tail, 64, 0),
        (prefix_middle, all, 0, 64),
        (all, prefix_middle, 64, 0),
    ] {
        let mut diffs = Vec::new();
        let counters = diff_directory_entries(&store, old, new, |diff| {
            diffs.push(diff);
            Ok(())
        })
        .unwrap();
        assert_eq!(diffs.len(), 64);
        assert_eq!(
            diffs
                .iter()
                .filter(|diff| diff.before.is_some() && diff.after.is_none())
                .count(),
            before
        );
        assert_eq!(
            diffs
                .iter()
                .filter(|diff| diff.before.is_none() && diff.after.is_some())
                .count(),
            after
        );
        assert!(counters.nodes_read <= 7, "{counters:?}");
    }

    let renamed_prefix = leaf(&mut store, "b", 1);
    let renamed_tail = leaf(&mut store, "y", 3);
    for new in [
        root(
            &mut store,
            vec![renamed_prefix, middle.clone(), tail.clone()],
        ),
        root(&mut store, vec![prefix, middle, renamed_tail]),
    ] {
        let mut diffs = Vec::new();
        diff_directory_entries(&store, all, new, |diff| {
            diffs.push(diff);
            Ok(())
        })
        .unwrap();
        assert_eq!(diffs.len(), 128);
        assert_eq!(diffs.iter().filter(|diff| diff.after.is_none()).count(), 64);
        assert_eq!(
            diffs.iter().filter(|diff| diff.before.is_none()).count(),
            64
        );
    }
}

#[test]
fn underfull_middle_directory_borrows_right_before_merging_left() {
    let mut store = MemoryStore::default();
    let mut entries = Vec::new();
    for serial in 0..193_u64 {
        let text = format!("{serial:017}");
        let name = CanonicalName::new(&text).unwrap();
        entries.push((name, InodeId::allocate([0x31; 32], serial)));
    }
    let mut children = Vec::new();
    for entries in [&entries[..64], &entries[64..128], &entries[128..]] {
        let bytes = entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len() as u64)
            .sum();
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: bytes,
                    entries: entries.to_vec(),
                })
                .unwrap(),
            )
            .unwrap();
        children.push((entries.last().unwrap().0.clone(), id));
    }
    let mapping = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 1,
                subtree_entry_count: 193,
                subtree_encoded_bytes: entries
                    .iter()
                    .map(|(name, _)| 34 + name.as_bytes().len() as u64)
                    .sum(),
                children,
            })
            .unwrap(),
        )
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 193,
                    tree_level: 1,
                    profile_id: profile_id(),
                    mapping_root: mapping,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(directory_leaf_counts(&store, root), [64, 64, 65]);
    let retained = root;
    let removed_name = entries[64].0.clone();
    let root = directory_remove(&mut store, root, &removed_name).unwrap().0;
    assert_eq!(directory_leaf_counts(&store, root), [64, 64, 64]);
    assert!(directory_lookup(
        &store,
        retained,
        &removed_name,
        &mut NamespaceCounters::default()
    )
    .unwrap()
    .is_some());
    assert_eq!(
        directory_lookup(
            &store,
            root,
            &removed_name,
            &mut NamespaceCounters::default()
        )
        .unwrap(),
        None
    );
}

#[test]
fn variable_width_directory_borrows_until_both_leaves_are_filled() {
    let mut store = MemoryStore::default();
    let mut groups = vec![Vec::new(), Vec::new(), Vec::new()];
    let mut serial = 0_u64;
    for index in 0..131 {
        groups[0].push((
            CanonicalName::new(&format!("a{index:03}")).unwrap(),
            InodeId::allocate([0x41; 32], serial),
        ));
        serial += 1;
    }
    for index in 0..12 {
        groups[1].push((
            CanonicalName::new(&format!("m{}{index:04}", "m".repeat(250))).unwrap(),
            InodeId::allocate([0x41; 32], serial),
        ));
        serial += 1;
    }
    for index in 0..131 {
        groups[2].push((
            CanonicalName::new(&format!("z{index:03}")).unwrap(),
            InodeId::allocate([0x41; 32], serial),
        ));
        serial += 1;
    }
    let mut children = Vec::new();
    for entries in &groups {
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: entries
                        .iter()
                        .map(|(name, _)| 34 + name.as_bytes().len() as u64)
                        .sum(),
                    entries: entries.clone(),
                })
                .unwrap(),
            )
            .unwrap();
        children.push((entries.last().unwrap().0.clone(), id));
    }
    let mapping = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 1,
                subtree_entry_count: serial,
                subtree_encoded_bytes: groups
                    .iter()
                    .flatten()
                    .map(|(name, _)| 34 + name.as_bytes().len() as u64)
                    .sum(),
                children,
            })
            .unwrap(),
        )
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: serial,
                    tree_level: 1,
                    profile_id: profile_id(),
                    mapping_root: mapping,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    let removed = groups[1][0].0.clone();
    let next = directory_remove(&mut store, root, &removed).unwrap().0;
    assert_eq!(directory_leaf_counts(&store, next), [129, 13, 131]);
    assert!(
        directory_lookup(&store, root, &removed, &mut NamespaceCounters::default())
            .unwrap()
            .is_some()
    );
}

#[test]
fn underfull_middle_inode_leaf_borrows_right_before_merging_left() {
    let mut store = MemoryStore::default();
    let store_id = [0x41; 32];
    let mut entries = (0..193_u64)
        .map(|serial| {
            (
                InodeId::allocate(store_id, serial),
                ObjectId::for_bytes(&serial.to_be_bytes()),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.0);
    let mut children = Vec::new();
    for entries in [&entries[..64], &entries[64..128], &entries[128..]] {
        let id = store
            .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(entries.to_vec())).unwrap())
            .unwrap();
        children.push((entries.last().unwrap().0, id));
    }
    let retained = InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 1,
                    subtree_entry_count: 193,
                    children,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(inode_leaf_counts(&store, retained), [64, 64, 65]);
    let InodeTableNodeV1::Branch { children, .. } =
        decode_inode_table_node(store.0.get(&retained.0).unwrap()).unwrap()
    else {
        unreachable!()
    };
    let InodeTableNodeV1::Leaf(middle) =
        decode_inode_table_node(store.0.get(&children[1].1).unwrap()).unwrap()
    else {
        unreachable!()
    };
    let removed = middle[0].0;
    let current = inode_table_remove(&mut store, retained, removed).unwrap().0;
    assert_eq!(inode_leaf_counts(&store, current), [64, 64, 64]);
    assert!(inode_table_lookup(
        &store,
        retained,
        removed,
        &mut InodeTableCounters::default()
    )
    .unwrap()
    .is_some());
    assert_eq!(
        inode_table_lookup(&store, current, removed, &mut InodeTableCounters::default()).unwrap(),
        None
    );
}

#[test]
fn inode_table_scales_and_record_update_rewrites_only_its_spine() {
    let mut store = MemoryStore::default();
    let store_id = [9; 32];
    let root_inode = InodeId::allocate(store_id, 0);
    let mut generated = generated_inode_table_from_root(
        &mut store,
        root_inode,
        ObjectId::for_bytes(b"root record"),
    )
    .unwrap();
    for serial in 1..=10_000_u64 {
        let (next, counters) = generated_inode_table_upsert(
            &mut store,
            generated,
            InodeId::allocate(store_id, serial),
            ObjectId::for_bytes(&serial.to_be_bytes()),
        )
        .unwrap();
        assert!(
            counters.nodes_created <= 4,
            "inode insertion copied more than a spine: {counters:?}"
        );
        generated = next;
    }
    let root = generated.into_root();
    let mut streamed = 0;
    let mut visitor_reads = InodeTableCounters::default();
    visit_inode_table_entries(&store, root, &mut visitor_reads, |leaf| {
        assert!(leaf.len() <= 128);
        streamed += leaf.len();
        Ok(())
    })
    .unwrap();
    assert_eq!(streamed, 10_001);
    assert!(
        visitor_reads.nodes_read <= 200,
        "full inode visitor reloaded subtrees: {visitor_reads:?}"
    );
    let retained = root;
    let target = InodeId::allocate(store_id, 5_000);
    let replacement = ObjectId::for_bytes(b"changed inode record");
    let (next, counters) = inode_table_upsert(&mut store, root, target, replacement).unwrap();
    assert!(counters.nodes_created <= 4);
    let mut reads = InodeTableCounters::default();
    assert_eq!(
        inode_table_lookup(&store, next, target, &mut reads).unwrap(),
        Some(replacement)
    );
    assert!(
        reads.nodes_read <= 4,
        "lookup read more than one spine: {reads:?}"
    );
    let mut reads = InodeTableCounters::default();
    assert_eq!(
        inode_table_lookup(&store, retained, target, &mut reads).unwrap(),
        Some(ObjectId::for_bytes(&5_000_u64.to_be_bytes()))
    );
    assert!(
        reads.nodes_read <= 4,
        "lookup read more than one spine: {reads:?}"
    );
}

#[test]
fn remove_merge_root_collapse_and_rename_match_oracle() {
    let mut store = MemoryStore::default();
    let mut root = empty_directory(&mut store).unwrap();
    let store_id = [7; 32];
    for serial in 1..=1_000_u64 {
        let (next, _) = directory_insert(
            &mut store,
            root,
            CanonicalName::new(&format!("n-{serial:04}")).unwrap(),
            InodeId::allocate(store_id, serial),
        )
        .unwrap();
        root = next;
    }
    let retained = root;
    for serial in 1..=900_u64 {
        let name = CanonicalName::new(&format!("n-{serial:04}")).unwrap();
        let (next, removed, counters) = directory_remove(&mut store, root, &name).unwrap();
        assert_eq!(removed, InodeId::allocate(store_id, serial));
        assert!(counters.nodes_created <= 8, "nonlocal delete: {counters:?}");
        root = next;
    }
    let from = CanonicalName::new("n-0950").unwrap();
    let to = CanonicalName::new("renamed").unwrap();
    let (renamed, _) = directory_rename(&mut store, root, &from, to.clone()).unwrap();
    let mut counters = NamespaceCounters::default();
    assert_eq!(
        directory_lookup(&store, renamed, &from, &mut counters).unwrap(),
        None
    );
    assert_eq!(
        directory_lookup(&store, renamed, &to, &mut counters).unwrap(),
        Some(InodeId::allocate(store_id, 950))
    );
    assert_eq!(
        directory_lookup(
            &store,
            retained,
            &CanonicalName::new("n-0001").unwrap(),
            &mut counters
        )
        .unwrap(),
        Some(InodeId::allocate(store_id, 1))
    );
}

#[test]
fn deterministic_mixed_directory_edits_preserve_periodic_roots() {
    fn entries(store: &MemoryStore, root: DirectoryStateRoot) -> Vec<(CanonicalName, InodeId)> {
        let mut values = Vec::new();
        visit_directory_entries(store, root, &mut NamespaceCounters::default(), |leaf| {
            values.extend_from_slice(leaf);
            Ok(())
        })
        .unwrap();
        values
    }

    let mut store = MemoryStore::default();
    let mut root = empty_directory(&mut store).unwrap();
    let mut oracle = BTreeMap::new();
    let mut retained = Vec::new();
    let mut random = 0x6a09_e667_f3bc_c909_u64;
    let mut serial = 1_u64;
    for step in 0..2_000 {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        if step % 137 == 0 {
            retained.push((root, oracle.clone()));
        }
        match random % 3 {
            0 => {
                let name = CanonicalName::new(&format!("r-{:04}", random % 5_000)).unwrap();
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    oracle.entry(name.clone())
                {
                    let inode = InodeId::allocate([0x6a; 32], serial);
                    serial += 1;
                    let (next, counters) =
                        directory_insert(&mut store, root, name.clone(), inode).unwrap();
                    assert!(counters.nodes_created <= 8);
                    entry.insert(inode);
                    root = next;
                }
            }
            1 if !oracle.is_empty() => {
                let index = random as usize % oracle.len();
                let name = oracle.keys().nth(index).unwrap().clone();
                let (next, inode, counters) = directory_remove(&mut store, root, &name).unwrap();
                assert!(counters.nodes_created <= 8);
                assert_eq!(oracle.remove(&name), Some(inode));
                root = next;
            }
            _ if !oracle.is_empty() => {
                let index = random as usize % oracle.len();
                let from = oracle.keys().nth(index).unwrap().clone();
                let to = CanonicalName::new(&format!("q-{:04}", random.rotate_left(17) % 5_000))
                    .unwrap();
                if !oracle.contains_key(&to) {
                    let inode = oracle.remove(&from).unwrap();
                    root = directory_rename(&mut store, root, &from, to.clone())
                        .unwrap()
                        .0;
                    oracle.insert(to, inode);
                }
            }
            _ => {}
        }
        if step % 31 == 0 {
            assert_eq!(
                entries(&store, root),
                oracle.clone().into_iter().collect::<Vec<_>>()
            );
        }
    }
    assert_eq!(
        entries(&store, root),
        oracle.into_iter().collect::<Vec<_>>()
    );
    for (root, oracle) in retained {
        assert_eq!(
            entries(&store, root),
            oracle.into_iter().collect::<Vec<_>>()
        );
    }
}

#[test]
fn remove_and_rename_persist_only_reachable_directory_and_inode_nodes() {
    let mut store = MemoryStore::default();
    let store_id = [0x37; 32];
    let mut directory = empty_directory(&mut store).unwrap();
    let root_inode = InodeId::allocate(store_id, 0);
    let mut inodes = generated_inode_table_from_root(
        &mut store,
        root_inode,
        ObjectId::for_bytes(b"root-record"),
    )
    .unwrap();
    for serial in 1..=1_000_u64 {
        let inode = InodeId::allocate(store_id, serial);
        directory = directory_insert(
            &mut store,
            directory,
            CanonicalName::new(&format!("entry-{serial:04}")).unwrap(),
            inode,
        )
        .unwrap()
        .0;
        inodes = generated_inode_table_upsert(
            &mut store,
            inodes,
            inode,
            ObjectId::for_bytes(&serial.to_be_bytes()),
        )
        .unwrap()
        .0;
    }

    let before = store.0.keys().copied().collect::<BTreeSet<_>>();
    let (removed_directory, _, _) = directory_remove(
        &mut store,
        directory,
        &CanonicalName::new("entry-0064").unwrap(),
    )
    .unwrap();
    assert!(new_ids(&store, &before).is_subset(&directory_reachable(&store, removed_directory)));

    let before = store.0.keys().copied().collect::<BTreeSet<_>>();
    let (renamed, _) = directory_rename(
        &mut store,
        removed_directory,
        &CanonicalName::new("entry-0128").unwrap(),
        CanonicalName::new("renamed").unwrap(),
    )
    .unwrap();
    assert!(new_ids(&store, &before).is_subset(&directory_reachable(&store, renamed)));

    let inodes = inodes.into_root();
    let before = store.0.keys().copied().collect::<BTreeSet<_>>();
    let (next, _, _) =
        inode_table_remove(&mut store, inodes, InodeId::allocate(store_id, 64)).unwrap();
    assert!(new_ids(&store, &before).is_subset(&inode_reachable(&store, next)));
}

#[test]
fn ten_thousand_names_path_copy_and_retained_roots_match_ordered_oracle() {
    let mut store = MemoryStore::default();
    let empty = empty_directory(&mut store).unwrap();
    let mut root = empty;
    let mut retained = Vec::new();
    let store_id = [0x5e; 32];
    for serial in 1..=10_000_u64 {
        if serial % 997 == 0 {
            retained.push((serial - 1, root));
        }
        let name = CanonicalName::new(&format!("entry-{serial:05}")).unwrap();
        let (next, counters) =
            directory_insert(&mut store, root, name, InodeId::allocate(store_id, serial)).unwrap();
        assert!(
            counters.nodes_created <= 6,
            "nonlocal path copy: {counters:?}"
        );
        root = next;
    }

    for serial in [1, 2, 127, 128, 4_999, 10_000] {
        let mut counters = NamespaceCounters::default();
        let name = CanonicalName::new(&format!("entry-{serial:05}")).unwrap();
        assert_eq!(
            directory_lookup(&store, root, &name, &mut counters).unwrap(),
            Some(InodeId::allocate(store_id, serial))
        );
        assert!(
            counters.nodes_read <= 5,
            "lookup exceeded bounded authenticated branch fanout: {counters:?}"
        );
    }

    let mut streamed = 0;
    let mut visitor_reads = NamespaceCounters::default();
    visit_directory_entries(&store, root, &mut visitor_reads, |leaf| {
        assert!(leaf.len() <= 256);
        streamed += leaf.len();
        Ok(())
    })
    .unwrap();
    assert_eq!(streamed, 10_000);
    assert!(
        visitor_reads.nodes_read <= 200,
        "full directory visitor reloaded subtrees: {visitor_reads:?}"
    );

    for (max_serial, retained_root) in retained {
        let mut counters = NamespaceCounters::default();
        let existing = CanonicalName::new(&format!("entry-{max_serial:05}")).unwrap();
        let future = CanonicalName::new(&format!("entry-{:05}", max_serial + 1)).unwrap();
        assert!(
            directory_lookup(&store, retained_root, &existing, &mut counters)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            directory_lookup(&store, retained_root, &future, &mut counters).unwrap(),
            None
        );
    }
    let mut counters = NamespaceCounters::default();
    assert_eq!(
        directory_lookup(
            &store,
            empty,
            &CanonicalName::new("entry-00001").unwrap(),
            &mut counters
        )
        .unwrap(),
        None
    );
}

#[test]
fn directory_state_and_branch_summaries_are_authoritative() {
    let mut store = MemoryStore::default();
    let empty = empty_directory(&mut store).unwrap();
    let bad_state = DirectoryStateV1 {
        entry_count: 1,
        tree_level: 0,
        profile_id: profile_id(),
        mapping_root: ObjectId::from_bytes(
            &layerfs_core::decode_bytes_object(&store.get(empty.0).unwrap()).unwrap()[53..85],
        )
        .unwrap(),
    };
    let bad_root = DirectoryStateRoot(
        store
            .put(&encode_directory_state(bad_state).unwrap())
            .unwrap(),
    );
    assert_eq!(
        directory_lookup(
            &store,
            bad_root,
            &CanonicalName::new("missing").unwrap(),
            &mut NamespaceCounters::default()
        ),
        Err(CoreError::InvalidRecord("directory state summary"))
    );

    let store_id = [0x44; 32];
    let mut leaves = Vec::new();
    for prefix in ['a', 'b'] {
        let entries = (0..12)
            .map(|index| {
                let name = format!("{prefix}{index:02}{}", "x".repeat(252));
                (
                    CanonicalName::new(&name).unwrap(),
                    InodeId::allocate(store_id, index),
                )
            })
            .collect::<Vec<_>>();
        let bytes = entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len() as u64)
            .sum();
        let max = entries.last().unwrap().0.clone();
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: bytes,
                    entries,
                })
                .unwrap(),
            )
            .unwrap();
        leaves.push((max, id, bytes));
    }
    let branch = DirectoryNodeV1::Branch {
        level: 1,
        subtree_entry_count: 25,
        subtree_encoded_bytes: leaves[0].2 + leaves[1].2,
        children: leaves
            .iter()
            .map(|(max, id, _)| (max.clone(), *id))
            .collect(),
    };
    let mapping_root = store.put(&encode_directory_node(&branch).unwrap()).unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 25,
                    tree_level: 1,
                    profile_id: profile_id(),
                    mapping_root,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(
        visit_directory_entries(&store, root, &mut NamespaceCounters::default(), |_| Ok(())),
        Err(CoreError::InvalidRecord("directory branch summary"))
    );
}

#[test]
fn inode_branch_summary_and_occupancy_are_authoritative() {
    use layerfs_core::namespace_codec::{encode_inode_table_node, InodeTableNodeV1};

    let mut store = MemoryStore::default();
    let mut entries = (0..128_u64)
        .map(|serial| {
            (
                InodeId::allocate([0x91; 32], serial),
                ObjectId::for_bytes(&serial.to_be_bytes()),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.0);
    let mut children = Vec::new();
    for group in entries.chunks_exact(64) {
        let id = store
            .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(group.to_vec())).unwrap())
            .unwrap();
        children.push((group.last().unwrap().0, id));
    }
    let branch = InodeTableNodeV1::Branch {
        level: 1,
        subtree_entry_count: 129,
        children,
    };
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(&encode_inode_table_node(&branch).unwrap())
            .unwrap(),
    );
    assert_eq!(
        visit_inode_table_entries(&store, root, &mut InodeTableCounters::default(), |_| Ok(())),
        Err(CoreError::InvalidRecord("inode branch summary"))
    );

    let one_child = InodeTableNodeV1::Branch {
        level: 1,
        subtree_entry_count: 64,
        children: vec![(entries[63].0, root.0)],
    };
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(&encode_inode_table_node(&one_child).unwrap())
            .unwrap(),
    );
    assert_eq!(
        inode_table_lookup(
            &store,
            root,
            entries[0].0,
            &mut InodeTableCounters::default()
        ),
        Err(CoreError::NonCanonicalPagePartition)
    );
}

#[test]
fn directory_and_inode_child_ranges_must_not_overlap() {
    use layerfs_core::namespace_codec::{encode_inode_table_node, InodeTableNodeV1};

    let mut store = MemoryStore::default();
    let store_id = [0x62; 32];
    let groups = [
        (0..11)
            .map(|index| format!("a{index:02}{}", "x".repeat(252)))
            .chain(std::iter::once(format!("m00{}", "x".repeat(252))))
            .collect::<Vec<_>>(),
        std::iter::once(format!("l00{}", "x".repeat(252)))
            .chain((0..11).map(|index| format!("z{index:02}{}", "x".repeat(252))))
            .collect::<Vec<_>>(),
    ];
    let mut children = Vec::new();
    let mut total_bytes = 0_u64;
    for (group_index, names) in groups.into_iter().enumerate() {
        let entries = names
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                (
                    CanonicalName::new(&name).unwrap(),
                    InodeId::allocate(store_id, (group_index * 12 + index) as u64),
                )
            })
            .collect::<Vec<_>>();
        let bytes = entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len() as u64)
            .sum::<u64>();
        total_bytes += bytes;
        let maximum = entries.last().unwrap().0.clone();
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: bytes,
                    entries,
                })
                .unwrap(),
            )
            .unwrap();
        children.push((maximum, id));
    }
    let mapping_root = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 1,
                subtree_entry_count: 24,
                subtree_encoded_bytes: total_bytes,
                children,
            })
            .unwrap(),
        )
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 24,
                    tree_level: 1,
                    profile_id: profile_id(),
                    mapping_root,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(
        visit_directory_entries(&store, root, &mut NamespaceCounters::default(), |_| Ok(())),
        Err(CoreError::NonCanonicalOrdering)
    );

    let mut ids = (0..127_u64)
        .map(|serial| InodeId::allocate([0x63; 32], serial))
        .collect::<Vec<_>>();
    ids.sort();
    let left = ids[..64]
        .iter()
        .map(|id| (*id, ObjectId::for_bytes(id.as_bytes())))
        .collect::<Vec<_>>();
    let right = ids[63..127]
        .iter()
        .map(|id| (*id, ObjectId::for_bytes(id.as_bytes())))
        .collect::<Vec<_>>();
    let left_id = store
        .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(left)).unwrap())
        .unwrap();
    let right_id = store
        .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(right)).unwrap())
        .unwrap();
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 1,
                    subtree_entry_count: 128,
                    children: vec![(ids[63], left_id), (ids[126], right_id)],
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(
        visit_inode_table_entries(&store, root, &mut InodeTableCounters::default(), |_| Ok(())),
        Err(CoreError::NonCanonicalOrdering)
    );
}

#[test]
fn level_two_ranges_use_true_leftmost_leaf_minimum() {
    use layerfs_core::namespace_codec::{encode_inode_table_node, InodeTableNodeV1};

    let mut store = MemoryStore::default();
    let directory_leaf = |store: &mut MemoryStore, prefixes: Vec<String>, serial_base: u64| {
        let entries = prefixes
            .into_iter()
            .enumerate()
            .map(|(index, prefix)| {
                (
                    CanonicalName::new(&format!("{prefix}{}", "x".repeat(252))).unwrap(),
                    InodeId::allocate([0x72; 32], serial_base + index as u64),
                )
            })
            .collect::<Vec<_>>();
        let bytes = entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len() as u64)
            .sum::<u64>();
        let maximum = entries.last().unwrap().0.clone();
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: bytes,
                    entries,
                })
                .unwrap(),
            )
            .unwrap();
        (maximum, id, bytes)
    };
    let mut left_leaves = Vec::new();
    for (leaf, prefix) in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "m"]
        .into_iter()
        .enumerate()
    {
        left_leaves.push(directory_leaf(
            &mut store,
            (0..12).map(|index| format!("{prefix}{index:02}")).collect(),
            (leaf * 12) as u64,
        ));
    }
    let mut right_leaves = vec![directory_leaf(
        &mut store,
        std::iter::once("l00".to_owned())
            .chain((0..11).map(|index| format!("n{index:02}")))
            .collect(),
        144,
    )];
    for (leaf, prefix) in ["o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "z"]
        .into_iter()
        .enumerate()
    {
        right_leaves.push(directory_leaf(
            &mut store,
            (0..12).map(|index| format!("{prefix}{index:02}")).collect(),
            (156 + leaf * 12) as u64,
        ));
    }
    let directory_branch =
        |store: &mut MemoryStore, level: u8, children: &[(CanonicalName, ObjectId, u64)]| {
            let bytes = children.iter().map(|child| child.2).sum::<u64>();
            let id = store
                .put(
                    &encode_directory_node(&DirectoryNodeV1::Branch {
                        level,
                        subtree_entry_count: children.len() as u64 * 12,
                        subtree_encoded_bytes: bytes,
                        children: children
                            .iter()
                            .map(|child| (child.0.clone(), child.1))
                            .collect(),
                    })
                    .unwrap(),
                )
                .unwrap();
            (children.last().unwrap().0.clone(), id, bytes)
        };
    let left = directory_branch(&mut store, 1, &left_leaves);
    let right = directory_branch(&mut store, 1, &right_leaves);
    let root_branch = DirectoryNodeV1::Branch {
        level: 2,
        subtree_entry_count: 288,
        subtree_encoded_bytes: left.2 + right.2,
        children: vec![(left.0, left.1), (right.0, right.1)],
    };
    let mapping_root = store
        .put(&encode_directory_node(&root_branch).unwrap())
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 288,
                    tree_level: 2,
                    profile_id: profile_id(),
                    mapping_root,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(
        visit_directory_entries(&store, root, &mut NamespaceCounters::default(), |_| Ok(())),
        Err(CoreError::NonCanonicalOrdering)
    );

    let inode = |value: u16| {
        let mut bytes = [0_u8; 32];
        bytes[..2].copy_from_slice(&value.to_be_bytes());
        InodeId(bytes)
    };
    let inode_leaf = |store: &mut MemoryStore, start: u16| {
        let entries = (start..start + 64)
            .map(|value| (inode(value), ObjectId::for_bytes(&value.to_be_bytes())))
            .collect::<Vec<_>>();
        let maximum = entries.last().unwrap().0;
        let id = store
            .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(entries)).unwrap())
            .unwrap();
        (maximum, id)
    };
    let mut left_leaves = Vec::new();
    for leaf in 0..64_u16 {
        left_leaves.push(inode_leaf(&mut store, leaf * 64));
    }
    let mut right_leaves = vec![inode_leaf(&mut store, 4050)];
    for leaf in 1..64_u16 {
        right_leaves.push(inode_leaf(&mut store, 4050 + leaf * 64));
    }
    let inode_branch = |store: &mut MemoryStore, children: &[(InodeId, ObjectId)]| {
        let maximum = children.last().unwrap().0;
        let id = store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 1,
                    subtree_entry_count: children.len() as u64 * 64,
                    children: children.to_vec(),
                })
                .unwrap(),
            )
            .unwrap();
        (maximum, id)
    };
    let left = inode_branch(&mut store, &left_leaves);
    let right = inode_branch(&mut store, &right_leaves);
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 2,
                    subtree_entry_count: 8192,
                    children: vec![left, right],
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(
        visit_inode_table_entries(&store, root, &mut InodeTableCounters::default(), |_| Ok(())),
        Err(CoreError::NonCanonicalOrdering)
    );
}

#[test]
fn repeated_child_levels_are_rejected_before_loading_grandchildren() {
    use layerfs_core::namespace_codec::{encode_inode_table_node, InodeTableNodeV1};

    let mut store = MemoryStore::default();
    let inode = |value: u16| {
        let mut bytes = [0_u8; 32];
        bytes[..2].copy_from_slice(&value.to_be_bytes());
        InodeId(bytes)
    };
    let child_entries = (0..64_u16)
        .map(|value| (inode(value), ObjectId::for_bytes(&value.to_be_bytes())))
        .collect::<Vec<_>>();
    let child_max = child_entries.last().unwrap().0;
    let child = store
        .put(
            &encode_inode_table_node(&InodeTableNodeV1::Branch {
                level: 2,
                subtree_entry_count: 64,
                children: child_entries,
            })
            .unwrap(),
        )
        .unwrap();
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 2,
                    subtree_entry_count: 128,
                    children: vec![
                        (child_max, child),
                        (inode(u16::MAX), ObjectId::for_bytes(b"unused")),
                    ],
                })
                .unwrap(),
            )
            .unwrap(),
    );
    let mut counters = InodeTableCounters::default();
    assert_eq!(
        visit_inode_table_entries(&store, root, &mut counters, |_| Ok(())),
        Err(CoreError::InvalidRecord("inode child summary"))
    );
    assert_eq!(counters.nodes_read, 2);

    let names = (0..12)
        .map(|index| CanonicalName::new(&format!("a{index:02}{}", "x".repeat(252))).unwrap())
        .collect::<Vec<_>>();
    let child_max = names.last().unwrap().clone();
    let child = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 2,
                subtree_entry_count: 12,
                subtree_encoded_bytes: 12,
                children: names
                    .iter()
                    .map(|name| (name.clone(), ObjectId::for_bytes(name.as_bytes())))
                    .collect(),
            })
            .unwrap(),
        )
        .unwrap();
    let mapping = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 2,
                subtree_entry_count: 24,
                subtree_encoded_bytes: 24,
                children: vec![
                    (child_max, child),
                    (
                        CanonicalName::new(&format!("z99{}", "x".repeat(252))).unwrap(),
                        ObjectId::for_bytes(b"unused directory child"),
                    ),
                ],
            })
            .unwrap(),
        )
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 24,
                    tree_level: 2,
                    profile_id: profile_id(),
                    mapping_root: mapping,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    let mut counters = NamespaceCounters::default();
    assert_eq!(
        visit_directory_entries(&store, root, &mut counters, |_| Ok(())),
        Err(CoreError::InvalidRecord("directory child summary"))
    );
    assert_eq!(counters.nodes_read, 3);
}

#[test]
fn inode_remove_merges_collapses_and_preserves_retained_root() {
    let mut store = MemoryStore::default();
    let store_id = [0xa5; 32];
    let root_inode = InodeId::allocate(store_id, 0);
    let root_record = ObjectId::for_bytes(b"root record");
    let mut generated =
        generated_inode_table_from_root(&mut store, root_inode, root_record).unwrap();
    for serial in 1..=1_000_u64 {
        generated = generated_inode_table_upsert(
            &mut store,
            generated,
            InodeId::allocate(store_id, serial),
            ObjectId::for_bytes(&serial.to_be_bytes()),
        )
        .unwrap()
        .0;
    }
    let retained = generated.into_root();
    let mut current = retained;
    for serial in 1..=1_000_u64 {
        let (next, removed, counters) =
            inode_table_remove(&mut store, current, InodeId::allocate(store_id, serial)).unwrap();
        assert_eq!(removed, ObjectId::for_bytes(&serial.to_be_bytes()));
        assert!(
            counters.nodes_created <= 8,
            "delete copied more than a spine: {counters:?}"
        );
        current = next;
    }
    assert_eq!(
        inode_table_lookup(
            &store,
            current,
            root_inode,
            &mut InodeTableCounters::default()
        )
        .unwrap(),
        Some(root_record)
    );
    assert_eq!(
        inode_table_lookup(
            &store,
            retained,
            InodeId::allocate(store_id, 500),
            &mut InodeTableCounters::default()
        )
        .unwrap(),
        Some(ObjectId::for_bytes(&500_u64.to_be_bytes()))
    );
}
