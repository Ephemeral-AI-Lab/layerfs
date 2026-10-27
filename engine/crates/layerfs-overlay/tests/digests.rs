//! #364 item 2 (b): a captured file's whole-content digest exists only when
//! every byte it holds was appended by the Workspace and no later change
//! touched its row. Public engine API only: compound jobs, opens and capture.
mod payload_support;
use layerfs_overlay::*;
use payload_support::Temp;
use std::sync::Arc;

const DIRECTORY: u64 = 50;
const FILE: u64 = 60;

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
fn apply(db: &Overlay, changes: &Changes) {
    let base = db.acquire_base_source(1).unwrap();
    let publication = db.apply(base, changes).unwrap();
    db.reply_attempted(publication).unwrap();
    db.release_base_source(base).unwrap();
}
/// A fresh empty file bound in a fresh directory.
fn created() -> (Temp, Overlay) {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([211; 32]).unwrap();
    apply(
        &db,
        &Changes {
            created: Some(FILE),
            inodes: vec![
                node(DIRECTORY, InodeKind::Directory, 0),
                node(FILE, InodeKind::File, 0),
            ],
            directory_entries: vec![DirectoryEntryChange {
                parent: DIRECTORY,
                name: b"f".to_vec(),
                binding: Binding::Bound {
                    serial: FILE,
                    inherited: false,
                },
            }],
            ..Changes::default()
        },
    );
    (temp, db)
}
fn write(db: &Overlay, open: Option<OpenFile>, offset: u64, data: &[u8], size: u64) {
    apply(
        db,
        &Changes {
            open,
            inodes: vec![node(FILE, InodeKind::File, size)],
            write: Some(PayloadWrite {
                serial: FILE,
                offset,
                data: Arc::from(data),
            }),
            ..Changes::default()
        },
    );
}
fn captured(db: &Overlay) -> Option<[u8; 32]> {
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let digest = db.captured_digests(reader, &[FILE]).unwrap()[0].map(|row| row.digest);
    db.release_captured_reader(reader).unwrap();
    digest
}

#[test]
fn appended_bytes_carry_their_digest() {
    let (_temp, db) = created();
    write(&db, None, 0, b"hello ", 6);
    write(&db, None, 6, b"world", 11);
    assert_eq!(
        captured(&db),
        Some(*blake3::hash(b"hello world").as_bytes())
    );
}

#[test]
fn an_empty_created_file_carries_the_empty_digest() {
    let (_temp, db) = created();
    assert_eq!(captured(&db), Some(*blake3::hash(b"").as_bytes()));
}

#[test]
fn a_write_at_another_offset_has_no_digest() {
    let (_temp, db) = created();
    write(&db, None, 0, b"hello", 5);
    write(&db, None, 0, b"j", 5);
    assert_eq!(captured(&db), None);
}

#[test]
fn a_cell_change_has_no_digest() {
    let (_temp, db) = created();
    let mut data = Box::new([0_u8; CELL_BYTES]);
    data[..3].copy_from_slice(b"abc");
    let mut validity = Box::new([0_u8; MASK_BYTES]);
    validity[0] = 0b111;
    apply(
        &db,
        &Changes {
            inodes: vec![node(FILE, InodeKind::File, 3)],
            cell: Some((
                FILE,
                Cell {
                    offset: 0,
                    data,
                    validity,
                },
            )),
            ..Changes::default()
        },
    );
    assert_eq!(captured(&db), None);
}

#[test]
fn truncate_then_write_has_no_digest() {
    let (_temp, db) = created();
    write(&db, None, 0, b"hello", 5);
    apply(
        &db,
        &Changes {
            inodes: vec![node(FILE, InodeKind::File, 0)],
            ..Changes::default()
        },
    );
    write(&db, None, 0, b"hello", 5);
    assert_eq!(captured(&db), None);
}

#[test]
fn a_hard_link_has_no_digest() {
    let (_temp, db) = created();
    write(&db, None, 0, b"hello", 5);
    let mut linked = node(FILE, InodeKind::File, 5);
    linked.nlink = 2;
    apply(
        &db,
        &Changes {
            inodes: vec![linked],
            directory_entries: vec![DirectoryEntryChange {
                parent: DIRECTORY,
                name: b"g".to_vec(),
                binding: Binding::Bound {
                    serial: FILE,
                    inherited: false,
                },
            }],
            ..Changes::default()
        },
    );
    assert_eq!(captured(&db), None);
}

/// A mode or time change touches no byte: the digest still describes the
/// row. Whether the mode equals a base file's is the Commit's own check.
#[test]
fn a_later_mode_change_keeps_the_digest() {
    let (_temp, db) = created();
    write(&db, None, 0, b"hello", 5);
    let mut changed = node(FILE, InodeKind::File, 5);
    changed.mode = 0o600;
    changed.mtime_seconds = 99;
    apply(
        &db,
        &Changes {
            inodes: vec![changed],
            ..Changes::default()
        },
    );
    assert_eq!(captured(&db), Some(*blake3::hash(b"hello").as_bytes()));
}

#[test]
fn appends_after_a_mode_change_continue_the_digest() {
    let (_temp, db) = created();
    write(&db, None, 0, b"hel", 3);
    let mut changed = node(FILE, InodeKind::File, 3);
    changed.mode = 0o600;
    apply(
        &db,
        &Changes {
            inodes: vec![changed],
            ..Changes::default()
        },
    );
    write(&db, None, 3, b"lo", 5);
    assert_eq!(captured(&db), Some(*blake3::hash(b"hello").as_bytes()));
}

#[test]
fn a_size_change_without_bytes_has_no_digest() {
    let (_temp, db) = created();
    write(&db, None, 0, b"hello", 5);
    apply(
        &db,
        &Changes {
            inodes: vec![node(FILE, InodeKind::File, 9)],
            ..Changes::default()
        },
    );
    assert_eq!(captured(&db), None);
}

fn opened(db: &Overlay) -> OpenFile {
    let source = db.acquire_base_source(2).unwrap();
    let open = db
        .open_file(source, 7, &node(FILE, InodeKind::File, 0), true)
        .unwrap();
    db.release_base_source(source).unwrap();
    open
}

#[test]
fn a_file_open_at_capture_has_no_digest() {
    let (_temp, db) = created();
    let open = opened(&db);
    write(&db, Some(open), 0, b"hello", 5);
    assert_eq!(captured(&db), None);
}

#[test]
fn a_descriptor_write_closed_before_capture_carries_its_digest() {
    let (_temp, db) = created();
    let open = opened(&db);
    write(&db, Some(open), 0, b"hel", 3);
    write(&db, Some(open), 3, b"lo", 5);
    db.close_file(open).unwrap();
    assert_eq!(captured(&db), Some(*blake3::hash(b"hello").as_bytes()));
}

#[test]
fn a_finished_file_opened_again_during_the_commit_has_no_digest() {
    let (_temp, db) = created();
    write(&db, None, 0, b"hello", 5);
    let open = opened(&db);
    assert_eq!(captured(&db), None);
    db.close_file(open).unwrap();
}
