use super::*;

#[test]
fn admitted_store_id_is_cached_and_scratch_reuses_it_without_store_sql() {
    let path = test_path();
    let engine = Engine::open(&path).unwrap();
    assert_eq!(engine.counters().unwrap().store_id_queries, 1);
    let expected = engine.store_id().unwrap();
    engine.reset_counters().unwrap();

    for _ in 0..4 {
        assert_eq!(engine.store_id().unwrap(), expected);
    }
    let scratch = engine.create_scratch_table("cached-store-id").unwrap();
    let observation = scratch.observation().unwrap();
    assert_eq!(observation.store_reopens, 0);
    assert_eq!(observation.store_inspection_statements, 0);
    assert_eq!(observation.store_inspection_wall_ns, 0);
    let counters = engine.counters().unwrap();
    assert_eq!(counters.statements, 0);
    assert_eq!(counters.store_id_queries, 0);
    assert_eq!(counters.connection_mutex_wait_ns, 0);

    drop(scratch);
    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn explicit_primary_close_reports_zero_terminal_connections() {
    let path = test_path();
    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    assert_eq!(engine.active_connection_count().unwrap(), 1);
    engine.close_primary_connection().unwrap();
    assert_eq!(engine.active_connection_count().unwrap(), 0);
    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn storage_observation_counts_its_three_queries_as_primary_reads() {
    let path = test_path();
    let engine = Engine::open(&path).unwrap();
    engine.reset_counters().unwrap();

    assert!(engine.observations().logical_engine_bytes.is_some());
    let counters = engine.counters().unwrap();
    assert_eq!(counters.statements, 3);
    assert_eq!(counters.primary_read_statements, 3);
    assert_eq!(counters.publication_statements, 0);
    assert_eq!(counters.live_verified_integrity_statements, 0);
    assert_eq!(counters.reconciliation_statements, 0);
    assert_eq!(counters.compaction_statements, 0);

    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn fresh_engine_open_has_reproducible_physical_size() {
    let mut samples = Vec::new();
    for sample in 0..3 {
        let path = test_path();
        let engine = Engine::open(&path).unwrap();
        let database_bytes = engine.observations().database_bytes.unwrap();
        let page_size = u64::try_from(engine.profile().page_size).unwrap();
        let connection = engine.lock_connection().unwrap();
        let page_count = u64::try_from(
            connection
                .query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))
                .unwrap(),
        )
        .unwrap();
        let table_count = u64::try_from(
            connection
                .query_row(
                    "SELECT count(*) FROM sqlite_schema
                 WHERE type = 'table' AND name NOT GLOB 'sqlite_*'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
        )
        .unwrap();
        drop(connection);
        assert_eq!(page_size, 4_096);
        assert_eq!(table_count, 29);
        assert_eq!(database_bytes, page_count * page_size);
        eprintln!(
            "fresh-engine-open sample={sample} page_size={page_size} \
             page_count={page_count} database_bytes={database_bytes} tables={table_count}"
        );
        samples.push((page_count, database_bytes, table_count));
        drop(engine);
        std::fs::remove_file(path).unwrap();
    }
    assert!(samples.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn capture_is_atomic_and_durable() {
    let path = test_path();
    let (directory_id, directory_bytes) = empty_directory();
    let base_root = root(1, directory_id, None);
    let child_root = root(2, directory_id, Some(base_root.id));
    let base_delta = DeltaRecord::new(None, base_root.id, b"base".to_vec());
    let child_delta = DeltaRecord::new(Some(base_root.id), child_root.id, b"child".to_vec());
    {
        let engine = Engine::open(&path).expect("open");
        let mut capture = engine.begin_capture(None).expect("base capture");
        capture
            .put_object_if_absent(directory_id, &directory_bytes)
            .expect("directory");
        capture.write_delta(&base_delta).expect("base delta");
        capture.commit_root(base_root.clone()).expect("base root");
        assert_eq!(
            engine.load_visible_root().expect("visible"),
            Some(base_root.id)
        );

        let (child_id, child_bytes) = bytes_object(b"child object");
        let mut capture = engine
            .begin_capture(Some(base_root.id))
            .expect("child capture");
        capture
            .put_object_if_absent(child_id, &child_bytes)
            .expect("child object");
        capture.write_delta(&child_delta).expect("child delta");
        capture.fail_before_visible_root();
        assert!(matches!(
            capture.commit_root(child_root.clone()),
            Err(EngineError::InjectedFailure(_))
        ));
        assert_eq!(
            engine.load_visible_root().expect("old visible"),
            Some(base_root.id)
        );
        assert!(matches!(
            engine.load_root(child_root.id),
            Err(EngineError::MissingRoot(_))
        ));

        let mut capture = engine
            .begin_capture(Some(base_root.id))
            .expect("retry capture");
        capture
            .put_object_if_absent(child_id, &child_bytes)
            .expect("child object retry");
        capture
            .write_delta(&child_delta)
            .expect("child delta retry");
        capture.commit_root(child_root.clone()).expect("child root");
    }
    {
        let engine = Engine::open(&path).expect("reopen");
        assert_eq!(
            engine.load_visible_root().expect("visible"),
            Some(child_root.id)
        );
        assert_eq!(engine.load_root(child_root.id).expect("root"), child_root);
        assert_eq!(
            engine.load_delta(child_delta.id).expect("delta"),
            child_delta
        );
    }
    let _ = fs::remove_file(path);
}
