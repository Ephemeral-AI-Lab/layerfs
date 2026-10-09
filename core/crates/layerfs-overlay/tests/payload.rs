//! Public payload proofs through the production overlay.
mod payload_support;
use layerfs_overlay::*;
use payload_support::*;
use std::sync::Arc;

#[test]
fn layered_reads_match_a_reference_model_across_writes_resizes_capture_and_install() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([1; 32], [2; 32]).unwrap();
    let mut source = db.acquire_base_source(route, 1).unwrap();
    let mut random = Random(7);
    // An inherited 300,001-byte base file and a file created locally.
    let mut inherited = File::new(&db, source, 2, pattern(300_001, 3));
    let mut created = File::new(&db, source, 9, Vec::new());
    assert_eq!(inherited.read(299_990, 64), pattern(300_001, 3)[299_990..]);
    for round in 0..6 {
        for file in [&mut inherited, &mut created] {
            for step in 0..60 {
                let size = file.expect.len() as u64;
                match random.next(10) {
                    // Writes inside, across and beyond EOF, of every alignment.
                    0..=5 => {
                        let offset = random.next(size + 20_000);
                        let length = 1 + random.next(match step % 4 {
                            0 => 3,
                            1 => 5_000,
                            2 => 40_000,
                            _ => WRITE_WINDOW as u64,
                        });
                        let data = pattern(length as usize, (round * 16 + step) as u8);
                        file.write(offset, &data);
                    }
                    6 => file.write(size, &pattern(1 + random.next(200) as usize, 99)),
                    // Shrinks to every kind of boundary, including zero.
                    7 => file.resize(random.next(size + 1)),
                    8 => file.resize(if random.next(3) == 0 {
                        0
                    } else {
                        size - size % 4096
                    }),
                    _ => file.resize(size + random.next(30_000)),
                }
                if step % 6 == 0 {
                    file.check();
                }
            }
            file.check();
        }
        // Seal the generation. The sealed view stays exactly readable while
        // the next layer shrinks, regrows and overwrites above it.
        db.release_base_source(source).unwrap();
        let capture = db.capture(route).unwrap();
        let sealed: Vec<(u64, Vec<u8>, Vec<u8>)> = [&inherited, &created]
            .iter()
            .map(|file| (file.serial, file.expect.clone(), file.base.clone()))
            .collect();
        source = db.acquire_base_source(route, 100 + round).unwrap();
        for file in [&mut inherited, &mut created] {
            file.source = source;
            let size = file.expect.len() as u64;
            file.resize(size / 3);
            file.write(size / 2 + 5, b"regrown after a shrink");
            file.check();
        }
        for (serial, expect, base) in &sealed {
            let mut offset = 0;
            while offset < expect.len() {
                let local = db
                    .captured_read(capture, *serial, offset as u64, READ_WINDOW as u32)
                    .unwrap()
                    .unwrap();
                assert_eq!(local.size, expect.len() as u64);
                for (slot, byte) in local.data.iter().enumerate() {
                    let at = offset + slot;
                    let inherit = local
                        .inherited
                        .get(slot / 8)
                        .is_some_and(|bits| bits & (1 << (slot % 8)) != 0);
                    let got = if inherit {
                        base.get(at).copied().unwrap_or(0)
                    } else {
                        *byte
                    };
                    assert_eq!(got, expect[at], "sealed serial {serial} byte {at}");
                }
                offset += local.data.len().max(1);
            }
        }
        // Known install: the next base is exactly the sealed view. Later
        // layers keep their bytes without being rewritten.
        if round % 2 == 1 {
            db.release_base_source(source).unwrap();
            db.install(capture, [round as u8 + 10; 32]).unwrap();
            source = db.acquire_base_source(route, 200 + round).unwrap();
            for (file, (_, expect, _)) in [&mut inherited, &mut created].into_iter().zip(sealed) {
                file.source = source;
                file.base = expect;
                file.check();
            }
        } else {
            // No resolution API exists before S6: a closed capture is the only
            // release. Keep the layer live by installing in the next round.
            db.release_base_source(source).unwrap();
            db.install(capture, [round as u8 + 10; 32]).unwrap();
            source = db.acquire_base_source(route, 200 + round).unwrap();
            for (file, (_, expect, _)) in [&mut inherited, &mut created].into_iter().zip(sealed) {
                file.source = source;
                file.base = expect;
                file.check();
            }
        }
    }
}

#[test]
fn shrink_work_is_independent_of_discarded_data_and_regrow_reads_zero() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([5; 32], [6; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let window = pattern(WRITE_WINDOW, 1);
    let mut small = File::new(&db, source, 30, pattern(9_000, 2));
    small.write(0, &window[..6_000]);
    // 2,048 written cells over an inherited 8 MiB file.
    let mut large = File::new(&db, source, 31, pattern(8 << 20, 4));
    for at in (0..8_u64 << 20).step_by(WRITE_WINDOW) {
        large.write(at, &window);
    }
    let pages_before = db.pages().unwrap();
    let shrink_small = work(&db, || small.resize(5));
    let shrink_large = work(&db, || large.resize(5));
    assert_eq!(
        (shrink_large.0, shrink_large.2),
        (shrink_small.0, shrink_small.2),
        "no discarded row is visited"
    );
    // The boundary row of the large file is 32 KiB and is cut without being
    // loaded; the small file's one-cell row is returned by its lookup.
    assert!(shrink_large.1 <= shrink_small.1, "{shrink_large:?}");
    // Discarded rows are debt, not foreground work: only the two cut
    // boundary rows gave anything back, one cell and one row of eight.
    let pages_after = db.pages().unwrap();
    assert_eq!(pages_after.0, pages_before.0);
    assert!(pages_after.1 - pages_before.1 <= 1 + 8, "{pages_after:?}");
    for file in [&mut small, &mut large] {
        file.check();
        // Regrow far past the old data: neither the stale local cells nor the
        // inherited bytes above the cut come back.
        let end = file.base.len() as u64;
        file.resize(end);
        file.check();
        assert!(file.read(4_096, 8_192).iter().all(|byte| *byte == 0));
        file.write(end - 3, b"tail");
        file.write(8_190, b"over a stale cell boundary");
        file.check();
    }
    // Zero truncate then rewrite, as O_TRUNC does.
    let truncate = work(&db, || large.resize(0));
    large.write(0, b"new content");
    large.check();
    println!(
        "S5_SHRINK discarded_cells_small=2 discarded_cells_large=2048 statements={} vm={} rows_changed={} identical=true zero_truncate_statements={} fullscan=0 sorts=0",
        shrink_large.0, shrink_large.1, shrink_large.2, truncate.0
    );
}

#[test]
fn a_tall_shrink_staircase_keeps_logarithmic_lookups_and_exact_bytes() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([7; 32], [8; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, 40, Vec::new());
    const STEPS: u64 = 1_500;
    // Fill every cell, then repeatedly regrow one cell further and shrink back
    // to a boundary one cell higher than before: no step dominates another.
    file.write(0, &pattern(8 * 4096, 6));
    for extra in 0..STEPS / 32 + 2 {
        file.write((8 + extra * 32) * 4096, &pattern(WRITE_WINDOW, extra as u8));
    }
    let mut shrink_work = Vec::new();
    for step in 0..STEPS {
        let boundary = (8 + step) * 4096 + 100;
        let top = file.expect.len() as u64;
        let cost = work(&db, || file.resize(boundary));
        shrink_work.push(cost.0);
        file.write(top - 1, &[7]);
        if step % 300 == 0 {
            file.check();
        }
    }
    file.check();
    // Cells above each boundary predate that shrink and stay hidden; a read
    // across many steps pays a binary search per old cell, not a step walk.
    // The cells above the last boundary still exist and are all stale.
    let across = work(&db, || {
        let got = file.read((8 + STEPS + 2) * 4096, READ_WINDOW as u32);
        assert!(got.len() > 30 * 4096 && got[..got.len() - 1].iter().all(|byte| *byte == 0));
    });
    assert!(
        across.0 > 4 + 30,
        "the stale cells were tested: {}",
        across.0
    );
    let steps = (STEPS as f64).log2().ceil() as u64 + 1;
    assert!(
        across.0 <= 8 + 33 * (steps + 1),
        "read statements {} exceed a binary search per cell",
        across.0
    );
    let (first, last) = (shrink_work[1], *shrink_work.last().unwrap());
    // Two binary searches: the boundary cell's staleness and the dominated
    // suffix of the staircase.
    assert!(
        last <= first + 2 * (steps + 1),
        "shrink grew from {first} to {last} statements over {STEPS} steps"
    );
    // One shrink below the whole staircase abandons every step in logarithmic
    // work and hides all of it.
    let collapse = work(&db, || file.resize(3 * 4096 + 1));
    assert!(
        collapse.0 <= first + 2 * (steps + 1),
        "collapse {}",
        collapse.0
    );
    file.check();
    let end = (8 + STEPS) * 4096;
    file.resize(end);
    file.check();
    assert!(file
        .read(4 * 4096, READ_WINDOW as u32)
        .iter()
        .all(|byte| *byte == 0));
    println!(
        "S5_STAIRCASE steps={STEPS} first_shrink_statements={first} last_shrink_statements={last} collapse_statements={} read_window_across_steps_statements={} bound_per_cell_log2={steps} fullscan=0 sorts=0",
        collapse.0, across.0
    );
}

#[test]
fn holes_cost_no_rows_and_tiny_and_dense_files_report_measured_pages() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([9; 32], [10; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    // One byte at one tebibyte: the hole below it is no row at all.
    let (start, _) = db.pages().unwrap();
    let mut sparse = File::new(&db, source, 50, Vec::new());
    let far = 1_u64 << 40;
    let publication = db
        .apply(
            source,
            &Changes {
                inodes: vec![sparse.inode(far + 1)],
                write: Some(PayloadWrite {
                    serial: 50,
                    offset: far,
                    data: Arc::from(&b"x"[..]),
                }),
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(publication).unwrap();
    sparse.touched = true;
    let hole = work(&db, || {
        let local = db
            .source_read(source, 50, far / 2, READ_WINDOW as u32)
            .unwrap()
            .unwrap();
        assert_eq!((local.size, local.data.len()), (far + 1, READ_WINDOW));
        assert!(local.data.iter().all(|byte| *byte == 0) && local.span.is_none());
    });
    let tail = db.source_read(source, 50, far - 2, 64).unwrap().unwrap();
    assert_eq!(tail.data, [0, 0, b'x']);
    let (after_sparse, _) = db.pages().unwrap();
    println!(
        "S5_HOLE logical_bytes={} pages_added={} hole_window_read_statements={} vm={}",
        far + 1,
        after_sparse - start,
        hole.0,
        hole.1
    );
    assert!(after_sparse - start <= 4);

    // 2,000 files of 100 bytes: trimmed rows, not fixed 4.6 KiB cells.
    let tiny = pattern(100, 8);
    for serial in 1_000..3_000 {
        let mut file = File::new(&db, source, serial, Vec::new());
        file.write(0, &tiny);
    }
    let (after_tiny, _) = db.pages().unwrap();
    let per_tiny = (after_tiny - after_sparse) as f64 * 4096.0 / 2_000.0;
    // One dense 8 MiB file: 2,048 full cells.
    let mut dense = File::new(&db, source, 60, Vec::new());
    let window = pattern(WRITE_WINDOW, 3);
    for at in (0..8_u64 << 20).step_by(WRITE_WINDOW) {
        dense.write(at, &window);
    }
    let (after_dense, free) = db.pages().unwrap();
    let per_cell = (after_dense - after_tiny) as f64 / 2_048.0;
    // A one-byte append rewrites one stored cell, never the file.
    let mut log = File::new(&db, source, 61, Vec::new());
    log.write(0, &pattern(3_000, 1));
    let append = work(&db, || log.write(3_000, b"!"));
    log.check();
    println!(
        "S5_SPACE tiny_files=2000 logical_bytes_each=100 bytes_on_disk_each={per_tiny:.0} amplification={:.2} dense_cells=2048 pages_per_cell={per_cell:.3} amplification={:.3} freelist={free} one_byte_append statements={} vm={} bound_bytes_rewritten<=4096",
        per_tiny / 100.0,
        per_cell,
        append.0,
        append.1
    );
    assert!(
        per_tiny < 1_024.0,
        "a tiny file must not cost a fixed cell: {per_tiny}"
    );
    assert!(per_cell < 1.25, "dense pages per cell: {per_cell}");
}
