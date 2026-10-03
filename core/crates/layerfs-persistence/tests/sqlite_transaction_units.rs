//! Independent atomic byte charges and acknowledged reservation-tail ownership.
mod support;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_storage::{Storage, StoragePolicy};
fn create(path: &std::path::Path) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"transaction-units".to_vec(),
            cursor_key: [71; 32],
            incarnation: 1,
        },
    )
    .unwrap()
}
fn object(seed: u64, len: usize) -> FinalizedObject {
    let mut state = seed;
    let data = (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect::<Vec<_>>();
    FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(&data).unwrap(),
    )
    .unwrap()
}
#[test]
fn canonical_and_shared_physical_bytes_are_bounded_independently() {
    let t = support::Temp::new("independent-charges");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let objects = (1..=60).map(|seed| object(seed, 46000)).collect::<Vec<_>>();
    let before = h.diagnostics().unwrap();
    let save = storage.begin_save().unwrap();
    for o in &objects {
        save.accept(o.clone()).unwrap();
    }
    assert_eq!(save.finish().unwrap().inserted, 60);
    let after = h.diagnostics().unwrap();
    // One reservation, one wave publication and one final open-group publication.
    // Each side is below4MiB; adding the shared representations would need an
    // additional wave publication. Finishing keeps its existing separate boundary.
    assert_eq!(after.write_commits - before.write_commits, 3);
    assert!(after.sealed_body_bytes - before.sealed_body_bytes > 2500000);
    assert!(
        after.sealed_body_bytes - before.sealed_body_bytes
            < layerfs_storage::policy::TRANSACTION_PHYSICAL_BYTES_LIMIT
    );
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&objects.iter().map(FinalizedObject::id).collect::<Vec<_>>())
            .unwrap(),
        objects
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
}
#[test]
fn unused_pack_ids_transfer_across_saves_without_recycling_used_ids() {
    let t = support::Temp::new("reservation-tail");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let mut expected = Vec::new();
    for seed in [123, 456, 789] {
        let o = object(seed, 1000);
        let save = storage.begin_save().unwrap();
        save.accept(o.clone()).unwrap();
        save.finish().unwrap();
        expected.push(o);
    }
    assert_eq!(storage.diagnostics().reserve, 1);
    let sql = rusqlite::Connection::open(t.join("db")).unwrap();
    let (rows, distinct): (i64, i64) = sql
        .query_row(
            "SELECT count(*),count(DISTINCT pack_id) FROM pack",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((rows, distinct), (3, 3));
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&expected.iter().map(FinalizedObject::id).collect::<Vec<_>>())
            .unwrap(),
        expected
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
}
#[test]
fn dropping_unfinished_save_does_not_return_consumed_ids_to_the_unused_tail() {
    let t = support::Temp::new("abandoned-range");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let before = object(123, 46000);
    let after = object(456, 46000);
    {
        let save = storage.begin_save().unwrap();
        for _ in 0..100 {
            save.accept(before.clone()).unwrap();
        }
    }
    let save = storage.begin_save().unwrap();
    save.accept(after.clone()).unwrap();
    save.finish().unwrap();
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[after.id()])
            .unwrap(),
        vec![after.canonical().to_vec()]
    );
    let sql = rusqlite::Connection::open(t.join("db")).unwrap();
    let (rows, distinct): (i64, i64) = sql
        .query_row(
            "SELECT count(*),count(DISTINCT pack_id) FROM pack",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(rows, distinct);
}

#[test]
fn reader_reuses_authenticated_pack_within_one_operation() {
    let t = support::Temp::new("reader-pack-lifetime");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let o = object(987, 46000);
    let save = storage.begin_save().unwrap();
    save.accept(o.clone()).unwrap();
    save.finish().unwrap();
    let reader = storage.reader().unwrap();
    assert_eq!(
        reader.read_objects(&[o.id()]).unwrap(),
        vec![o.canonical().to_vec()]
    );
    let acquired = storage.diagnostics();
    assert_eq!(
        reader.read_objects(&[o.id(), o.id()]).unwrap(),
        vec![o.canonical().to_vec(); 2]
    );
    let reused = storage.diagnostics();
    assert_eq!(reused.read_packs, acquired.read_packs);
    assert_eq!(reused.payload_reads, acquired.payload_reads);
    assert_eq!(reused.pack_read_bytes, acquired.pack_read_bytes);
    assert_eq!(reused.payload_read_bytes, acquired.payload_read_bytes);
    // A new operation owns an empty cache and must pay for acquisition again.
    drop(reader);
    assert_eq!(
        storage.reader().unwrap().read_objects(&[o.id()]).unwrap(),
        vec![o.canonical().to_vec()]
    );
    let fresh = storage.diagnostics();
    assert!(fresh.read_packs + fresh.payload_reads > reused.read_packs + reused.payload_reads);
}
