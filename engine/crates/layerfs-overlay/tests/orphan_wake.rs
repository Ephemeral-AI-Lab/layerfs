//! An orphan parked behind a captured generation is woken when nothing awaits
//! that generation any more: not by a capture that settles while a captured
//! reader still holds it, and then by the reader's last release.
mod payload_support;

use layerfs_overlay::*;
use payload_support::{File, Temp};

/// Maintenance turns that are enough for every drain here (each takes a few).
const TURNS: usize = 256;

/// Runs maintenance turns until none is ready. False when `TURNS` turns left
/// work ready: a drain that does not end.
fn idle(db: &Overlay) -> bool {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..TURNS {
        let Some(step) = db.maintain(cursor).unwrap() else {
            return true;
        };
        cursor = step.cursor;
    }
    false
}
/// The generation waits and ready items now.
fn waiting(db: &Overlay) -> (u64, u64) {
    let resources = db.resources().unwrap();
    (
        resources.open.generation_waits,
        resources.counts.ready_targets,
    )
}
/// A file written in the generation a capture seals, and that capture. A
/// reader holds the generation when `hold`.
fn sealed(db: &Overlay, hold: bool) -> (File<'_>, Capture, Option<CapturedReader>) {
    let source = db.acquire_base_source(1).unwrap();
    let mut file = File::new(db, source, 7, Vec::new());
    file.write(0, b"sealed bytes");
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = hold.then(|| db.acquire_captured_reader(capture, 1).unwrap());
    (file, capture, reader)
}
/// The file unlinked after the capture, with a descriptor open on it when
/// `open`: its orphan reads the sealed generation, so it waits for it.
fn unlink(db: &Overlay, file: &File, open: bool) -> Option<OpenFile> {
    let source = db.acquire_base_source(2).unwrap();
    let held = open.then(|| db.open_file(source, 1, &file.inode(12), true).unwrap());
    let mut removed = file.inode(12);
    removed.nlink = 0;
    let publication = db
        .apply(
            source,
            &Changes {
                inodes: vec![removed],
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(publication).unwrap();
    db.release_base_source(source).unwrap();
    held
}

/// Settles the capture while its reader is held, then releases the reader and
/// the descriptor. Everything is observed first and asserted last, so the
/// custody is released also when the old code fails an assertion.
fn settle_while_held(label: &str, settle: impl FnOnce(&Overlay, Capture), seed: u8) {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([seed; 32]).unwrap();
    let (file, capture, reader) = sealed(&db, true);
    let (reader, open) = (reader.unwrap(), unlink(&db, &file, true).unwrap());
    assert!(idle(&db), "the orphan parks behind the captured generation");
    assert_eq!(waiting(&db), (1, 0));

    settle(&db, capture);
    let ends = idle(&db);
    let held = waiting(&db);
    let bytes = db.read_captured(reader, 7, 0, 32).unwrap().unwrap().data;

    db.release_captured_reader(reader).unwrap();
    let released = idle(&db);
    let after_release = waiting(&db);
    db.close_file(open).unwrap();
    let closed = idle(&db);
    let end = db.resources().unwrap();

    assert!(
        ends,
        "{label} while the reader holds the generation: the drain does not end"
    );
    assert_eq!(
        held,
        (1, 0),
        "{label}: the orphan stays parked, nothing is ready"
    );
    assert_eq!(
        bytes, b"sealed bytes",
        "{label}: the reader keeps its sealed input"
    );
    assert!(
        released,
        "{label}: the reader's release ends in an idle engine"
    );
    assert_eq!(
        after_release,
        (0, 0),
        "{label}: the release woke the orphan and it migrated"
    );
    assert!(closed, "{label}: the last close ends in an idle engine");
    assert_eq!(
        (
            end.open.orphan_files,
            end.counts.payload_cells,
            end.counts.maintenance_targets
        ),
        (0, 0, 0),
        "{label}: everything is reclaimed"
    );
}

#[test]
fn install_does_not_wake_an_orphan_while_a_captured_reader_holds_its_generation() {
    settle_while_held(
        "install",
        |db, capture| db.install(capture, [153; 32]).unwrap(),
        161,
    );
}

#[test]
fn a_failed_capture_does_not_wake_an_orphan_while_a_captured_reader_holds_its_generation() {
    settle_while_held(
        "failed capture",
        |db, capture| db.resolve_failed_capture(capture).unwrap(),
        162,
    );
}

#[test]
fn a_released_reader_does_not_wake_an_orphan_while_the_capture_still_awaits_its_generation() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([163; 32]).unwrap();
    let (file, capture, reader) = sealed(&db, true);
    let open = unlink(&db, &file, true).unwrap();
    assert!(idle(&db));

    db.release_captured_reader(reader.unwrap()).unwrap();
    let ends = idle(&db);
    let before = waiting(&db);
    db.install(capture, [164; 32]).unwrap();
    let installed = idle(&db);
    let after = waiting(&db);
    db.close_file(open).unwrap();
    let closed = idle(&db);

    assert!(ends, "a release before the install: the drain does not end");
    assert_eq!(before, (1, 0), "the orphan stays parked until the install");
    assert!(installed && closed);
    assert_eq!(
        after,
        (0, 0),
        "the install woke it, nothing else held the generation"
    );
}

#[test]
fn install_wakes_an_orphan_that_only_the_capture_held_back() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([165; 32]).unwrap();
    let (file, capture, _) = sealed(&db, false);
    let open = unlink(&db, &file, true).unwrap();
    let parked = idle(&db) && waiting(&db) == (1, 0);

    db.install(capture, [166; 32]).unwrap();
    let installed = idle(&db);
    let after = waiting(&db);
    db.close_file(open).unwrap();
    let closed = idle(&db);

    assert!(parked, "the orphan parks behind the capture");
    assert!(installed && closed);
    assert_eq!(
        after,
        (0, 0),
        "no reader holds the generation: the install woke it"
    );
}

#[test]
fn a_file_nothing_holds_waits_for_no_generation() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([167; 32]).unwrap();
    let (file, capture, reader) = sealed(&db, true);
    assert!(unlink(&db, &file, false).is_none());

    db.install(capture, [168; 32]).unwrap();
    let installed = idle(&db);
    let held = waiting(&db);
    db.release_captured_reader(reader.unwrap()).unwrap();
    let released = idle(&db);
    let end = db.resources().unwrap();

    assert!(installed && released);
    assert_eq!(held, (0, 0), "no descriptor, so no orphan waits");
    assert_eq!(
        (end.counts.payload_cells, end.counts.maintenance_targets),
        (0, 0),
        "the sealed payload is reclaimed once its reader is gone"
    );
}
