use layerfs_core::ObjectId;
use layerfs_storage_core::{
    object_batches, CanonicalObject, MissingBitmap, SchemaKind, StoreDb, OBJECT_BATCH_BYTES,
};

#[test]
fn greedy_packer_honors_count_and_bytes() {
    let object = |len| CanonicalObject {
        id: ObjectId::for_bytes(&vec![0; len]),
        bytes: vec![0; len],
    };
    let objects = vec![object(OBJECT_BATCH_BYTES - 1), object(2), object(2)];
    assert_eq!(
        object_batches(&objects)
            .unwrap()
            .iter()
            .map(|batch| batch.len())
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn bitmap_tail_is_fixed_and_checked() {
    let bitmap = MissingBitmap::from_missing(3, |index| index == 1).unwrap();
    assert_eq!(bitmap.as_bytes().len(), 64);
    assert!(bitmap.is_missing(1).unwrap());
    assert!(bitmap.validate_tail(3).is_ok());
}

#[test]
fn valid_oversize_singleton_is_admitted() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-oversize-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let db = StoreDb::open(root.join("store.sqlite"), SchemaKind::Full).unwrap();
    let object = CanonicalObject::new(
        layerfs_core::encode_bytes_object(&vec![7; OBJECT_BATCH_BYTES + 1]).unwrap(),
    )
    .unwrap();
    assert!(object.bytes.len() > OBJECT_BATCH_BYTES);
    db.transfer_exchange(&[object], &[], &[], None, true)
        .unwrap();
    assert_eq!(
        rusqlite::Connection::open(root.join("store.sqlite"))
            .unwrap()
            .query_row("SELECT count(*) FROM objects", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}
