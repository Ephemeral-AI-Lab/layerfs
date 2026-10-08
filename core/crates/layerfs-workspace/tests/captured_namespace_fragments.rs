//! R4-4 and R3 row 6: the same final bytes, reached through differently
//! fragmented, ordered and overlapping writes, truncations, regrowth and
//! descriptor operations, give the same content root and the same filesystem
//! root, and always the model's bytes. Each shape runs on its own identical
//! fixture. Root identity is claimed only between shapes whose unwritten
//! ranges are unwritten in both; a hole against written zeros is compared by
//! bytes only.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::{append, create, resize, write, Bench, T1};
use layerfs_content::{filesystem::FilesystemRootId, ObjectId};
use layerfs_overlay::OpenFile;
use layerfs_workspace::{Operation, Outcome, Position};
use oracle::{value, Model, Stamp};
use producer::{build, hold};

const NOW: Stamp = (T1.seconds, T1.nanoseconds);
/// The changed window of the 400 000-byte base file, serial 8.
const START: usize = 10_000;
const LENGTH: usize = 20_000;

fn replacement() -> Vec<u8> {
    (0..LENGTH)
        .map(|index| ((index * 31) % 253) as u8 ^ 0x80)
        .collect()
}
fn put(b: &Bench, serial: u64, at: usize, piece: &[u8]) {
    b.applied(write(serial, at as u64, piece), T1);
}
/// One shape of writes over the base file, then the one captured attempt.
fn changed(tag: &str, shape: impl Fn(&Bench, &[u8])) -> (FilesystemRootId, ObjectId) {
    let b = Bench::new(tag);
    let bytes = replacement();
    shape(&b, &bytes);
    let mut model = Model::fixture(&b.fixture.bytes);
    model.write(".git/index", START as u64, &bytes, NOW);
    let built = build(&b, &model, tag);
    assert_eq!(
        (built.work.files_constructed, built.work.files_unchanged),
        (1, 0),
        "{tag}"
    );
    let content = value(&b.fixture.store, built.root, 8).content_root;
    assert_ne!(
        content,
        value(&b.fixture.store, b.fixture.root, 8).content_root
    );
    let root = built.root;
    built.release(&b);
    (root, content)
}

#[test]
fn fragmented_writes_of_a_base_file_give_one_root() {
    let whole = changed("cf-whole", |b, bytes| put(b, 8, START, bytes));
    let scrambled = changed("cf-scrambled", |b, bytes| {
        // 200 pieces of 100 bytes in a fixed scrambled order.
        for step in 0..200 {
            let piece = step * 73 % 200;
            let at = piece * 100;
            put(b, 8, START + at, &bytes[at..at + 100]);
        }
    });
    let reversed = changed("cf-reversed", |b, bytes| {
        for piece in (0..20).rev() {
            let at = piece * 1000;
            put(b, 8, START + at, &bytes[at..at + 1000]);
        }
    });
    let unaligned = changed("cf-unaligned", |b, bytes| {
        // Pieces that cross every 4096-byte cell boundary differently.
        let mut at = 0;
        while at < bytes.len() {
            let end = (at + 4097).min(bytes.len());
            put(b, 8, START + at, &bytes[at..end]);
            at = end;
        }
    });
    let overlapped = changed("cf-overlapped", |b, bytes| {
        // Other bytes first, wholly rewritten by overlapping final pieces.
        put(b, 8, START, &[0xee; 12_000]);
        put(b, 8, START + 8000, &[0x11; 12_000]);
        let mut at = 0;
        while at < bytes.len() {
            let end = (at + 3500).min(bytes.len());
            put(b, 8, START + at, &bytes[at..end]);
            at += 3000;
        }
    });
    let inward = changed("cf-inward", |b, bytes| {
        // Every other piece first: nineteen transient gaps, then filled.
        for parity in [0, 1] {
            for piece in (0..20).filter(|piece| piece % 2 == parity) {
                let at = piece * 1000;
                put(b, 8, START + at, &bytes[at..at + 1000]);
            }
        }
    });
    for (shape, other) in [
        ("scrambled", scrambled),
        ("reversed", reversed),
        ("unaligned", unaligned),
        ("overlapped", overlapped),
        ("inward", inward),
    ] {
        assert_eq!(other.1, whole.1, "{shape}: content root");
        assert_eq!(other.0, whole.0, "{shape}: filesystem root");
    }
}

/// A fresh 300 000-byte file whose middle third is never written.
const HOLE: std::ops::Range<usize> = 100_000..200_000;
const SIZE: usize = 300_000;

fn fresh_bytes() -> Vec<u8> {
    (0..SIZE)
        .map(|index| {
            if HOLE.contains(&index) {
                0
            } else {
                ((index * 7 + index / 300) % 251) as u8 | 1
            }
        })
        .collect()
}
/// One shape of writes over a new file, then the one captured attempt.
fn created(tag: &str, shape: impl Fn(&Bench, u64, &[u8])) -> (FilesystemRootId, ObjectId) {
    let b = Bench::new(tag);
    let bytes = fresh_bytes();
    let serial = b.applied(create(1, "new"), T1).unwrap().serial;
    shape(&b, serial, &bytes);
    let mut model = Model::fixture(&b.fixture.bytes);
    model.create("new", 0o640, NOW);
    model.write("new", 0, &bytes[..HOLE.start], NOW);
    model.write("new", HOLE.end as u64, &bytes[HOLE.end..], NOW);
    let built = build(&b, &model, tag);
    assert_eq!(built.walked.serial("new"), serial);
    assert_eq!(built.work.files_constructed, 1, "{tag}");
    let content = value(&b.fixture.store, built.root, serial).content_root;
    let root = built.root;
    built.release(&b);
    (root, content)
}
/// The two written thirds as pieces of at most `size` bytes, in file order.
fn pieces(size: usize) -> Vec<(usize, usize)> {
    let mut all = Vec::new();
    for range in [0..HOLE.start, HOLE.end..SIZE] {
        let mut at = range.start;
        while at < range.end {
            let end = (at + size).min(range.end);
            all.push((at, end));
            at = end;
        }
    }
    all
}

#[test]
fn fragmented_writes_around_a_hole_of_a_new_file_give_one_root() {
    let forward = created("cf-new-forward", |b, serial, bytes| {
        for (at, end) in pieces(50_000) {
            put(b, serial, at, &bytes[at..end]);
        }
    });
    let backward = created("cf-new-backward", |b, serial, bytes| {
        // The last byte first: the hole exists before anything precedes it.
        for (at, end) in pieces(50_000).into_iter().rev() {
            put(b, serial, at, &bytes[at..end]);
        }
    });
    let scattered = created("cf-new-scattered", |b, serial, bytes| {
        let all = pieces(4097);
        let count = all.len();
        // 37 is coprime with every count this size can produce here.
        assert_ne!(count % 37, 0);
        for step in 0..count {
            let (at, end) = all[step * 37 % count];
            put(b, serial, at, &bytes[at..end]);
        }
    });
    let rewritten = created("cf-new-rewritten", |b, serial, bytes| {
        // Other bytes over both thirds, then the final bytes over them.
        for (at, end) in pieces(120_000) {
            put(b, serial, at, &vec![0xc3; end - at]);
        }
        for (at, end) in pieces(30_000) {
            put(b, serial, at, &bytes[at..end]);
        }
    });
    for (shape, other) in [
        ("backward", backward),
        ("scattered", scattered),
        ("rewritten", rewritten),
    ] {
        assert_eq!(other.1, forward.1, "{shape}: content root");
        assert_eq!(other.0, forward.0, "{shape}: filesystem root");
    }
}

#[test]
fn a_hole_and_written_zeros_are_the_same_bytes() {
    // The hole written as explicit zeros has the same bytes, time and names.
    // Only the complete comparison is claimed here, not root identity.
    created("cf-new-zeros", |b, serial, bytes| {
        for at in (0..SIZE).step_by(60_000) {
            let end = (at + 60_000).min(SIZE);
            put(b, serial, at, &bytes[at..end]);
        }
    });
}

// R3 row 6 and the remaining fragment shapes, all over the multi-cell base
// file 8 (400 000 bytes, 4096-byte cells). Every step uses the one time T1,
// so shapes with the same final bytes are the same inode.

const BASE_SIZE: usize = 400_000;

fn pattern(length: usize, salt: usize) -> Vec<u8> {
    (0..length)
        .map(|index| ((index * 13 + salt) % 239) as u8 | 0x40)
        .collect()
}
/// One shape of operations over base file 8 and the model's own statement of
/// the final file; the result is compared completely with the model.
fn shaped(
    tag: &str,
    shape: impl Fn(&Bench, OpenFile),
    expect: impl Fn(&mut Model),
) -> (FilesystemRootId, ObjectId) {
    let b = Bench::new(tag);
    let file = hold(&b, 8);
    shape(&b, file);
    let mut model = Model::fixture(&b.fixture.bytes);
    expect(&mut model);
    let built = build(&b, &model, tag);
    assert_eq!(built.work.files_constructed, 1, "{tag}");
    let content = value(&b.fixture.store, built.root, 8).content_root;
    let root = built.root;
    built.release(&b);
    b.overlay.close_file(file).unwrap();
    (root, content)
}
fn same(first: (FilesystemRootId, ObjectId), others: &[(&str, (FilesystemRootId, ObjectId))]) {
    for (shape, other) in others {
        assert_eq!(other.1, first.1, "{shape}: content root");
        assert_eq!(other.0, first.0, "{shape}: filesystem root");
    }
}
fn truncate_open(b: &Bench, file: OpenFile, size: u64) {
    let operation = Operation::SetOpenAttributes {
        file,
        mode: None,
        mtime: None,
        size: Some(size),
    };
    b.applied(operation, T1);
}
fn write_open(b: &Bench, file: OpenFile, position: Position, piece: &[u8]) {
    let operation = Operation::WriteOpen {
        file,
        position,
        data: piece.into(),
    };
    b.applied(operation, T1);
}

#[test]
fn pieces_crossing_the_end_of_a_base_file_give_one_root() {
    // 21 000 bytes from 1000 below the old end to 20 000 beyond it.
    const AT: usize = BASE_SIZE - 1000;
    let bytes = pattern(21_000, 3);
    let expect = |model: &mut Model| {
        model.write(".git/index", AT as u64, &bytes, NOW);
    };
    let whole = shaped("cf-end-whole", |b, _| put(b, 8, AT, &bytes), expect);
    let forward = shaped(
        "cf-end-forward",
        |b, _| {
            // 700-byte pieces: the second one crosses the old end.
            for at in (0..bytes.len()).step_by(700) {
                put(b, 8, AT + at, &bytes[at..at + 700]);
            }
        },
        expect,
    );
    let backward = shaped(
        "cf-end-backward",
        |b, _| {
            // The farthest piece first: a gap beyond the old end, then filled.
            for at in (0..bytes.len()).step_by(3000).rev() {
                put(b, 8, AT + at, &bytes[at..at + 3000]);
            }
        },
        expect,
    );
    let appended = shaped(
        "cf-end-appended",
        |b, file| {
            // Up to the old end by offset, then appended, partly by descriptor.
            put(b, 8, AT, &bytes[..1000]);
            b.applied(append(8, &bytes[1000..9000]), T1);
            write_open(b, file, Position::End, &bytes[9000..]);
        },
        expect,
    );
    let straddling = shaped(
        "cf-end-straddling",
        |b, _| {
            // Overlapping pieces, each rewritten by the next across the end.
            let mut at = 0;
            while at < bytes.len() {
                let end = (at + 4097).min(bytes.len());
                put(b, 8, AT + at, &bytes[at..end]);
                at += 2048;
            }
        },
        expect,
    );
    same(
        whole,
        &[
            ("forward", forward),
            ("backward", backward),
            ("appended", appended),
            ("straddling", straddling),
        ],
    );
}

#[test]
fn a_truncate_between_pieces_cuts_what_was_written_before_it() {
    // Final file: the base below 10 000, then 20 000 new bytes; nothing of
    // the old file or of an earlier piece survives beyond a cut.
    let bytes = pattern(20_000, 5);
    let expect = |model: &mut Model| {
        model.resize(".git/index", 10_000, NOW);
        model.write(".git/index", 10_000, &bytes, NOW);
    };
    let plain = shaped(
        "cf-cut-plain",
        |b, _| {
            b.applied(resize(8, 10_000), T1);
            put(b, 8, 10_000, &bytes);
        },
        expect,
    );
    let between = shaped(
        "cf-cut-between",
        |b, _| {
            // A first piece too long and wrong beyond 15 000, cut there.
            put(b, 8, 10_000, &bytes[..5000]);
            put(b, 8, 15_000, &[0xee; 40_000]);
            b.applied(resize(8, 15_000), T1);
            put(b, 8, 15_000, &bytes[5000..12_000]);
            // Cut inside the second piece's cell, and written again from there.
            b.applied(resize(8, 20_001), T1);
            put(b, 8, 20_001, &bytes[10_001..]);
        },
        expect,
    );
    let descriptor = shaped(
        "cf-cut-descriptor",
        |b, file| {
            // The same through the descriptor: ftruncate between its writes.
            write_open(b, file, Position::At(10_000), &[0x11; 60_000]);
            truncate_open(b, file, 10_000);
            write_open(b, file, Position::End, &bytes[..7000]);
            truncate_open(b, file, 16_000);
            write_open(b, file, Position::At(16_000), &bytes[6000..]);
        },
        expect,
    );
    same(plain, &[("between", between), ("descriptor", descriptor)]);
}

#[test]
fn shrinking_and_regrowing_across_cells_never_returns_cut_bytes() {
    // Final file: the base below 100 000 (not a cell boundary), nothing
    // written up to 250 000 except one patch that spans three cells.
    const KEPT: u64 = 100_000;
    const SIZE: u64 = 250_000;
    const PATCH: usize = 180_000;
    let patch = pattern(9000, 7);
    let expect = |model: &mut Model| {
        model.resize(".git/index", KEPT, NOW);
        model.resize(".git/index", SIZE, NOW);
        model.write(".git/index", PATCH as u64, &patch, NOW);
    };
    let plain = shaped(
        "cf-regrow-plain",
        |b, _| {
            b.applied(resize(8, KEPT), T1);
            b.applied(resize(8, SIZE), T1);
            put(b, 8, PATCH, &patch);
        },
        expect,
    );
    let stepped = shaped(
        "cf-regrow-stepped",
        |b, _| {
            // Down and up in steps that cross several cells each time.
            for size in [300_000, 200_001, KEPT, 150_000, 204_800, SIZE] {
                b.applied(resize(8, size), T1);
            }
            for at in (0..patch.len()).step_by(1000).rev() {
                put(b, 8, PATCH + at, &patch[at..at + 1000]);
            }
        },
        expect,
    );
    let below = shaped(
        "cf-regrow-below",
        |b, file| {
            // Regrown first, written, cut below the write, regrown again:
            // the bytes written before the cut must not return.
            b.applied(resize(8, 120_000), T1);
            put(b, 8, 110_000, &[0xaa; 9000]);
            truncate_open(b, file, KEPT);
            // A write beyond the end regrows the file over the cut range.
            write_open(b, file, Position::At(PATCH as u64), &patch);
            truncate_open(b, file, SIZE);
        },
        expect,
    );
    same(plain, &[("stepped", stepped), ("below", below)]);
}

#[test]
fn a_truncate_to_zero_then_appends_holds_only_the_appended_bytes() {
    let bytes = pattern(150_000, 11);
    let expect = |model: &mut Model| {
        model.resize(".git/index", 0, NOW);
        model.write(".git/index", 0, &bytes, NOW);
    };
    let plain = shaped(
        "cf-zero-plain",
        |b, _| {
            b.applied(resize(8, 0), T1);
            put(b, 8, 0, &bytes[..75_000]);
            put(b, 8, 75_000, &bytes[75_000..]);
        },
        expect,
    );
    let appended = shaped(
        "cf-zero-appended",
        |b, _| {
            b.applied(resize(8, 0), T1);
            for at in (0..bytes.len()).step_by(5000) {
                b.applied(append(8, &bytes[at..at + 5000]), T1);
            }
        },
        expect,
    );
    let descriptor = shaped(
        "cf-zero-descriptor",
        |b, file| {
            // O_TRUNC through the descriptor after an earlier write, twice.
            write_open(b, file, Position::At(0), &[0x77; 50_000]);
            truncate_open(b, file, 0);
            write_open(b, file, Position::End, &[0x78; 4097]);
            truncate_open(b, file, 0);
            for at in (0..bytes.len()).step_by(4097) {
                let end = (at + 4097).min(bytes.len());
                write_open(b, file, Position::End, &bytes[at..end]);
            }
        },
        expect,
    );
    same(plain, &[("appended", appended), ("descriptor", descriptor)]);
}

#[test]
fn mapped_stores_land_only_below_the_current_size() {
    // A store over three cells in the middle, one clipped at the end of the
    // file, and one wholly beyond it, which changes nothing.
    let middle = pattern(9000, 13);
    let tail = pattern(5000, 17);
    let expect = |model: &mut Model| {
        assert!(model.store(".git/index", 50_000, &middle, NOW));
        assert!(model.store(".git/index", BASE_SIZE as u64 - 1000, &tail, NOW));
        assert!(!model.store(".git/index", BASE_SIZE as u64, &tail, NOW));
        assert_eq!(model.bytes(".git/index").len(), BASE_SIZE);
    };
    let store = |b: &Bench, file: OpenFile, offset: u64, data: &[u8]| {
        let operation = Operation::StoreOpen {
            file,
            offset,
            data: data.into(),
        };
        b.run(operation, T1).unwrap()
    };
    let stored = shaped(
        "cf-store",
        |b, file| {
            assert!(matches!(
                store(b, file, 50_000, &middle),
                Outcome::Applied { .. }
            ));
            assert!(matches!(
                store(b, file, BASE_SIZE as u64 - 1000, &tail),
                Outcome::Applied { .. }
            ));
            assert!(matches!(
                store(b, file, BASE_SIZE as u64, &tail),
                Outcome::Unchanged { .. }
            ));
            assert!(matches!(
                store(b, file, BASE_SIZE as u64 + 70_000, &tail),
                Outcome::Unchanged { .. }
            ));
        },
        expect,
    );
    // The unfragmented equivalent: ordinary writes of exactly the kept bytes.
    let written = shaped(
        "cf-store-written",
        |b, _| {
            put(b, 8, 50_000, &middle);
            put(b, 8, BASE_SIZE - 1000, &tail[..1000]);
        },
        expect,
    );
    // Stores of the same bytes in cell-crossing pieces, last piece first.
    let pieces = shaped(
        "cf-store-pieces",
        |b, file| {
            store(b, file, BASE_SIZE as u64 - 1000, &tail);
            for at in (0..middle.len()).step_by(3000).rev() {
                store(b, file, 50_000 + at as u64, &middle[at..at + 3000]);
            }
        },
        expect,
    );
    same(stored, &[("written", written), ("pieces", pieces)]);
}

#[test]
fn a_store_after_a_shrink_is_clipped_to_the_new_size() {
    // The size current when the store is published decides what lands.
    let data = pattern(20_000, 19);
    let expect = |model: &mut Model| {
        model.resize(".git/index", 60_000, NOW);
        assert!(model.store(".git/index", 55_000, &data, NOW));
        assert_eq!(model.bytes(".git/index").len(), 60_000);
    };
    let stored = shaped(
        "cf-store-shrunk",
        |b, file| {
            truncate_open(b, file, 60_000);
            let operation = Operation::StoreOpen {
                file,
                offset: 55_000,
                data: data.as_slice().into(),
            };
            b.applied(operation, T1);
        },
        expect,
    );
    let written = shaped(
        "cf-store-shrunk-written",
        |b, _| {
            b.applied(resize(8, 60_000), T1);
            put(b, 8, 55_000, &data[..5000]);
        },
        expect,
    );
    same(stored, &[("written", written)]);
}
