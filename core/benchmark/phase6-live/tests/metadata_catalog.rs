//! Actual two-table SQLite catalog. The API owns no MinIO client or content reader.
use layerfs_content::{ObjectId, ObjectRole};
use phase6_live_probe::{
    metadata_catalog::{self, LocatorDb},
    objects::{Locator, Locators},
};
use std::path::PathBuf;
fn fresh() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "p6-locators-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
fn row(i: u32) -> Locator {
    Locator {
        id: ObjectId::for_bytes(&i.to_be_bytes()),
        role: ObjectRole::WholeFile,
        length: 32,
        pack: [3; 32],
        group: 0,
        record: i,
        pack_id: 0,
    }
}
fn counts(path: &std::path::Path) -> (i64, i64) {
    let db = rusqlite::Connection::open(path).unwrap();
    (
        db.query_row("SELECT count(*) FROM objects", [], |r| r.get(0))
            .unwrap(),
        db.query_row("SELECT count(*) FROM packs", [], |r| r.get(0))
            .unwrap(),
    )
}
#[test]
fn two_table_profile_and_readonly_query_exactly_preserve_identity() {
    let p = fresh();
    let path = p.join("locators.sqlite");
    let db = LocatorDb::create(&path).unwrap();
    let input = row(1);
    let saved = db.register_many(std::slice::from_ref(&input)).unwrap();
    assert!(saved[0].pack_id > 0);
    drop(db);
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.pragma_query_value(None, "application_id", |r| r.get::<_, u32>(0))
            .unwrap(),
        metadata_catalog::APPLICATION_ID
    );
    assert_eq!(
        sql.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    drop(sql);
    let read = LocatorDb::open_read_only(&path).unwrap();
    assert_eq!(read.lookup(input.id).unwrap().unwrap().pack, saved[0].pack);
    assert!(read.register_many(&[row(2)]).is_err());
    drop(read);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.pragma_update(None, "user_version", 1).unwrap();
    drop(sql);
    assert!(LocatorDb::open_read_only(&path).is_err());
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn invalid_page_and_conflicting_identity_refuse_before_effects() {
    let p = fresh();
    let path = p.join("locators.sqlite");
    let db = LocatorDb::create(&path).unwrap();
    let mut invalid = row(2);
    invalid.length = 0;
    assert!(db.register_many(&[row(1), invalid]).is_err());
    assert_eq!(counts(&path), (0, 0));
    let mut other = row(1);
    other.length = 33;
    assert!(db.register_many(&[row(1), other]).is_err());
    assert_eq!(counts(&path), (0, 0));
    db.register_many(&[row(1)]).unwrap();
    let mut conflict = row(1);
    conflict.role = ObjectRole::Chunk;
    assert!(db.register_many(&[row(2), conflict]).is_err());
    assert_eq!(counts(&path), (1, 1));
    assert!(db.register_many(&[]).is_err());
    assert!(db
        .register_many(&(0..129).map(row).collect::<Vec<_>>())
        .is_err());
    assert_eq!(counts(&path), (1, 1));
    drop(db);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn first_locator_wins_alternate_pack_and_same_page_duplicates() {
    let p = fresh();
    let path = p.join("locators.sqlite");
    let db = LocatorDb::create(&path).unwrap();
    let first = db.register_many(&[row(1)]).unwrap().remove(0);
    let mut alternate = row(1);
    alternate.pack = [4; 32];
    alternate.record = 9;
    let result = db.register_many(&[alternate.clone(), alternate]).unwrap();
    assert!(result
        .iter()
        .all(|r| r.pack == first.pack && r.pack_id == first.pack_id && r.record == first.record));
    assert_eq!(counts(&path), (1, 1));
    let fresh = row(2);
    let mut duplicate = fresh.clone();
    duplicate.pack = [5; 32];
    let selected = db.register_many(&[fresh.clone(), duplicate]).unwrap();
    assert!(selected.iter().all(|r| r.pack == fresh.pack));
    assert_eq!(counts(&path), (2, 1));
    drop(db);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn coordinate_conflict_rolls_back_exact_page_and_pack_insertion() {
    let p = fresh();
    let path = p.join("locators.sqlite");
    let db = LocatorDb::create(&path).unwrap();
    db.register_many(&[row(1)]).unwrap();
    let mut collision = row(2);
    collision.record = 1;
    let mut preceding = row(3);
    preceding.pack = [8; 32];
    assert!(db.register_many(&[preceding, collision]).is_err());
    assert_eq!(counts(&path), (1, 1));
    assert!(db.lookup(row(3).id).unwrap().is_none());
    drop(db);
    std::fs::remove_dir_all(p).unwrap();
}
