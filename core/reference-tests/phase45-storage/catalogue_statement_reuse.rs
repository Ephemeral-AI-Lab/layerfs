//! Catalogue statement preparation, fresh results and unchanged refusals.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::sqlite::{connection, pool::group_for};
use layerfs_storage::StorageError;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use support::{create_store, save_one, TempDir};

#[test]
fn catalogue_preparation_preserves_current_rows_parameters_and_errors() {
    let dir = TempDir::new("catalogue-statement");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    for group in 0..2_u8 {
        let rows = (0..2_u8)
            .map(|index| InodeLeafRow {
                serial: u64::from(index) + 1,
                value: encode_inode_value(InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(&[group, index]),
                    metadata_root: ObjectId::for_bytes(&[0]),
                }),
            })
            .collect();
        let bytes = InodeLeaf {
            rows,
            subtree_bytes: 2 * LEAF_ROW_BYTES as u64,
        }
        .encode()
        .unwrap();
        save_one(
            &store,
            FinalizedObject::new(ObjectRole::InodeLeaf, bytes).unwrap(),
        )
        .unwrap();
    }
    let connection = connection::open(&path, false).unwrap();
    let preparations = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&preparations);
    // Register once, after setup. Replacing the hook could invalidate cached
    // statements; this callback counts preparation, not query execution.
    connection
        .authorizer(Some(move |context: AuthContext<'_>| {
            if matches!(context.action, AuthAction::Select) {
                observed.fetch_add(1, Ordering::Relaxed);
            }
            Authorization::Allow
        }))
        .unwrap();

    // One preparation of the covering-row query issues two SELECT actions: the
    // outer statement and its `MAX(first_ordinal)` subquery. That subquery is what
    // keeps an unpublished covering group from silently falling back to an older
    // visible one, so the events per preparation are counted, not assumed to be one.
    const PER_PREPARATION: usize = 2;
    let first = group_for(&connection, 1).unwrap().unwrap();
    assert_eq!((first.first_ordinal, first.count), (1, 2));
    let replacement = ObjectId::for_bytes(b"new catalogue digest");
    connection
        .execute(
            "UPDATE metadata_value_groups SET digest = ?1 WHERE first_ordinal = 1",
            [replacement.as_bytes().as_slice()],
        )
        .unwrap();
    let updated = group_for(&connection, 2).unwrap().unwrap();
    assert_eq!(updated.first_ordinal, first.first_ordinal);
    assert_eq!(
        updated.digest, replacement,
        "reuse must not cache row contents"
    );
    assert_eq!(preparations.load(Ordering::Relaxed), PER_PREPARATION);

    let second = group_for(&connection, 3).unwrap().unwrap();
    assert_eq!((second.first_ordinal, second.count), (3, 2));
    assert_ne!(
        second.digest, replacement,
        "each call binds its own ordinal"
    );
    assert!(matches!(
        group_for(&connection, 5),
        Err(StorageError::Integrity("metadata ordinal range"))
    ));
    connection
        .execute("DELETE FROM metadata_value_groups", [])
        .unwrap();
    assert_eq!(group_for(&connection, 1).unwrap(), None);
    let before_invalid = preparations.load(Ordering::Relaxed);
    assert!(matches!(
        group_for(&connection, 0),
        Err(StorageError::Integrity("metadata ordinal"))
    ));
    assert_eq!(preparations.load(Ordering::Relaxed), before_invalid);
    assert_eq!(before_invalid, PER_PREPARATION);
    layerfs_storage::sqlite::pool::insert_group(&connection, &first).unwrap();
    assert_eq!(group_for(&connection, 1).unwrap(), Some(first));
    assert_eq!(preparations.load(Ordering::Relaxed), PER_PREPARATION);
    println!(
        "catalogue SELECT preparation events={PER_PREPARATION} for six helper queries, \
         i.e. one preparation; inserted row visible after absence"
    );
}
