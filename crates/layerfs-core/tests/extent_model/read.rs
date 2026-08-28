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

type RangeRequests = Vec<Vec<(ObjectId, std::ops::Range<u64>)>>;

struct RangeRead<'a> {
    store: &'a MemoryStore,
    requests: std::cell::RefCell<RangeRequests>,
}

impl ObjectRead for RangeRead<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        ObjectStore::get(self.store, id)
    }

    fn get_authenticated_payload_ranges_batch<F>(
        &self,
        requests: &[(ObjectId, std::ops::Range<u64>)],
        maximum_payload_len: u64,
        mut callback: F,
    ) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        self.requests.borrow_mut().push(requests.to_vec());
        for (id, range) in requests {
            let canonical = self.store.0.get(id).ok_or(CoreError::MissingObject)?;
            let payload = decode_bytes_object(canonical)?;
            if payload.len() as u64 > maximum_payload_len {
                return Err(CoreError::ChunkLengthMismatch);
            }
            callback(*id, &payload[range.start as usize..range.end as usize])?;
        }
        Ok(())
    }
}

#[test]
fn range_reader_requests_only_the_exact_payload_overlap() {
    let mut store = MemoryStore::default();
    let payload = store
        .put(&encode_bytes_object(b"01234567").unwrap())
        .unwrap();
    let mapping = store
        .put(
            &encode_node(&ExtentNodeV3::Leaf {
                subtree_logical_bytes: 4,
                extents: vec![ExtentSliceV3::new(payload, 2, 4).unwrap()],
            })
            .unwrap(),
        )
        .unwrap();
    let root = FileStateRoot(
        store
            .put(
                &encode_file_state(FileStateV3 {
                    logical_len: 4,
                    extent_count: 1,
                    tree_level: 0,
                    profile_id: profile_id(),
                    mapping_root: mapping,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    let reader = RangeRead {
        store: &store,
        requests: std::cell::RefCell::new(Vec::new()),
    };
    let mut actual = Vec::new();

    let counters = read_range(&reader, root, 1..3, &mut actual).unwrap();

    assert_eq!(actual, b"34");
    assert_eq!(*reader.requests.borrow(), vec![vec![(payload, 3..5)]]);
    assert_eq!(counters.payload_bytes_read, 2);
}

#[test]
fn payload_batches_continue_across_mapping_leaf_boundaries() {
    let mut store = MemoryStore::default();
    let payloads = [
        store.put(&encode_bytes_object(&[0x51]).unwrap()).unwrap(),
        store.put(&encode_bytes_object(&[0xa7]).unwrap()).unwrap(),
    ];
    let mut leaves = Vec::new();
    for leaf in 0..3_usize {
        let extents = (0..65_usize)
            .map(|index| ExtentSliceV3::new(payloads[(leaf * 65 + index) % 2], 0, 1).unwrap())
            .collect::<Vec<_>>();
        leaves.push(
            store
                .put(
                    &encode_node(&ExtentNodeV3::Leaf {
                        subtree_logical_bytes: 65,
                        extents,
                    })
                    .unwrap(),
                )
                .unwrap(),
        );
    }
    let mapping = store
        .put(
            &encode_node(&ExtentNodeV3::Branch {
                level: 1,
                subtree_logical_bytes: 195,
                subtree_extent_count: 195,
                children: leaves
                    .into_iter()
                    .enumerate()
                    .map(|(index, child_object_id)| ChildDescriptorV3 {
                        cumulative_logical_end: (index as u64 + 1) * 65,
                        cumulative_extent_end: (index as u64 + 1) * 65,
                        child_object_id,
                    })
                    .collect(),
            })
            .unwrap(),
        )
        .unwrap();
    let root = FileStateRoot(
        store
            .put(
                &encode_file_state(FileStateV3 {
                    logical_len: 195,
                    extent_count: 195,
                    tree_level: 1,
                    profile_id: profile_id(),
                    mapping_root: mapping,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    let reader = BatchRead {
        store: &store,
        batches: Cell::new(0),
        state_root: root.0,
        state_reads: Cell::new(0),
    };
    let mut actual = Vec::new();
    read_range(&reader, root, 0..195, &mut actual).unwrap();
    assert_eq!(
        actual,
        (0..195)
            .map(|index| if index % 2 == 0 { 0x51 } else { 0xa7 })
            .collect::<Vec<_>>()
    );
    assert_eq!(reader.batches.get(), 4);
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
