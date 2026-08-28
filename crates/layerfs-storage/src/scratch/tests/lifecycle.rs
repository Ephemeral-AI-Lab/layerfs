use super::super::*;
use super::fixture::{assert_default_cache_budget, ScratchDriver};
use crate::scratch::schema::SCRATCH_MARKER;
use crate::scratch::table::SCRATCH_SERIAL;
use crate::EngineError;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

#[test]
fn drop_closes_connection_and_removes_exact_owned_files() {
    let anchor =
        std::env::temp_dir().join(format!("layerfs-scratch-anchor-{}", std::process::id()));
    let engine = crate::Engine::open(&anchor).unwrap();
    let path = {
        let table = DiskTable::create_near(&anchor, "cleanup").unwrap();
        assert_default_cache_budget(&table);
        table.put(b"key", b"value").unwrap();
        assert_eq!(table.get(b"key").unwrap(), Some(b"value".to_vec()));
        table.enqueue_once(b"prefix01-b", b"second").unwrap();
        table.enqueue_once(b"prefix02-a", b"other").unwrap();
        table.enqueue_once(b"prefix01-a", b"first").unwrap();
        assert_eq!(
            table.pop_pending_prefix(b"prefix01").unwrap(),
            Some((b"prefix01-a".to_vec(), b"first".to_vec()))
        );
        assert_eq!(
            table.pop_pending_prefix(b"prefix02").unwrap(),
            Some((b"prefix02-a".to_vec(), b"other".to_vec()))
        );
        let mut prefixed = Vec::new();
        table
            .for_each_entry_prefix(b"prefix01", |key, value| {
                prefixed.push((key.to_vec(), value.to_vec()));
                Ok(())
            })
            .unwrap();
        assert_eq!(prefixed.len(), 2);
        let observation = table.observation().unwrap();
        assert_eq!(observation.tables, 1);
        assert!(observation.statements > 23);
        assert_eq!(observation.rows, 12);
        assert!(observation.high_water_bytes > 0);
        assert!(table.path.exists());
        table.path.clone()
    };
    assert!(!path.exists());
    let mut journal = path.into_os_string();
    journal.push("-journal");
    assert!(!PathBuf::from(journal).exists());
    drop(engine);
    std::fs::remove_file(anchor).unwrap();
}

#[test]
fn failed_operation_finish_observes_rollback_only_after_execution() {
    let anchor = std::env::temp_dir().join(format!(
        "layerfs-scratch-failed-finish-{}-{}",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let engine = crate::Engine::open(&anchor).unwrap();
    let table = DiskTable::create_near(&anchor, "failed-finish").unwrap();
    let path = table.path.clone();
    let namespace = table.namespace(b"records").unwrap();
    namespace.put(b"key", b"value").unwrap();
    assert!(matches!(
        namespace.get_ordered_batch(&[b"key"], |_, _| {
            Err(EngineError::InjectedFailure("batch callback"))
        }),
        Err(EngineError::InjectedFailure("batch callback"))
    ));
    let before_finish = table.observation().unwrap();
    assert_eq!(before_finish.derived_setup_statements, 2);
    assert_eq!(before_finish.operation_statements, 2);

    let finished = table.finish().unwrap();
    assert_eq!(finished.derived_setup_statements, 3);
    assert_eq!(finished.operation_statements, 2);
    assert_eq!(finished.statements, before_finish.statements + 1);
    assert!(!path.exists());

    drop(engine);
    std::fs::remove_file(anchor).unwrap();
}

#[test]
fn constructor_failure_removes_exact_owned_files() {
    let path = std::env::temp_dir().join(format!(
        ".layerfs-scratch-failed-{}-{}.sqlite",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    assert!(DiskTable::create_at(path.clone(), "not valid sqlite", [0; 32], 0, 0, 0).is_err());
    assert!(!path.exists());
    let mut journal = path.into_os_string();
    journal.push("-journal");
    assert!(!PathBuf::from(journal).exists());
}

#[test]
fn namespace_write_failure_still_removes_exact_owned_files() {
    let anchor = std::env::temp_dir().join(format!(
        "layerfs-scratch-write-failure-{}-{}",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let engine = crate::Engine::open(&anchor).unwrap();
    let path = {
        let table = DiskTable::create_near(&anchor, "write-failure").unwrap();
        let path = table.path.clone();
        table
            .connection()
            .execute_batch("PRAGMA query_only=ON")
            .unwrap();
        assert!(table
            .namespace(b"records")
            .unwrap()
            .put(b"key", b"value")
            .is_err());
        path
    };
    assert!(!path.exists());
    assert!(!PathBuf::from(format!("{}-journal", path.display())).exists());
    drop(engine);
    std::fs::remove_file(anchor).unwrap();
}

#[test]
fn store_bound_creation_does_not_reopen_exclusively_locked_authority() {
    let store = std::env::temp_dir().join(format!(
        "layerfs-scratch-exclusive-{}-{}.sqlite",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let engine = crate::Engine::open(&store).unwrap();
    let store_id = engine.store_id().unwrap();
    drop(engine);
    let connection = Connection::open(&store).unwrap();
    connection.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let table = DiskTable::create_near_with_store_id(&store, "exclusive", store_id).unwrap();
    assert_default_cache_budget(&table);
    table.put(b"key", b"value").unwrap();
    drop(table);
    connection.execute_batch("ROLLBACK").unwrap();
    drop(connection);
    std::fs::remove_file(store).unwrap();
}

#[test]
fn exclusive_recovery_removes_only_authenticated_unlocked_crash_scratch() {
    let store = std::env::temp_dir().join(format!(
        "layerfs-scratch-recovery-{}-{}.sqlite",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let engine = crate::Engine::open(&store).unwrap();
    let mut table = DiskTable::create_near(&store, "crash").unwrap();
    let scratch = table.path.clone();
    let foreign = scratch.with_file_name(format!(
        ".layerfs-foreign-{}-{}.sqlite",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&foreign, b"foreign").unwrap();
    recover_owned_near(&store, engine.store_id().unwrap(), &ScratchDriver).unwrap();
    assert!(scratch.exists(), "reopen removed live scratch");

    table
        .connection
        .take()
        .unwrap()
        .execute_batch("ROLLBACK")
        .unwrap();
    std::mem::forget(table);
    recover_owned_near(&store, engine.store_id().unwrap(), &ScratchDriver).unwrap();
    assert!(!scratch.exists(), "reopen retained stale owned scratch");
    assert!(
        foreign.exists(),
        "reopen removed foreign scratch-shaped file"
    );

    std::fs::remove_file(foreign).unwrap();
    drop(engine);
    std::fs::remove_file(store).unwrap();
}
#[test]
fn recovery_preserves_same_marker_impostor_schema() {
    let store = std::env::temp_dir().join(format!(
        "layerfs-scratch-impostor-store-{}-{}.sqlite",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let engine = crate::Engine::open(&store).unwrap();
    let impostor = store.with_file_name(format!(
        ".layerfs-impostor-{}-{}.sqlite",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let connection = Connection::open(&impostor).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE entries (key BLOB PRIMARY KEY, value BLOB, pending INTEGER);
             CREATE INDEX entries_pending_key ON entries (pending, key);
             CREATE TABLE scratch_owner (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                format_marker TEXT NOT NULL,
                store_id BLOB NOT NULL CHECK (length(store_id) = 32)
             );",
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO scratch_owner VALUES (1, ?1, ?2)",
            params![SCRATCH_MARKER, engine.store_id().unwrap().as_slice()],
        )
        .unwrap();
    drop(connection);
    let before = std::fs::read(&impostor).unwrap();
    recover_owned_near(&store, engine.store_id().unwrap(), &ScratchDriver).unwrap();
    assert_eq!(std::fs::read(&impostor).unwrap(), before);
    std::fs::remove_file(impostor).unwrap();
    drop(engine);
    std::fs::remove_file(store).unwrap();
}
