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
            CapturedRunStep::End => return (bytes, work),
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
                assert!(read.data.len() <= CELL_BYTES);
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
    let route = db.open_workspace([141; 32], [142; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
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
    let capture = db.capture(route).unwrap();
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
    assert_eq!(
        delta.statements[StatementKind::Workspace as usize].returned_blob_bytes,
        64
    );
    println!("CAPTURED_GAP_ROOT_METADATA {:?}", delta);
    db.release_captured_reader(reader).unwrap();
}

#[test]
fn masked_cells_and_shrink_regrow_match_independent_final_bytes_after_install() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([143; 32], [144; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, 19, pattern(CELL_BYTES * 3 + 91, 5));
    file.write(7, b"local");
    file.write(CELL_BYTES as u64 + 13, b"second");
    file.resize((CELL_BYTES * 2 + 3) as u64);
    file.resize((CELL_BYTES * 3 + 91) as u64);
    file.write((CELL_BYTES * 3 + 73) as u64, b"tail");
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let cursor = CapturedRunCursor::new(reader, 19, 0, file.expect.len() as u64).unwrap();
    let (bytes, _) = collect(&db, cursor, &file.base);
    assert_eq!(bytes, file.expect);
    db.install(capture, [145; 32]).unwrap();
    let (bytes, _) = collect(&db, cursor, &file.base);
    assert_eq!(bytes, file.expect);
    db.close(route).unwrap();
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
    let route = db.open_workspace([146; 32], [147; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, 23, Vec::new());
    for index in 0..96 {
        file.write((index * CELL_BYTES) as u64, b"stale");
    }
    file.resize(0);
    file.resize((CELL_BYTES * 97) as u64);
    file.write((CELL_BYTES * 96) as u64, b"live");
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let cursor = CapturedRunCursor::new(reader, 23, 0, file.expect.len() as u64).unwrap();
    let plans = db.explain_captured_run(cursor).unwrap();
    assert!(plans
        .iter()
        .any(|row| row.starts_with("captured-cell-metadata: SEARCH")));
    assert!(plans
        .iter()
        .any(|row| row.starts_with("captured-cell-metadata-vm:")));
    assert!(!plans
        .iter()
        .any(|row| row.contains("TEMP B-TREE") || row.contains(": SCAN")));
    println!("CAPTURED_RUN_PLAN {}", plans.join("\n"));
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
    let route = db.open_workspace([148; 32], [149; 32]).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let cursor = CapturedRunCursor::new(reader, 29, 0, 101).unwrap();
    let reply = db.captured_run_step(cursor).unwrap();
    let CapturedRunStep::Gap { kind, length, .. } = reply.step else {
        panic!("expected gap")
    };
    assert_eq!((kind, length), (CapturedGap::Inherited, 101));
    assert_eq!(db.retained_captured_reader(route, 1).unwrap(), Some(reader));
    db.release_captured_reader(reader).unwrap();

    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([150; 32], [151; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, 31, Vec::new());
    file.write(0, b"abc");
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let cursor = CapturedRunCursor::new(reader, 31, 0, 4).unwrap();
    assert!(matches!(
        db.captured_run_step(cursor),
        Err(OverlayError::Invalid("captured run size"))
    ));
    assert_eq!(db.retained_captured_reader(route, 2).unwrap(), Some(reader));
    db.release_captured_reader(reader).unwrap();
}

#[test]
fn interleaved_live_and_stale_cells_keep_each_seek_and_trimmed_start_once() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([152; 32], [153; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
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
    let capture = db.capture(route).unwrap();
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
