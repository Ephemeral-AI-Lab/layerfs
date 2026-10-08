//! R4-4: the same final bytes, reached through differently fragmented,
//! ordered and overlapping writes, give the same content root and the same
//! filesystem root. Each shape runs on its own identical fixture.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::{create, write, Bench, T1};
use layerfs_content::{filesystem::FilesystemRootId, ObjectId};
use oracle::{value, Model, Stamp};
use producer::build;

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
