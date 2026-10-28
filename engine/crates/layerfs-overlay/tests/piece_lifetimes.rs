//! Reference pieces along the existing lifecycles: stale rows after a shrink,
//! retirement after install, a failed capture folded into the active layer,
//! an unlinked file its owners still hold, and a closed Workspace. Every step
//! is one bounded page and every view reads the plain-write model.
mod payload_support;
use layerfs_overlay::*;
use payload_support::*;
use std::sync::Arc;

const CELL: u64 = CELL_BYTES as u64;
const WINDOW: u64 = WRITE_WINDOW as u64;

/// Runs maintenance to its end, with `between` after every step.
fn maintain(db: &Overlay, mut between: impl FnMut()) -> u64 {
    let mut cursor = MaintenanceCursor::default();
    for steps in 0..20_000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            return steps;
        };
        assert!(step.work.rows <= 64 && step.work.data_bytes <= 65536);
        cursor = step.cursor;
        between();
    }
    panic!("maintenance did not settle");
}
/// Single-cell pieces at every other cell: no two of them are one run.
fn scatter(file: &mut File, content: &[u8], pieces: u64) {
    for index in 0..pieces {
        let at = 2 * index * CELL;
        file.write_claimed(
            at,
            part(content, at + CELL, CELL),
            claims(None, &[(at, CELL, at + CELL)]),
        );
    }
}

#[test]
fn stale_pieces_are_cleaned_in_pages_and_an_installed_generation_retires_its_pieces() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([32; 32]).unwrap();
    let mut source = db.acquire_base_source(1).unwrap();
    let content = noise(2 << 20, 31);
    let mut file = committed(&db, source, &content);
    scatter(&mut file, &content, 80);
    assert_eq!(rows(&db, ()).0, 80);
    // A shrink hides 69 of them. It deletes none: maintenance does, in pages.
    file.resize(20 * CELL + 5);
    assert_eq!(rows(&db, ()).0, 80);
    file.resize(200 * CELL);
    file.check();
    // A claim after the shrink replaces the stale piece it meets and stays.
    file.write_claimed(
        100 * CELL,
        part(&content, 0, CELL),
        claims(None, &[(100 * CELL, CELL, 0)]),
    );
    assert_eq!(rows(&db, ()).0, 80);
    assert!(maintain(&db, || {}) >= 2);
    assert_eq!(rows(&db, ()).0, 12);
    file.check();

    // Seal and install. The layer above keeps its own piece; the retired
    // generation's pieces are deleted with its other rows.
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    source = db.acquire_base_source(2).unwrap();
    file.source = source;
    let sealed = file.expect.clone();
    file.write_claimed(
        4 * CELL,
        part(&content, 9 * CELL, 2 * CELL),
        claims(None, &[(4 * CELL, 2 * CELL, 9 * CELL)]),
    );
    file.check();
    assert_eq!(rows(&db, ()).0, 13);
    db.release_base_source(source).unwrap();
    db.install(capture, [33; 32]).unwrap();
    maintain(&db, || {});
    source = db.acquire_base_source(3).unwrap();
    file.source = source;
    file.base = sealed;
    file.check();
    assert_eq!(rows(&db, ()).0, 1);
    assert_eq!(db.resources().unwrap().counts.inode_rows, 1);
}

#[test]
fn a_failed_capture_folds_pieces_and_cells_below_the_active_layer() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([42; 32]).unwrap();
    let mut source = db.acquire_base_source(1).unwrap();
    let content = noise(4 << 20, 41);
    let mut first = committed(&db, source, &content);
    let mut second = File::new(&db, source, 8, pattern(70_000, 5));
    second.sources.push((ROOT, content.clone()));
    // The generation that will be sealed: a long piece with a cell above
    // it, a short piece, plain cells, and a second file with one piece.
    for window in 0..2 {
        let at = window * WINDOW;
        first.write_claimed(
            at,
            part(&content, 100 * CELL + at, WINDOW),
            claims(None, &[(at, WINDOW, 100 * CELL + at)]),
        );
    }
    first.write(3 * CELL + 9, b"a cell above the long piece");
    first.write_claimed(
        70 * CELL,
        part(&content, 500 * CELL, 4 * CELL),
        claims(None, &[(70 * CELL, 4 * CELL, 500 * CELL)]),
    );
    first.write(80 * CELL, &pattern(2 * CELL_BYTES, 7));
    second.write_claimed(
        0,
        part(&content, 9 * CELL, 8 * CELL),
        claims(None, &[(0, 8 * CELL, 9 * CELL)]),
    );
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    source = db.acquire_base_source(2).unwrap();
    first.source = source;
    second.source = source;
    // The active generation above it: cells and a piece over the long
    // piece, a piece inside the short one, and a shrink that cuts the lower
    // view off below the plain cells. The second file is not touched.
    first.write(10 * CELL + 100, &pattern(5_000, 9));
    first.write_claimed(
        16 * CELL,
        part(&content, 700 * CELL, 4 * CELL),
        claims(None, &[(16 * CELL, 4 * CELL, 700 * CELL)]),
    );
    first.write_claimed(
        71 * CELL,
        part(&content, 900 * CELL, CELL),
        claims(None, &[(71 * CELL, CELL, 900 * CELL)]),
    );
    first.resize(76 * CELL + 77);
    first.resize(90 * CELL);
    first.write(85 * CELL + 1, b"regrown");
    first.check();
    second.check();

    // Definite failure: the sealed layer is folded one bounded step at a
    // time, and both files read the same bytes after every step.
    db.release_base_source(source).unwrap();
    db.resolve_failed_capture(capture).unwrap();
    source = db.acquire_base_source(3).unwrap();
    first.source = source;
    second.source = source;
    let steps = maintain(&db, || {
        first.check();
        second.check();
    });
    assert!(steps > 10, "{steps}");
    assert!(db.capture_ready().unwrap());
    let counts = db.resources().unwrap().counts;
    assert_eq!(counts.inode_rows, 2);
    // The long piece is in two parts around the active piece over it, the
    // short one likewise; with the two active pieces and the second file's.
    assert_eq!(counts.reference_rows, 7);

    // The folded state seals and installs like any other.
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let sealed = (first.expect.clone(), second.expect.clone());
    db.install(capture, [43; 32]).unwrap();
    maintain(&db, || {});
    source = db.acquire_base_source(4).unwrap();
    (first.source, second.source) = (source, source);
    (first.base, second.base) = sealed;
    // Nothing was written above the installed generation: no local row.
    (first.touched, second.touched) = (false, false);
    first.check();
    second.check();
    assert_eq!(rows(&db, ()), (0, 0));
}

#[test]
fn folds_and_installs_keep_a_reference_model_exact_under_random_claims() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([52; 32]).unwrap();
    let mut source = db.acquire_base_source(1).unwrap();
    let mut random = Random(23);
    let content = noise(500_000, 51);
    let mut file = File::new(&db, source, 2, content.clone());
    file.sources.push((ROOT, content.clone()));
    let mutate = |file: &mut File, random: &mut Random, steps: usize| {
        for step in 0..steps {
            let size = file.expect.len() as u64;
            match random.next(10) {
                0..=1 => {
                    let offset = random.next(size + 20_000);
                    let length = 1 + random.next([3, 5_000, 40_000, WINDOW][step % 4]);
                    file.write(offset, &pattern(length as usize, step as u8));
                }
                2..=5 => scattered(file, &content, random),
                6 => run(file, &content, random),
                7 => file.resize(random.next(size + 1)),
                8 => file.resize(size - size % 4096),
                _ => file.resize(size + random.next(30_000)),
            }
        }
        file.check();
    };
    for round in 0..8_u64 {
        mutate(&mut file, &mut random, 30);
        db.release_base_source(source).unwrap();
        let capture = db.capture().unwrap();
        let sealed = file.expect.clone();
        source = db.acquire_base_source(100 + round).unwrap();
        file.source = source;
        // The layer above the sealed one, before its fate is known.
        mutate(&mut file, &mut random, 15);
        db.release_base_source(source).unwrap();
        if round % 3 == 2 {
            db.install(capture, [round as u8 + 60; 32]).unwrap();
            file.base = sealed;
        } else {
            db.resolve_failed_capture(capture).unwrap();
        }
        source = db.acquire_base_source(200 + round).unwrap();
        file.source = source;
        let mut step = 0;
        maintain(&db, || {
            step += 1;
            if step % 7 == 0 {
                file.check();
            }
        });
        file.check();
        assert!(db.capture_ready().unwrap());
        assert_eq!(db.resources().unwrap().counts.inode_rows, 1);
    }
}

/// The whole view of an unlinked file through its descriptor's read owner.
fn held(db: &Overlay, file: &File, read: FileRead, size: usize) -> Vec<u8> {
    let mut data = Vec::new();
    while data.len() < size {
        let local = db
            .read_file(read, data.len() as u64, READ_WINDOW as u32)
            .unwrap()
            .unwrap();
        assert_eq!(local.size, size as u64);
        let window = file.compose(&local, &file.base);
        assert!(!window.is_empty());
        data.extend(window);
    }
    data
}

#[test]
fn an_unlinked_open_file_keeps_its_pieces_until_its_last_owner() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([62; 32]).unwrap();
    let mut source = db.acquire_base_source(1).unwrap();
    let content = noise(2 << 20, 61);
    let mut file = committed(&db, source, &content);
    scatter(&mut file, &content, 20);
    file.write_claimed(
        64 * CELL,
        part(&content, 5 * CELL, WINDOW),
        claims(None, &[(64 * CELL, WINDOW, 5 * CELL)]),
    );
    file.write(66 * CELL + 3, b"a cell above the piece");
    let size = file.expect.len();
    let open = db
        .open_file(source, 1, &file.inode(size as u64), true)
        .unwrap();
    let mut removed = file.inode(size as u64);
    removed.nlink = 0;
    let unlink = Changes {
        inodes: vec![removed],
        ..Changes::default()
    };
    let publication = db.apply(source, &unlink).unwrap();
    db.reply_attempted(publication).unwrap();
    let mut expect = file.expect.clone();
    for round in 0..6_u64 {
        let read = db.acquire_file_read(source, open, 100 + round).unwrap();
        assert!(held(&db, &file, read, size) == expect, "round {round}");
        db.release_file_read(read).unwrap();
        // A claimed write through the descriptor lands in the orphan's own
        // layer, above whatever has migrated into it so far.
        let at = (30 + round) * CELL;
        let data = part(&content, (300 + round) * CELL, CELL);
        expect[at as usize..(at + CELL) as usize].copy_from_slice(data);
        let inode = db.source_inode(source, 7).unwrap().unwrap();
        let write = Changes {
            open: Some(open),
            inodes: vec![inode],
            write: Some(PayloadWrite {
                serial: 7,
                offset: at,
                data: Arc::from(data),
            }),
            claims: Some(Box::new(claims(None, &[(at, CELL, (300 + round) * CELL)]))),
            ..Changes::default()
        };
        let publication = db.apply(source, &write).unwrap();
        db.reply_attempted(publication).unwrap();
        db.release_base_source(source).unwrap();
        let capture = db.capture().unwrap();
        if round % 2 == 1 {
            db.install(capture, [round as u8 + 70; 32]).unwrap();
        } else {
            db.resolve_failed_capture(capture).unwrap();
        }
        maintain(&db, || {});
        source = db.acquire_base_source(2 + round).unwrap();
    }
    let read = db.acquire_file_read(source, open, 999).unwrap();
    assert!(held(&db, &file, read, size) == expect);
    // Every lower layer has migrated: the pieces are the orphan's own rows.
    // The six claimed cells are one run and took three of the scattered
    // pieces: 17 of those, the run and the long piece.
    assert_eq!(rows(&db, ()).0, 19);
    db.close_file(open).unwrap();
    assert!(held(&db, &file, read, size) == expect);
    db.release_file_read(read).unwrap();
    maintain(&db, || {});
    assert_eq!(rows(&db, ()), (0, 0));
    assert_eq!(db.resources().unwrap().open.orphan_files, 0);
}

#[test]
fn an_unlinked_file_nothing_holds_retires_its_pieces() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([72; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let content = noise(2 << 20, 71);
    let mut file = committed(&db, source, &content);
    scatter(&mut file, &content, 150);
    file.write(301 * CELL, &pattern(CELL_BYTES, 3));
    assert_eq!(rows(&db, ()), (150, 1));
    let mut removed = file.inode(file.expect.len() as u64);
    removed.nlink = 0;
    let unlink = Changes {
        inodes: vec![removed],
        ..Changes::default()
    };
    let publication = db.apply(source, &unlink).unwrap();
    db.reply_attempted(publication).unwrap();
    // Three pages of pieces, each one statement of one step.
    assert!(maintain(&db, || {}) >= 4);
    assert_eq!(rows(&db, ()), (0, 0));
}

#[test]
fn a_closed_workspace_retains_its_pieces_until_file_disposal() {
    let temp = Temp::new();
    let db = temp.db();
    let content = noise(2 << 20, 81);
    db.open_workspace([82; 32]).unwrap();
    let closed_source = db.acquire_base_source(1).unwrap();
    let mut file = committed(&db, closed_source, &content);
    scatter(&mut file, &content, 150);
    db.close().unwrap();
    db.release_base_source(closed_source).unwrap();
    assert_eq!(db.cleanup_state().unwrap(), CleanupState::Queued);
    assert_eq!(
        rows(&db, ()).0,
        150,
        "closed file is retained until disposal"
    );
    assert_eq!(db.resources().unwrap().counts.reference_rows, 150);
}
