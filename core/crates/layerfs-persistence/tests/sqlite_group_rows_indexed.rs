//! Schema3 covering-index creation, integrity and legacy-open behavior.
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
fn create(path: &std::path::Path, layout: SqlitePackLayout) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(path)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_pack_layout(layout),
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
fn declared_layouts_reopen_without_mutating_or_promoting_their_schema() {
    for (layout, version) in [
        (SqlitePackLayout::Monolithic, 1),
        (SqlitePackLayout::GroupRows, 2),
        (SqlitePackLayout::GroupRowsIndexed, 3),
    ] {
        let t = support::Temp::new("indexed-legacy-open");
        let path = t.join("db");
        let h = create(&path, layout);
        let objects = fixture(&h);
        drop(h);
        let before = std::fs::read(&path).unwrap();
        let h = Handles::open_read_only(
            PersistenceConfig::sqlite(&path)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
                .with_sqlite_pack_layout(SqlitePackLayout::GroupRowsIndexed),
            &config().binding_key,
            config().cursor_key,
        )
        .unwrap();
        assert_eq!(h.profile().pack_layout, layout);
        assert_eq!(
            Storage::new(h.storage.clone())
                .unwrap()
                .reader()
                .unwrap()
                .read_objects(&[objects[7].id(), objects[1].id(), objects[7].id()])
                .unwrap(),
            vec![
                objects[7].canonical().to_vec(),
                objects[1].canonical().to_vec(),
                objects[7].canonical().to_vec()
            ]
        );
        drop(h);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let db = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            version
        );
        let index: i64 = db
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='pack_unit_mapping'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(index, i64::from(version == 3));
    }
}
#[test]
fn mapping_is_covered_and_complete_pack_reassembles_exactly() {
    let t = support::Temp::new("indexed-cover");
    let path = t.join("db");
    let h = create(&path, SqlitePackLayout::GroupRowsIndexed);
    fixture(&h);
    let db = rusqlite::Connection::open(&path).unwrap();
    let plan: String = db.query_row("EXPLAIN QUERY PLAN SELECT unit_id,group_number,offset,length FROM pack_unit WHERE pack_id=?1 ORDER BY group_number LIMIT 257", [1], |r|r.get(3)).unwrap();
    assert!(plan.contains("COVERING INDEX pack_unit_mapping"), "{plan}");
    let control: Vec<u8> = db
        .query_row("SELECT control FROM pack WHERE pack_id=1", [], |r| r.get(0))
        .unwrap();
    let mut expected = control;
    let mut q = db
        .prepare("SELECT body FROM pack_unit WHERE pack_id=1 ORDER BY group_number")
        .unwrap();
    for body in q.query_map([], |r| r.get::<_, Vec<u8>>(0)).unwrap() {
        expected.extend(body.unwrap());
    }
    let mut packs = Vec::new();
    h.storage.read_packs(&[1], &mut packs).unwrap();
    let got = packs.remove(0);
    assert_eq!(got.body(), &expected);
    assert_eq!(got.info().key, ObjectKey::for_bytes(&expected));
}
#[test]
fn missing_covering_index_is_refused_at_open_without_automatic_repair() {
    let t = support::Temp::new("indexed-schema-binding");
    let path = t.join("db");
    let h = create(&path, SqlitePackLayout::GroupRowsIndexed);
    drop(h);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP INDEX pack_unit_mapping").unwrap();
    drop(db);
    let before = std::fs::read(&path).unwrap();
    assert!(Handles::open_read_only(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        &config().binding_key,
        config().cursor_key
    )
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn covered_mapping_still_refuses_wrong_extents_before_blob_open() {
    let t = support::Temp::new("indexed-misbinding");
    let path = t.join("db");
    let h = create(&path, SqlitePackLayout::GroupRowsIndexed);
    fixture(&h);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER pack_unit_immutable_update;UPDATE pack_unit SET offset=offset+1 WHERE group_number=7").unwrap();
    drop(db);
    let before = h.diagnostics().unwrap();
    assert!(h.storage.read_packs(&[1], &mut Vec::new()).is_err());
    assert_eq!(
        h.diagnostics().unwrap().blob_open_calls,
        before.blob_open_calls
    );
}
