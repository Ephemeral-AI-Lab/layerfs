#[test]
fn frozen_three_mib_mapping_is_below_one_percent() {
    let bytes = (0..3 * 1024 * 1024)
        .map(|index| (index as u64).wrapping_mul(0x9e37_79b9) as u8)
        .collect::<Vec<_>>();
    let mut store = MemoryStore::default();
    let (root, _) = build(&mut store, bytes.as_slice()).unwrap();
    let state_bytes = store.0.get(&root.0).unwrap();
    let state = decode_file_state(state_bytes).unwrap();

    fn mapping_bytes(store: &MemoryStore, id: ObjectId, root: bool) -> usize {
        let bytes = store.0.get(&id).unwrap();
        match decode_node_with_context(bytes, root).unwrap() {
            ExtentNodeV3::Leaf { .. } => bytes.len(),
            ExtentNodeV3::Branch { children, .. } => {
                bytes.len()
                    + children
                        .into_iter()
                        .map(|child| mapping_bytes(store, child.child_object_id, false))
                        .sum::<usize>()
            }
        }
    }

    let mapping = state_bytes.len() + mapping_bytes(&store, state.mapping_root, true);
    assert!(
        mapping * 100 < bytes.len(),
        "mapping={mapping}, logical={}",
        bytes.len()
    );
}

#[test]
fn replace_persists_no_new_unreachable_objects() {
    let original = (0..2_000_000)
        .map(|index| (index as u64).wrapping_mul(0x9e37_79b9) as u8)
        .collect::<Vec<_>>();
    let mut store = MemoryStore::default();
    let (root, _) = build(&mut store, original.as_slice()).unwrap();
    let before = store.0.keys().copied().collect::<BTreeSet<_>>();
    let (next, _) = replace(
        &mut store,
        root,
        123_457,
        456_789,
        vec![0xa5; 321_123].as_slice(),
    )
    .unwrap();
    let created = store
        .0
        .keys()
        .filter(|id| !before.contains(id))
        .copied()
        .collect::<BTreeSet<_>>();
    let reachable = rope_reachable(&store, next);
    let unreachable = created.difference(&reachable).copied().collect::<Vec<_>>();
    assert!(unreachable.is_empty(), "unreachable: {unreachable:?}");
}

#[test]
fn large_replacement_persists_no_flushed_unreachable_prefix_nodes() {
    let mut word = 0x74a9_32bc_51de_8801_u64;
    let mut bytes = |length: usize| {
        (0..length)
            .map(|_| {
                word ^= word << 13;
                word ^= word >> 7;
                word ^= word << 17;
                word as u8
            })
            .collect::<Vec<_>>()
    };
    let original = bytes(3_000_000);
    let replacement = bytes(8_000_000);
    let mut store = MemoryStore::default();
    let (root, _) = build(&mut store, original.as_slice()).unwrap();
    let before = store.0.keys().copied().collect::<BTreeSet<_>>();
    let (next, counters) = replace(&mut store, root, 1_500_000, 0, replacement.as_slice()).unwrap();
    assert!(
        counters.chunks_created > 192,
        "fixture did not cross streaming flush depth"
    );
    let created = store
        .0
        .keys()
        .filter(|id| !before.contains(id))
        .copied()
        .collect::<BTreeSet<_>>();
    let reachable = rope_reachable(&store, next);
    let unreachable = created.difference(&reachable).copied().collect::<Vec<_>>();
    assert!(unreachable.is_empty(), "unreachable: {unreachable:?}");
}

fn rope_reachable(store: &MemoryStore, root: FileStateRoot) -> BTreeSet<ObjectId> {
    fn visit(store: &MemoryStore, id: ObjectId, root: bool, reachable: &mut BTreeSet<ObjectId>) {
        if !reachable.insert(id) {
            return;
        }
        let node = decode_node_with_context(store.0.get(&id).unwrap(), root).unwrap();
        match node {
            ExtentNodeV3::Leaf { extents, .. } => {
                reachable.extend(extents.into_iter().map(|extent| extent.payload_object_id));
            }
            ExtentNodeV3::Branch { children, .. } => {
                for child in children {
                    visit(store, child.child_object_id, false, reachable);
                }
            }
        }
    }

    let mut reachable = BTreeSet::from([root.0]);
    let state = decode_file_state(store.0.get(&root.0).unwrap()).unwrap();
    visit(store, state.mapping_root, true, &mut reachable);
    reachable
}
