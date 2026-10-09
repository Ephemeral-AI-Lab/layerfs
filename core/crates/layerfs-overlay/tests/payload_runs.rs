//! Byte-exact proofs of the shapes a payload row of several cells must
//! survive: windows larger than one row, overwrites inside and across rows,
//! shrinks inside a row, sparse cell puts, capture, failed-capture
//! composition, install and orphan moves. Every byte is incompressible.
mod payload_support;
use layerfs_overlay::*;
use payload_support::*;
use std::sync::Arc;

const CELL: usize = CELL_BYTES;
/// Incompressible bytes: a 64-bit xorshift stream, never a repeated pattern.
fn noise(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}
fn maintain(db: &Overlay) {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..20_000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            return;
        };
        assert!(step.work.rows <= 64 && step.work.data_bytes <= 65536);
        cursor = step.cursor;
    }
    panic!("maintenance deadline");
}
/// Replaces one whole cell through the public compound job. Bytes the mask
/// leaves out fall through to the base below `cutoff` and read zero above.
fn put_cell(file: &mut File<'_>, cell: usize, data: &[u8], valid: &[bool], cutoff: usize) {
    let mut stored = Cell {
        offset: cell as u64,
        data: Box::new([0; CELL_BYTES]),
        validity: Box::new([0; MASK_BYTES]),
    };
    let size = file.expect.len().max(cell + CELL);
    file.expect.resize(size, 0);
    for at in 0..CELL {
        if valid[at] {
            stored.data[at] = data[at];
            stored.validity[at / 8] |= 1 << (at % 8);
            file.expect[cell + at] = data[at];
        } else {
            file.expect[cell + at] = if cell + at < cutoff {
                file.base.get(cell + at).copied().unwrap_or(0)
            } else {
                0
            };
        }
    }
    let publication = file
        .db
        .apply(
            file.source,
            &Changes {
                inodes: vec![file.inode(size as u64)],
                cell: Some((file.serial, stored)),
                ..Changes::default()
            },
        )
        .unwrap();
    file.db.reply_attempted(publication).unwrap();
    file.touched = true;
}
/// The sealed view of a capture, composed like a captured reader does.
fn sealed(db: &Overlay, capture: Capture, serial: u64, base: &[u8], size: usize) -> Vec<u8> {
    let mut view = Vec::new();
    while view.len() < size {
        let offset = view.len();
        let local = db
            .captured_read(capture, serial, offset as u64, READ_WINDOW as u32)
            .unwrap()
            .unwrap();
        assert_eq!(local.size, size as u64);
        assert!(!local.data.is_empty());
        for (slot, byte) in local.data.iter().enumerate() {
            let inherit = local
                .inherited
                .get(slot / 8)
                .is_some_and(|bits| bits & (1 << (slot % 8)) != 0);
            view.push(if inherit {
                base.get(offset + slot).copied().unwrap_or(0)
            } else {
                *byte
            });
        }
    }
    view
}

#[test]
fn rows_of_several_cells_match_a_reference_model_through_every_writer_and_reader() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([41; 32], [42; 32]).unwrap();
    let mut source = db.acquire_base_source(route, 1).unwrap();
    let base = noise(300_001, 1);
    let mut inherited = File::new(&db, source, 2, base.clone());
    let mut created = File::new(&db, source, 9, Vec::new());
    let window = noise(WRITE_WINDOW, 2);
    let mut seed = 10;
    for file in [&mut inherited, &mut created] {
        let cutoff = file.base.len();
        // Windows larger than one row, aligned and straddling every boundary.
        file.write(0, &window);
        file.write(WRITE_WINDOW as u64, &window[..100_000]);
        file.write(100_000, &noise(WRITE_WINDOW, seed));
        file.check();
        // Overwrites inside one row, at a cell boundary, across two rows and
        // across an unaligned boundary between rows.
        file.write(8_192, &noise(CELL, seed + 1));
        file.write(12_288, &[0xA7]);
        file.write(28_672, &noise(3 * CELL, seed + 2));
        file.write(32_766, &noise(3, seed + 3));
        file.write(20_000, &noise(40_000, seed + 4));
        file.write(65_535, &noise(2, seed + 5));
        file.check();
        // A sparse cell put inside a row: half its bytes, every byte, none.
        let data = noise(CELL, seed + 6);
        let half: Vec<bool> = (0..CELL).map(|at| at % 3 != 0 && at < 3_000).collect();
        put_cell(file, 16_384, &data, &half, cutoff);
        put_cell(file, 49_152, &data, &[true; CELL], cutoff);
        put_cell(file, 36_864, &data, &[false; CELL], cutoff);
        put_cell(file, 0, &data, &half, cutoff);
        file.check();
        file.write(16_000, &noise(1_000, seed + 7));
        file.write(0, &window);
        file.check();
        // Shrinks strictly inside a row, to a cell boundary inside a row and
        // to a boundary between rows; each regrow reads zero.
        for (to, back) in [
            (20_000_u64, 70_000_u64),
            (16_384, 40_000),
            (32_768, 200_000),
            (36_869, 36_870),
            (5, 140_000),
        ] {
            file.write(0, &window);
            file.check();
            file.resize(to);
            file.check();
            file.resize(back);
            file.check();
            file.write(to + 3, &noise(9_000, seed + to));
            file.write(24_000, &noise(10_000, seed + 8));
            file.check();
        }
        file.write(0, &window);
        file.write(WRITE_WINDOW as u64, &window);
        file.check();
        seed += 100;
    }
    maintain(&db);
    for file in [&inherited, &created] {
        file.check();
    }
    for round in 0..4_u64 {
        // Seal the generation, then overwrite, shrink and regrow above it.
        db.release_base_source(source).unwrap();
        let capture = db.capture(route).unwrap();
        let views: Vec<(u64, Vec<u8>, Vec<u8>)> = [&inherited, &created]
            .iter()
            .map(|file| (file.serial, file.expect.clone(), file.base.clone()))
            .collect();
        source = db.acquire_base_source(route, 100 + round).unwrap();
        for file in [&mut inherited, &mut created] {
            file.source = source;
            file.write(40_960, &noise(CELL, seed + round));
            file.write(70_001, &noise(40_000, seed + round + 1));
            file.write(12_288, &[0x5B]);
            file.check();
            if round % 2 == 0 {
                file.resize(50_000);
                file.resize(262_144);
                file.write(60_000, &noise(70_000, seed + round + 2));
                file.check();
            }
        }
        for (serial, expect, base) in &views {
            assert_eq!(&sealed(&db, capture, *serial, base, expect.len()), expect);
        }
        db.release_base_source(source).unwrap();
        if round % 2 == 0 {
            // A failed capture folds the sealed rows into the active layer.
            db.resolve_failed_capture(capture).unwrap();
            maintain(&db);
            assert!(db.capture_ready(route).unwrap());
            source = db.acquire_base_source(route, 200 + round).unwrap();
            for file in [&mut inherited, &mut created] {
                file.source = source;
                file.check();
            }
        } else {
            // Known install: the next base is exactly the sealed view.
            db.install(capture, [round as u8 + 50; 32]).unwrap();
            maintain(&db);
            source = db.acquire_base_source(route, 200 + round).unwrap();
            for (file, (_, expect, _)) in [&mut inherited, &mut created].into_iter().zip(views) {
                file.source = source;
                file.base = expect;
                file.check();
            }
        }
        for file in [&mut inherited, &mut created] {
            file.write(0, &window);
            file.write(
                WRITE_WINDOW as u64 - 5,
                &noise(WRITE_WINDOW, seed + round + 3),
            );
            file.check();
        }
        seed += 10;
    }
    let state = db.resources(Some(route)).unwrap();
    println!(
        "R7_RUN_MODEL payload_rows={} payload_bytes={}",
        state.counts.payload_cells, state.counts.payload_bytes
    );
}

fn orphan_bytes(db: &Overlay, read: FileRead, base: &[u8], size: usize) -> Vec<u8> {
    let mut view = Vec::new();
    while view.len() < size {
        let offset = view.len();
        let plan = db
            .read_file(read, offset as u64, READ_WINDOW as u32)
            .unwrap()
            .unwrap();
        assert_eq!(plan.size, size as u64);
        for (slot, byte) in plan.data.iter().enumerate() {
            let inherit = plan
                .inherited
                .get(slot / 8)
                .is_some_and(|b| b & (1 << (slot % 8)) != 0);
            view.push(if inherit {
                base.get(offset + slot).copied().unwrap_or(0)
            } else {
                *byte
            });
        }
    }
    view
}

#[test]
fn an_orphan_keeps_every_byte_while_rows_of_several_cells_move_into_its_domain() {
    for inherited in [false, true] {
        let temp = Temp::new();
        let db = temp.db();
        let route = db.open_workspace([43; 32], [44; 32]).unwrap();
        let source = db.acquire_base_source(route, 1).unwrap();
        let base = if inherited {
            noise(300_001, 3)
        } else {
            Vec::new()
        };
        let mut file = File::new(&db, source, 7, base);
        let window = noise(WRITE_WINDOW, 4);
        for at in [0, 1, 2] {
            file.write(at * WRITE_WINDOW as u64, &window);
        }
        // Rows of one cell and a masked row beside the wide ones.
        file.write(3 * WRITE_WINDOW as u64 + 100, &noise(5_000, 5));
        file.write(40_000, &noise(100, 6));
        file.check();
        let size = file.expect.len() as u64;
        let open = db.open_file(source, 1, &file.inode(size), true).unwrap();
        let mut removed = file.inode(size);
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
        let mut expect = file.expect.clone();
        // Descriptor writes land in the orphan's own domain before any row
        // of the lower layer has moved: one byte inside a lower row, a whole
        // lower row, an unaligned span across two lower rows.
        let write = |expect: &mut Vec<u8>, offset: usize, data: &[u8], size: usize| {
            if size < expect.len() {
                expect.truncate(size);
            }
            expect.resize(size, 0);
            if !data.is_empty() {
                expect[offset..offset + data.len()].copy_from_slice(data);
            }
            let mut inode = db.source_inode(source, 7).unwrap().unwrap();
            inode.size = size as u64;
            let publication = db
                .apply(
                    source,
                    &Changes {
                        open: Some(open),
                        inodes: vec![inode],
                        write: (!data.is_empty()).then(|| PayloadWrite {
                            serial: 7,
                            offset: offset as u64,
                            data: Arc::from(data),
                        }),
                        ..Changes::default()
                    },
                )
                .unwrap();
            db.reply_attempted(publication).unwrap();
        };
        let size = expect.len();
        write(&mut expect, 70_000, &[0xC3], size);
        write(&mut expect, 131_072, &noise(32_768, 7), size);
        write(&mut expect, 190_000, &noise(20_000, 8), size);
        // A cut inside a lower row, then a regrow: nothing above it returns.
        write(&mut expect, 0, &[], 300_000);
        write(&mut expect, 0, &[], 350_000);
        let read = db.acquire_file_read(source, open, 50).unwrap();
        assert_eq!(orphan_bytes(&db, read, &file.base, expect.len()), expect);
        maintain(&db);
        assert_eq!(orphan_bytes(&db, read, &file.base, expect.len()), expect);
        let size = expect.len();
        write(&mut expect, 250_000, &noise(60_000, 9), size);
        write(&mut expect, 8_192, &noise(CELL, 10), size);
        maintain(&db);
        assert_eq!(orphan_bytes(&db, read, &file.base, expect.len()), expect);
        db.release_file_read(read).unwrap();
        db.close_file(open).unwrap();
        maintain(&db);
        let left = db.resources(Some(route)).unwrap().counts;
        assert_eq!(
            (left.payload_cells, left.payload_bytes, left.orphan_rows),
            (0, 0, 0),
            "the last owner's release reclaims every row"
        );
        db.release_base_source(source).unwrap();
    }
}

/// Stored rows, stored bytes and pages in use of one shape on a fresh engine.
fn shape(build: impl FnOnce(&mut File<'_>)) -> (u64, u64, u64) {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([45; 32], [46; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let (pages, free) = db.pages().unwrap();
    let mut file = File::new(&db, source, 7, Vec::new());
    build(&mut file);
    file.check();
    maintain(&db);
    file.check();
    let counts = db.resources(Some(route)).unwrap().counts;
    let (after, after_free) = db.pages().unwrap();
    (
        counts.payload_cells,
        counts.payload_bytes,
        (after - after_free) - (pages - free),
    )
}

#[test]
fn worst_case_shapes_report_their_stored_rows_bytes_and_pages() {
    let window = noise(WRITE_WINDOW, 11);
    let alternating = shape(|file| {
        for cell in (0..64).step_by(2) {
            file.write(cell * CELL as u64, &window[..CELL]);
        }
    });
    let appends = shape(|file| {
        for cell in 0..64 {
            file.write(cell * CELL as u64, &window[..CELL]);
        }
    });
    let boundary_bytes = shape(|file| {
        for cell in 0..64 {
            file.write(cell * CELL as u64, &[0xD1]);
        }
    });
    let shrink_inside = shape(|file| {
        file.write(0, &window);
        file.resize(20_000);
    });
    let sparse_put = shape(|file| {
        file.write(0, &window);
        let half: Vec<bool> = (0..CELL).map(|at| at % 2 == 0).collect();
        put_cell(file, 16_384, &window[..CELL], &half, 0);
    });
    let dense = shape(|file| {
        for at in 0..16 {
            file.write(at * WRITE_WINDOW as u64, &window);
        }
    });
    let large = shape(|file| {
        for at in 0..512 {
            file.write(at * WRITE_WINDOW as u64, &window);
        }
    });
    println!(
        "R7_RUN_SHAPES (rows,bytes,pages) alternating={alternating:?} appends={appends:?} boundary_bytes={boundary_bytes:?} shrink_inside={shrink_inside:?} sparse_put={sparse_put:?} dense_2mib={dense:?} dense_64mib={large:?}"
    );
    // (rows, bytes, pages) of each shape with one cell per row, measured at
    // 935fdc685 on this host, beside the rows and bytes stored now. No shape
    // stores more rows, bytes or pages than it did.
    for (name, now, before, rows) in [
        ("alternating cells", alternating, (32, 131_072, 36), 32),
        ("4 KiB appends", appends, (64, 262_144, 72), 64),
        (
            "1-byte writes at cell boundaries",
            boundary_bytes,
            (64, 64, 0),
            64,
        ),
        // Four whole cells in one row and the cut cell in another.
        ("shrink inside a row", shrink_inside, (5, 20_000, 6), 2),
        // The row gives up one cell: its part below, the put cell, its part
        // above, beside the three other rows of the window.
        ("sparse put inside a row", sparse_put, (32, 131_583, 36), 6),
        ("dense 2 MiB", dense, (512, 2 << 20, 582), 64),
        // Not measured before: 32 times the 2 MiB file's rows and pages.
        ("dense 64 MiB", large, (16_384, 64 << 20, 32 * 582), 2_048),
    ] {
        assert_eq!((now.0, now.1), (rows, before.1), "{name}");
        assert!(
            now.2 <= before.2,
            "{name}: {} pages, {} before",
            now.2,
            before.2
        );
    }
}

#[test]
fn a_maintenance_page_is_fourteen_cells_of_bytes_whatever_the_rows() {
    // Four rows of 32 KiB, then twenty rows of one cell, unlinked with no
    // owner: every step of the layer's retirement drops at most 14 cells.
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([47; 32], [48; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, 7, Vec::new());
    file.write(0, &noise(WRITE_WINDOW, 12));
    for cell in 32..52 {
        file.write(cell * CELL as u64, &noise(CELL, 13));
    }
    let size = file.expect.len() as u64;
    let mut removed = file.inode(size);
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
    let mut steps = Vec::new();
    let mut cursor = MaintenanceCursor::default();
    while let Some(step) = db.maintain(cursor).unwrap() {
        cursor = step.cursor;
        if step.work.data_bytes != 0 {
            steps.push((step.work.rows, step.work.data_bytes));
        }
        assert!(steps.len() < 100);
    }
    // One row of 32 KiB a step while the next is another: two are 16
    // cells, more than a page. The last one leaves room for six rows of
    // one cell; the rest go 14 a step, as they always did.
    assert_eq!(
        steps,
        [
            (1, 32_768),
            (1, 32_768),
            (1, 32_768),
            (7, 14 * 4_096),
            (14, 14 * 4_096)
        ]
    );
    assert_eq!(db.resources(Some(route)).unwrap().counts.payload_cells, 0);
}
