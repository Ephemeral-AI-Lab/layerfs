#[test]
fn persistent_metadata_tree_round_trips_multiple_levels() {
    let entries = (0..300_u16)
        .map(|index| MetadataEntryV1 {
            key: MetadataKey::new(
                "apple.xattr".into(),
                format!("user.attribute-{index:04}").into_bytes(),
            )
            .unwrap(),
            value_file_root: ObjectId::for_bytes(&index.to_be_bytes()),
        })
        .collect::<Vec<_>>();
    let mut store = MemoryStore::default();
    let root = build_metadata_tree(&mut store, &entries).unwrap();
    let mut streaming = MetadataTreeBuilder::new();
    for entry in entries.iter().cloned() {
        streaming.push(&mut store, entry).unwrap();
    }
    assert_eq!(streaming.finish(&mut store).unwrap(), root);
    assert_eq!(metadata_tree_entries(&store, root).unwrap(), entries);
}

#[test]
fn metadata_empty_root_is_canonical_and_one_child_root_is_not() {
    let mut store = MemoryStore::default();
    let empty = build_metadata_tree(&mut store, &[]).unwrap();
    assert!(metadata_tree_entries(&store, empty).unwrap().is_empty());

    let entry = MetadataEntryV1 {
        key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
        value_file_root: ObjectId::for_bytes(b"mode"),
    };
    let leaf = MetadataNodeV1::Leaf {
        subtree_encoded_bytes: 49,
        entries: vec![entry.clone()],
    };
    let child = store.put(&encode_metadata_node(&leaf).unwrap()).unwrap();
    let branch = MetadataNodeV1::Branch {
        level: 1,
        subtree_entry_count: 1,
        subtree_encoded_bytes: 49,
        children: vec![(entry.key, child)],
    };
    let root = store.put(&encode_metadata_node(&branch).unwrap()).unwrap();
    assert_eq!(
        metadata_tree_entries(&store, root),
        Err(CoreError::NonCanonicalPagePartition)
    );
    assert_eq!(
        metadata_lookup(
            &store,
            root,
            &MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap()
        ),
        Err(CoreError::NonCanonicalPagePartition)
    );
}

#[test]
fn metadata_child_ranges_must_not_overlap() {
    let mut store = MemoryStore::default();
    let entries = (0..127_u16)
        .map(|index| MetadataEntryV1 {
            key: MetadataKey::new(
                "apple.xattr".into(),
                format!("user.attribute-{index:04}").into_bytes(),
            )
            .unwrap(),
            value_file_root: ObjectId::for_bytes(&index.to_be_bytes()),
        })
        .collect::<Vec<_>>();
    let leaf = |entries: &[MetadataEntryV1]| MetadataNodeV1::Leaf {
        subtree_encoded_bytes: entries
            .iter()
            .map(|entry| 37 + entry.key.domain.len() as u64 + entry.key.key.len() as u64)
            .sum(),
        entries: entries.to_vec(),
    };
    let left = leaf(&entries[..64]);
    let right = leaf(&entries[63..127]);
    let left_bytes = match &left {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            ..
        } => *subtree_encoded_bytes,
        _ => unreachable!(),
    };
    let right_bytes = match &right {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            ..
        } => *subtree_encoded_bytes,
        _ => unreachable!(),
    };
    let left_id = store.put(&encode_metadata_node(&left).unwrap()).unwrap();
    let right_id = store.put(&encode_metadata_node(&right).unwrap()).unwrap();
    let root = store
        .put(
            &encode_metadata_node(&MetadataNodeV1::Branch {
                level: 1,
                subtree_entry_count: 128,
                subtree_encoded_bytes: left_bytes + right_bytes,
                children: vec![
                    (entries[63].key.clone(), left_id),
                    (entries[126].key.clone(), right_id),
                ],
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        metadata_tree_entries(&store, root),
        Err(CoreError::NonCanonicalOrdering)
    );
    assert_eq!(
        metadata_lookup(&store, root, &entries[100].key),
        Err(CoreError::NonCanonicalOrdering)
    );
}
