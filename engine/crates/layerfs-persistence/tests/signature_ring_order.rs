//! The delta-candidate ring reads back in insertion order from a slot-order
//! scan (#352 record 034, S1-a).
//!
//! The ring's rows are read in primary-key (slot) order and put in stamp order
//! in memory, instead of an `ORDER BY stamp` the table has no index for. After
//! more admissions than the ring has slots (wrap-around), the rows the port
//! returns equal a stamp-ordered read row for row, so the ring a Storage loads
//! from them is the same slot for slot. A fresh Storage on the Store then
//! finds an entry written after the wrap as a delta base.
mod support;
use layerfs_content::{ConstructionPolicy, ContentResult, FinalizedConsumer, FinalizedObject};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{location::SignatureRow, port::PackPersistence, Storage, StoragePolicy};

/// More whole-file admissions than the ring's 8,192 slots.
const OBJECTS: u64 = 9_000;

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

/// 256 bytes no other file shares a 16-byte window with.
fn bytes(index: u64) -> Vec<u8> {
    (0..32_u64)
        .flat_map(|word| {
            blake3::hash(&[index.to_le_bytes(), word.to_le_bytes()].concat()).as_bytes()[..8]
                .to_vec()
        })
        .collect()
}

fn whole_file(out: &mut Emitted, content: &[u8]) {
    let policy = ConstructionPolicy::frozen_default();
    layerfs_telemetry::timer::Timing::disabled("construct", |scope| {
        layerfs_content::construct_bytes(
            policy,
            &policy.capacities(),
            content,
            out,
            scope.child("construct"),
        )
    })
    .0
    .unwrap();
}

#[test]
fn a_wrapped_ring_reads_back_in_stamp_order_and_proposes_its_newest_entries() {
    let temp = support::Temp::new("signature-ring-order");
    let path = temp.join("store.sqlite");
    let handles = Handles::create(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"signature-ring-order".to_vec(),
            cursor_key: [34; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let mut emitted = Emitted::default();
    for index in 0..OBJECTS {
        whole_file(&mut emitted, &bytes(index));
    }
    let save = storage.begin_save().unwrap();
    for object in emitted.0 {
        save.accept(object).unwrap();
    }
    let outcome = save.finish().unwrap();
    assert!(
        outcome.delta.no_candidate >= OBJECTS - 8,
        "{:?}",
        outcome.delta
    );
    drop(storage);

    let mut port = Vec::new();
    handles.storage.signatures(&mut port).unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    let ordered: Vec<SignatureRow> = raw
        .prepare("SELECT slot,stamp,object_id,signature FROM content_signature ORDER BY stamp")
        .unwrap()
        .query_map([], |row| {
            Ok(SignatureRow {
                slot: row.get::<_, i64>(0)? as usize,
                stamp: row.get::<_, i64>(1)? as u64,
                object_id: layerfs_content::ObjectId::from_bytes(&row.get::<_, Vec<u8>>(2)?)
                    .unwrap(),
                signature: row.get::<_, Vec<u8>>(3)?.try_into().unwrap(),
            })
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    drop(raw);
    assert_eq!(ordered.len(), 8_192, "a full ring");
    let newest = ordered.last().unwrap().stamp;
    assert!(newest > 8_192, "the ring wrapped: newest stamp {newest}");
    assert_eq!(port.len(), ordered.len());
    for (index, (got, want)) in port.iter().zip(&ordered).enumerate() {
        assert_eq!(
            (got.slot, got.stamp, got.object_id, got.signature),
            (want.slot, want.stamp, want.object_id, want.signature),
            "row {index}"
        );
    }

    // A fresh Storage loads that ring and proposes an entry written after
    // the wrap as the base of a near copy that names no predecessor.
    let fresh = Storage::new(handles.storage.clone()).unwrap();
    let mut near = bytes(OBJECTS - 1);
    near[0] ^= 1;
    let mut emitted = Emitted::default();
    whole_file(&mut emitted, &near);
    let save = fresh.begin_save().unwrap();
    for object in emitted.0 {
        save.accept(object).unwrap();
    }
    let outcome = save.finish().unwrap();
    assert_eq!(outcome.delta.no_candidate, 0, "{:?}", outcome.delta);
    assert_eq!(outcome.delta.prefix_selected, 1, "{:?}", outcome.delta);
}
