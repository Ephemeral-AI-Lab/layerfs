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
    assert_eq!(reused.read_pack_selections, acquired.read_pack_selections);
    assert_eq!(reused.range_scan_bytes, acquired.range_scan_bytes);
    assert_eq!(
        reused.range_materialized_bytes,
        acquired.range_materialized_bytes
    );
    assert_eq!(reused.pack_read_bytes, acquired.pack_read_bytes);
    assert_eq!(reused.payload_read_bytes, acquired.payload_read_bytes);
    // A new operation owns an empty cache and must pay for acquisition again.
    drop(reader);
    assert_eq!(
        storage.reader().unwrap().read_objects(&[o.id()]).unwrap(),
        vec![o.canonical().to_vec()]
    );
    let fresh = storage.diagnostics();
    assert!(
        fresh.read_packs + fresh.payload_reads + fresh.read_pack_selections
            > reused.read_packs + reused.payload_reads + reused.read_pack_selections
    );
}

#[test]
fn partial_pack_queue_survives_wave_boundary_within_existing_budget() {
    let t = support::Temp::new("pack-wave-utilization");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let objects = (1..=1536).map(|seed| object(seed, 500)).collect::<Vec<_>>();
    let before = h.diagnostics().unwrap();
    let save = storage.begin_save().unwrap();
    for o in &objects {
        save.accept(o.clone()).unwrap();
    }
    save.finish().unwrap();
    let after = h.diagnostics().unwrap();
    eprintln!(
        "DIAGNOSTIC sealed packs={} publications={}",
        after.sealed_inserts - before.sealed_inserts,
        storage.diagnostics().publish
    );
    assert_eq!(after.sealed_inserts - before.sealed_inserts, 4);
    for ids in objects.chunks(128) {
        assert_eq!(
            storage
                .reader()
                .unwrap()
                .read_objects(&ids.iter().map(FinalizedObject::id).collect::<Vec<_>>())
                .unwrap(),
            ids.iter()
                .map(|o| o.canonical().to_vec())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn same_save_read_and_dependency_force_closure_of_carried_queue() {
    let t = support::Temp::new("pack-carry-closure");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let children = (1..=513).map(|seed| object(seed, 500)).collect::<Vec<_>>();
    let save = storage.begin_save().unwrap();
    for child in &children {
        save.accept(child.clone()).unwrap();
    }
    assert_eq!(
        save.read_objects(&[children[0].id(), children[511].id()])
            .unwrap(),
        vec![
            children[0].canonical().to_vec(),
            children[511].canonical().to_vec()
        ]
    );
    let parent = object(9001, 500).with_references(vec![children[0].id(), children[511].id()]);
    save.accept(parent.clone()).unwrap();
    save.finish().unwrap();
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[parent.id(), children[511].id()])
            .unwrap(),
        vec![
            parent.canonical().to_vec(),
            children[511].canonical().to_vec()
        ]
    );
}

#[test]
fn a_small_acknowledged_pack_tail_serves_a_larger_following_wave() {
    let t = support::Temp::new("pack-tail-wave-demand");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let objects = (1..=91)
        .map(|seed| object(seed, 46000))
        .chain((1000..2024).map(|seed| object(seed, 500)))
        .collect::<Vec<_>>();
    let save = storage.begin_save().unwrap();
    for o in &objects {
        save.accept(o.clone()).unwrap();
    }
    save.finish().unwrap();
    let calls = storage.diagnostics().reserve;
    eprintln!(
        "DIAGNOSTIC pack reservations={calls} body inserts={}",
        h.diagnostics().unwrap().sealed_inserts
    );
    assert_eq!(
        calls, 1,
        "existing tail covers actual pack demand of the small-record wave"
    );
    for chunk in objects.chunks(32) {
        assert_eq!(
            storage
                .reader()
                .unwrap()
                .read_objects(&chunk.iter().map(FinalizedObject::id).collect::<Vec<_>>())
                .unwrap(),
            chunk
                .iter()
                .map(|o| o.canonical().to_vec())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn allocation_replenishes_before_pressure_seals_when_the_tail_is_consumed() {
    let t = support::Temp::new("pack-tail-replenish");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let objects = (1..=1024)
        .map(|seed| object(seed, 46000))
        .collect::<Vec<_>>();
    let save = storage.begin_save().unwrap();
    for o in &objects[..92] {
        save.accept(o.clone()).unwrap();
    }
    let sql = rusqlite::Connection::open(t.join("db")).unwrap();
    let initial_end: i64 = sql
        .query_row("SELECT next_pack_id FROM store_policy", [], |r| r.get(0))
        .unwrap();
    drop(sql);
    for o in &objects[92..] {
        save.accept(o.clone()).unwrap();
    }
    save.finish().unwrap();
    assert!(storage.diagnostics().reserve > 1);
    for chunk in objects.chunks(8) {
        assert_eq!(
            storage
                .reader()
                .unwrap()
                .read_objects(&chunk.iter().map(FinalizedObject::id).collect::<Vec<_>>())
                .unwrap(),
            chunk
                .iter()
                .map(|o| o.canonical().to_vec())
                .collect::<Vec<_>>()
        );
    }
    let sql = rusqlite::Connection::open(t.join("db")).unwrap();
    let (rows, distinct): (i64, i64) = sql
        .query_row(
            "SELECT count(*),count(DISTINCT pack_id) FROM pack",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(
        rows >= initial_end,
        "workload consumes more than the initial acknowledged range"
    );
    assert_eq!(rows, distinct);
}

#[test]
fn successful_save_history_retains_selection_and_group_work_from_final_drain() {
    let t = support::Temp::new("selection-final-drain");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let raw = object(9001, 4000);
    let whole = FinalizedObject::new(
        ObjectRole::WholeFile,
        layerfs_content::file::encode_whole_file_payload(
            layerfs_content::object::codec::decode_bytes_object(raw.canonical()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(whole.clone()).unwrap();
    assert_eq!(storage.save_work().completed, 0);
    assert_eq!(storage.save_work().total.selection.full_ns, 0);
    save.finish().unwrap();
    let observed = storage.save_work();
    assert_eq!(observed.completed, 1);
    assert!(observed.recent[0].selection.full_ns > 0);
    assert!(observed.recent[0].selection.group_ns > 0);
    assert_eq!(
        observed.total.selection.full_ns,
        observed.recent[0].selection.full_ns
    );
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[whole.id()])
            .unwrap(),
        vec![whole.canonical().to_vec()]
    );
}

#[test]
fn queued_chunk_groups_use_the_acknowledged_pack_tail() {
    let t = support::Temp::new("queued-group-reservation-tail");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    // One small initial Save acknowledges seven IDs and consumes only one.
    let initial = object(9000, 16);
    let save = storage.begin_save().unwrap();
    save.accept(initial).unwrap();
    save.finish().unwrap();
    let before = storage.diagnostics().reserve;
    let chunks = (1_u64..=128)
        .map(|seed| {
            let mut raw = vec![seed as u8; 32768];
            raw[..8].copy_from_slice(&seed.to_be_bytes());
            FinalizedObject::new(
                ObjectRole::Chunk,
                layerfs_content::file::mapping::encode_chunk_object(&raw).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let save = storage.begin_save().unwrap();
    for chunk in &chunks {
        save.accept(chunk.clone()).unwrap();
    }
    save.finish().unwrap();
    let after = storage.diagnostics().reserve;
    println!(
        "QUEUED_PACK_RESERVATION before={before} after={after} chunks={}",
        chunks.len()
    );
    assert_eq!(
        after, before,
        "six acknowledged IDs cover the queued native pack; groups are not packs"
    );
    let ids = chunks.iter().map(FinalizedObject::id).collect::<Vec<_>>();
    assert_eq!(
        storage.reader().unwrap().read_objects(&ids).unwrap(),
        chunks
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
    let sql = rusqlite::Connection::open(t.join("db")).unwrap();
    let packs: i64 = sql
        .query_row("SELECT count(*) FROM pack", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        packs, 2,
        "one initial pack and one tight-directory native pack"
    );
}
