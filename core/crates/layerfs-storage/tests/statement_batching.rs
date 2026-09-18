//! The statement counter: one `INSERT` per object row today.
//!
//! `SaveOutcome::statements` counts the SQL statements that inserted object rows.
//! It exists because the INSERT-batching item (`P2-2`) has to be *measured*, not
//! argued: `inserted` counts rows and cannot see a change that keeps every row
//! and drops the statement count. These cases pin the pre-batching identity
//! (`statements == inserted` for a save that reuses nothing) against the engine's
//! own table, and prove the counter is live with two controls - a save that
//! doubles the rows doubles the statements, and a save that inserts nothing
//! charges nothing.

mod support;

use support::{construct_file, create_store, patterned, save_all, TempDir};

/// Rows actually present in the `objects` table, read on an external connection.
fn stored_rows(path: &std::path::Path) -> i64 {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    connection
        .query_row("SELECT COUNT(*) FROM objects", [], |row| row.get(0))
        .expect("object row count")
}

#[test]
fn a_save_charges_one_statement_per_inserted_row() {
    let dir = TempDir::new("statements");
    let path = dir.store_path("statements");
    let bytes = patterned(300_000);
    let (collected, _root, _) = construct_file(&bytes);
    let store = create_store(&path);

    let outcome = save_all(&store, &collected).expect("save");
    assert!(
        outcome.inserted > 1,
        "the fixture must insert several rows to be a measurement"
    );
    assert_eq!(
        outcome.statements, outcome.inserted,
        "before batching every inserted row is its own INSERT statement"
    );
    assert_eq!(
        i64::try_from(outcome.statements).unwrap(),
        stored_rows(&path),
        "the charge agrees with the engine's own row count"
    );
}

#[test]
fn the_statement_count_tracks_the_rows_and_not_a_constant() {
    let dir = TempDir::new("statements-control");
    let path = dir.store_path("statements-control");
    let small = patterned(120_000);
    let (small_objects, _root, _) = construct_file(&small);
    let store = create_store(&path);

    let first = save_all(&store, &small_objects).expect("first save");
    assert_eq!(first.statements, first.inserted);

    // Control 1: a second save of the same objects inserts nothing and issues no
    // object statement at all, so the counter is not a per-save constant.
    let repeat = save_all(&store, &small_objects).expect("re-save");
    assert_eq!(repeat.inserted, 0);
    assert_eq!(repeat.statements, 0);

    // Control 2: a strictly larger file charges strictly more statements, and
    // exactly as many more as the rows it added.
    let large = patterned(900_000);
    let (large_objects, _root, _) = construct_file(&large);
    let second = save_all(&store, &large_objects).expect("second save");
    assert!(second.inserted > 0);
    assert_eq!(second.statements, second.inserted);
    assert_eq!(
        i64::try_from(first.statements + second.statements).unwrap(),
        stored_rows(&path),
        "every charged statement is a row the engine holds"
    );
}
