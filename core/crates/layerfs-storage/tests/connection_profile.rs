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
use support::{create_store, TempDir};

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
