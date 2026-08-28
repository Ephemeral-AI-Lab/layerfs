//! Verified integrity scrub accounting.

use super::*;

#[test]
fn failed_live_scrub_rollback_drops_the_ambiguous_primary() {
    let path = test_path();
    let mut verified = Engine::open(&path).unwrap();
    let trusted = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let root = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([0x61; 32], 0),
        inode_table_root: ObjectId::for_bytes(b"missing table"),
    })
    .unwrap();
    trusted
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&root)
        .unwrap();
    drop(trusted);
    verified.reset_counters().unwrap();
    verified.commit_dispatch = std::sync::Arc::new(RollbackFailure);

    let failed_scrub = match verified.begin_publication(None, "probe") {
        Ok(_) => panic!("failed scrub unexpectedly opened a publication"),
        Err(error) => error,
    };
    assert!(
        matches!(failed_scrub, EngineError::MissingObject(_)),
        "unexpected failed-scrub result: {failed_scrub:?}"
    );
    let counters = verified.counters().unwrap();
    assert_eq!(counters.transactions_started, 1);
    assert_eq!(counters.transactions_rolled_back, 0);
    assert_eq!(counters.publication_transactions_started, 1);
    assert_eq!(counters.publication_transactions_rolled_back, 0);
    assert_eq!(counters.statements, 7);
    assert_eq!(counters.publication_statements, 3);
    assert_eq!(counters.live_verified_integrity_statements, 4);
    assert_eq!(counters.integrity_statements, 4);
    assert_eq!(counters.fetched_rows, 1);
    assert_eq!(counters.fetched_row_authentication_passes, 1);
    assert_eq!(counters.fetched_row_role_decode_passes, 1);
    assert_eq!(counters.scratch_tables, 2);
    assert_eq!(counters.scratch_statements, 46);
    assert!(matches!(
        verified.read_ref("main"),
        Err(EngineError::AmbiguousDurability)
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn failed_initial_scrub_preserves_its_complete_admission_equation() {
    let path = test_path();
    let trusted = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let root = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([0x62; 32], 0),
        inode_table_root: ObjectId::for_bytes(b"missing initial table"),
    })
    .unwrap();
    trusted
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&root)
        .unwrap();
    let store_id = trusted.store_id().unwrap();
    drop(trusted);

    let connection = Connection::open(&path).unwrap();
    let failure = initial_verified_scrub(&connection, &path, store_id).unwrap_err();
    assert!(matches!(
        failure.error,
        EngineError::MissingObject(_) | EngineError::MalformedObject { .. }
    ));
    assert_eq!(failure.observation.statements, 19);
    assert_eq!(failure.observation.transactions_started, 1);
    assert_eq!(failure.observation.transactions_committed, 0);
    assert_eq!(failure.observation.transactions_rolled_back, 1);
    assert_eq!(failure.observation.failed_verification.fetched_rows, 1);
    assert_eq!(
        failure
            .observation
            .failed_verification
            .authentication_passes,
        1
    );
    assert_eq!(
        failure.observation.failed_verification.role_decode_passes,
        1
    );
    assert_eq!(failure.observation.failed_verification.scratch_tables, 2);
    assert_eq!(
        failure.observation.failed_verification.scratch_statements,
        46
    );
    drop(connection);
    assert!(matches!(
        Engine::open(&path),
        Err(EngineError::MissingObject(_)) | Err(EngineError::MalformedObject { .. })
    ));

    std::fs::remove_file(path).unwrap();
}

#[test]
fn verified_read_attribution_closes_sql_and_timer_facts() {
    let path = test_path();
    let (id, canonical) = bytes_object(b"attributed payload");
    let writer = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    writer.put_object_if_absent(id, &canonical).unwrap();
    drop(writer);

    let engine = Engine::open(&path).unwrap();
    engine.reset_counters().unwrap();
    let mut callbacks = 0;
    engine
        .for_each_authenticated_payload_batch(&[id, id], |_, payload| {
            assert_eq!(payload, b"attributed payload");
            callbacks += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(callbacks, 2);
    let counters = engine.counters().unwrap();
    assert_eq!(counters.statements, 4);
    assert_eq!(counters.primary_read_statements, 4);
    assert_eq!(counters.publication_statements, 0);
    assert_eq!(counters.live_verified_integrity_statements, 0);
    assert_eq!(counters.fetched_rows, 2);
    assert_eq!(counters.fetched_row_authentication_passes, 2);
    assert_eq!(counters.fetched_row_role_decode_passes, 2);
    assert!(counters.connection_mutex_wait_ns > 0);
    assert!(counters.trust_guard_ns > 0);
    assert!(counters.payload_query_ns > 0);
    assert!(counters.identity_authentication_ns > 0);
    assert!(counters.role_decode_ns > 0);
    assert!(counters.counter_merge_ns > 0);
    drop(engine);
    std::fs::remove_file(path).unwrap();
}
