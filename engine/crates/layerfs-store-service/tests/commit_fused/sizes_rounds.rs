//! #768 stage 2b size boundaries, part five: bounds crossed over several
//! Commits of one Workspace.
//!
//! Each cell applies rounds of changes to one bound Workspace and Commits
//! after every round, fused on one Store and forced on its twin, on SQLite
//! and PostgreSQL (`support/sizes.rs`); every round's root and statistics
//! must equal the twin's. The bounds (values at efcd412dcb):
//!
//! * Metadata delta depth (`DEFAULT_METADATA_DELTA_MAX_DEPTH`, 8): the
//!   same inode leaf rewritten in 12 Commits, so its pooled PREFIX chain
//!   reaches the bound and restarts.
//! * `KEPT_ORDINALS` (128): the unused pooled ordinals a Storage handle
//!   keeps for its next Save; Saves of 126 to 130 and 256 new inode values
//!   after a first one (the release row of the held publication).
//! * Whole-file delta depth (`DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH`, 16):
//!   three 100 KiB files appended to in 18 Commits.
//! * Chunk delta depth (`DEFAULT_CHUNK_DELTA_MAX_DEPTH`, 8): three 600 KiB
//!   chunked files appended to in 10 Commits.
//! * Several mid-sized Saves on one handle: 5 Commits of 1,500 new files.
//! * Large, then small, then large again: 70 files of 120 KiB (SQLite: two
//!   4 MiB publications), 2,000 tiny files, the 70 rewritten, then deleted.
//!
//! The pack-id reservation block (`ReservationBlocks::packs`, 4,096) is not
//! crossed: one Save would need about 4,096 packs (~1 GiB of payload), or
//! one handle hundreds of Commits.
//!
//! The PostgreSQL tests need `LAYERFS_TEST_POSTGRES_URL` and report
//! NOT_RUN without it.
use crate::adversary;
use crate::verify_sizes as sizes;

use adversary::{text_bytes, Kind, Op};
use sizes::{noise, one_file_seed, run, tiny, Cell, Stores};

/// Text bytes for file `n` of `cell` in `round`.
fn text(cell: usize, n: usize, round: usize, length: usize) -> Vec<u8> {
    text_bytes(
        ((cell as u64) << 40) ^ ((round as u64) << 20) ^ n as u64,
        length,
    )
}

fn cells() -> Vec<Cell> {
    let mut cells = Vec::new();

    let cell = cells.len();
    let mut rounds = vec![std::iter::once(Op::Mkdir("/m".into()))
        .chain((0..50).map(|n| Op::Write(format!("/m/f{n:02}"), tiny(cell, n, 0))))
        .collect::<Vec<_>>()];
    rounds.extend((1..=12).map(|round| {
        (0..3)
            .map(|n| Op::Write(format!("/m/f{n:02}"), tiny(cell, n, round)))
            .collect()
    }));
    cells.push(Cell::rounds(
        "metadata-depth",
        "one inode leaf rewritten in 12 Commits: metadata delta depth 8",
        rounds,
    ));

    let cell = cells.len();
    let mut rounds = vec![std::iter::once(Op::Mkdir("/k".into()))
        .chain((0..10).map(|n| Op::Write(format!("/k/a{n:03}"), tiny(cell, n, 0))))
        .collect::<Vec<_>>()];
    for (round, count) in [126, 127, 128, 129, 130, 256].into_iter().enumerate() {
        let round = round + 1;
        rounds.push(
            (0..count)
                .map(|n| Op::Write(format!("/k/r{round}_{n:03}"), tiny(cell, n, round)))
                .collect(),
        );
    }
    cells.push(Cell::rounds(
        "kept-ordinals",
        "Saves of 126-130 and 256 new values after a first: KEPT_ORDINALS 128",
        rounds,
    ));

    let cell = cells.len();
    let mut rounds = vec![std::iter::once(Op::Mkdir("/w".into()))
        .chain((0..3).map(|n| Op::Write(format!("/w/f{n}"), text(cell, n, 0, 100 * 1024))))
        .collect::<Vec<_>>()];
    rounds.extend((1..=18).map(|round| {
        (0..3)
            .map(|n| Op::Append(format!("/w/f{n}"), text(cell, n, round, 1024)))
            .collect()
    }));
    cells.push(Cell::rounds(
        "whole-file-delta-depth",
        "three 100 KiB files appended to in 18 Commits: whole-file depth 16",
        rounds,
    ));

    let cell = cells.len();
    let mut rounds = vec![std::iter::once(Op::Mkdir("/c".into()))
        .chain((0..3).map(|n| Op::Write(format!("/c/f{n}"), text(cell, n, 0, 600 * 1024))))
        .collect::<Vec<_>>()];
    rounds.extend((1..=10).map(|round| {
        (0..3)
            .map(|n| Op::Append(format!("/c/f{n}"), text(cell, n, round, 2048)))
            .collect()
    }));
    cells.push(Cell::rounds(
        "chunk-delta-depth",
        "three 600 KiB files appended to in 10 Commits: chunk depth 8",
        rounds,
    ));

    let cell = cells.len();
    let rounds = (0..5)
        .map(|round| {
            std::iter::once(Op::Mkdir(format!("/p{round}")))
                .chain(
                    (0..1_500)
                        .map(|n| Op::Write(format!("/p{round}/f{n:04}"), tiny(cell, n, round))),
                )
                .collect()
        })
        .collect();
    cells.push(Cell::rounds(
        "five-mid-saves",
        "5 Commits of 1,500 new files on one handle",
        rounds,
    ));

    let cell = cells.len();
    let large = |round: usize| -> Vec<Op> {
        (0..70)
            .map(|n| {
                Op::Write(
                    format!("/g/l{n:02}"),
                    noise(cell, n + round * 100, 120 * 1024),
                )
            })
            .collect()
    };
    let mut first = vec![Op::Mkdir("/g".into())];
    first.extend(large(0));
    let rounds = vec![
        first,
        std::iter::once(Op::Mkdir("/h".into()))
            .chain((0..2_000).map(|n| Op::Write(format!("/h/f{n:04}"), tiny(cell, n, 1))))
            .collect(),
        large(2),
        (0..70).map(|n| Op::Unlink(format!("/g/l{n:02}"))).collect(),
    ];
    cells.push(Cell::rounds(
        "large-small-large",
        "70 x 120 KiB, 2,000 tiny, the 70 rewritten, then deleted",
        rounds,
    ));
    cells
}

#[test]
fn rounds_sqlite() {
    run(
        Kind::Sqlite,
        "rounds",
        &one_file_seed(),
        &cells(),
        Stores::PerCell,
    );
}

#[test]
fn rounds_postgresql() {
    run(
        Kind::Postgres,
        "rounds",
        &one_file_seed(),
        &cells(),
        Stores::PerCell,
    );
}
