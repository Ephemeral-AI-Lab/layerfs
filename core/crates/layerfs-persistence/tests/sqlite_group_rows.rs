//! Explicit schema2 units preserve the public canonical and complete-pack contracts.
mod support;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePackLayout, SqlitePersistenceProfile};
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo},
    pack::{assemble, build_group, layout::PackLane},
    port::*,
    Storage, StoragePolicy,
};
use std::sync::Arc;
fn config() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"group-rows".to_vec(),
        cursor_key: [73; 32],
        incarnation: 1,
    }
}
fn create(path: &std::path::Path) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(path)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_pack_layout(SqlitePackLayout::GroupRows),
        StoragePolicy::frozen_default(),
        &config(),
    )
    .unwrap()
}
fn fixture(h: &Handles) -> Vec<FinalizedObject> {
    let mut random = 931823u64;
    let objects = (0..8)
        .map(|_| {
            let bytes = (0..9000)
                .map(|_| {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    random as u8
                })
                .collect::<Vec<_>>();
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&bytes).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let groups = objects
        .iter()
        .map(|o| {
            let mut record = vec![0];
            record.extend_from_slice(o.canonical());
            build_group(PackLane::Ordinary, &[record], None).unwrap()
        })
        .collect::<Vec<_>>();
    let body = assemble(PackLane::Ordinary, &groups).unwrap();
    h.storage
        .publish(&Publication {
            packs: vec![PublishedPack {
                info: PackInfo {
                    pack_id: 1,
                    domain: PackDomain::Metadata,
                    key: ObjectKey::for_bytes(&body),
                    length: body.len(),
                },
                body: Arc::new(body),
            }],
            objects: objects
                .iter()
                .enumerate()
                .map(|(n, o)| ObjectLocation {
                    object_id: o.id(),
                    role: o.role(),
                    canonical_length: o.canonical_len(),
                    pack_id: 1,
                    group_number: n,
                    record_number: 0,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    objects
}
#[test]
fn explicit_creation_reopens_declared_units_and_keeps_sparse_canonical_order() {
    let t = support::Temp::new("unit-canonical");
    let path = t.join("db");
    let h = create(&path);
    assert_eq!(h.profile().pack_layout, SqlitePackLayout::GroupRows);
    let objects = fixture(&h);
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pack_unit", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        8
    );
    assert!(db.prepare("SELECT body FROM pack").is_err());
    drop(db);
    let storage = Storage::new(h.storage.clone()).unwrap();
    let reader = storage.reader().unwrap();
    let before = h.diagnostics().unwrap();
    assert_eq!(
        reader
            .read_objects(&[objects[7].id(), objects[1].id(), objects[7].id()])
            .unwrap(),
        vec![
            objects[7].canonical().to_vec(),
            objects[1].canonical().to_vec(),
            objects[7].canonical().to_vec()
        ]
    );
    let work = h.diagnostics().unwrap();
    assert!(work.blob_read_bytes - before.blob_read_bytes < 30000);
    assert_eq!(
        work.blob_open_calls - before.blob_open_calls,
        work.blob_close_calls - before.blob_close_calls
    );
    drop(reader);
    drop(storage);
    drop(h);
    let reopened = Handles::open_read_only(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        &config().binding_key,
        config().cursor_key,
    )
    .unwrap();
    assert_eq!(
        Storage::new(reopened.storage.clone())
            .unwrap()
            .reader()
            .unwrap()
            .read_objects(&[objects[0].id()])
            .unwrap(),
        vec![objects[0].canonical().to_vec()]
    );
}
#[test]
fn complete_reads_reassemble_exact_bytes_and_digest_for_every_lane() {
    let t = support::Temp::new("unit-audit-lanes");
    let h = create(&t.join("db"));
    for (n, lane) in [
        PackLane::Ordinary,
        PackLane::Native,
        PackLane::WholeFile,
        PackLane::PooledMetadata,
        PackLane::Singleton,
    ]
    .into_iter()
    .enumerate()
    {
        let body = assemble(
            lane,
            &[build_group(lane, &[vec![0x10; 6400]], None).unwrap()],
        )
        .unwrap();
        let id = n as i64 + 1;
        let info = PackInfo {
            pack_id: id,
            domain: PackDomain::for_lane(lane),
            key: ObjectKey::for_bytes(&body),
            length: body.len(),
        };
        let pack = PublishedPack {
            info,
            body: Arc::new(body.clone()),
        };
        assert_eq!(
            h.storage.publication_pack_cost(&pack).unwrap(),
            (2, body.len() as u64 + 88)
        );
        h.storage
            .publish(&Publication {
                packs: vec![pack],
                ..Publication::default()
            })
            .unwrap();
        let mut out = Vec::new();
        h.storage.read_packs(&[id], &mut out).unwrap();
        assert_eq!(out[0].info(), info);
        assert_eq!(out[0].body(), &body);
    }
}
#[test]
fn unread_corruption_is_scoped_and_whole_audit_refuses_it() {
    let t = support::Temp::new("unit-unread-corrupt");
    let path = t.join("db");
    let h = create(&path);
    let objects = fixture(&h);
    let db = rusqlite::Connection::open(&path).unwrap();
    let unit = db
        .query_row(
            "SELECT unit_id FROM pack_unit WHERE pack_id=1 AND group_number=7",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap();
    let mut blob = db
        .blob_open("main", "pack_unit", "body", unit, false)
        .unwrap();
    blob.write_at(&[0x7f], 0).unwrap();
    blob.close().unwrap();
    drop(db);
    assert_eq!(
        Storage::new(h.storage.clone())
            .unwrap()
            .reader()
            .unwrap()
            .read_objects(&[objects[0].id()])
            .unwrap(),
        vec![objects[0].canonical().to_vec()]
    );
    let mut out = Vec::new();
    assert!(h.storage.read_packs(&[1], &mut out).is_err());
    assert!(Storage::new(h.storage.clone())
        .unwrap()
        .reader()
        .unwrap()
        .read_objects(&[objects[7].id()])
        .is_err());
}
#[test]
fn malformed_unit_extent_is_refused_before_any_unit_blob_read() {
    let t = support::Temp::new("unit-misbinding");
    let path = t.join("db");
    let h = create(&path);
    let objects = fixture(&h);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER pack_unit_immutable_update;UPDATE pack_unit SET offset=offset+1 WHERE group_number=0").unwrap();
    drop(db);
    let before = h.diagnostics().unwrap();
    assert!(Storage::new(h.storage.clone())
        .unwrap()
        .reader()
        .unwrap()
        .read_objects(&[objects[0].id()])
        .is_err());
    assert_eq!(
        h.diagnostics().unwrap().blob_open_calls,
        before.blob_open_calls
    );
}
#[test]
fn late_locator_failure_rolls_back_control_and_every_unit() {
    let t = support::Temp::new("unit-rollback");
    let path = t.join("db");
    let h = create(&path);
    let body = assemble(
        PackLane::Ordinary,
        &[build_group(PackLane::Ordinary, &[vec![0; 64]], None).unwrap()],
    )
    .unwrap();
    let pack = PublishedPack {
        info: PackInfo {
            pack_id: 1,
            domain: PackDomain::Metadata,
            key: ObjectKey::for_bytes(&body),
            length: body.len(),
        },
        body: Arc::new(body),
    };
    let batch = Publication {
        packs: vec![pack],
        objects: vec![ObjectLocation {
            object_id: layerfs_content::ObjectId::for_bytes(b"missing"),
            role: ObjectRole::FileState,
            canonical_length: 1,
            pack_id: 999,
            group_number: 0,
            record_number: 0,
        }],
        ..Publication::default()
    };
    assert!(h.storage.publish(&batch).is_err());
    let db = rusqlite::Connection::open(&path).unwrap();
    for table in ["pack", "pack_unit", "object_location"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
#[test]
fn exact_physical_fanout_refuses_over_budget_batch_before_begin() {
    let t = support::Temp::new("unit-row-limit");
    let h = create(&t.join("db"));
    let groups = (0..256)
        .map(|_| build_group(PackLane::PooledMetadata, &[vec![0; 64]], None).unwrap())
        .collect::<Vec<_>>();
    let body = assemble(PackLane::PooledMetadata, &groups).unwrap();
    let pack = PublishedPack {
        info: PackInfo {
            pack_id: 1,
            domain: PackDomain::Metadata,
            key: ObjectKey::for_bytes(&body),
            length: body.len(),
        },
        body: Arc::new(body),
    };
    let batch = Publication {
        packs: vec![pack],
        objects: (0u64..8000)
            .map(|i| ObjectLocation {
                object_id: layerfs_content::ObjectId::for_bytes(&i.to_be_bytes()),
                role: ObjectRole::FileState,
                canonical_length: 1,
                pack_id: 1,
                group_number: 0,
                record_number: 0,
            })
            .collect(),
        ..Publication::default()
    };
    let before = h.diagnostics().unwrap();
    assert_eq!(h.storage.publish(&batch), Err(PersistenceError::Malformed));
    assert_eq!(h.diagnostics().unwrap().transactions, before.transactions);
}
