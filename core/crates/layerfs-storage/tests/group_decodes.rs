//! The ordinary-lane group-decode counter.
//!
//! The ordinary resolver decompresses a group's whole body once per record it
//! serves out of it (`encoding/decode.rs`, the `GroupCodec::Zstandard` arm), so
//! `k` records sharing one group cost `k` decompressions. `ReadCounters` had no
//! counter that could see that: `packs_read` charges one per pack per wave and
//! `objects` charges one per returned object, which is exactly the number the
//! defect hides behind. These cases pin the pre-cache behaviour - the decode
//! count is the number of ordinary records read, and that is strictly more than
//! the number of distinct groups they came from - against the engine's own
//! `objects` table, so the decoded-group cache (`P2-4`) has a before-anchor.

mod support;

use layerfs_content::read_all;
use layerfs_storage::StoreProvider;
use support::{construct_file, create_store, disabled, patterned, save_all, TempDir};

/// `(ordinary-lane records, distinct groups they occupy)` in the store.
///
/// Ordinary-lane roles are every role whose record the ordinary resolver decodes:
/// whole-file (1) and chunk (2) records use their own lanes, and inode leaves (6)
/// are served by the pooled reader, which has its own cache and its own charge.
fn ordinary_shape(path: &std::path::Path) -> (i64, i64) {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    connection
        .query_row(
            "SELECT COUNT(*), COUNT(DISTINCT pack_id * 1000000 + group_number) \
             FROM objects WHERE object_role IN (3,4,5,7,8,9,10,11,12,13)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("ordinary-lane shape")
}

#[test]
fn a_decode_is_charged_once_per_record_and_not_once_per_group() {
    let dir = TempDir::new("group-decodes");
    let path = dir.store_path("group-decodes");
    // A chunked file: its mapping records are small, so several of them share one
    // ordinary-lane group.
    let bytes = patterned(1_200_000);
    let (collected, root, _) = construct_file(&bytes);
    let store = create_store(&path);
    save_all(&store, &collected).expect("save");

    let (records, groups) = ordinary_shape(&path);
    assert!(
        records > groups,
        "the fixture must put several ordinary records in one group ({records} records, {groups} groups)"
    );

    let provider = StoreProvider::new(&store);
    let mut out = Vec::new();
    disabled(|scope| read_all(&provider, root, &mut out, scope.child("content.read")))
        .expect("readback");
    assert_eq!(out, bytes, "the readback reproduces the file");
    assert_eq!(
        provider.group_decodes(),
        u64::try_from(records).unwrap(),
        "one decode per ordinary record read, not one per distinct group"
    );
    assert!(
        provider.group_decodes() > u64::try_from(groups).unwrap(),
        "the counter is strictly above the distinct-group count, which is the defect P2-4 removes"
    );
}

#[test]
fn the_decode_charge_is_per_read_and_not_per_store() {
    let dir = TempDir::new("group-decodes-control");
    let path = dir.store_path("group-decodes-control");
    let bytes = patterned(400_000);
    let (collected, root, _) = construct_file(&bytes);
    let store = create_store(&path);
    save_all(&store, &collected).expect("save");

    // Control: a save of the same objects reads them back for membership, but the
    // read counter belongs to *reads*; a second provider over the same Store
    // charges its own read again rather than reporting a Store-wide total.
    let first = StoreProvider::new(&store);
    let mut out = Vec::new();
    disabled(|scope| read_all(&first, root, &mut out, scope.child("content.read"))).unwrap();
    let charged = first.group_decodes();
    assert!(charged > 0, "the read decompressed at least one group");

    let second = StoreProvider::new(&store);
    let mut again = Vec::new();
    disabled(|scope| read_all(&second, root, &mut again, scope.child("content.read"))).unwrap();
    assert_eq!(again, out);
    assert_eq!(
        second.group_decodes(),
        charged,
        "a fresh read charges the same decodes: the charge is the read's, not a lifetime total"
    );

    // The per-wave figure is available as well, so a caller can see how one
    // wave's work decomposes instead of only the operation's total.
    let (_values, counters) = disabled(|scope| {
        StoreProvider::new(&store)
            .read_wave(std::slice::from_ref(&root), scope.child("storage.read"))
    })
    .unwrap();
    assert_eq!(
        counters.group_decodes, 1,
        "one record read is one group decompression"
    );
}
