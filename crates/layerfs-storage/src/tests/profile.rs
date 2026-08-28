use super::*;

#[test]
fn cache_spill_requires_enable_before_threshold() {
    let path = test_path();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE probe(value INTEGER); PRAGMA cache_size=1280; PRAGMA cache_spill=ON;",
        )
        .unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA cache_spill", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1280
    );
    drop(connection);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn foreign_hot_journal_child() {
    let Some(path) = std::env::var_os("LAYERFS_FOREIGN_HOT_JOURNAL") else {
        return;
    };
    let connection = Connection::open(path).unwrap();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    connection
        .execute("UPDATE foreign_table SET value = 'mutated'", [])
        .unwrap();
    std::process::exit(92);
}

#[test]
fn profile_range_reopen_and_counters() {
    let path = test_path();
    let (id, bytes) = bytes_object(b"durable range payload");
    {
        let engine = Engine::open(&path).expect("open");
        assert_eq!(engine.profile().journal_mode.to_ascii_uppercase(), "DELETE");
        assert_eq!(engine.profile().synchronous, 2);
        assert_eq!(engine.profile().temp_store, 1);
        assert_eq!(engine.profile().mmap_size, 0);
        assert_eq!(engine.profile().cache_pages, 1280);
        assert_eq!(engine.profile().cache_spill_pages, 1280);
        assert_eq!(
            engine.put_object_if_absent(id, &bytes),
            Ok(PutOutcome::Created)
        );
        assert_eq!(
            engine.read_object_range(id, 2..7).expect("range"),
            bytes[2..7]
        );
        let reversed_start = 7;
        let reversed_end = 2;
        assert!(matches!(
            engine.read_object_range(id, reversed_start..reversed_end),
            Err(EngineError::InvalidRange { .. })
        ));
        assert!(matches!(
            engine.read_object_range(id, 0..bytes.len() as u64 + 1),
            Err(EngineError::InvalidRange { .. })
        ));
        assert_eq!(
            engine
                .read_object_range(id, bytes.len() as u64..bytes.len() as u64)
                .expect("empty"),
            Vec::<u8>::new()
        );
        let counters = engine.counters().expect("counters");
        assert!(counters.objects_validated >= 2);
        assert_eq!(counters.range_bytes_returned, 5);
    }
    {
        let engine = Engine::open(&path).expect("reopen");
        assert_eq!(
            engine.load_object(id).expect("object").canonical_bytes,
            bytes
        );
        assert!(engine.observations().logical_engine_bytes.is_some());
    }
    let _ = fs::remove_file(path);
}
