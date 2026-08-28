use layerfs_storage::{Engine, FullStorage, StoreRole, FULL_SCHEMA, WORKING_SCHEMA};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-full-storage-{}-{}.sqlite",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

fn remove(path: &PathBuf) {
    std::fs::remove_file(path).unwrap();
}

fn schema_counts(path: &PathBuf) -> (i64, i64) {
    let connection = Connection::open(path).unwrap();
    let tables = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema
             WHERE type = 'table' AND name NOT GLOB 'sqlite_*'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let indexes = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND sql IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    (tables, indexes)
}

fn physical_size(path: &PathBuf) -> (i64, i64, u64) {
    let connection = Connection::open(path).unwrap();
    let page_size = connection
        .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
        .unwrap();
    let page_count = connection
        .query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))
        .unwrap();
    (
        page_size,
        page_count,
        std::fs::metadata(path).unwrap().len(),
    )
}

fn percentile(samples: &[u128], numerator: usize) -> u128 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (sorted.len() * numerator).div_ceil(100);
    sorted[rank.saturating_sub(1)]
}

#[test]
fn fresh_durable_full_storage_has_exact_identity_schema_and_profile() {
    let path = path();
    let storage = FullStorage::create_durable(&path).unwrap();
    let storage_id = storage.storage_id();
    assert_eq!(storage.role(), StoreRole::Durable);
    assert_eq!(storage_id, storage.durable_storage_id());
    assert_eq!(storage.profile().page_size, 4_096);
    assert_eq!(
        storage.profile().journal_mode.to_ascii_uppercase(),
        "DELETE"
    );
    assert_eq!(storage.active_connection_count().unwrap(), 1);
    assert_eq!(schema_counts(&path), (21, 4));
    drop(storage);

    let reopened = FullStorage::open_durable(&path).unwrap();
    assert_eq!(reopened.storage_id(), storage_id);
    assert_eq!(reopened.durable_storage_id(), storage_id);
    drop(reopened);
    remove(&path);
}

#[test]
fn cache_role_is_immutable_and_cannot_reopen_as_authority() {
    let path = path();
    let durable_storage_id = [0x51; 32];
    let cache = FullStorage::create_cache(&path, durable_storage_id).unwrap();
    assert_eq!(cache.role(), StoreRole::DurableCache);
    assert_eq!(cache.durable_storage_id(), durable_storage_id);
    assert_ne!(cache.storage_id(), durable_storage_id);
    drop(cache);

    assert!(FullStorage::open_durable(&path).is_err());
    assert!(FullStorage::open_cache(&path, [0x52; 32]).is_err());
    let cache = FullStorage::open_cache(&path, durable_storage_id).unwrap();
    assert_eq!(cache.durable_storage_id(), durable_storage_id);
    drop(cache);
    remove(&path);
}

#[test]
fn full_open_rejects_legacy_without_mutating_it_and_missing_without_creating_it() {
    let missing = path();
    assert!(FullStorage::open_durable(&missing).is_err());
    assert!(!missing.exists());

    let legacy = path();
    let engine = Engine::open(&legacy).unwrap();
    let store_id = engine.store_id().unwrap();
    let bytes = std::fs::metadata(&legacy).unwrap().len();
    drop(engine);
    assert!(FullStorage::open_durable(&legacy).is_err());
    assert_eq!(std::fs::metadata(&legacy).unwrap().len(), bytes);
    let engine = Engine::open(&legacy).unwrap();
    assert_eq!(engine.store_id().unwrap(), store_id);
    drop(engine);
    remove(&legacy);
}

#[test]
fn full_open_rejects_working_family_and_wrong_named_index_shape() {
    let working = path();
    let connection = Connection::open(&working).unwrap();
    connection
        .execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA synchronous=FULL;
             PRAGMA temp_store=FILE;
             PRAGMA mmap_size=0;
             PRAGMA foreign_keys=ON;",
        )
        .unwrap();
    for partition in WORKING_SCHEMA.table_partitions {
        for (_, sql) in *partition {
            connection.execute_batch(sql).unwrap();
        }
    }
    for (_, sql) in WORKING_SCHEMA.index_schemas {
        connection.execute_batch(sql).unwrap();
    }
    connection
        .execute(
            "INSERT INTO layerfs_store_meta
             (store_id, format_marker, schema_version, store_role, storage_id,
              next_inode_serial, trusted_history, journal_mode, synchronous,
              temp_store, mmap_size)
             VALUES (1, ?1, ?2, 'working', ?3, 0, 0, 'DELETE', 2, 1, 0)",
            params![
                WORKING_SCHEMA.format_marker,
                WORKING_SCHEMA.schema_version,
                [0x61_u8; 32]
            ],
        )
        .unwrap();
    drop(connection);
    assert!(FullStorage::open_durable(&working).is_err());
    remove(&working);

    let full = path();
    drop(FullStorage::create_durable(&full).unwrap());
    let connection = Connection::open(&full).unwrap();
    connection
        .execute_batch(
            "DROP INDEX layerfs_full_transfer_state_owner_idx;
             CREATE INDEX layerfs_full_transfer_state_owner_idx
             ON layerfs_transfer_state (direction, owner_request_id);",
        )
        .unwrap();
    drop(connection);
    assert!(FullStorage::open_durable(&full).is_err());
    remove(&full);
}

#[test]
fn full_open_rejects_unexpected_trigger_and_view() {
    let path = path();
    drop(FullStorage::create_durable(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER unexpected_trigger AFTER INSERT ON layerfs_objects BEGIN
                 SELECT 1;
             END;",
        )
        .unwrap();
    drop(connection);
    assert!(FullStorage::open_durable(&path).is_err());

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER unexpected_trigger;
             CREATE VIEW unexpected_view AS SELECT object_id FROM layerfs_objects;",
        )
        .unwrap();
    drop(connection);
    assert!(FullStorage::open_durable(&path).is_err());
    remove(&path);
}

#[test]
fn verified_full_backup_preserves_identity_and_schema() {
    let source = path();
    let backup = path();
    let storage = FullStorage::create_durable(&source).unwrap();
    let storage_id = storage.storage_id();
    storage.backup_to(&backup).unwrap();
    assert_eq!(schema_counts(&backup), (21, 4));
    let reopened = FullStorage::open_durable_verified(&backup).unwrap();
    assert_eq!(reopened.storage_id(), storage_id);
    drop(reopened);
    drop(storage);
    remove(&source);
    remove(&backup);
}

#[test]
fn create_never_relabels_an_existing_full_store() {
    let path = path();
    let durable = FullStorage::create_durable(&path).unwrap();
    let storage_id = durable.storage_id();
    drop(durable);
    assert!(FullStorage::create_cache(&path, storage_id).is_err());
    let durable = FullStorage::open_durable(&path).unwrap();
    assert_eq!(durable.storage_id(), storage_id);
    drop(durable);
    remove(&path);
}

#[test]
fn create_never_claims_an_existing_empty_file() {
    let path = path();
    std::fs::File::create(&path).unwrap();
    assert!(FullStorage::create_durable(&path).is_err());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    remove(&path);
}

#[test]
fn full_contract_used_by_runtime_is_the_frozen_manifest() {
    assert_eq!(FULL_SCHEMA.table_names.len(), 21);
    assert_eq!(FULL_SCHEMA.index_schemas.len(), 4);
}

#[test]
fn full_create_reopen_and_physical_size_meet_preregistered_p1_budgets() {
    let mut legacy_create = Vec::new();
    let mut legacy_reopen = Vec::new();
    let mut full_create = Vec::new();
    let mut full_reopen = Vec::new();
    let mut full_physical = Vec::new();
    for _ in 0..31 {
        let legacy = path();
        let started = Instant::now();
        let engine = Engine::open(&legacy).unwrap();
        legacy_create.push(started.elapsed().as_nanos());
        drop(engine);
        let started = Instant::now();
        drop(Engine::open(&legacy).unwrap());
        legacy_reopen.push(started.elapsed().as_nanos());

        let full = path();
        let started = Instant::now();
        drop(FullStorage::create_durable(&full).unwrap());
        full_create.push(started.elapsed().as_nanos());
        let started = Instant::now();
        drop(FullStorage::open_durable_verified(&full).unwrap());
        full_reopen.push(started.elapsed().as_nanos());
        assert_eq!(schema_counts(&full), (21, 4));
        full_physical.push(physical_size(&full));

        remove(&legacy);
        remove(&full);
    }
    let values = (
        percentile(&legacy_create, 50),
        percentile(&legacy_create, 95),
        percentile(&full_create, 50),
        percentile(&full_create, 95),
        percentile(&legacy_reopen, 50),
        percentile(&legacy_reopen, 95),
        percentile(&full_reopen, 50),
        percentile(&full_reopen, 95),
    );
    eprintln!(
        "legacy_create_ns={legacy_create:?}\nfull_create_ns={full_create:?}\n\
         legacy_reopen_ns={legacy_reopen:?}\nfull_reopen_ns={full_reopen:?}\n\
         medians_p95={values:?}\nfull_physical={full_physical:?}"
    );
    assert!(full_physical
        .iter()
        .all(|&(page_size, pages, bytes)| page_size == 4_096
            && pages <= 84
            && bytes <= 344_064
            && bytes == u64::try_from(page_size * pages).unwrap()));
    assert!(values.2 * 100 <= values.0 * 125);
    assert!(values.3 * 100 <= values.1 * 150);
    assert!(values.6 * 100 <= values.4 * 150);
    assert!(values.7 * 100 <= values.5 * 200);
}
