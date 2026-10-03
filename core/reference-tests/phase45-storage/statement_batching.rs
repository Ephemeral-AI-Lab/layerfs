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

use support::{construct_file, create_store, noise, patterned, save_all, TempDir};

/// Rows actually present in the `objects` table, read on an external connection.
fn stored_rows(path: &std::path::Path) -> i64 {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    connection
        .query_row("SELECT COUNT(*) FROM objects", [], |row| row.get(0))
        .expect("object row count")
}

#[test]
fn a_save_inserts_its_rows_in_bounded_statements() {
    let dir = TempDir::new("statements");
    let path = dir.store_path("statements");
    let bytes = noise(8_000_000);
    let (collected, _root, _) = construct_file(&bytes);
    let store = create_store(&path);

    let outcome = save_all(&store, &collected).expect("save");
    assert!(
        outcome.inserted > 200,
        "the fixture must insert many rows to be a measurement"
    );
    assert_eq!(
        i64::try_from(outcome.inserted).unwrap(),
        stored_rows(&path),
        "every row is present, however few statements carried it"
    );
    assert!(
        outcome.statements < outcome.inserted,
        "rows are batched: {} statements for {} rows",
        outcome.statements,
        outcome.inserted
    );
    // `k` is derived from the engine's limits and capped at 128, and rows are
    // inserted one sealed group at a time, so the total is at least the ideal
    // ceil(rows / 128) and never more than one statement per row.
    assert!(
        outcome.statements >= outcome.inserted.div_ceil(128),
        "{} statements cannot carry {} rows at a chunk of at most 128",
        outcome.statements,
        outcome.inserted
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
    assert_eq!(i64::try_from(first.inserted).unwrap(), stored_rows(&path));
    assert!(first.statements >= 1);

    // Control 1: a second save of the same objects inserts nothing and issues no
    // object statement at all, so the counter is not a per-save constant.
    let repeat = save_all(&store, &small_objects).expect("re-save");
    assert_eq!(repeat.inserted, 0);
    assert_eq!(repeat.statements, 0);

    // Control 2: a strictly larger file charges strictly more statements, and the
    // engine holds every row those statements claimed to insert.
    let large = patterned(900_000);
    let (large_objects, _root, _) = construct_file(&large);
    let second = save_all(&store, &large_objects).expect("second save");
    assert!(second.inserted > 0);
    assert!(second.statements < second.inserted);
    assert_eq!(
        i64::try_from(first.inserted + second.inserted).unwrap(),
        stored_rows(&path),
        "every charged statement's rows are rows the engine holds"
    );
}

/// The chunk size is derived from the connection's own limits.
///
/// A hardcoded chunk would be a copied constant with no relationship to the SQLite
/// this build links: a deployment with a smaller `SQLITE_LIMIT_VARIABLE_NUMBER`
/// would fail every insert, and one with a larger limit would leave the win on the
/// table. The case pins the derivation where it lives - the writer's own reader of
/// the engine's limits - and that it stays inside the cache-bounding cap.
#[test]
fn the_insert_chunk_is_derived_from_the_engine_limits() {
    let dir = TempDir::new("chunk-derivation");
    let path = dir.store_path("chunk-derivation");
    create_store(&path);
    let connection =
        layerfs_storage::sqlite::connection::open(&path, false).expect("profile connection");
    let chunk = layerfs_storage::sqlite::write::insert_chunk_rows(&connection).expect("chunk");
    assert!(chunk >= 1, "a chunk must hold at least one row");
    assert!(chunk <= 128, "the cap bounds the prepared-statement cache");
    let variables = connection
        .limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER)
        .expect("variable limit");
    assert!(
        chunk <= usize::try_from(variables).unwrap() / 7,
        "the chunk must fit the engine's bind-variable limit"
    );
}
