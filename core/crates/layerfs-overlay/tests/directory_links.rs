//! The `subdirs` column of an inode row: the exact number of child
//! directories bound in a directory. Public engine proofs of its round trip
//! through every whole-row reader and of its value grammar.
use layerfs_overlay::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-overlay-links-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn db(&self) -> PathBuf {
        self.0.join("overlay.sqlite")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn directory(serial: u64, entries: u64, subdirs: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::Directory,
        mode: 0o755,
        mtime_seconds: 7,
        mtime_nanoseconds: 9,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries,
        subdirs,
    }
}
fn file(serial: u64) -> Inode {
    Inode {
        kind: InodeKind::File,
        mode: 0o644,
        ..directory(serial, 0, 0)
    }
}
fn drain(db: &Overlay) {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..10_000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            return;
        };
        cursor = step.cursor;
    }
    panic!("maintenance did not terminate");
}
fn publish(db: &Overlay, route: Route, inode: &Inode) {
    let publication = db.publish(route, inode, None, None).unwrap();
    db.reply_attempted(publication).unwrap();
}

#[test]
fn the_count_round_trips_through_every_whole_row_reader() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([1; 32], [2; 32]).unwrap();
    // The count is independent of `entries` and of `nlink`: five bindings,
    // three of them directories, one namespace reference.
    let first = directory(8, 5, 3);
    publish(&db, route, &first);
    publish(&db, route, &file(9));
    assert_eq!(db.inode(route, 8).unwrap(), Some(first.clone()));
    assert_eq!(db.inode(route, 9).unwrap().unwrap().subdirs, 0);

    // An update of the same generation's row replaces the count.
    let second = directory(8, 4, 2);
    publish(&db, route, &second);
    assert_eq!(db.inode(route, 8).unwrap(), Some(second.clone()));

    // The sealed generation: the capture page and both reader selects.
    let capture = db.capture(route).unwrap();
    let page = db.captured_inodes(capture, 0).unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0], second);
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    assert_eq!(db.reader_inodes(reader, 0).unwrap()[0], second);
    assert_eq!(db.reader_inode(reader, 8).unwrap(), Some(second.clone()));
    // The active view still reads the sealed row.
    assert_eq!(db.inode(route, 8).unwrap(), Some(second.clone()));

    // A later change of the active generation is a new row with its own
    // count; the sealed row keeps the captured one.
    let third = directory(8, 6, 4);
    publish(&db, route, &third);
    assert_eq!(db.inode(route, 8).unwrap(), Some(third.clone()));
    assert_eq!(db.reader_inode(reader, 8).unwrap(), Some(second.clone()));
    db.release_captured_reader(reader).unwrap();
}

#[test]
fn a_failed_capture_folds_the_sealed_count_into_the_active_generation() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([3; 32], [4; 32]).unwrap();
    let sealed = directory(8, 7, 6);
    publish(&db, route, &sealed);
    let capture = db.capture(route).unwrap();
    let active = db.active_generation(route).unwrap();
    assert_ne!(active, capture.generation);
    db.resolve_failed_capture(capture).unwrap();
    drain(&db);
    assert!(db.state(route).unwrap().consolidating.is_none());
    // The fold copied the whole row, count included, and retired the sealed
    // generation: the active generation is now the only one that holds it.
    assert_eq!(db.inode(route, 8).unwrap(), Some(sealed));
    let next = db.capture(route).unwrap();
    assert_eq!(next.generation, active);
    assert_eq!(
        db.captured_inodes(next, 0).unwrap(),
        vec![directory(8, 7, 6)]
    );
}

#[test]
fn the_engine_refuses_a_count_on_a_file_and_an_unrepresentable_count() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([5; 32], [6; 32]).unwrap();
    for kind in [InodeKind::File, InodeKind::Symlink] {
        let mut value = file(8);
        value.kind = kind;
        value.mode = if kind == InodeKind::Symlink {
            0o777
        } else {
            0o644
        };
        value.subdirs = 1;
        assert!(matches!(
            db.publish(route, &value, None, None),
            Err(OverlayError::Invalid("inode metadata"))
        ));
    }
    // SQLite integers are signed: a count above i64::MAX has no column value
    // and is refused before any statement, never stored as a negative one.
    assert!(db
        .publish(route, &directory(8, 0, i64::MAX as u64 + 1), None, None)
        .is_err());
    assert_eq!(db.inode(route, 8).unwrap(), None);
    publish(&db, route, &directory(8, 0, i64::MAX as u64));
    assert_eq!(
        db.inode(route, 8).unwrap().unwrap().subdirs,
        i64::MAX as u64
    );
}

/// The table's own CHECK, exercised on the schema the product created: a
/// second plain connection to the closed database file tries the two values
/// the engine's grammar already refuses.
#[test]
fn the_schema_check_refuses_a_negative_count_and_a_count_on_a_file() {
    let temp = Temp::new();
    {
        let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
        let route = db.open_workspace([7; 32], [8; 32]).unwrap();
        publish(&db, route, &directory(8, 2, 1));
        assert_eq!(db.profile().schema_version, 23);
    }
    let raw = rusqlite::Connection::open(temp.db()).unwrap();
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 23);
    let stored: (i64, i64, i64) = raw
        .query_row(
            "SELECT entries,subdirs,nlink FROM inode WHERE serial=8",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(stored, (2, 1, 1));
    let update = |sql: &str| match raw.execute(sql, []) {
        Ok(changed) => Ok(changed),
        Err(rusqlite::Error::SqliteFailure(code, _)) => Err(code.code),
        Err(other) => panic!("unexpected error {other:?}"),
    };
    assert_eq!(
        update("UPDATE inode SET subdirs=-1 WHERE serial=8"),
        Err(rusqlite::ErrorCode::ConstraintViolation)
    );
    assert_eq!(
        update("UPDATE inode SET kind=1,mode=420,entries=0 WHERE serial=8"),
        Err(rusqlite::ErrorCode::ConstraintViolation),
        "a file row cannot keep a non-zero count"
    );
    assert_eq!(update("UPDATE inode SET subdirs=0 WHERE serial=8"), Ok(1));
    assert_eq!(
        update("UPDATE inode SET kind=1,mode=420,entries=0 WHERE serial=8"),
        Ok(1)
    );
    assert_eq!(
        update("UPDATE inode SET subdirs=1 WHERE serial=8"),
        Err(rusqlite::ErrorCode::ConstraintViolation)
    );
}
