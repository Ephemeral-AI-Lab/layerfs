//! #359 E10: a row `keep_live_row` carries for an mtime-only change is not
//! counted dirty, and every writer of that serial's active row counts it.
//! Public engine API only: compound jobs, the first-path publish, a failed
//! capture's fold and a job that fails after its kept write.
mod payload_support;
use layerfs_overlay::*;
use payload_support::Temp;
use std::sync::Arc;

const FILE: u64 = 60;
const DIRECTORY: u64 = 50;

fn node(serial: u64, kind: InodeKind, size: u64) -> Inode {
    Inode {
        serial,
        kind,
        mode: if kind == InodeKind::Directory {
            0o755
        } else {
            0o644
        },
        mtime_seconds: 11,
        mtime_nanoseconds: 13,
        nlink: 1,
        size,
        inherited_cutoff: 0,
        born: 0,
        entries: u64::from(kind == InodeKind::Directory),
        subdirs: 0,
    }
}
fn cell(bytes: &[u8]) -> Cell {
    let mut data = Box::new([0_u8; CELL_BYTES]);
    data[..bytes.len()].copy_from_slice(bytes);
    let mut validity = Box::new([0_u8; MASK_BYTES]);
    for bit in 0..bytes.len() {
        validity[bit / 8] |= 1 << (bit % 8);
    }
    Cell {
        offset: 0,
        data,
        validity,
    }
}
fn bound(parent: u64, name: &[u8], serial: u64) -> DirectoryEntryChange {
    DirectoryEntryChange {
        parent,
        name: name.to_vec(),
        binding: Binding::Bound {
            serial,
            inherited: false,
        },
    }
}
fn removed(parent: u64, name: &[u8]) -> DirectoryEntryChange {
    DirectoryEntryChange {
        parent,
        name: name.to_vec(),
        binding: Binding::Removed { inherited: false },
    }
}
fn apply(db: &Overlay, source: u64, changes: &Changes) -> OverlayResult<()> {
    let base = db.acquire_base_source(source).unwrap();
    let applied = db
        .apply(base, changes)
        .map(|p| db.reply_attempted(p).unwrap());
    db.release_base_source(base).unwrap();
    applied
}
fn dirty(db: &Overlay) -> (u64, u64) {
    let state = db.state().unwrap();
    (state.dirty_inodes, state.dirty_directory_entries)
}
/// A directory binding one file of three bytes, captured; both rows are
/// then kept live, as construction does for mtime-only changes.
fn kept() -> (Temp, Overlay, Capture, CapturedReader) {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([210; 32]).unwrap();
    let changes = Changes {
        inodes: vec![
            node(DIRECTORY, InodeKind::Directory, 0),
            node(FILE, InodeKind::File, 3),
        ],
        directory_entries: vec![bound(DIRECTORY, b"f", FILE)],
        cell: Some((FILE, cell(b"abc"))),
        ..Changes::default()
    };
    apply(&db, 1, &changes).unwrap();
    let capture = db.capture().unwrap();
    assert_eq!(dirty(&db), (0, 0));
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    assert!(db.keep_live_row(reader, FILE).unwrap());
    assert!(db.keep_live_row(reader, DIRECTORY).unwrap());
    // Already active: nothing is carried again.
    assert!(!db.keep_live_row(reader, FILE).unwrap());
    (temp, db, capture, reader)
}

#[test]
fn a_kept_row_is_not_counted_dirty() {
    let (_temp, db, _capture, reader) = kept();
    assert_eq!(dirty(&db), (0, 0));
    assert!(db.state().unwrap().captured.is_some());
    db.release_captured_reader(reader).unwrap();
}

/// Every writer of a kept serial's active row, each from a fresh engine.
/// Each one counts exactly the serials whose rows it wrote.
#[test]
fn every_writer_of_a_kept_serial_counts_it_once() {
    let file = node(FILE, InodeKind::File, 3);
    let directory = node(DIRECTORY, InodeKind::Directory, 0);
    let with = |f: &dyn Fn(&mut Inode)| {
        let mut inode = file.clone();
        f(&mut inode);
        inode
    };
    let cases: Vec<(&str, Changes, (u64, u64))> = vec![
        (
            "write",
            Changes {
                inodes: vec![with(&|i| i.size = 5)],
                write: Some(PayloadWrite {
                    serial: FILE,
                    offset: 3,
                    data: Arc::from(&b"de"[..]),
                }),
                ..Changes::default()
            },
            (1, 0),
        ),
        (
            "truncate",
            Changes {
                inodes: vec![with(&|i| i.size = 1)],
                ..Changes::default()
            },
            (1, 0),
        ),
        (
            "extend",
            Changes {
                inodes: vec![with(&|i| i.size = 9000)],
                ..Changes::default()
            },
            (1, 0),
        ),
        (
            "chmod",
            Changes {
                inodes: vec![with(&|i| i.mode = 0o600)],
                ..Changes::default()
            },
            (1, 0),
        ),
        (
            "touch",
            Changes {
                inodes: vec![with(&|i| i.mtime_seconds = 99)],
                ..Changes::default()
            },
            (1, 0),
        ),
        (
            "payload cell",
            Changes {
                inodes: vec![file.clone()],
                cell: Some((FILE, cell(b"xyz"))),
                ..Changes::default()
            },
            (1, 0),
        ),
        (
            "link",
            Changes {
                inodes: vec![with(&|i| i.nlink = 2)],
                directory_entries: vec![bound(DIRECTORY, b"g", FILE)],
                ..Changes::default()
            },
            (1, 1),
        ),
        (
            "rename in the kept directory",
            Changes {
                inodes: vec![{
                    let mut d = directory.clone();
                    d.mtime_seconds = 77;
                    d
                }],
                directory_entries: vec![removed(DIRECTORY, b"f"), bound(DIRECTORY, b"h", FILE)],
                ..Changes::default()
            },
            (1, 2),
        ),
        (
            "unlink",
            Changes {
                detached: Some(FILE),
                inodes: vec![with(&|i| i.nlink = 0), {
                    let mut d = directory.clone();
                    d.entries = 0;
                    d
                }],
                directory_entries: vec![removed(DIRECTORY, b"f")],
                ..Changes::default()
            },
            (2, 1),
        ),
    ];
    // Every case runs; a mismatch names its writer.
    let mut wrong = Vec::new();
    for (what, changes, expected) in cases {
        let (_temp, db, _capture, reader) = kept();
        apply(&db, 2, &changes).unwrap_or_else(|e| panic!("{what}: {e:?}"));
        if dirty(&db) != expected {
            wrong.push((what, dirty(&db), expected));
        }
        db.release_captured_reader(reader).unwrap();
    }
    // The first-path publish job, with a payload cell.
    let (_temp, db, _capture, reader) = kept();
    let mut inode = file.clone();
    inode.mode = 0o640;
    let publication = db.publish(&inode, None, Some(&cell(b"pqr"))).unwrap();
    db.reply_attempted(publication).unwrap();
    if dirty(&db) != (1, 0) {
        wrong.push(("first-path publish", dirty(&db), (1, 0)));
    }
    db.release_captured_reader(reader).unwrap();
    assert!(wrong.is_empty(), "(writer, counted, expected): {wrong:?}");
}

/// A writer counts a kept serial once; a later write of it, and a write of
/// another serial, count as before.
#[test]
fn a_kept_serial_is_counted_once_and_others_as_before() {
    let (_temp, db, _capture, reader) = kept();
    let mut file = node(FILE, InodeKind::File, 3);
    file.mode = 0o600;
    let one = Changes {
        inodes: vec![file.clone()],
        ..Changes::default()
    };
    apply(&db, 2, &one).unwrap();
    file.mode = 0o640;
    apply(
        &db,
        3,
        &Changes {
            inodes: vec![file],
            ..Changes::default()
        },
    )
    .unwrap();
    assert_eq!(dirty(&db), (1, 0));
    apply(
        &db,
        4,
        &Changes {
            created: Some(61),
            inodes: vec![node(61, InodeKind::File, 0)],
            ..Changes::default()
        },
    )
    .unwrap();
    assert_eq!(dirty(&db), (2, 0));
    db.release_captured_reader(reader).unwrap();
}

/// A job that writes the kept row and then fails is rolled back whole: the
/// serial is kept again, so the next writer still counts it.
#[test]
fn a_failed_job_restores_the_kept_set() {
    let (_temp, db, _capture, reader) = kept();
    // An active row of 61, so creating it again fails after the kept write.
    apply(
        &db,
        2,
        &Changes {
            created: Some(61),
            inodes: vec![node(61, InodeKind::File, 0)],
            ..Changes::default()
        },
    )
    .unwrap();
    assert_eq!(dirty(&db), (1, 0));
    let mut file = node(FILE, InodeKind::File, 3);
    file.mode = 0o600;
    let failing = Changes {
        created: Some(61),
        inodes: vec![file.clone(), node(61, InodeKind::File, 0)],
        ..Changes::default()
    };
    assert!(apply(&db, 3, &failing).is_err());
    assert_eq!(dirty(&db), (1, 0), "nothing of the failed job is counted");
    apply(
        &db,
        4,
        &Changes {
            inodes: vec![file],
            ..Changes::default()
        },
    )
    .unwrap();
    assert_eq!(
        dirty(&db),
        (2, 0),
        "the kept serial is counted by the next writer"
    );
    db.release_captured_reader(reader).unwrap();
}

/// A failed capture folds into the active generation. Folding a captured
/// layer into a kept row writes that row: it is counted.
#[test]
fn folding_a_failed_capture_into_a_kept_row_counts_it() {
    let (_temp, db, capture, reader) = kept();
    db.release_captured_reader(reader).unwrap();
    db.resolve_failed_capture(capture).unwrap();
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..20_000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            break;
        };
        cursor = step.cursor;
    }
    let state = db.state().unwrap();
    assert_eq!(state.consolidating, None, "the fold finished");
    assert_eq!(
        state.dirty_inodes, 2,
        "both kept rows were written by the fold"
    );
}
