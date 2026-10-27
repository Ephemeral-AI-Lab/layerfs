//! Reference pieces through the public compound job. A claimed write reads
//! exactly like the same plain write in every piece shape; a claim replaces
//! cells and an ordinary write never touches a piece; a shrink cuts the
//! crossing piece; bytes of an earlier request join a piece only while the
//! content sequence holds; the captured run steps report a piece as one
//! `Reference`; and every piece statement is an indexed seek.
mod payload_support;
use layerfs_overlay::*;
use payload_support::*;
use std::sync::Arc;

const CELL: u64 = CELL_BYTES as u64;
const WINDOW: u64 = WRITE_WINDOW as u64;

#[test]
fn claimed_ranges_become_pieces_and_read_like_the_plain_write() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([2; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let content = noise(4 << 20, 5);
    let mut file = committed(&db, source, &content);

    // A whole aligned window of committed content: one piece and no cell.
    let at = 8 * CELL;
    file.write_claimed(
        at,
        part(&content, 0, WINDOW),
        claims(None, &[(at, WINDOW, 0)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (1, 0));
    // The next window continues the same content: the one row is extended.
    let next = at + WINDOW;
    file.write_claimed(
        next,
        part(&content, WINDOW, WINDOW),
        claims(None, &[(next, WINDOW, WINDOW)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (1, 0));

    // A claim that is not cell-aligned: its whole cells are a piece and its
    // two ragged edges are stored bytes.
    let ragged = 100 * CELL + 100;
    file.write_claimed(
        ragged,
        part(&content, 5000, 3 * CELL),
        claims(None, &[(ragged, 3 * CELL, 5000)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (2, 2));

    // Cells win inside a layer: an ordinary write inside a piece adds its
    // cell and changes no piece row.
    file.write(9 * CELL + 10, b"literal bytes inside a piece");
    file.check();
    assert_eq!(rows(&db, ()), (2, 3));
    // A claim replaces cells: the cell is deleted, and the piece that
    // already names these bytes takes them without a new row.
    file.write_claimed(
        9 * CELL,
        part(&content, CELL, CELL),
        claims(None, &[(9 * CELL, CELL, CELL)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (2, 2));

    // Other content inside a piece splits it: before, the claim, after.
    file.write_claimed(
        16 * CELL,
        part(&content, 300 * CELL, 2 * CELL),
        claims(None, &[(16 * CELL, 2 * CELL, 300 * CELL)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (4, 2));

    // Two claims of one window with literal bytes between them.
    let mut data = part(&content, 50 * CELL, 2 * CELL).to_vec();
    data.extend(noise(CELL_BYTES, 99));
    data.extend_from_slice(part(&content, 700, 2 * CELL));
    let at = 200 * CELL;
    file.write_claimed(
        at,
        &data,
        claims(
            None,
            &[(at, 2 * CELL, 50 * CELL), (at + 3 * CELL, 2 * CELL, 700)],
        ),
    );
    file.check();
    assert_eq!(rows(&db, ()), (6, 3));

    // A claim over a wide row of the layer keeps the row's cells outside it.
    file.write(232 * CELL, &noise(RUN_BYTES, 4));
    assert_eq!(rows(&db, ()), (6, 4));
    let at = 234 * CELL;
    file.write_claimed(
        at,
        part(&content, 9 * CELL, 2 * CELL),
        claims(None, &[(at, 2 * CELL, 9 * CELL)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (7, 5));
}

#[test]
fn a_shrink_cuts_the_crossing_piece_and_regrown_bytes_read_zero() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([4; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let content = noise(4 << 20, 9);
    let mut file = committed(&db, source, &content);
    // One piece of 64 cells that names other bytes than the base holds there.
    let far = 128 * CELL;
    for window in 0..2 {
        let at = window * WINDOW;
        file.write_claimed(
            at,
            part(&content, far + at, WINDOW),
            claims(None, &[(at, WINDOW, far + at)]),
        );
    }
    file.check();
    assert_eq!(rows(&db, ()), (1, 0));

    // A shrink inside a cell: the piece ends at the new size.
    file.resize(10 * CELL + 1000);
    file.check();
    file.resize(40 * CELL);
    file.check();
    // A write into the boundary cell beyond the cut.
    file.write(10 * CELL + 2000, b"x");
    file.check();
    assert_eq!(rows(&db, ()), (1, 1));

    // A claim across the cut end: the older piece gives up the range and
    // the cell inside it is deleted.
    file.write_claimed(
        8 * CELL,
        part(&content, 500 * CELL, 4 * CELL),
        claims(None, &[(8 * CELL, 4 * CELL, 500 * CELL)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (2, 0));

    // A shrink to a cell boundary, a regrow and a claim that starts there.
    file.resize(9 * CELL);
    file.check();
    file.resize(20 * CELL);
    file.check();
    file.write_claimed(
        9 * CELL,
        part(&content, 9 * CELL, 2 * CELL),
        claims(None, &[(9 * CELL, 2 * CELL, 9 * CELL)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (3, 0));

    // Pieces beyond a shrink are stale: a regrow reads zero over them, an
    // ordinary write lands above them and a claim removes the ones it meets.
    file.resize(4 * CELL);
    file.resize(30 * CELL);
    file.check();
    file.write(9 * CELL + 7, b"above a stale piece");
    file.check();
    file.write_claimed(
        8 * CELL,
        part(&content, 0, 2 * CELL),
        claims(None, &[(8 * CELL, 2 * CELL, 0)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (2, 0));

    file.resize(0);
    file.resize(5 * CELL);
    file.check();
    assert_eq!(file.read(0, 5 * CELL as u32), vec![0; 5 * CELL_BYTES]);
}

#[test]
fn bytes_of_an_earlier_request_join_the_piece_only_while_the_sequence_holds() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([6; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let content = noise(1 << 20, 13);
    let mut file = committed(&db, source, &content);
    // The file is rewritten in place with its own committed bytes.
    // Request 1 ends inside a chunk that starts at 32768: the bytes after
    // that boundary are stored as cells.
    file.write_claimed(
        0,
        part(&content, 0, 40_000),
        claims(None, &[(0, 32_768, 0)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (1, 2));
    // Request 2 completes the chunk. Its claim starts in the bytes request 1
    // stored, and nothing was published since: they join the piece.
    let after = file.revision;
    file.write_claimed(
        40_000,
        part(&content, 40_000, 40_000),
        claims(Some(after), &[(32_768, 47_232, 32_768)]),
    );
    file.check();
    assert_eq!(rows(&db, ()), (1, 1));

    // Another publication changes a byte request 2 stored. Request 3 still
    // names the earlier sequence, so its claim starts at its own window.
    let after = file.revision;
    file.write(79_000, &[content[79_000] ^ 0xff]);
    file.write_claimed(
        80_000,
        part(&content, 80_000, 40_000),
        claims(Some(after), &[(77_824, 42_176, 77_824)]),
    );
    file.check();
    assert_eq!(file.read(79_000, 1), [content[79_000] ^ 0xff]);
    // The piece of the window is a second row: 81920 is not where the first ends.
    assert_eq!(rows(&db, ()).0, 2);

    // A claim that names no sequence never takes earlier bytes.
    file.write_claimed(
        120_000,
        part(&content, 120_000, 20_000),
        claims(None, &[(118_784, 21_216, 118_784)]),
    );
    file.check();
    assert_eq!(rows(&db, ()).0, 3);
}

#[test]
fn bytes_of_an_earlier_request_after_the_window_join_the_piece_while_the_sequence_holds() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([9; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let content = noise(1 << 20, 13);
    let mut file = committed(&db, source, &content);
    // The file is rewritten in place from its end toward its start, as a
    // program that grows a file moves the tail. Request 1 begins inside a
    // chunk that ends at 98304: the bytes before that boundary are cells.
    file.write_claimed(
        90_000,
        part(&content, 90_000, 41_072),
        claims(None, &[(98_304, 32_768, 98_304)]),
    );
    file.check();
    assert_eq!(rows(&db, ()).0, 1);
    // Request 2 lies below it and holds the beginning of that chunk. Its
    // claim ends in the bytes request 1 stored and nothing was published
    // since: they join, and the two pieces are one row.
    let after = file.revision;
    file.write_claimed(
        50_000,
        part(&content, 50_000, 40_000),
        claims(Some(after), &[(65_536, 32_768, 65_536)]),
    );
    file.check();
    assert_eq!(rows(&db, ()).0, 1);

    // Another publication changes a byte request 2 stored below its piece.
    // Request 3 still names the earlier sequence: its claim ends with its
    // own window and the changed byte stays.
    let after = file.revision;
    file.write(60_000, &[content[60_000] ^ 0xff]);
    file.write_claimed(
        20_000,
        part(&content, 20_000, 30_000),
        claims(Some(after), &[(32_768, 32_768, 32_768)]),
    );
    file.check();
    assert_eq!(file.read(60_000, 1), [content[60_000] ^ 0xff]);
    assert_eq!(rows(&db, ()).0, 2);

    // A claim that names no sequence never takes later bytes.
    file.write_claimed(
        0,
        part(&content, 0, 20_000),
        claims(None, &[(0, 32_768, 0)]),
    );
    file.check();
    assert_eq!(rows(&db, ()).0, 3);
}

#[test]
fn malformed_claims_are_refused_before_any_row_changes() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([8; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let content = noise(1 << 20, 17);
    let mut file = committed(&db, source, &content);
    file.write(0, b"a first local row");
    let at = 64 * CELL;
    let job = |claims: Option<WriteClaims>, write: bool| Changes {
        inodes: vec![file.inode(1 << 20)],
        write: write.then(|| PayloadWrite {
            serial: 7,
            offset: at,
            data: Arc::from(part(&content, 0, 8 * CELL)),
        }),
        claims: claims.map(Box::new),
        ..Changes::default()
    };
    let before = db.resources().unwrap().counts;
    let many: Vec<(u64, u64, u64)> = (0..WRITE_CLAIMS as u64 + 1)
        .map(|index| (at + index * 8, 4, 0))
        .collect();
    let far = RUN_BYTES as u64 + CELL + 1;
    let refused: [&[(u64, u64, u64)]; 12] = [
        // Nothing claimed, and an empty range.
        &[],
        &[(at, 0, 0)],
        // Overlapping, and out of order.
        &[(at, 2 * CELL, 0), (at + CELL, CELL, 0)],
        &[(at + 2 * CELL, CELL, 0), (at, CELL, 0)],
        // More than one chunk beyond the window, wholly after it, a range
        // beyond it that is not the last, and wholly before it.
        &[(at + 7 * CELL, far, 0)],
        &[(at + 8 * CELL, CELL, 0)],
        &[
            (at + 6 * CELL, 3 * CELL, 0),
            (at + 9 * CELL, CELL, 4 * CELL),
        ],
        &[(at - CELL, CELL, 0)],
        // More than one chunk before the window.
        &[(at - RUN_BYTES as u64 - 1, RUN_BYTES as u64 + 2, 0)],
        // A second range that starts before the window.
        &[(at - 2 * CELL, CELL + 1, 0), (at - CELL + 1, CELL, 0)],
        // A source range that no offset can name.
        &[(at, CELL, u64::MAX - 100)],
        &many,
    ];
    for ranges in refused {
        let result = db.apply(source, &job(Some(claims(None, ranges)), true));
        assert!(
            matches!(result, Err(OverlayError::Invalid("write claims"))),
            "{ranges:?}: {result:?}"
        );
    }
    let result = db.apply(source, &job(Some(claims(None, &[(at, CELL, 0)])), false));
    assert!(matches!(result, Err(OverlayError::Invalid("write claims"))));
    assert_eq!(db.resources().unwrap().counts, before);
    file.check();
}

#[test]
fn claimed_and_plain_writes_match_a_reference_model_across_resizes_capture_and_install() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([10; 32]).unwrap();
    let mut source = db.acquire_base_source(1).unwrap();
    let mut random = Random(11);
    let content = noise(600_000, 3);
    let mut file = File::new(&db, source, 2, content.clone());
    file.sources.push((ROOT, content.clone()));
    let mut most = 0;
    for round in 0..5 {
        for step in 0..80 {
            let size = file.expect.len() as u64;
            match random.next(10) {
                0..=1 => {
                    let offset = random.next(size + 20_000);
                    let length = 1 + random.next([3, 5_000, 40_000, WINDOW][step % 4]);
                    file.write(offset, &pattern(length as usize, (round * 16 + step) as u8));
                }
                2..=5 => scattered(&mut file, &content, &mut random),
                6 => run(&mut file, &content, &mut random),
                7 => file.resize(random.next(size + 1)),
                8 => file.resize(if random.next(4) == 0 {
                    0
                } else {
                    size - size % 4096
                }),
                _ => file.resize(size + random.next(30_000)),
            }
            if step % 5 == 0 {
                file.check();
            }
        }
        file.check();
        most = most.max(rows(&db, ()).0);
        // Seal the generation. Its view stays exactly readable while the
        // next layer shrinks, regrows and claims above it.
        db.release_base_source(source).unwrap();
        let capture = db.capture().unwrap();
        let sealed = (file.expect.clone(), file.base.clone());
        source = db.acquire_base_source(100 + round as u64).unwrap();
        file.source = source;
        file.resize(file.expect.len() as u64 / 3);
        scattered(&mut file, &content, &mut random);
        run(&mut file, &content, &mut random);
        file.check();
        let mut offset = 0;
        while offset < sealed.0.len() {
            let local = db
                .captured_read(capture, 2, offset as u64, READ_WINDOW as u32)
                .unwrap()
                .unwrap();
            assert_eq!(local.size, sealed.0.len() as u64);
            let got = file.compose(&local, &sealed.1);
            assert!(!got.is_empty());
            assert!(
                got == sealed.0[offset..offset + got.len()],
                "sealed window at {offset}"
            );
            offset += got.len();
        }
        // The run steps of the sealed file give the same bytes, and a
        // shapes-only pass the same classes without reading a local byte.
        let reader = db
            .acquire_captured_reader(capture, 500 + round as u64)
            .unwrap();
        let cursor = CapturedRunCursor::new(reader, 2, 0, sealed.0.len() as u64).unwrap();
        let (bytes, classes, _) = walk(&db, &file, &sealed.1, cursor);
        assert!(bytes == sealed.0, "sealed run steps");
        let (_, shapes, _) = walk(&db, &file, &sealed.1, cursor.shapes_only());
        assert!(shapes == classes, "sealed shapes");
        db.release_captured_reader(reader).unwrap();
        // Known install: the next base is exactly the sealed view. Pieces of
        // the layer above keep naming their own source.
        db.release_base_source(source).unwrap();
        db.install(capture, [round as u8 + 20; 32]).unwrap();
        source = db.acquire_base_source(200 + round as u64).unwrap();
        file.source = source;
        file.base = sealed.0;
        file.check();
    }
    assert!(most > 8, "{most}");
}

#[test]
fn piece_statements_are_indexed_and_their_work_does_not_grow_with_the_piece() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([12; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();

    // The runtime profile of the same statements: `work` fails on a full
    // scan step, a sort or an automatic index of any statement family.
    let content = noise(8 << 20, 21);
    let mut file = committed(&db, source, &content);
    let far = 1 << 20;
    let claim = |file: &mut File, window: u64| {
        let at = window * WINDOW;
        work(&db, || {
            file.write_claimed(
                at,
                part(&content, far + at, WINDOW),
                claims(None, &[(at, WINDOW, far + at)]),
            )
        })
    };
    let first = claim(&mut file, 0);
    let second = claim(&mut file, 1);
    let read_short = work(&db, || drop(file.read(WINDOW + 5, READ_WINDOW as u32)));
    for window in 2..48 {
        claim(&mut file, window);
    }
    let last = claim(&mut file, 48);
    let read_long = work(&db, || drop(file.read(40 * WINDOW + 5, READ_WINDOW as u32)));
    assert_eq!(rows(&db, ()), (1, 0));
    println!(
        "PIECE_WORK first={first:?} second={second:?} last={last:?} read_short={read_short:?} \
         read_long={read_long:?}"
    );
    // Extending a piece of one window and of 48 windows is the same work, and
    // so is a read inside it: statements and changed rows are equal, VM steps
    // differ by at most the depth of the index.
    assert_eq!((second.0, second.2), (last.0, last.2));
    assert!(last.1 <= second.1 + 16, "{second:?} {last:?}");
    assert_eq!((read_short.0, read_short.2), (read_long.0, read_long.2));
    assert!(
        read_long.1 <= read_short.1 + 16,
        "{read_short:?} {read_long:?}"
    );
    file.check();
}

/// Steps of one pass over a captured file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Steps {
    units: usize,
    rows: u64,
    gaps: usize,
    windows: usize,
    references: usize,
    locals: usize,
}
/// One pass of the captured run cursor: every byte of the file, its class
/// (`I` inherited at its own offset, `R` supplied by a reference, `C` local
/// or zero) and the steps taken. A `Local` step carries no bytes: its bytes
/// are left zero here.
fn walk(
    db: &Overlay,
    file: &File,
    base: &[u8],
    mut cursor: CapturedRunCursor,
) -> (Vec<u8>, Vec<u8>, Steps) {
    let (mut bytes, mut classes, mut steps) = (Vec::new(), Vec::new(), Steps::default());
    for _ in 0..200_000 {
        let reply = db.captured_run_step(cursor).unwrap();
        steps.units += 1;
        steps.rows += reply.work.metadata_rows;
        assert!(reply.work.metadata_rows <= 12);
        let at = cursor.offset();
        match reply.step {
            CapturedRunStep::End => return (bytes, classes, steps),
            CapturedRunStep::Continue(next) => {
                assert!(next.metadata_advanced_from(cursor));
                cursor = next;
            }
            CapturedRunStep::Gap {
                kind,
                offset,
                length,
                next,
            } => {
                assert_eq!((offset, next.offset()), (at, at + length));
                steps.gaps += 1;
                let (from, to) = (at as usize, (at + length) as usize);
                let end = bytes.len() + length as usize;
                if kind == CapturedGap::Zero {
                    classes.resize(end, b'C');
                } else {
                    // Beyond the base's own end an inherited byte is zero.
                    bytes.extend_from_slice(&base[from.min(base.len())..to.min(base.len())]);
                    classes.resize(end, b'I');
                }
                bytes.resize(end, 0);
                cursor = next;
            }
            CapturedRunStep::Local {
                offset,
                length,
                next,
            } => {
                assert!(cursor.is_shapes_only());
                assert_eq!((offset, next.offset()), (at, at + length));
                steps.locals += 1;
                bytes.resize(bytes.len() + length as usize, 0);
                classes.resize(classes.len() + length as usize, b'C');
                cursor = next;
            }
            CapturedRunStep::Reference {
                offset,
                length,
                root,
                source_offset,
                next,
            } => {
                assert_eq!((offset, next.offset()), (at, at + length));
                assert_ne!(length, 0);
                steps.references += 1;
                let source = file.sources.iter().find(|(known, _)| *known == root);
                bytes.extend_from_slice(part(&source.unwrap().1, source_offset, length));
                classes.resize(classes.len() + length as usize, b'R');
                cursor = next;
            }
            CapturedRunStep::Window { read, next } => {
                assert_eq!(read.offset, at);
                assert_eq!(next.offset(), at + read.data.len() as u64);
                assert!(read.data.len() <= RUN_BYTES);
                steps.windows += 1;
                for index in 0..read.data.len() {
                    let inherited = read
                        .inherited
                        .get(index / 8)
                        .is_some_and(|bits| bits & (1 << (index % 8)) != 0);
                    let referenced = read
                        .referenced
                        .get(index / 8)
                        .is_some_and(|bits| bits & (1 << (index % 8)) != 0);
                    classes.push(match (inherited, referenced) {
                        (true, _) => b'I',
                        (_, true) => b'R',
                        _ => b'C',
                    });
                }
                bytes.extend(file.compose(&read, base));
                cursor = next;
            }
        }
    }
    panic!("cursor did not finish");
}

#[test]
fn captured_steps_report_a_piece_as_one_reference_and_equal_the_sealed_view() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([14; 32]).unwrap();
    let source = db.acquire_base_source(1).unwrap();
    let content = noise(4 << 20, 29);
    let mut file = committed(&db, source, &content);
    // A long piece with a masked cell and a dense row above it, a short
    // piece, plain cells, and a shrink with a regrow beyond the base bytes.
    for window in 0..2 {
        let at = window * WINDOW;
        file.write_claimed(
            at,
            part(&content, 100 * CELL + at, WINDOW),
            claims(None, &[(at, WINDOW, 100 * CELL + at)]),
        );
    }
    file.write(3 * CELL + 9, b"a masked cell above the piece");
    file.write(10 * CELL, &noise(2 * CELL_BYTES, 3));
    file.write_claimed(
        70 * CELL,
        part(&content, 500 * CELL, 4 * CELL),
        claims(None, &[(70 * CELL, 4 * CELL, 500 * CELL)]),
    );
    file.write(80 * CELL, &noise(2 * CELL_BYTES, 4));
    file.resize(300 * CELL);
    file.resize(310 * CELL + 17);
    file.check();
    db.release_base_source(source).unwrap();
    let capture = db.capture().unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let size = file.expect.len() as u64;
    let cursor = CapturedRunCursor::new(reader, 7, 0, size).unwrap();

    let (bytes, classes, steps) = walk(&db, &file, &file.base, cursor);
    assert!(bytes == file.expect);
    println!("PIECE_STEPS {steps:?}");
    // Reference to the masked cell, its window, reference to the dense row,
    // its window, reference to the end of the long piece, inherited, the
    // short piece, inherited, the plain row, inherited, zero.
    assert_eq!((steps.gaps, steps.windows, steps.references), (4, 3, 4));
    let class = |at: u64| classes[at as usize];
    for (at, expected) in [
        (0, b'R'),
        (3 * CELL, b'R'),
        (3 * CELL + 9, b'C'),
        (4 * CELL, b'R'),
        (10 * CELL, b'C'),
        (12 * CELL, b'R'),
        (64 * CELL - 1, b'R'),
        (64 * CELL, b'I'),
        (70 * CELL, b'R'),
        (74 * CELL, b'I'),
        (80 * CELL, b'C'),
        (299 * CELL, b'I'),
        (300 * CELL, b'C'),
        (size - 1, b'C'),
    ] {
        assert_eq!(class(at), expected, "byte {at}");
    }
    // A shapes-only pass: the same classes, the two dense rows without
    // their payload, the masked cell still as its composed window.
    let (_, shapes, shaped) = walk(&db, &file, &file.base, cursor.shapes_only());
    assert!(shapes == classes);
    assert_eq!(
        (
            shaped.gaps,
            shaped.windows,
            shaped.references,
            shaped.locals
        ),
        (4, 1, 4, 2)
    );
    // A pass that starts inside a piece seeks it by key.
    let inside = CapturedRunCursor::new(reader, 7, 40 * CELL + 5, size).unwrap();
    let (tail, _, from_inside) = walk(&db, &file, &file.base, inside);
    assert!(tail == file.expect[(40 * CELL + 5) as usize..]);
    assert_eq!(from_inside.references, 2);
}

#[test]
fn captured_steps_do_not_grow_with_the_length_of_a_piece() {
    let content = noise(8 << 20, 33);
    // A file whose first `windows` windows are one piece with one written
    // cell in its second window; the rest is inherited.
    let pass = |_tag: u8, windows: u64| {
        let temp = Temp::new();
        let db = temp.db();
        db.open_workspace([16; 32]).unwrap();
        let source = db.acquire_base_source(1).unwrap();
        let mut file = committed(&db, source, &content);
        for window in 0..windows {
            let at = window * WINDOW;
            file.write_claimed(
                at,
                part(&content, CELL + at, WINDOW),
                claims(None, &[(at, WINDOW, CELL + at)]),
            );
        }
        file.write(WINDOW, &noise(CELL_BYTES, 8));
        assert_eq!(rows(&db, ()), (1, 1));
        db.release_base_source(source).unwrap();
        let capture = db.capture().unwrap();
        let reader = db.acquire_captured_reader(capture, 1).unwrap();
        let cursor = CapturedRunCursor::new(reader, 7, 0, file.expect.len() as u64).unwrap();
        let mut steps = Steps::default();
        let cost = work(&db, || {
            let (bytes, _, taken) = walk(&db, &file, &file.base, cursor);
            assert!(bytes == file.expect);
            steps = taken;
        });
        (steps, cost)
    };
    let (short, short_cost) = pass(15, 2);
    let (long, long_cost) = pass(17, 48);
    println!("PIECE_STEP_COST short={short:?} {short_cost:?} long={long:?} {long_cost:?}");
    // Reference, the cell's window, reference, inherited: the same steps,
    // rows and statements for a piece of 256 KiB and of 6 MiB.
    assert_eq!(short, long);
    assert_eq!((short.gaps, short.windows, short.references), (1, 1, 2));
    assert_eq!((short_cost.0, short_cost.2), (long_cost.0, long_cost.2));
    assert!(
        long_cost.1 <= short_cost.1 + 32,
        "{short_cost:?} {long_cost:?}"
    );
}
