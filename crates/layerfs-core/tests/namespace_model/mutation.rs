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
