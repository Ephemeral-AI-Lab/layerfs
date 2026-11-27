//! Actual raw SQLite error identity, not a product rollback qualification.
//! The caller's outcome wrapper must keep its uncertainty when its nested
//! cause is a real capacity error; there is no product error injection.
use layerfs_overlay::OverlayError;

#[test]
fn a_real_sqlite_full_identity_never_unwraps_an_uncertain_outcome() {
    let raw = rusqlite::Connection::open_in_memory().unwrap();
    raw.execute_batch(
        "PRAGMA journal_mode=MEMORY;
         PRAGMA synchronous=OFF;
         PRAGMA max_page_count=2;
         CREATE TABLE capacity(value BLOB);",
    )
    .unwrap();
    let failed = raw
        .execute("INSERT INTO capacity VALUES(zeroblob(65536))", [])
        .unwrap_err();
    assert_eq!(
        failed.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    let direct = OverlayError::Sql(failed);
    assert!(direct.is_sqlite_full());
    let uncertain = OverlayError::Uncertain {
        cause: Box::new(direct),
        completion: None,
    };
    assert!(!uncertain.is_sqlite_full());
    let unrelated = raw
        .execute("INSERT INTO missing VALUES(1)", [])
        .unwrap_err();
    assert!(!OverlayError::Sql(unrelated).is_sqlite_full());
}
