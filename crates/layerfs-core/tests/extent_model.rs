use layerfs_core::content::extent::{ChildDescriptorV3, ExtentNodeV3, ExtentSliceV3, FileStateV3};
use layerfs_core::content::extent_codec::{
    decode_file_state, decode_node_with_context, encode_file_state, encode_node, profile_id,
};
use layerfs_core::content::rope::FileStateRoot;
use layerfs_core::content::rope::{
    build, diff_ranges, read_all, read_range, replace, validate_file, visit_extents, ObjectRead,
    ObjectStore,
};
use layerfs_core::{decode_bytes_object, encode_bytes_object};
use layerfs_core::{CoreError, CoreResult, ObjectId};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

impl ObjectStore for MemoryStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        if let Some(incumbent) = self.0.get(&id) {
            if incumbent != canonical {
                return Err(CoreError::IdentityMismatch);
            }
        } else {
            self.0.insert(id, canonical.to_vec());
        }
        Ok(id)
    }
}

#[test]
fn extent_visitor_streams_the_mapping_without_fetching_payload_bytes() {
    let bytes = (0..1_000_000)
        .map(|index| (index as u64).wrapping_mul(0x9e37_79b9) as u8)
        .collect::<Vec<_>>();
    let mut store = MemoryStore::default();
    let (root, built) = build(&mut store, bytes.as_slice()).unwrap();
    let mut extents = 0_u64;
    let mut logical = 0_u64;
    let (state, visited) = visit_extents(&store, root, |batch| {
        extents += batch.len() as u64;
        logical += batch
            .iter()
            .map(|extent| u64::from(extent.logical_length))
            .sum::<u64>();
        Ok(())
    })
    .unwrap();
    assert_eq!(extents, state.extent_count);
    assert_eq!(logical, state.logical_len);
    assert_eq!(visited.payload_bytes_read, 0);
    assert_eq!(visited.cdc_bytes_scanned, 0);
    assert!(visited.nodes_read <= built.nodes_created + 1);
}

struct BatchRead<'a> {
    store: &'a MemoryStore,
    batches: Cell<u64>,
    state_root: ObjectId,
    state_reads: Cell<u64>,
}

impl ObjectRead for BatchRead<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        if id == self.state_root {
            self.state_reads.set(self.state_reads.get() + 1);
        }
        ObjectStore::get(self.store, id)
    }

    fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        assert!(!ids.is_empty() && ids.len() <= 64);
        self.batches.set(self.batches.get() + 1);
        for id in ids {
            let bytes = self.store.0.get(id).ok_or(CoreError::MissingObject)?;
            if ObjectId::for_bytes(bytes) != *id {
                return Err(CoreError::IdentityMismatch);
            }
            callback(*id, decode_bytes_object(bytes)?)?;
        }
        Ok(())
    }
}

struct CountRead<'a> {
    store: &'a MemoryStore,
    reads: Cell<u64>,
}

impl ObjectRead for CountRead<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.reads.set(self.reads.get() + 1);
        ObjectStore::get(self.store, id)
    }
}

struct MissingBatchCallback<'a>(&'a MemoryStore);

impl ObjectRead for MissingBatchCallback<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        ObjectStore::get(self.0, id)
    }

    fn get_authenticated_batch<F>(&self, _ids: &[ObjectId], _callback: F) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        Ok(())
    }
}

struct ExtraBatchCallback<'a>(&'a MemoryStore);

impl ObjectRead for ExtraBatchCallback<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        ObjectStore::get(self.0, id)
    }

    fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        for id in ids {
            let canonical = self.0 .0.get(id).ok_or(CoreError::MissingObject)?;
            callback(*id, decode_bytes_object(canonical)?)?;
        }
        let id = *ids.last().ok_or(CoreError::MissingObject)?;
        callback(id, decode_bytes_object(self.0 .0.get(&id).unwrap())?)
    }
}

#[test]
fn paired_diff_skips_equal_roots_and_length_mismatch_mappings() {
    let mut store = MemoryStore::default();
    let (old, _) = build(&mut store, [1_u8; 4096].as_slice()).unwrap();
    let (shorter, _) = build(&mut store, [2_u8; 2048].as_slice()).unwrap();
    let reader = CountRead {
        store: &store,
        reads: Cell::new(0),
    };
    let mut ranges = Vec::new();
    assert_eq!(
        diff_ranges(&reader, old, old, |range| {
            ranges.push(range);
            Ok(())
        })
        .unwrap(),
        (true, Default::default())
    );
    assert_eq!(reader.reads.get(), 0);
    let (same_length, counters) = diff_ranges(&reader, old, shorter, |_| Ok(())).unwrap();
    assert!(!same_length);
    assert_eq!(reader.reads.get(), 2);
    assert_eq!(counters.nodes_read, 2);
    assert_eq!(counters.payload_bytes_read, 0);
    assert!(ranges.is_empty());
}

#[test]
fn paired_diff_handles_unequal_height_and_partition_without_payload_reads() {
    let mut store = MemoryStore::default();
    let payload = store.put(&encode_bytes_object(&[7]).unwrap()).unwrap();
    let extents = (0..128)
        .map(|_| ExtentSliceV3::new(payload, 0, 1).unwrap())
        .collect::<Vec<_>>();
    let leaf_root = store
        .put(
            &encode_node(&ExtentNodeV3::Leaf {
                subtree_logical_bytes: 128,
                extents: extents.clone(),
            })
            .unwrap(),
        )
        .unwrap();
    let left = store
        .put(
            &encode_node(&ExtentNodeV3::Leaf {
                subtree_logical_bytes: 64,
                extents: extents[..64].to_vec(),
            })
            .unwrap(),
        )
        .unwrap();
    let right = store
        .put(
            &encode_node(&ExtentNodeV3::Leaf {
                subtree_logical_bytes: 64,
                extents: extents[64..].to_vec(),
            })
            .unwrap(),
        )
        .unwrap();
    let branch_root = store
        .put(
            &encode_node(&ExtentNodeV3::Branch {
                level: 1,
                subtree_logical_bytes: 128,
                subtree_extent_count: 128,
                children: vec![
                    ChildDescriptorV3 {
                        cumulative_logical_end: 64,
                        cumulative_extent_end: 64,
                        child_object_id: left,
                    },
                    ChildDescriptorV3 {
                        cumulative_logical_end: 128,
                        cumulative_extent_end: 128,
                        child_object_id: right,
                    },
                ],
            })
            .unwrap(),
        )
        .unwrap();
    let state = |mapping_root, tree_level| FileStateV3 {
        logical_len: 128,
        extent_count: 128,
        tree_level,
        profile_id: profile_id(),
        mapping_root,
    };
    let old = FileStateRoot(
        store
            .put(&encode_file_state(state(leaf_root, 0)).unwrap())
            .unwrap(),
    );
    let new = FileStateRoot(
        store
            .put(&encode_file_state(state(branch_root, 1)).unwrap())
            .unwrap(),
    );
    let mut ranges = Vec::new();
    let (same_length, counters) = diff_ranges(&store, old, new, |range| {
        ranges.push(range);
        Ok(())
    })
    .unwrap();
    assert!(same_length);
    assert!(ranges.is_empty());
    assert_eq!(counters.payload_bytes_read, 0);
    assert!(counters.nodes_read <= 6);
}

#[test]
fn streamed_build_and_ranges_match_vec_oracle() {
    let mut word = 0x5eed_c0de_u64;
    let bytes: Vec<u8> = (0..5_000_000)
        .map(|_| {
            word ^= word << 13;
            word ^= word >> 7;
            word ^= word << 17;
            word as u8
        })
        .collect();
    let mut store = MemoryStore::default();
    let (root, build_counters) = build(&mut store, bytes.as_slice()).unwrap();
    assert_eq!(build_counters.cdc_bytes_scanned, bytes.len() as u64);
    assert!(build_counters.nodes_created >= 2);

    for range in [
        0..1,
        8_191..33_001,
        500_000..700_000,
        bytes.len() as u64 - 17..bytes.len() as u64,
    ] {
        let mut actual = Vec::new();
        let counters = read_range(&store, root, range.clone(), &mut actual).unwrap();
        assert_eq!(actual, bytes[range.start as usize..range.end as usize]);
        assert!(counters.nodes_read < build_counters.chunks_created + 2);
    }
}

#[test]
fn product_range_reader_uses_bounded_authenticated_payload_batches() {
    let bytes = vec![0x5a; 1_000_000];
    let mut store = MemoryStore::default();
    let (root, _) = build(&mut store, bytes.as_slice()).unwrap();
    let reader = BatchRead {
        store: &store,
        batches: Cell::new(0),
        state_root: root.0,
        state_reads: Cell::new(0),
    };
    let mut actual = Vec::new();
    read_range(&reader, root, 0..bytes.len() as u64, &mut actual).unwrap();
    assert_eq!(actual, bytes);
    assert!(reader.batches.get() > 0);
    assert_eq!(reader.state_reads.get(), 1);
}

#[test]
fn missing_payload_batch_callback_is_rejected() {
    let bytes = b"callback cardinality";
    let mut store = MemoryStore::default();
    let (root, _) = build(&mut store, bytes.as_slice()).unwrap();

    assert_eq!(
        read_range(
            &MissingBatchCallback(&store),
            root,
            0..bytes.len() as u64,
            Vec::new(),
        ),
        Err(CoreError::InvalidRecord("payload batch cardinality"))
    );
}

#[test]
fn extra_payload_batch_callback_is_rejected_without_panicking() {
    let bytes = b"callback cardinality";
    let mut store = MemoryStore::default();
    let (root, _) = build(&mut store, bytes.as_slice()).unwrap();
    let extra = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_range(
            &ExtraBatchCallback(&store),
            root,
            0..bytes.len() as u64,
            Vec::new(),
        )
    }));
    assert!(matches!(
        extra,
        Ok(Err(CoreError::InvalidRecord("payload batch cardinality")))
    ));
}

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

#[test]
fn fetched_identity_is_unconditional() {
    let mut store = MemoryStore::default();
    let (root, _) = build(&mut store, b"authenticated".as_slice()).unwrap();
    store.0.get_mut(&root.0).unwrap()[13] ^= 1;
    assert_eq!(
        read_range(&store, root, 0..1, Vec::new()),
        Err(CoreError::IdentityMismatch)
    );
}

#[test]
fn splices_match_vec_and_preserve_retained_root_without_suffix_payload_reads() {
    let mut word = 0x1234_5678_9abc_def0_u64;
    let original: Vec<u8> = (0..3_000_000)
        .map(|_| {
            word ^= word << 13;
            word ^= word >> 7;
            word ^= word << 17;
            word as u8
        })
        .collect();
    let mut expected = original.clone();
    let mut store = MemoryStore::default();
    let (old_root, _) = build(&mut store, original.as_slice()).unwrap();
    let mut root = old_root;

    for (start, delete, replacement) in [
        (7_u64, 4_u64, vec![9; 4]),
        (1_000_000, 0, vec![1, 2, 3, 4, 5]),
        (2_000_000, 13, Vec::new()),
        (2_999_800, 100, vec![8; 3]),
    ] {
        let end = start as usize + delete as usize;
        expected.splice(start as usize..end, replacement.iter().copied());
        let (next, counters) =
            replace(&mut store, root, start, delete, replacement.as_slice()).unwrap();
        assert_eq!(
            counters.payload_bytes_read, 0,
            "structural splice read unchanged payload"
        );
        assert!(
            counters.nodes_read < 32,
            "splice walked more than boundary-height work: {counters:?}"
        );
        root = next;
        let mut actual = Vec::new();
        read_range(&store, root, 0..expected.len() as u64, &mut actual).unwrap();
        assert_eq!(actual, expected);
    }

    let mut retained = Vec::new();
    read_range(&store, old_root, 0..original.len() as u64, &mut retained).unwrap();
    assert_eq!(retained, original);
}

#[test]
fn two_thousand_randomized_splices_match_every_revision_and_retained_history() {
    fn run(seed: u64) {
        const MAX_FILE_BYTES: usize = 256 * 1024;
        const REVISIONS: usize = 1_000;
        const CHECKPOINTS: [usize; 6] = [0, 1, 127, 499, 999, 1_000];

        let mut random = seed;
        let mut next = || {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            random
        };
        let mut expected = (0..200_000).map(|_| next() as u8).collect::<Vec<_>>();
        let mut store = MemoryStore::default();
        let (mut root, _) = build(&mut store, expected.as_slice()).unwrap();
        let mut roots = Vec::with_capacity(REVISIONS + 1);
        roots.push(root);
        let mut checkpoints = vec![(0, expected.clone())];

        for revision in 1..=REVISIONS {
            let start = next() as usize % (expected.len() + 1);
            let delete = (next() as usize % 2049).min(expected.len() - start);
            let retained_len = expected.len() - delete;
            let maximum_replacement = (MAX_FILE_BYTES - retained_len).min(2048);
            let replacement_len = next() as usize % (maximum_replacement + 1);
            let replacement = (0..replacement_len)
                .map(|_| next() as u8)
                .collect::<Vec<_>>();
            expected.splice(start..start + delete, replacement.iter().copied());
            root = replace(
                &mut store,
                root,
                start as u64,
                delete as u64,
                replacement.as_slice(),
            )
            .unwrap()
            .0;
            roots.push(root);

            let mut actual = Vec::new();
            read_all(&store, root, &mut actual).unwrap();
            assert_eq!(actual, expected, "seed={seed:#x}, revision={revision}");
            if CHECKPOINTS.contains(&revision) {
                checkpoints.push((revision, expected.clone()));
            }
        }

        assert_eq!(roots.len(), REVISIONS + 1);
        for (revision, expected) in checkpoints {
            let mut actual = Vec::new();
            read_all(&store, roots[revision], &mut actual).unwrap();
            assert_eq!(actual, expected, "seed={seed:#x}, checkpoint={revision}");
        }
    }

    run(0x8f31_27ab_5ce4_d901);
    run(0x196a_0c43_e7b2_85df);
}

#[test]
fn authenticated_child_summary_mismatch_and_underfull_nonroot_are_rejected() {
    let mut store = MemoryStore::default();
    let mut extents = Vec::new();
    for index in 0..128_u16 {
        let payload = encode_bytes_object(&index.to_be_bytes()).unwrap();
        let payload_id = store.put(&payload).unwrap();
        extents.push(ExtentSliceV3::new(payload_id, 0, 1).unwrap());
    }
    let left = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 64,
        extents: extents[..64].to_vec(),
    };
    let right = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 64,
        extents: extents[64..].to_vec(),
    };
    let left_id = store.put(&encode_node(&left).unwrap()).unwrap();
    let right_id = store.put(&encode_node(&right).unwrap()).unwrap();
    let branch = ExtentNodeV3::Branch {
        level: 1,
        subtree_logical_bytes: 129,
        subtree_extent_count: 128,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 65,
                cumulative_extent_end: 64,
                child_object_id: left_id,
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 129,
                cumulative_extent_end: 128,
                child_object_id: right_id,
            },
        ],
    };
    let branch_id = store.put(&encode_node(&branch).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 129,
        extent_count: 128,
        tree_level: 1,
        profile_id: profile_id(),
        mapping_root: branch_id,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    assert_eq!(
        read_range(&store, root, 0..1, Vec::new()),
        Err(CoreError::InvalidRecord("extent summary"))
    );

    let underfull = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 1,
        extents: vec![extents[0]],
    };
    let underfull_id = store.put(&encode_node(&underfull).unwrap()).unwrap();
    let valid_id = store.put(&encode_node(&left).unwrap()).unwrap();
    let branch = ExtentNodeV3::Branch {
        level: 1,
        subtree_logical_bytes: 65,
        subtree_extent_count: 65,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 1,
                cumulative_extent_end: 1,
                child_object_id: underfull_id,
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 65,
                cumulative_extent_end: 65,
                child_object_id: valid_id,
            },
        ],
    };
    let branch_id = store.put(&encode_node(&branch).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 65,
        extent_count: 65,
        tree_level: 1,
        profile_id: profile_id(),
        mapping_root: branch_id,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    assert_eq!(
        read_range(&store, root, 0..1, Vec::new()),
        Err(CoreError::NonCanonicalPagePartition)
    );
}

#[test]
fn fetched_payload_above_frozen_ceiling_is_rejected() {
    let mut store = MemoryStore::default();
    let payload = encode_bytes_object(&vec![7; 32_769]).unwrap();
    let payload_id = store.put(&payload).unwrap();
    let leaf = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 1,
        extents: vec![ExtentSliceV3::new(payload_id, 0, 1).unwrap()],
    };
    let leaf_id = store.put(&encode_node(&leaf).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 1,
        extent_count: 1,
        tree_level: 0,
        profile_id: profile_id(),
        mapping_root: leaf_id,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    assert_eq!(
        read_range(&store, root, 0..1, Vec::new()),
        Err(CoreError::ChunkLengthMismatch)
    );
}

#[test]
fn deep_splice_rejoins_boundary_fragments_before_they_become_nonroots() {
    let mut store = MemoryStore::default();
    let payload_a = store.put(&encode_bytes_object(&[0xaa]).unwrap()).unwrap();
    let payload_b = store.put(&encode_bytes_object(&[0xbb]).unwrap()).unwrap();
    let mut leaves = Vec::new();
    for leaf_index in 0..128 {
        let extents = (0..64)
            .map(|index| {
                ExtentSliceV3::new(
                    if (leaf_index + index) % 2 == 0 {
                        payload_a
                    } else {
                        payload_b
                    },
                    0,
                    1,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let node = ExtentNodeV3::Leaf {
            subtree_logical_bytes: 64,
            extents,
        };
        leaves.push(store.put(&encode_node(&node).unwrap()).unwrap());
    }
    let mut branches = Vec::new();
    for group in leaves.chunks_exact(64) {
        let children = group
            .iter()
            .enumerate()
            .map(|(index, id)| ChildDescriptorV3 {
                cumulative_logical_end: (index as u64 + 1) * 64,
                cumulative_extent_end: (index as u64 + 1) * 64,
                child_object_id: *id,
            })
            .collect();
        let branch = ExtentNodeV3::Branch {
            level: 1,
            subtree_logical_bytes: 4096,
            subtree_extent_count: 4096,
            children,
        };
        branches.push(store.put(&encode_node(&branch).unwrap()).unwrap());
    }
    let root_node = ExtentNodeV3::Branch {
        level: 2,
        subtree_logical_bytes: 8192,
        subtree_extent_count: 8192,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 4096,
                cumulative_extent_end: 4096,
                child_object_id: branches[0],
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 8192,
                cumulative_extent_end: 8192,
                child_object_id: branches[1],
            },
        ],
    };
    let mapping_root = store.put(&encode_node(&root_node).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 8192,
        extent_count: 8192,
        tree_level: 2,
        profile_id: profile_id(),
        mapping_root,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    let (next, counters) = replace(&mut store, root, 4095, 2, [0xcc].as_slice()).unwrap();
    assert_eq!(counters.payload_bytes_read, 0);
    let mut bytes = Vec::new();
    read_range(&store, next, 0..8191, &mut bytes).unwrap();
    assert_eq!(bytes.len(), 8191);
    assert_eq!(bytes[4095], 0xcc);
}

#[test]
fn validation_binds_empty_state_and_visits_unread_children() {
    let mut store = MemoryStore::default();
    let payload = store.put(&encode_bytes_object(&[7]).unwrap()).unwrap();
    let extent = ExtentSliceV3::new(payload, 0, 1).unwrap();
    let nonempty = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 1,
        extents: vec![extent],
    };
    let mapping = store.put(&encode_node(&nonempty).unwrap()).unwrap();
    let empty_state = FileStateV3 {
        logical_len: 0,
        extent_count: 0,
        tree_level: 0,
        profile_id: profile_id(),
        mapping_root: mapping,
    };
    let root = FileStateRoot(store.put(&encode_file_state(empty_state).unwrap()).unwrap());
    assert_eq!(
        validate_file(&store, root),
        Err(CoreError::InvalidRecord("extent summary"))
    );

    let leaf = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 64,
        extents: vec![extent; 64],
    };
    let payload_right = store.put(&encode_bytes_object(&[8]).unwrap()).unwrap();
    let right_leaf = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 64,
        extents: vec![ExtentSliceV3::new(payload_right, 0, 1).unwrap(); 64],
    };
    let left = store.put(&encode_node(&leaf).unwrap()).unwrap();
    let right = store.put(&encode_node(&right_leaf).unwrap()).unwrap();
    let branch = ExtentNodeV3::Branch {
        level: 1,
        subtree_logical_bytes: 128,
        subtree_extent_count: 128,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 64,
                cumulative_extent_end: 64,
                child_object_id: left,
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 128,
                cumulative_extent_end: 128,
                child_object_id: right,
            },
        ],
    };
    let mapping = store.put(&encode_node(&branch).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 128,
        extent_count: 128,
        tree_level: 1,
        profile_id: profile_id(),
        mapping_root: mapping,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    store.0.get_mut(&right).unwrap()[13] ^= 1;
    assert_eq!(
        validate_file(&store, root),
        Err(CoreError::IdentityMismatch)
    );
}

#[test]
fn full_empty_read_authenticates_and_binds_its_mapping_root() {
    let mut store = MemoryStore::default();
    let (valid, _) = build(&mut store, [].as_slice()).unwrap();
    let valid_state = decode_file_state(store.0.get(&valid.0).unwrap()).unwrap();
    assert_eq!(read_all(&store, valid, Vec::new()).unwrap().nodes_read, 2);

    let missing_mapping = valid_state.mapping_root;
    let missing_bytes = store.0.remove(&missing_mapping).unwrap();
    assert_eq!(
        read_all(&store, valid, Vec::new()),
        Err(CoreError::MissingObject)
    );
    store.0.insert(missing_mapping, missing_bytes);

    let wrong_state = FileStateV3 {
        mapping_root: valid.0,
        ..valid_state
    };
    let wrong = FileStateRoot(store.put(&encode_file_state(wrong_state).unwrap()).unwrap());
    assert_eq!(
        read_all(&store, wrong, Vec::new()),
        Err(CoreError::InvalidMappingTag { tag: 0x0a })
    );

    let payload = store.put(&encode_bytes_object(&[7]).unwrap()).unwrap();
    let nonempty_mapping = store
        .put(
            &encode_node(&ExtentNodeV3::Leaf {
                subtree_logical_bytes: 1,
                extents: vec![ExtentSliceV3::new(payload, 0, 1).unwrap()],
            })
            .unwrap(),
        )
        .unwrap();
    let nonempty_state = FileStateV3 {
        mapping_root: nonempty_mapping,
        ..valid_state
    };
    let nonempty = FileStateRoot(
        store
            .put(&encode_file_state(nonempty_state).unwrap())
            .unwrap(),
    );
    assert_eq!(
        read_all(&store, nonempty, Vec::new()),
        Err(CoreError::InvalidRecord("extent summary"))
    );
}
