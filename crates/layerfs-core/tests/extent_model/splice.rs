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
