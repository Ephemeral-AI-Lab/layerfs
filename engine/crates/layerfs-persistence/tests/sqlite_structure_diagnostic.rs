//! Raw SQLite structural errors remain distinct from unsupported app schemas.
mod support;
use layerfs_persistence::diagnostic::{sqlite_structure, Observation, Options, Outcome};

fn probe(path: &std::path::Path, quick: bool) -> (bool, Vec<Observation>) {
    let mut out = Vec::new();
    let result = sqlite_structure(
        path,
        Options {
            quick,
            ..Options::default()
        },
        &mut |o| out.push(o),
    );
    (result.is_ok(), out)
}
#[test]
fn malformed_header_and_corrupt_btree_are_confirmed_structural_failures() {
    let temp = support::Temp::new("doctor-raw-structure");
    let header = temp.join("bad-header");
    std::fs::write(&header, vec![0xA5; 8192]).unwrap();
    for quick in [false, true] {
        let before = std::fs::read(&header).unwrap();
        let (_, out) = probe(&header, quick);
        assert!(
            out.iter().any(|o| o.id == "store.database.structure"
                && o.outcome == Outcome::Fail
                && o.message.contains("SQLITE_NOTADB")),
            "{out:?}"
        );
        assert_eq!(std::fs::read(&header).unwrap(), before);
    }
    let page = temp.join("bad-page");
    let db = rusqlite::Connection::open(&page).unwrap();
    db.execute_batch("CREATE TABLE data(value BLOB); INSERT INTO data VALUES(zeroblob(100));")
        .unwrap();
    let root: i64 = db
        .query_row(
            "SELECT rootpage FROM sqlite_schema WHERE name='data'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let size: i64 = db.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap();
    drop(db);
    let mut bytes = std::fs::read(&page).unwrap();
    bytes[((root - 1) * size) as usize] = 0x7F;
    std::fs::write(&page, &bytes).unwrap();
    for quick in [false, true] {
        let (_, out) = probe(&page, quick);
        assert!(
            out.iter()
                .any(|o| o.id == "store.database.structure" && o.outcome == Outcome::Fail),
            "{out:?}"
        );
        assert_eq!(std::fs::read(&page).unwrap(), bytes);
    }
}
#[test]
fn unsupported_schema_and_missing_file_are_not_reported_as_corruption() {
    let temp = support::Temp::new("doctor-raw-unavailable");
    let path = temp.join("new-schema");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA user_version=999; CREATE TABLE future_schema(a);")
        .unwrap();
    drop(db);
    let (ok, out) = probe(&path, false);
    assert!(ok, "{out:?}");
    assert!(!out.iter().any(|o| o.outcome == Outcome::Fail), "{out:?}");
    let missing = temp.join("missing");
    let (ok, out) = probe(&missing, false);
    assert!(!ok);
    assert!(!missing.exists());
    assert!(!out.iter().any(|o| o.outcome == Outcome::Fail), "{out:?}");
    let before = std::fs::read_dir(temp.path()).unwrap().count();
    let (ok, out) = probe(temp.path(), true);
    assert!(!ok);
    assert!(!out.iter().any(|o| o.outcome == Outcome::Fail), "{out:?}");
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), before);
}
#[test]
fn exclusive_busy_is_not_corruption_and_deadline_is_honored() {
    let temp = support::Temp::new("doctor-raw-busy");
    let path = temp.join("db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE a(b); BEGIN EXCLUSIVE;")
        .unwrap();
    let (ok, out) = probe(&path, false);
    assert!(!ok);
    assert!(!out.iter().any(|o| o.outcome == Outcome::Fail), "{out:?}");
    let mut out = Vec::new();
    let result = sqlite_structure(
        &path,
        Options {
            deadline: Some(std::time::Instant::now()),
            ..Options::default()
        },
        &mut |o| out.push(o),
    );
    assert!(result.is_err());
    assert!(out.is_empty());
}
