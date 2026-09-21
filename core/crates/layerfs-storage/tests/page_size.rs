//! The declared Store page size, and the Stores that do not have it.
//!
//! `page_size` is a property of the database **file header**, not of the schema,
//! the policy row or the pack framing: every page number in the file is read
//! against it, and the engine takes it from the file it opened. The product
//! declares the size a Store is *created* with (`STORE_PAGE_SIZE_BYTES`) and
//! never compares an opened Store to it, so a Store laid out at another page size
//! — including every Store created before the declaration existed — has to open,
//! validate, write and read like any other.
//!
//! These two cases are that claim: one reads the created file back with an
//! independent connection, the other relayouts a real Store at the previous
//! default with `VACUUM` and then uses it.

mod support;

use layerfs_storage::{Store, STORE_PAGE_SIZE_BYTES};
use support::{
    construct_file, create_store, open_store, patterned, read_objects, save_all, TempDir,
};

/// The engine's own answer for a file the product created.
fn engine_page_size(path: &std::path::Path) -> i64 {
    let connection = rusqlite::Connection::open(path).expect("independent connection");
    connection
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .expect("page size")
}

#[test]
fn a_created_store_is_larger_than_the_engine_default_page() {
    let dir = TempDir::new("page_size_created");
    let path = dir.store_path("created");
    let _store = create_store(&path);
    assert_eq!(
        engine_page_size(&path),
        STORE_PAGE_SIZE_BYTES as i64,
        "a created Store carries the declared page size"
    );
    // The engine's own default, for the contrast: a file this product did not
    // create, read on this host rather than quoted.
    let plain = dir.join("plain.sqlite");
    let raw = rusqlite::Connection::open(&plain).expect("plain connection");
    raw.execute_batch("CREATE TABLE t(x)").expect("plain table");
    let default = engine_page_size(&plain);
    drop(raw);
    assert_eq!(default, 4_096, "the engine's default page size");
    assert_ne!(
        STORE_PAGE_SIZE_BYTES as i64, default,
        "the declared page size is not the engine's default"
    );
}

#[test]
fn a_store_at_another_page_size_opens_writes_and_reads() {
    let dir = TempDir::new("page_size_legacy");
    let path = dir.store_path("legacy");
    let first = patterned(40_000);
    let (first_collected, first_root, _) = construct_file(&first);
    let store = create_store(&path);
    save_all(&store, &first_collected).expect("first save at the declared page size");

    // The Store every earlier build produced: the same schema, the same policy
    // row and the same pack bytes, laid out at the engine's 4096-byte default.
    // `VACUUM` is the only way to move an existing file's page size, and it
    // rewrites no row.
    let raw = rusqlite::Connection::open(&path).expect("relayout connection");
    raw.execute_batch("PRAGMA page_size = 4096; VACUUM;")
        .expect("relayout");
    drop(raw);
    assert_eq!(
        engine_page_size(&path),
        4_096,
        "the file now declares the previous page size"
    );

    let reopened = open_store(&path);
    let (values, _counters) = read_objects(&reopened, &[first_root]).expect("read back");
    assert_eq!(values[0], first_collected.objects()[0].2);

    let second = patterned(9_000);
    let (second_collected, second_root, _) = construct_file(&second);
    let outcome = save_all(&reopened, &second_collected).expect("write at 4096");
    assert_eq!(outcome.inserted, 1);
    assert_eq!(
        engine_page_size(&path),
        4_096,
        "a write does not move the file's page size"
    );
    let (values, _counters) = read_objects(&reopened, &[second_root]).expect("read the new object");
    assert_eq!(values[0], second_collected.objects()[0].2);
    assert!(Store::default_policy().small_file_threshold_bytes() > 0);
}
