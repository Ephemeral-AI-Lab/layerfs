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
