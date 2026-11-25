//! #768 stage 2b size boundaries, part two: large files and publication
//! bytes.
//!
//! Each cell writes new incompressible files (and sometimes many tiny
//! ones) and Commits once, fused on one Store and forced on its twin, on
//! SQLite and PostgreSQL (`support/sizes.rs`). The bounds (values at
//! efcd412dcb):
//!
//! * The whole-file cutoff (`DEFAULT_SMALL_FILE_THRESHOLD_BYTES`, 131,072):
//!   131,071 raw bytes are one whole-file object, 131,072 are chunked.
//! * `PACK_LIMIT` (256 KiB): files of 262,143 / 262,144 / 262,145 bytes;
//!   one incompressible whole file fills most of a pack, two never share
//!   one.
//! * The publication byte bound: SQLite's `TRANSACTION_CANONICAL_BYTES_LIMIT`
//!   (4 MiB - 1) and PostgreSQL's `PUBLICATION_BYTES_MAX` (16 MiB - 1),
//!   counted in canonical and in physical bytes. N files of 120 KiB just
//!   below, at and above each, with and without 300 tiny files in the same
//!   Save: a Save past the bound publishes in several batches and holds
//!   the last one for the fused transaction.
//!
//! * Zero-padded (compressible) files: a pack of them stays open across
//!   waves until the finish seals it, so the finish's last publication is
//!   split. 600 files of 48 KiB (28 MiB canonical), and 120 files of
//!   120 KiB Committed, then each rewritten with one byte changed (PREFIX
//!   records whose bases are stored).
//!
//! Cells meant to split the finish's last publication say so
//! (`Cell::splits`) and prove it: the Store gains object, pack or value
//! rows while the fused Commit's `finish` phase runs (`support/sizes.rs`
//! `split_seen`). Which cells split was read off that observation at
//! efcd412dcb, where each of them was refused `Integrity("held pack
//! missing")`: the noise cells whose 120 KiB files fill 4 MiB waves
//! exactly (68, 102 and 136 files) on SQLite, 272 files (two full 16 MiB
//! waves) on PostgreSQL, and the zero-padded cells.
//!
//! The Singleton lane is not reachable here: at the frozen default cutoff
//! a whole file is at most 131,071 raw bytes and its compact record always
//! fits a 256 KiB pack (`encoding/full.rs` `plan_lane`), and chunks and
//! metadata never take it. The Stores these tests create use the frozen
//! default policy, as Project Init does.
//!
//! The PostgreSQL halves need `LAYERFS_TEST_POSTGRES_URL` and report
//! NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/sizes.rs"]
#[allow(dead_code)]
mod sizes;

use adversary::{text_bytes, Kind, Op};
use sizes::{noise, one_file_seed, run, tiny, Cell, Rng, Stores};

const KIB: usize = 1024;

/// One cell: `large` new files of `length` bytes, compressible text or
/// incompressible noise, then `small` tiny files, in new directories.
struct Case {
    large: usize,
    length: usize,
    text: bool,
    small: usize,
    probe: &'static str,
    /// Backends on which the Commit must show a split finish.
    splits: &'static [Kind],
}

const fn case(large: usize, length: usize, small: usize, probe: &'static str) -> Case {
    Case {
        large,
        length,
        text: false,
        small,
        probe,
        splits: &[],
    }
}

impl Case {
    /// Meant to split the finish's last publication on `kinds`.
    const fn splits(self, kinds: &'static [Kind]) -> Self {
        Case {
            splits: kinds,
            ..self
        }
    }
}

const fn text(large: usize, length: usize, small: usize, probe: &'static str) -> Case {
    Case {
        text: true,
        ..case(large, length, small, probe)
    }
}

fn files(cell: usize, case: &Case) -> Vec<Op> {
    let mut ops = vec![Op::Mkdir(format!("/b{cell}"))];
    ops.extend((0..case.large).map(|n| {
        let bytes = match case.text {
            true => text_bytes(((cell as u64) << 32) ^ n as u64, case.length),
            false => noise(cell, n, case.length),
        };
        Op::Write(format!("/b{cell}/large{n:03}"), bytes)
    }));
    if case.small > 0 {
        ops.push(Op::Mkdir(format!("/t{cell}")));
        ops.extend(
            (0..case.small).map(|n| Op::Write(format!("/t{cell}/f{n:04}"), tiny(cell, n, 0))),
        );
    }
    ops
}

fn cells(cases: &[Case]) -> Vec<Cell> {
    cases
        .iter()
        .enumerate()
        .map(|(cell, case)| {
            let kind = if case.text { "text" } else { "" };
            Cell::new(
                format!("{}x{}{kind}+{}", case.large, case.length, case.small),
                case.probe,
                files(cell, case),
            )
            .splits(case.splits)
        })
        .collect()
}

fn cutoff_cells() -> Vec<Cell> {
    cells(&[
        case(
            36,
            131_071,
            0,
            "whole-file cutoff, just below: 36 whole files, 4.7 MB",
        ),
        case(36, 131_072, 0, "whole-file cutoff, at: 36 chunked files"),
        case(16, 262_143, 0, "PACK_LIMIT 256 KiB, just below (chunked)"),
        case(16, 262_144, 0, "PACK_LIMIT 256 KiB, at"),
        case(16, 262_145, 0, "PACK_LIMIT 256 KiB, just above"),
        case(
            3,
            1024 * KIB,
            2_000,
            "three 1 MiB files and 2,000 tiny files",
        ),
    ])
}

/// Around SQLite's 4 MiB - 1 publication (PostgreSQL publishes them whole).
fn four_mib_cells() -> Vec<Cell> {
    cells(&[
        case(30, 120 * KIB, 0, "4 MiB - 1 publication, below (3.7 MB)"),
        case(
            34,
            120 * KIB,
            0,
            "4 MiB - 1, just below (4.18 MB canonical)",
        ),
        case(35, 120 * KIB, 0, "4 MiB - 1, just above (4.30 MB)"),
        case(
            35,
            120 * KIB,
            300,
            "4 MiB - 1 crossed, 300 tiny files after",
        ),
        case(
            40,
            120 * KIB,
            300,
            "open split case: 40 x 120 KiB + 300 tiny",
        ),
        text(
            40,
            120 * KIB,
            300,
            "open split case, text: 40 x 120 KiB + 300 tiny",
        ),
        case(
            68,
            120 * KIB,
            0,
            "4 MiB - 1 waves filled exactly twice (8.4 MB)",
        )
        .splits(&[Kind::Sqlite]),
        case(
            102,
            120 * KIB,
            0,
            "4 MiB - 1 waves filled exactly three times (12.5 MB)",
        )
        .splits(&[Kind::Sqlite]),
        case(
            136,
            120 * KIB,
            0,
            "4 MiB - 1 waves filled exactly four times (16.7 MB)",
        )
        .splits(&[Kind::Sqlite]),
        case(
            170,
            120 * KIB,
            0,
            "4 MiB - 1 waves filled exactly five times (20.9 MB)",
        ),
    ])
}

/// Around PostgreSQL's 16 MiB - 1 publication.
fn sixteen_mib_cells() -> Vec<Cell> {
    cells(&[
        case(
            136,
            120 * KIB,
            0,
            "16 MiB - 1 publication, just below (16.71 MB)",
        )
        .splits(&[Kind::Sqlite]),
        case(137, 120 * KIB, 0, "16 MiB - 1, just above (16.83 MB)"),
        case(
            137,
            120 * KIB,
            300,
            "16 MiB - 1 crossed, 300 tiny files after",
        ),
        case(200, 120 * KIB, 0, "open split case: 200 x 120 KiB"),
        text(200, 120 * KIB, 0, "open split case, text: 200 x 120 KiB"),
        case(
            272,
            120 * KIB,
            0,
            "16 MiB - 1 waves filled exactly twice (33.4 MB)",
        )
        .splits(&[Kind::Postgres]),
    ])
}

/// Seeded mixes of large files of random sizes (whole and chunked) and
/// tiny files, interleaved, in four directories: publication batch splits
/// at whatever byte offsets the sizes put them.
fn mixed(cell: usize, seed: u64, large: (usize, usize)) -> Vec<Op> {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut ops: Vec<Op> = (0..4).map(|d| Op::Mkdir(format!("/m{cell}_{d}"))).collect();
    let count = large.0 + rng.below(large.1 - large.0);
    let small = rng.below(600);
    let (mut l, mut s) = (0, 0);
    while l < count || s < small {
        let dir = rng.below(4);
        if s >= small || (l < count && rng.below(count + small) < count) {
            // Mostly whole files near the cutoff, sometimes chunked ones.
            let length = match rng.below(8) {
                0 => 131_072 + rng.below(300 * KIB),
                1 => 1 + rng.below(16 * KIB),
                _ => 48 * KIB + rng.below(131_071 - 48 * KIB),
            };
            ops.push(Op::Write(
                format!("/m{cell}_{dir}/l{l:04}"),
                noise(cell, l, length),
            ));
            l += 1;
        } else {
            ops.push(Op::Write(
                format!("/m{cell}_{dir}/s{s:04}"),
                tiny(cell, s, 0),
            ));
            s += 1;
        }
    }
    ops
}

/// Seeds of the 4 MiB-scale mixes (20 to 60 large files).
const SEEDS_4MIB: [u64; 8] = [1, 2, 3, 5, 8, 13, 21, 34];
/// Seeds of the 16 MiB-scale mixes (100 to 180 large files).
const SEEDS_16MIB: [u64; 3] = [768, 4096, 65537];

fn mixed_cells() -> Vec<Cell> {
    let mut cells = Vec::new();
    for seed in SEEDS_4MIB {
        let cell = cells.len();
        cells.push(Cell::new(
            format!("mix-seed{seed}"),
            "seeded 20-60 large + 0-600 tiny: 4 MiB-scale batch splits",
            mixed(cell, seed, (20, 60)),
        ));
    }
    for seed in SEEDS_16MIB {
        let cell = cells.len();
        cells.push(Cell::new(
            format!("mix16-seed{seed}"),
            "seeded 100-180 large + 0-600 tiny: 16 MiB-scale batch splits",
            mixed(cell, seed, (100, 180)),
        ));
    }
    cells
}

/// `length` bytes: an 8-byte name prefix, then zeros; with `changed`, one
/// byte in the middle is 1. Compressible: a pack of such files stays open
/// across waves until the finish seals it.
fn zero_padded(cell: usize, n: usize, length: usize, changed: bool) -> Vec<u8> {
    let mut bytes = format!("{cell:02}{n:06}").into_bytes();
    bytes.resize(length, 0);
    if changed {
        bytes[length / 2] = 1;
    }
    bytes
}

fn zero_padded_cells() -> Vec<Cell> {
    let write = |cell: usize, count: usize, length: usize, changed: bool| -> Vec<Op> {
        let mut ops = vec![Op::Mkdir(format!("/z{cell}"))];
        ops.extend((0..count).map(|n| {
            Op::Write(
                format!("/z{cell}/f{n:03}"),
                zero_padded(cell, n, length, changed),
            )
        }));
        ops
    };
    let rewrite = |cell: usize, count: usize, length: usize| -> Vec<Op> {
        (0..count)
            .map(|n| {
                Op::Write(
                    format!("/z{cell}/f{n:03}"),
                    zero_padded(cell, n, length, true),
                )
            })
            .collect()
    };
    vec![
        Cell::rounds(
            "delta-rewrite-120x120K",
            "120 zero-padded 120 KiB files, then each rewritten with one byte \
             changed: PREFIX records whose bases are stored",
            vec![write(0, 120, 120 * KIB, false), rewrite(0, 120, 120 * KIB)],
        )
        .splits_in(0, &[Kind::Sqlite])
        .splits_in(1, &[Kind::Sqlite]),
        Cell::new(
            "zero-600x48K",
            "600 zero-padded 48 KiB files (28 MiB canonical, past 16 MiB - 1)",
            write(1, 600, 48 * KIB, false),
        )
        .splits(&[Kind::Sqlite, Kind::Postgres]),
    ]
}

/// 120 zero-padded 120 KiB files as the seed, so the Commit that rewrites
/// them is the first fused one and its PREFIX records' bases are stored.
fn zero_padded_seed() -> Vec<(String, Vec<u8>)> {
    (0..120)
        .map(|n| (format!("/z/f{n:03}"), zero_padded(99, n, 120 * KIB, false)))
        .collect()
}

fn delta_rewrite_cells() -> Vec<Cell> {
    vec![Cell::new(
        "seeded-delta-rewrite-120x120K",
        "120 seeded zero-padded 120 KiB files each rewritten with one byte changed",
        (0..120)
            .map(|n| Op::Write(format!("/z/f{n:03}"), zero_padded(99, n, 120 * KIB, true)))
            .collect(),
    )
    .splits(&[Kind::Sqlite])]
}

#[test]
fn seeded_delta_rewrite_sqlite() {
    let (seed, cells) = (zero_padded_seed(), delta_rewrite_cells());
    run(Kind::Sqlite, "bytes6", &seed, &cells, Stores::PerCell);
}

#[test]
fn seeded_delta_rewrite_postgresql() {
    let (seed, cells) = (zero_padded_seed(), delta_rewrite_cells());
    run(Kind::Postgres, "bytes6", &seed, &cells, Stores::PerCell);
}

#[test]
fn zero_padded_sqlite() {
    run(
        Kind::Sqlite,
        "bytes5",
        &one_file_seed(),
        &zero_padded_cells(),
        Stores::PerCell,
    );
}

#[test]
fn zero_padded_postgresql() {
    run(
        Kind::Postgres,
        "bytes5",
        &one_file_seed(),
        &zero_padded_cells(),
        Stores::PerCell,
    );
}

#[test]
fn cutoff_and_pack_limit_sqlite() {
    run(
        Kind::Sqlite,
        "bytes1",
        &one_file_seed(),
        &cutoff_cells(),
        Stores::PerCell,
    );
}

#[test]
fn cutoff_and_pack_limit_postgresql() {
    run(
        Kind::Postgres,
        "bytes1",
        &one_file_seed(),
        &cutoff_cells(),
        Stores::PerCell,
    );
}

#[test]
fn four_mib_publications_sqlite() {
    run(
        Kind::Sqlite,
        "bytes2",
        &one_file_seed(),
        &four_mib_cells(),
        Stores::PerCell,
    );
}

#[test]
fn four_mib_publications_postgresql() {
    run(
        Kind::Postgres,
        "bytes2",
        &one_file_seed(),
        &four_mib_cells(),
        Stores::PerCell,
    );
}

#[test]
fn sixteen_mib_publications_sqlite() {
    run(
        Kind::Sqlite,
        "bytes3",
        &one_file_seed(),
        &sixteen_mib_cells(),
        Stores::PerCell,
    );
}

#[test]
fn sixteen_mib_publications_postgresql() {
    run(
        Kind::Postgres,
        "bytes3",
        &one_file_seed(),
        &sixteen_mib_cells(),
        Stores::PerCell,
    );
}

#[test]
fn seeded_mixes_sqlite() {
    run(
        Kind::Sqlite,
        "bytes4",
        &one_file_seed(),
        &mixed_cells(),
        Stores::PerCell,
    );
}

#[test]
fn seeded_mixes_postgresql() {
    run(
        Kind::Postgres,
        "bytes4",
        &one_file_seed(),
        &mixed_cells(),
        Stores::PerCell,
    );
}
