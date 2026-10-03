//! The persisted cross-save content-signature index (#188d, W2).
//!
//! The index proposes a delta base by content rather than by path, so it reaches
//! a predecessor the caller never declared. These cases pin the two properties
//! that make it the *product's* capability rather than one save's: it outlives the
//! operation that filled it, and it outlives the handle.
//!
//! Every case observes the public save outcome and the public read wave. There is
//! no test hook in `src/`: `content_index_entries` is the live occupancy the
//! memory ledger reads.

mod support;

use layerfs_content::{FinalizedObject, ObjectRole};
use support::{
    assembled_small_object, create_store, noise, open_store, read_objects, save_one, TempDir,
};

fn whole(raw: &[u8]) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::WholeFile, assembled_small_object(raw)).expect("canonical")
}

/// The bytes of a second version of `raw`: one changed kilobyte in a hundred.
///
/// The payload is `noise`, not `patterned`: a periodic payload has only 256
/// distinct rolling windows, so a fresh kilobyte of bytes rewrites which eight of
/// them are the smallest and the two sketches share one hash rather than eight.
/// That is a property of the fixture, not of the index, and it is recorded here
/// because the first version of this case asserted the wrong thing for it.
fn near_copy(raw: &[u8]) -> Vec<u8> {
    let mut changed = raw.to_vec();
    for byte in &mut changed[40_000..41_000] {
        *byte ^= 0x5a;
    }
    changed
}

/// Rows the persisted table holds, read on an independent connection.
fn persisted_rows(path: &std::path::Path) -> i64 {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    connection
        .query_row("SELECT COUNT(*) FROM content_signatures", [], |row| {
            row.get(0)
        })
        .expect("row count")
}

#[test]
fn an_admitted_full_enters_the_index_and_is_written_in_the_saving_transaction() {
    let dir = TempDir::new("content-index-write");
    let path = dir.store_path("index");
    let store = create_store(&path);
    assert_eq!(
        store.content_index_entries(),
        0,
        "a fresh Store indexes nothing"
    );
    let outcome = save_one(&store, whole(&noise(100_000))).expect("base save");
    assert_eq!(outcome.full_records, 1);
    assert_eq!(
        store.content_index_entries(),
        1,
        "the admitted FULL entered"
    );
    assert_eq!(persisted_rows(&path), 1, "and the table carries it");
    // The structural size, and it does not depend on how many objects were
    // admitted: 8,192 slots of 68 bytes plus 65,536 references of 2 bytes. The
    // declared bound is `candidates::INDEX_BYTES` = 704 KiB, above this.
    assert_eq!(store.content_index_bytes(), 688_128);
}

#[test]
fn the_index_outlives_the_handle_that_filled_it() {
    let dir = TempDir::new("content-index-reopen");
    let path = dir.store_path("index");
    let raw = noise(100_000);
    {
        let store = create_store(&path);
        let outcome = save_one(&store, whole(&raw)).expect("base save");
        assert_eq!(outcome.full_records, 1);
        assert_eq!(store.content_index_entries(), 1);
    }
    // A new handle with no memory in common with the one that saved.
    let reopened = open_store(&path);
    assert_eq!(
        reopened.content_index_entries(),
        1,
        "Store::open read the index back"
    );
    // No declared predecessor: the Store's own index is the only route to a base.
    let outcome = save_one(&reopened, whole(&near_copy(&raw))).expect("second save");
    assert_eq!(
        outcome.delta.no_candidate, 0,
        "the index proposed something"
    );
    assert_eq!(outcome.delta.trials, 1, "exactly one trial");
    assert_eq!(
        outcome.prefix_records, 1,
        "the reopened index supplied the base"
    );
    assert_eq!(outcome.full_records, 0);
}

#[test]
fn the_index_is_cross_save_within_one_handle_as_well() {
    let dir = TempDir::new("content-index-cross-save");
    let path = dir.store_path("index");
    let store = create_store(&path);
    let raw = noise(100_000);
    save_one(&store, whole(&raw)).expect("base save");
    // The second save shares no transaction and no operation with the first;
    // before the index was Store-owned this was FULL by construction.
    let outcome = save_one(&store, whole(&near_copy(&raw))).expect("second save");
    assert_eq!(outcome.prefix_records, 1);
    assert_eq!(outcome.delta.no_candidate, 0);
}

#[test]
fn an_abandoned_save_leaves_no_entry_behind() {
    let dir = TempDir::new("content-index-abandon");
    let path = dir.store_path("index");
    let raw = noise(100_000);
    {
        let store = create_store(&path);
        let object = whole(&raw);
        let mut operation =
            support::disabled(|scope| store.begin_save(scope.child("storage.begin")))
                .expect("begin");
        operation.accept(object).expect("accept");
        support::disabled(|scope| operation.abort(scope.child("storage.abort"))).expect("abort");
        assert_eq!(
            store.content_index_entries(),
            0,
            "the abandoned save dropped the entry it admitted"
        );
    }
    assert_eq!(persisted_rows(&path), 0, "and the table was never written");
    // The Store is still usable and still answers from an empty index.
    let reopened = open_store(&path);
    assert_eq!(reopened.content_index_entries(), 0);
    let outcome = save_one(&reopened, whole(&near_copy(&raw))).expect("save after abandon");
    assert_eq!(
        outcome.full_records, 1,
        "nothing was proposed, so FULL by policy"
    );
    assert_eq!(outcome.delta.no_candidate, 1);
}

#[test]
fn a_stored_candidate_still_reads_back_through_the_reopened_index() {
    let dir = TempDir::new("content-index-read");
    let path = dir.store_path("index");
    let raw = noise(100_000);
    let changed = near_copy(&raw);
    let base = whole(&raw);
    let base_id = base.id();
    let dependent = whole(&changed);
    let dependent_id = dependent.id();
    {
        let store = create_store(&path);
        save_one(&store, base).expect("base save");
        let outcome = save_one(&store, dependent).expect("dependent save");
        assert_eq!(outcome.prefix_records, 1);
    }
    let reopened = open_store(&path);
    let (values, counters) = read_objects(&reopened, &[dependent_id]).expect("read");
    assert_eq!(values[0], assembled_small_object(&changed));
    assert_eq!(counters.edges, 1, "the index-proposed base is a real edge");
    let (base_values, _) = read_objects(&reopened, &[base_id]).expect("base read");
    assert_eq!(base_values[0], assembled_small_object(&raw));
}
