use super::*;

#[test]
fn publication_reuse_authenticates_the_exact_derived_candidate_without_rehashing() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-derived-reuse-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let canonical = encode_bytes_object(b"same canonical bytes").unwrap();
    let mut publication = engine.begin_publication(None, "main").unwrap();
    let expected = publication.put_object(&canonical).unwrap();

    engine.reset_counters().unwrap();
    assert_eq!(publication.put_object(&canonical).unwrap(), expected);
    let counters = engine.counters().unwrap();
    assert_eq!(counters.objects_validated, 2);
    assert_eq!(counters.objects_reused, 1);
    assert_eq!(counters.object_bytes_read, canonical.len() as u64);
    assert_eq!(counters.incumbent_authentication_passes, 1);
    assert_eq!(counters.identity_authentication_ns, 0);

    drop(publication);
    drop(engine);
    fs::remove_file(path).unwrap();
}

#[test]
fn publication_reuse_over_one_mib_falls_back_and_rejects_corrupt_incumbent() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-large-derived-reuse-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let canonical = encode_bytes_object(&vec![0x5a; 1_048_577]).unwrap();
    let corrupt = encode_bytes_object(&vec![0xa5; 1_048_577]).unwrap();
    let mut publication = engine.begin_publication(None, "main").unwrap();
    let id = publication.put_object(&canonical).unwrap();
    let state = publication
        .publish_namespace(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: InodeId::allocate([0x71; 32], 0),
                inode_table_root: ObjectId::for_bytes(b"large-fallback-table"),
            })
            .unwrap(),
        )
        .unwrap();
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
            params![corrupt, id.as_bytes().as_slice()],
        )
        .unwrap();

    let mut publication = engine.begin_publication(Some(&state), "main").unwrap();
    engine.reset_counters().unwrap();
    assert!(matches!(
        publication.put_object(&canonical),
        Err(EngineError::MalformedObject { .. })
    ));
    let counters = engine.counters().unwrap();
    assert_eq!(counters.put_lookup_statements, 1);
    assert_eq!(counters.objects_validated, 1);
    assert_eq!(counters.new_object_authentication_passes, 1);
    assert_eq!(counters.incumbent_authentication_passes, 0);
    assert_eq!(counters.object_bytes_read, 0);
    assert!(counters.identity_authentication_ns > 0);

    drop(publication);
    drop(engine);
    fs::remove_file(path).unwrap();
}

#[test]
fn trusted_borrowed_load_and_ordered_batch_skip_identity_authentication() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-store-test-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let first = encode_bytes_object(b"first").unwrap();
    let second = encode_bytes_object(b"second").unwrap();
    let first_id = ObjectId::for_bytes(&first);
    let second_id = ObjectId::for_bytes(&second);
    let mut publication = engine.begin_publication(None, "objects").unwrap();
    publication.put_object(&first).unwrap();
    publication.put_object(&second).unwrap();
    publication
        .publish_namespace(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: InodeId::allocate([1; 32], 0),
                inode_table_root: ObjectId::for_bytes(b"reader-test-table"),
            })
            .unwrap(),
        )
        .unwrap();

    engine.reset_counters().unwrap();
    assert_eq!(engine.load_object(first_id).unwrap().canonical_bytes, first);
    let one = engine.counters().unwrap();
    assert_eq!(one.statements, 1);
    assert_eq!(one.objects_validated, 1);
    assert_eq!(one.fetched_rows, 0);
    assert_eq!(one.fetched_row_authentication_passes, 0);
    assert_eq!(one.fetched_row_role_decode_passes, 0);

    engine.reset_counters().unwrap();
    let requested = [second_id, first_id, second_id];
    let mut observed = Vec::new();
    engine
        .for_each_authenticated_payload_batch(&requested, |id, bytes| {
            observed.push((id, bytes.to_vec()));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        observed.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        requested
    );
    let batch = engine.counters().unwrap();
    assert_eq!(batch.statements, 1);
    assert_eq!(batch.objects_validated, 3);
    assert_eq!(batch.fetched_rows, 3);
    assert_eq!(batch.fetched_row_authentication_passes, 0);
    assert_eq!(batch.fetched_row_role_decode_passes, 3);
    assert_eq!(batch.identity_authentication_ns, 0);
    assert_eq!(batch.payload_batch_queries, 1);
    assert_eq!(batch.payload_batch_references, 3);
    assert_eq!(batch.payload_batch_maximum, 3);

    assert!(matches!(
        engine.for_each_authenticated_payload_batch(&[ObjectId::for_bytes(b"missing")], |_, _| Ok(
            ()
        )),
        Err(EngineError::MissingObject(_))
    ));
    assert!(matches!(
        engine.for_each_authenticated_payload_batch(
            &[first_id, ObjectId::for_bytes(b"missing-middle"), second_id],
            |_, _| Ok(())
        ),
        Err(EngineError::MissingObject(_))
    ));
    assert_eq!(
        engine.for_each_authenticated_payload_batch(&vec![first_id; 65], |_, _| Ok(())),
        Err(EngineError::InvalidRecord("payload batch exceeds 64"))
    );
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn trusted_reads_are_weaker_but_incumbent_writes_and_verified_reads_authenticate() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-trusted-read-boundary-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let original = encode_bytes_object(b"first").unwrap();
    let substituted = encode_bytes_object(b"other").unwrap();
    let original_id = ObjectId::for_bytes(&original);
    let mut publication = engine.begin_publication(None, "main").unwrap();
    publication.put_object(&original).unwrap();
    let state = publication
        .publish_namespace(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: InodeId::allocate([0x91; 32], 0),
                inode_table_root: ObjectId::for_bytes(b"unused table"),
            })
            .unwrap(),
        )
        .unwrap();
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
            params![&substituted, original_id.as_bytes().as_slice()],
        )
        .unwrap();

    engine.reset_counters().unwrap();
    assert_eq!(
        engine.load_object(original_id).unwrap().canonical_bytes,
        substituted
    );
    let read = engine.counters().unwrap();
    assert_eq!(read.objects_validated, 1);
    assert_eq!(read.fetched_row_authentication_passes, 0);
    assert_eq!(read.identity_authentication_ns, 0);

    let mut publication = engine.begin_publication(Some(&state), "main").unwrap();
    assert!(matches!(
        publication.put_object(&original),
        Err(EngineError::MalformedObject { .. })
    ));
    drop(publication);
    drop(engine);

    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE layerfs_authority SET trusted_history = 0 WHERE authority_id = 1",
            [],
        )
        .unwrap();
    let verified = Engine::open(&path).unwrap();
    assert!(matches!(
        verified.load_object(original_id),
        Err(EngineError::MalformedObject { .. })
    ));
    drop(verified);
    fs::remove_file(path).unwrap();
}
