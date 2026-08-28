use super::*;

#[test]
fn empty_compaction_counts_every_statement_in_its_disjoint_family() {
    let path = test_path();
    let destination = path.with_extension("compacted");
    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    engine.reset_counters().unwrap();

    engine.compact_to(&destination).unwrap();
    let counters = engine.counters().unwrap();
    assert_eq!(counters.statements, 92);
    assert_eq!(counters.compaction_statements, 92);
    assert_eq!(counters.primary_read_statements, 0);
    assert_eq!(counters.publication_statements, 0);
    assert_eq!(counters.live_verified_integrity_statements, 0);
    assert_eq!(counters.reconciliation_statements, 0);
    assert_eq!(counters.scratch_tables, 4);
    assert_eq!(counters.scratch_statements, 73);
    assert_eq!(counters.scratch_rows, 4);
    assert_eq!(counters.statements + counters.scratch_statements, 165);

    drop(engine);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(destination).unwrap();
}

#[test]
fn failed_retained_union_assigns_completed_sql_to_compaction() {
    let path = test_path();
    let destination = path.with_extension("failed-compaction");
    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([0x42; 32], 0),
        inode_table_root: ObjectId::for_bytes(b"missing inode table"),
    })
    .unwrap();
    engine
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&namespace)
        .unwrap();
    engine.reset_counters().unwrap();

    assert!(matches!(
        engine.compact_to(&destination),
        Err(EngineError::MissingObject(_)) | Err(EngineError::MalformedObject { .. })
    ));
    let counters = engine.counters().unwrap();
    assert_eq!(counters.statements, 57);
    assert_eq!(counters.compaction_statements, 57);
    assert_eq!(counters.primary_read_statements, 0);
    assert_eq!(counters.fetched_rows, 1);
    assert_eq!(counters.fetched_row_authentication_passes, 1);
    assert_eq!(counters.fetched_row_role_decode_passes, 1);
    assert_eq!(counters.scratch_tables, 2);
    assert_eq!(counters.scratch_statements, 46);
    assert!(!destination.exists());

    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn post_retained_copy_failure_finishes_and_merges_source_scratch() {
    let path = test_path();
    let candidate_path = path.with_extension("retained-copy-failure");
    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    drop(
        Engine::open_with_mode(&candidate_path, integrity::IntegrityMode::TrustedLocalDev).unwrap(),
    );
    let source = engine.lock_connection().unwrap();
    let statements = Cell::new(0);
    let failed = Cell::new(integrity::VerificationObservation::default());
    let retained = integrity::retained_union(
        &source,
        &path,
        engine.store_id().unwrap(),
        &statements,
        &failed,
    )
    .unwrap();
    let candidate = Connection::open(&candidate_path).unwrap();
    candidate
        .execute(
            "ATTACH DATABASE ?1 AS source",
            params![path.to_str().unwrap()],
        )
        .unwrap();
    candidate
        .execute_batch(
            "CREATE TEMP TRIGGER fail_retained_copy_authority
                 BEFORE INSERT ON layerfs_authority
                 BEGIN SELECT RAISE(ABORT, 'injected retained copy'); END;",
        )
        .unwrap();
    engine.reset_counters().unwrap();

    assert!(engine
        .copy_retained_to_candidate(&source, retained, &candidate)
        .is_err());
    let counters = engine.counters().unwrap();
    assert_eq!(counters.statements, 5);
    assert_eq!(counters.compaction_statements, 5);
    assert_eq!(counters.scratch_tables, 2);
    assert_eq!(counters.scratch_statements, 36);
    assert_eq!(counters.scratch_rows, 2);
    assert_eq!(counters.statements + counters.scratch_statements, 41);

    drop(candidate);
    drop(source);
    drop(engine);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(candidate_path).unwrap();
}

#[test]
fn partial_compaction_metadata_copy_counts_only_attempted_statements() {
    let path = test_path();
    let candidate_path = path.with_extension("partial-copy");
    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    drop(
        Engine::open_with_mode(&candidate_path, integrity::IntegrityMode::TrustedLocalDev).unwrap(),
    );
    let candidate = Connection::open(&candidate_path).unwrap();
    candidate
        .execute(
            "ATTACH DATABASE ?1 AS source",
            params![path.to_str().unwrap()],
        )
        .unwrap();
    candidate
        .execute_batch(
            "CREATE TEMP TRIGGER fail_compaction_authority
                 BEFORE INSERT ON layerfs_authority
                 BEGIN SELECT RAISE(ABORT, 'injected partial copy'); END;",
        )
        .unwrap();
    engine.reset_counters().unwrap();

    assert!(engine.copy_compaction_metadata(&candidate).is_err());
    let counters = engine.counters().unwrap();
    assert_eq!(counters.statements, 5);
    assert_eq!(counters.compaction_statements, 5);
    assert_eq!(counters.primary_read_statements, 0);

    drop(candidate);
    drop(engine);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(candidate_path).unwrap();
}
