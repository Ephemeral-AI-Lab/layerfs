//! The connection profile is re-verified on acquisition, not trusted.
//!
//! `Store::open` applies the declared profile (MEMORY journal, synchronous OFF,
//! foreign keys ON, zero busy timeout). A connection that reaches an owner by
//! any other route is verified against the same profile before the first
//! write, so a caller-supplied connection with another profile is refused
//! instead of silently writing under different persistence semantics.

mod support;

use layerfs_storage::sqlite::connection::{configure, verify_profile};
use layerfs_storage::StorageError;
use support::{construct_file, create_store, disabled, patterned, TempDir};

#[test]
fn an_unconfigured_connection_is_refused_by_profile_verification() {
    let dir = TempDir::new("profile_default");
    let path = dir.store_path("profile_default");
    create_store(&path);

    // A fresh connection to the same database carries the engine defaults
    // (rollback journal on disk, synchronous FULL, foreign keys OFF), not the
    // declared profile: MEMORY journal mode is per connection.
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    match verify_profile(&connection) {
        Err(StorageError::Integrity(what)) => assert_eq!(what, "journal mode"),
        Err(other) => panic!("expected a journal-mode refusal, got {other}"),
        Ok(()) => panic!("an unconfigured connection passed profile verification"),
    }
}

#[test]
fn a_configured_connection_passes_profile_verification() {
    let dir = TempDir::new("profile_configured");
    let path = dir.store_path("profile_configured");
    create_store(&path);

    let connection = rusqlite::Connection::open(&path).expect("external connection");
    configure(&connection).expect("configure");
    verify_profile(&connection).expect("the declared profile verifies");
}

#[test]
fn a_degraded_connection_is_refused_by_profile_verification() {
    let dir = TempDir::new("profile_degraded");
    let path = dir.store_path("profile_degraded");
    create_store(&path);

    let connection = rusqlite::Connection::open(&path).expect("external connection");
    configure(&connection).expect("configure");
    connection
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .expect("degrade");
    match verify_profile(&connection) {
        Err(StorageError::Integrity(what)) => assert_eq!(what, "foreign key enforcement"),
        Err(other) => panic!("expected a foreign-key refusal, got {other}"),
        Ok(()) => panic!("a degraded connection passed profile verification"),
    }
}

#[test]
fn a_connection_with_a_busy_timeout_is_refused_by_profile_verification() {
    let dir = TempDir::new("profile_busy");
    let path = dir.store_path("profile_busy");
    create_store(&path);

    let connection = rusqlite::Connection::open(&path).expect("external connection");
    configure(&connection).expect("configure");
    connection
        .busy_timeout(std::time::Duration::from_secs(1))
        .expect("set a busy timeout");
    match verify_profile(&connection) {
        Err(StorageError::Integrity(what)) => assert_eq!(what, "busy timeout"),
        Err(other) => panic!("expected a busy-timeout refusal, got {other}"),
        Ok(()) => panic!("a waiting connection passed profile verification"),
    }
}

/// The save reports the cache profile of **its own** connection.
///
/// P0-2 recorded the limit this case lifts: `MutationOwner` is private and
/// `SaveOperation` exposed no connection, so a statement about the cache a save
/// ran under could only be made by replaying the row shape on a harness-owned
/// connection. The accessor reads the pragmas back on the connection that
/// performs the save.
///
/// What it deliberately cannot report is the page-cache spill counter: reading
/// `SQLITE_DBSTATUS_CACHE_SPILL` needs an FFI call, and this crate allows
/// `unsafe` in exactly one audited module (`encoding::codec`). The case pins what
/// *is* observable and says so, rather than pinning a value the profile item
/// (`P2-1`) will legitimately change.
#[test]
fn a_save_reports_the_cache_profile_of_its_own_connection() {
    let dir = TempDir::new("save_profile");
    let path = dir.store_path("save_profile");
    let bytes = patterned(300_000);
    let (collected, _root, _) = construct_file(&bytes);
    let store = create_store(&path);

    let profile = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in collected.finalized() {
            operation.accept(object)?;
        }
        let profile = operation.connection_profile()?;
        let _outcome = operation.finish(scope.child("storage.finish"))?;
        Ok::<_, StorageError>(profile)
    })
    .expect("save with a profile reading");

    // The page size is the database's own, and the engine reports it.
    assert_eq!(profile.page_size, 4_096, "the store's page size");
    // The cache setting is the engine's own answer, whichever profile is
    // declared: it is never zero, and SQLite's signed form is negative or a
    // positive page count.
    assert_ne!(
        profile.cache_size, 0,
        "the engine always reports a cache size"
    );
    // `cache_spill` is SQLite's threshold encoding: 0 means spilling is off, a
    // positive value is the page threshold. The declared profile decides which,
    // so the case pins the encoding and not the policy.
    assert!(
        profile.cache_spill >= 0,
        "cache_spill is 0 or a page threshold, read {}",
        profile.cache_spill
    );
    assert_eq!(profile.mmap_size, 0, "the profile does not set mmap_size");
}

#[test]
fn the_profile_reading_agrees_with_the_store_itself() {
    let dir = TempDir::new("save_profile_agrees");
    let path = dir.store_path("save_profile_agrees");
    let store = create_store(&path);

    let profile = disabled(|scope| {
        let operation = store.begin_save(scope.child("storage.begin"))?;
        let profile = operation.connection_profile()?;
        let _outcome = operation.finish(scope.child("storage.finish"))?;
        Ok::<_, StorageError>(profile)
    })
    .expect("empty save with a profile reading");

    // An independent connection through the product's own open path reports the
    // same profile: the accessor is reading the connection, not a literal.
    let independent = layerfs_storage::sqlite::connection::open(&path, false).expect("open");
    let page_size: i64 = independent
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .expect("page size");
    let cache_size: i64 = independent
        .query_row("PRAGMA cache_size", [], |row| row.get(0))
        .expect("cache size");
    let cache_spill: i64 = independent
        .query_row("PRAGMA cache_spill", [], |row| row.get(0))
        .expect("cache spill");
    assert_eq!(profile.page_size, page_size);
    assert_eq!(profile.cache_size, cache_size);
    assert_eq!(profile.cache_spill, cache_spill);
}
