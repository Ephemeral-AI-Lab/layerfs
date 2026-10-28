mod payload_support;
use layerfs_overlay::*;
use payload_support::*;

fn collect(db: &Overlay, mut cursor: CapturedRunCursor, base: &[u8]) -> (Vec<u8>, CapturedRunWork) {
    let mut bytes = Vec::new();
    let mut work = CapturedRunWork::default();
    for _ in 0..4096 {
        let reply = db.captured_run_step(cursor).unwrap();
        assert!(reply.work.metadata_rows <= 4);
        work.metadata_rows += reply.work.metadata_rows;
        work.stale_rows += reply.work.stale_rows;
        match reply.step {
            CapturedRunStep::Reference { .. } => panic!("no file of these proofs holds a piece"),
            CapturedRunStep::End => return (bytes, work),
            CapturedRunStep::Local { .. } => panic!("shape step on a payload cursor"),
            CapturedRunStep::Continue(next) => {
                assert!(reply.work.metadata_rows > 0);
                assert_eq!(next.offset(), cursor.offset());
                assert_eq!(next.reader(), cursor.reader());
                cursor = next;
            }
            CapturedRunStep::Gap {
                kind,
                offset,
                length,
                next,
            } => {
                assert_eq!(offset, cursor.offset());
                assert_eq!(next.offset(), offset + length);
                assert_eq!(next.reader(), cursor.reader());
                match kind {
                    CapturedGap::Zero => bytes.resize(bytes.len() + length as usize, 0),
                    CapturedGap::Inherited => {
                        bytes.extend_from_slice(&base[offset as usize..(offset + length) as usize])
                    }
                }
                cursor = next;
            }
            CapturedRunStep::Window { read, next } => {
                assert!(read.data.len() <= RUN_BYTES);
                assert_eq!(read.offset, cursor.offset());
                assert_eq!(read.base_root, Some(cursor.reader().root()));
                assert_eq!(next.reader(), cursor.reader());
                assert_eq!(next.offset(), read.offset + read.data.len() as u64);
                for (index, local) in read.data.into_iter().enumerate() {
                    let inherited = read
                        .inherited
                        .get(index / 8)
                        .is_some_and(|bits| bits & (1 << (index % 8)) != 0);
                    bytes.push(if inherited {
                        base[(read.offset as usize) + index]
                    } else {
                        local
                    });
                }
                cursor = next;
            }
        }
    }
    panic!("fixture run cursor did not finish");
}

#[test]
fn sparse_cutoff_gap_above_four_gib_avoids_payload_windows_and_pays_root_metadata() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([142; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let size = (1_u64 << 32) + 99;
    let inode = Inode {
        serial: 17,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 1,
        mtime_nanoseconds: 0,
        nlink: 1,
        size,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    };
    let publication = db
        .apply(
            source,
            &Changes {
                inodes: vec![inode],
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(publication).unwrap();
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let cursor = CapturedRunCursor::new(reader, 17, 0, size).unwrap();
    let before = db.diagnostics();
    let payload_before = db.payload_work();
    let reply = db.captured_run_step(cursor).unwrap();
    let CapturedRunStep::Gap {
        kind,
        offset,
        length,
        next,
    } = reply.step
    else {
        panic!("expected gap")
    };
    assert_eq!((kind, offset, length), (CapturedGap::Zero, 0, size));
    assert_eq!(reply.work.metadata_rows, 0);
    assert!(matches!(
        db.captured_run_step(next).unwrap().step,
        CapturedRunStep::End
    ));
    assert_eq!(db.payload_work(), payload_before);
    let after = db.diagnostics();
    let delta = after.since(&before);
    assert_eq!(
        delta.statements[StatementKind::Payload as usize].returned_blob_bytes,
        0
    );
    println!("CAPTURED_GAP_ROOT_METADATA {:?}", delta);
    db.release_captured_reader(reader).unwrap();
}

#[test]
fn masked_cells_and_shrink_regrow_match_independent_final_bytes_after_install() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([144; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let mut file = File::new(&db, source, 19, pattern(CELL_BYTES * 3 + 91, 5));
    file.write(7, b"local");
    file.write(CELL_BYTES as u64 + 13, b"second");
    file.resize((CELL_BYTES * 2 + 3) as u64);
    file.resize((CELL_BYTES * 3 + 91) as u64);
    file.write((CELL_BYTES * 3 + 73) as u64, b"tail");
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let cursor = CapturedRunCursor::new(reader, 19, 0, file.expect.len() as u64).unwrap();
    let (bytes, _) = collect(&db, cursor, &file.base);
    assert_eq!(bytes, file.expect);
    db.install(capture, [145; 32]).unwrap();
    let (bytes, _) = collect(&db, cursor, &file.base);
    assert_eq!(bytes, file.expect);
    db.close().unwrap();
    let (bytes, _) = collect(&db, cursor, &file.base);
    assert_eq!(bytes, file.expect);
    db.release_captured_reader(reader).unwrap();
    assert!(matches!(
        db.captured_run_step(cursor),
        Err(OverlayError::Stale)
    ));
}

#[test]
fn stale_metadata_has_bounded_continuations_and_monotone_seek_work() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([147; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let mut file = File::new(&db, source, 23, Vec::new());
    for index in 0..96 {
        file.write((index * CELL_BYTES) as u64, b"stale");
    }
    file.resize(0);
    file.resize((CELL_BYTES * 97) as u64);
    file.write((CELL_BYTES * 96) as u64, b"live");
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let cursor = CapturedRunCursor::new(reader, 23, 0, file.expect.len() as u64).unwrap();
    let before = db.diagnostics();
    let (bytes, work) = collect(&db, cursor, &[]);
    println!(
        "CAPTURED_RUN_STALE_WORK {:?} SQL {:?}",
        work,
        db.diagnostics().since(&before)
    );
    assert_eq!(bytes, file.expect);
    assert_eq!(work.stale_rows, 96);
    assert_eq!(work.metadata_rows, 97);
    db.release_captured_reader(reader).unwrap();
}

#[test]
fn absent_local_cells_are_inherited_and_wrong_size_retains_original_reader() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([149; 32]).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let cursor = CapturedRunCursor::new(reader, 29, 0, 101).unwrap();
    let reply = db.captured_run_step(cursor).unwrap();
    let CapturedRunStep::Gap { kind, length, .. } = reply.step else {
        panic!("expected gap")
    };
    assert_eq!((kind, length), (CapturedGap::Inherited, 101));
    assert_eq!(db.retained_captured_reader(1).unwrap(), Some(reader));
    db.release_captured_reader(reader).unwrap();

    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([151; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let mut file = File::new(&db, source, 31, Vec::new());
    file.write(0, b"abc");
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let cursor = CapturedRunCursor::new(reader, 31, 0, 4).unwrap();
    assert!(matches!(
        db.captured_run_step(cursor),
        Err(OverlayError::Invalid("captured run size"))
    ));
    assert_eq!(db.retained_captured_reader(2).unwrap(), Some(reader));
    db.release_captured_reader(reader).unwrap();
}

#[test]
fn interleaved_live_and_stale_cells_keep_each_seek_and_trimmed_start_once() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([153; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let mut file = File::new(&db, source, 37, Vec::new());
    for cell in 0..20 {
        file.write((cell * CELL_BYTES) as u64, b"old");
    }
    file.resize(0);
    file.resize((20 * CELL_BYTES) as u64);
    for cell in (1..20).step_by(2) {
        file.write((cell * CELL_BYTES + 31) as u64, b"fresh");
    }
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let cursor = CapturedRunCursor::new(reader, 37, 0, file.expect.len() as u64).unwrap();
    let (bytes, work) = collect(&db, cursor, &[]);
    assert_eq!(bytes, file.expect);
    assert_eq!((work.metadata_rows, work.stale_rows), (20, 10));
    // Start beyond the trimmed payload prefix of a live physical cell. Its
    // metadata is consumed once; it must not supply data or rewind the seek.
    let offset = CELL_BYTES + 200;
    let cursor =
        CapturedRunCursor::new(reader, 37, offset as u64, file.expect.len() as u64).unwrap();
    let (bytes, work) = collect(&db, cursor, &[]);
    assert_eq!(bytes, file.expect[offset..]);
    assert_eq!((work.metadata_rows, work.stale_rows), (19, 9));
    db.release_captured_reader(reader).unwrap();
}

#[test]
fn a_row_of_several_cells_is_one_metadata_row_and_one_window() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([150; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let mut file = File::new(&db, source, 23, Vec::new());
    // Four rows of 32 KiB in the sealed generation.
    file.write(0, &pattern(WRITE_WINDOW, 7));
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let size = WRITE_WINDOW as u64;
    let windows = |from: u64| {
        let mut cursor = CapturedRunCursor::new(reader, 23, from, size).unwrap();
        let (mut seen, mut rows) = (Vec::new(), 0);
        for _ in 0..64 {
            let reply = db.captured_run_step(cursor).unwrap();
            rows += reply.work.metadata_rows;
            match reply.step {
                CapturedRunStep::Reference { .. } => {
                    panic!("no file of these proofs holds a piece")
                }
                CapturedRunStep::End => return (seen, rows),
                CapturedRunStep::Continue(next) => cursor = next,
                CapturedRunStep::Gap { .. } => panic!("no gap in a dense file"),
                CapturedRunStep::Local { .. } => panic!("shape step on a payload cursor"),
                CapturedRunStep::Window { read, next } => {
                    let at = read.offset as usize;
                    assert_eq!(read.data, file.expect[at..at + read.data.len()]);
                    assert!(read.inherited.is_empty());
                    seen.push((read.offset, read.data.len()));
                    cursor = next;
                }
            }
        }
        panic!("cursor did not finish");
    };
    // From the start: each row is found once and returned once, whole.
    let run = RUN_BYTES as u64;
    assert_eq!(
        windows(0),
        (
            (0..4).map(|row| (row * run, RUN_BYTES)).collect::<Vec<_>>(),
            4
        )
    );
    // From inside a row: the row that holds the first byte starts before
    // it and is still found by one seek; the window ends with the row.
    let from = run + 3 * CELL_BYTES as u64 + 17;
    assert_eq!(
        windows(from),
        (
            vec![
                (from, (2 * run - from) as usize),
                (2 * run, RUN_BYTES),
                (3 * run, RUN_BYTES)
            ],
            3
        )
    );
    db.release_captured_reader(reader).unwrap();
}

/// Which bytes one pass reports as changed (`C`: a local byte, or zero by
/// EOF/cutoff) or inherited (`I`), with the number of `Local` steps it
/// received. This is all a normalizing caller takes from a pass.
fn classes(db: &Overlay, mut cursor: CapturedRunCursor) -> (Vec<u8>, usize) {
    let (mut classes, mut locals) = (Vec::new(), 0);
    for _ in 0..4096 {
        let reply = db.captured_run_step(cursor).unwrap();
        match reply.step {
            CapturedRunStep::Reference { .. } => panic!("no file of these proofs holds a piece"),
            CapturedRunStep::End => return (classes, locals),
            CapturedRunStep::Continue(next) => cursor = next,
            CapturedRunStep::Gap {
                kind, length, next, ..
            } => {
                let class = if kind == CapturedGap::Zero {
                    b'C'
                } else {
                    b'I'
                };
                classes.resize(classes.len() + length as usize, class);
                cursor = next;
            }
            CapturedRunStep::Local {
                offset,
                length,
                next,
            } => {
                assert!(cursor.is_shapes_only());
                assert_eq!(offset, cursor.offset());
                assert_eq!(next.offset(), offset + length);
                assert!(next.is_shapes_only());
                classes.resize(classes.len() + length as usize, b'C');
                locals += 1;
                cursor = next;
            }
            CapturedRunStep::Window { read, next } => {
                for index in 0..read.data.len() {
                    let inherited = read
                        .inherited
                        .get(index / 8)
                        .is_some_and(|bits| bits & (1 << (index % 8)) != 0);
                    classes.push(if inherited { b'I' } else { b'C' });
                }
                cursor = next;
            }
        }
    }
    panic!("cursor did not finish");
}

#[test]
fn a_shapes_only_pass_names_the_same_local_bytes_and_reads_no_dense_payload() {
    // Dense rows only: 256 KiB written in whole windows over a larger base.
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([155; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let mut file = File::new(&db, source, 41, pattern(3 * WRITE_WINDOW + 500, 3));
    file.write(RUN_BYTES as u64, &pattern(WRITE_WINDOW, 9));
    file.write(
        (RUN_BYTES + WRITE_WINDOW) as u64,
        &pattern(WRITE_WINDOW, 11),
    );
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let size = file.expect.len() as u64;
    let payload = CapturedRunCursor::new(reader, 41, 0, size).unwrap();
    let (expected, none) = classes(&db, payload);
    assert_eq!(none, 0);
    let before = db.diagnostics();
    let (shapes, locals) = classes(&db, payload.shapes_only());
    let delta = db.diagnostics().since(&before);
    assert_eq!(differences(&shapes, &expected), Vec::new());
    // Eight rows of 32 KiB join into one unit; no row's bytes are returned.
    assert_eq!(locals, 1);
    assert_eq!(
        delta.statements[StatementKind::Payload as usize].returned_blob_bytes,
        0
    );
    // From inside a row the first unit starts at that byte.
    let from = (RUN_BYTES + CELL_BYTES + 17) as u64;
    let inside = CapturedRunCursor::new(reader, 41, from, size).unwrap();
    let shapes = classes(&db, inside.shapes_only()).0;
    assert_eq!(differences(&shapes, &expected[from as usize..]), Vec::new());
    db.release_captured_reader(reader).unwrap();

    // Masked cells, a trimmed dense cell, stale rows and a shrink/regrow.
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([157; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let mut file = File::new(&db, source, 43, pattern(CELL_BYTES * 40 + 91, 5));
    for cell in 0..12 {
        file.write((cell * CELL_BYTES) as u64, b"stale");
    }
    file.resize((CELL_BYTES * 2 + 3) as u64);
    file.resize((CELL_BYTES * 40 + 91) as u64);
    file.write(7, b"masked");
    file.write((CELL_BYTES * 4) as u64, &pattern(CELL_BYTES * 3 + 100, 7));
    file.write((CELL_BYTES * 8) as u64, &pattern(RUN_BYTES, 13));
    file.write((CELL_BYTES * 16) as u64, &pattern(RUN_BYTES, 17));
    file.write((CELL_BYTES * 30 + 73) as u64, b"tail");
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let size = file.expect.len() as u64;
    let payload = CapturedRunCursor::new(reader, 43, 0, size).unwrap();
    let (expected, _) = classes(&db, payload);
    let (shapes, locals) = classes(&db, payload.shapes_only());
    assert_eq!(differences(&shapes, &expected), Vec::new());
    assert!(locals >= 2);
    for from in [5, CELL_BYTES * 4 + 9, CELL_BYTES * 7 + 50, CELL_BYTES * 9] {
        let cursor = CapturedRunCursor::new(reader, 43, from as u64, size).unwrap();
        let shapes = classes(&db, cursor.shapes_only()).0;
        assert_eq!(differences(&shapes, &expected[from..]), Vec::new());
    }
    db.release_captured_reader(reader).unwrap();
}

/// Runs where two classifications differ: `(start, end, left, right)`.
fn differences(left: &[u8], right: &[u8]) -> Vec<(usize, usize, char, char)> {
    assert_eq!(left.len(), right.len());
    let mut runs: Vec<(usize, usize, char, char)> = Vec::new();
    for (index, (a, b)) in left.iter().zip(right).enumerate() {
        if a != b {
            match runs.last_mut() {
                Some(run) if run.1 == index && run.2 == *a as char && run.3 == *b as char => {
                    run.1 = index + 1
                }
                _ => runs.push((index, index + 1, *a as char, *b as char)),
            }
        }
    }
    runs
}
