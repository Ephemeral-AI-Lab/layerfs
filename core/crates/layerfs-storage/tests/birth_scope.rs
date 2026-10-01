//! Real compatibility birth scope and private/public visibility, no engine qualification.
mod support;
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{StorageError, Store};
use support::{create_store, disabled, TempDir};
fn object(owner: u8, ordinal: u64) -> (FinalizedObject, Vec<u8>, ObjectId) {
    let mut payload = vec![owner];
    payload.extend_from_slice(&ordinal.to_be_bytes());
    let canonical = layerfs_content::encode_whole_file_payload(&payload).unwrap();
    let id = ObjectId::for_bytes(&canonical);
    (
        FinalizedObject::new(ObjectRole::WholeFile, canonical.clone()).unwrap(),
        canonical,
        id,
    )
}
#[test]
fn known_birth_binds_actual_private_read_scope_before_two_owners_can_read() {
    let temp = TempDir::new("birth-scope");
    let path = temp.store_path("birth-scope");
    let store = create_store(&path);
    assert!(
        store.engine_guard().is_none(),
        "explicit compatibility proof does not forge a guard"
    );
    assert_eq!(store.capacities().batch_objects, 512);
    let mut left = disabled(|s| store.begin_save(s.child("left"))).unwrap();
    let mut right = disabled(|s| store.begin_save(s.child("right"))).unwrap();
    // Independent 513 offers force the frozen512-object batch to leave the
    // first identity in native storage, rather than serving it from pending.
    for ordinal in 0..513 {
        left.accept(object(41, ordinal).0).unwrap();
        right.accept(object(42, ordinal).0).unwrap();
    }
    assert_eq!(left.pending().0, 1);
    assert_eq!(right.pending().0, 1);
    let (_, a, aid) = object(41, 0);
    let (_, b, bid) = object(42, 0);
    assert_eq!(
        disabled(|s| left.read_batch(&[aid], s.child("private-left"))).unwrap(),
        vec![a]
    );
    assert_eq!(
        disabled(|s| right.read_batch(&[bid], s.child("private-right"))).unwrap(),
        vec![b]
    );
    // Both own first objects now have real committed private locators. A
    // missing/foreign temp binding cannot make these own reads succeed.
    let independent =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let active: i64 = independent
        .query_row(
            "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active, 2);
    let rows: i64 = independent
        .query_row(
            "SELECT count(*) FROM objects WHERE object_id IN (?1,?2)",
            rusqlite::params![aid.as_bytes().as_slice(), bid.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 2);
    drop(independent);
    for id in [aid, bid] {
        assert!(
            matches!(disabled(|s|store.read_batch(&[id],s.child("public"))),Err(StorageError::Unpublished(found)) if found==id)
        );
    }
    assert!(disabled(|s| left.read_batch(&[bid], s.child("foreign-right"))).is_err());
    assert!(disabled(|s| right.read_batch(&[aid], s.child("foreign-left"))).is_err());
    disabled(|s| left.abort(s.child("abort-left"))).unwrap();
    disabled(|s| right.abort(s.child("abort-right"))).unwrap();
    let reopened = disabled(|s| Store::open(&path, s.child("reopen"))).unwrap();
    assert_eq!(
        disabled(|s| reopened.contains(&[aid, bid], s.child("absent"))).unwrap(),
        Vec::<ObjectId>::new()
    );
    assert!(reopened.engine_guard().is_none());
}
