//! P5: actual Store length facts avoid whole-file payload materialization.
#![cfg(target_os = "macos")]
mod support;
use layerfs_content::{construct_bytes, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{Storage, StorageError, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use support::Temp;

fn handles(temp: &Temp) -> Handles {
    let mut config = PersistenceConfig::sqlite(temp.join("store"));
    config.sqlite_profile = SqlitePersistenceProfile::Disposable;
    Handles::create(
        config,
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"lengths".to_vec(),
            cursor_key: [9; 32],
            incarnation: 7,
        },
    )
    .unwrap()
}
fn save(storage: &Storage, bytes: &[u8]) -> ObjectId {
    let save = storage.begin_save().unwrap();
    let result = Timing::disabled("construct", |scope| {
        construct_bytes(
            storage.policy().construction(),
            &storage.policy().construction().capacities(),
            bytes,
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap();
    save.finish().unwrap();
    result.root
}

#[test]
fn a_cold_whole_file_length_uses_only_indexed_catalogue_rows() {
    let temp = Temp::new("metadata-length");
    let handles = handles(&temp);
    let writer = Storage::new(handles.storage.clone()).unwrap();
    let root = save(&writer, &vec![0xa7; 131_071]);
    // A fresh owning handle has no earlier own-write/canonical/body cache credit.
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let reader = storage.reader().unwrap();
    let before = handles.diagnostics().unwrap();
    assert_eq!(
        reader.file_lengths(&[root, root]).unwrap(),
        [131_071, 131_071]
    );
    let after = handles.diagnostics().unwrap();
    let work = storage.diagnostics();
    assert_eq!(work.locate, 1);
    assert_eq!(work.read_packs, 0);
    assert_eq!(work.read_pack_selections, 0);
    assert_eq!(work.payload_read_bytes, 0);
    assert_eq!(work.pack_read_bytes, 0);
    assert_eq!(after.blob_open_calls - before.blob_open_calls, 0);
    assert_eq!(after.blob_read_calls - before.blob_read_calls, 0);
    assert_eq!(after.fullscan_steps - before.fullscan_steps, 0);
    assert_eq!(after.sorts - before.sorts, 0);
    assert_eq!(after.autoindex_rows - before.autoindex_rows, 0);
    assert_eq!(after.reprepares - before.reprepares, 0);
    let plans = handles.explain_locate(&[root]).unwrap();
    assert!(
        plans[0]
            .iter()
            .any(|line| line.contains("SEARCH o USING PRIMARY KEY")),
        "{plans:?}"
    );
    assert!(
        plans[0]
            .iter()
            .all(|line| !line.contains("SCAN") && !line.contains("TEMP B-TREE")),
        "{plans:?}"
    );
    println!("FILE_LENGTH rows=2 distinct=1 logical_each=131071 canonical_payload_read=0 locate={} statements={} vm_steps={} returned_rows={} fullscan={} sorts={} autoindex={} reprepares={} plans={plans:?}",work.locate,after.statements-before.statements,after.vm_steps-before.vm_steps,after.returned_rows-before.returned_rows,after.fullscan_steps-before.fullscan_steps,after.sorts-before.sorts,after.autoindex_rows-before.autoindex_rows,after.reprepares-before.reprepares);
}

#[test]
fn mixed_lengths_preserve_order_without_fetching_chunk_payloads() {
    let temp = Temp::new("mixed-lengths");
    let handles = handles(&temp);
    let writer = Storage::new(handles.storage.clone()).unwrap();
    let small = save(&writer, b"small");
    let empty = save(&writer, b"");
    let large = save(&writer, &vec![9; 400_000]);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let reader = storage.reader().unwrap();
    assert_eq!(
        reader.file_lengths(&[large, small, empty, large]).unwrap(),
        [400_000, 5, 0, 400_000]
    );
    let work = storage.diagnostics();
    assert_eq!(work.payload_reads, 0);
    assert_eq!(work.payload_read_bytes, 0);
    println!("FILE_LENGTH mixed rows=4 state_roots=2 payload_reads={} payload_bytes={} metadata_pack_calls={} metadata_pack_bytes={}",work.payload_reads,work.payload_read_bytes,work.read_packs,work.pack_read_bytes);
    let missing = ObjectId::from_bytes(&[99; 32]).unwrap();
    assert!(
        matches!(reader.file_lengths(&[missing]),Err(StorageError::ObjectMissing(id)) if id==missing)
    );
    let root_before = storage.diagnostics().locate;
    assert!(matches!(
        reader.file_lengths(&vec![small; 4097]),
        Err(StorageError::CapacityExceeded {
            what: "storage.file_lengths",
            ..
        })
    ));
    assert_eq!(storage.diagnostics().locate, root_before);
    let save = writer.begin_save().unwrap();
    let link = layerfs_content::filesystem::SymlinkTarget::new(b"target".to_vec())
        .unwrap()
        .finalize()
        .unwrap();
    let link_id = link.id();
    save.accept(link).unwrap();
    save.finish().unwrap();
    assert!(matches!(
        reader.file_lengths(&[link_id]),
        Err(StorageError::Content(
            layerfs_content::ContentError::WrongLogicalRole
        ))
    ));
}
