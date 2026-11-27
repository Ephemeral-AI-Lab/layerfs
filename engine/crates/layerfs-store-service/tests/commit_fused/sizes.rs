//! #768 stage 2b size boundaries, part one: many tiny new files.
//!
//! Each cell writes new 40-byte files into a bound Workspace and Commits
//! once with defaults (fused) on one Store and with `force: true` (the
//! separate path) on a twin Store from the same seed, on SQLite and
//! PostgreSQL; `support/sizes.rs` has the checks and the report. The bounds
//! the cells cross (values at efcd412dcb):
//!
//! * `READ_OBJECT_LIMIT` (4,096): the locator cache and one read wave; a
//!   held publication with more locations than it keeps.
//! * Hint `BATCH` (4,096, `store/hints.rs`): a fused Commit carries at most
//!   this many new binding hints in its own transaction; past it they are
//!   recorded on their own first.
//! * `TRANSACTION_ROW_LIMIT` (8,191): rows of one publication (objects,
//!   packs, inode values, signatures); a Save past it publishes in several
//!   batches and holds only the last.
//! * `BATCH_OBJECT_LIMIT` (512): objects of one pending wave.
//! * Directory fan-out and depth: one directory table, many directories,
//!   deep chains (the tree's own objects and binding hints).
//! * The Commit's staging set of constructed objects (8 MiB at the
//!   default 64 MiB cache): below it the Commit is fused; above it the
//!   Commit legitimately takes the separate path (the cell says why).
//!
//! The PostgreSQL halves need `LAYERFS_TEST_POSTGRES_URL` and report
//! NOT_RUN without it.
use crate::adversary;
use crate::verify_sizes as sizes;

use adversary::{Kind, Op};
use sizes::{one_file_seed, run, tiny, Cell, Stores};

const BOTH: &[Kind] = &[Kind::Sqlite, Kind::Postgres];

/// `files` new tiny files in one new directory.
fn one_directory(cell: usize, files: usize) -> Vec<Op> {
    let mut ops = vec![Op::Mkdir(format!("/d{cell}"))];
    ops.extend((0..files).map(|n| Op::Write(format!("/d{cell}/f{n:05}"), tiny(cell, n, 0))));
    ops
}

/// `directories` new directories of `files` tiny files each.
fn spread(cell: usize, directories: usize, files: usize) -> Vec<Op> {
    let mut ops = vec![Op::Mkdir(format!("/s{cell}"))];
    for d in 0..directories {
        ops.push(Op::Mkdir(format!("/s{cell}/d{d:04}")));
        ops.extend((0..files).map(|f| {
            Op::Write(
                format!("/s{cell}/d{d:04}/f{f:03}"),
                tiny(cell, d * files + f, 0),
            )
        }));
    }
    ops
}

/// `chains` directory chains `depth` deep, one tiny file at every level.
fn chains(cell: usize, chains: usize, depth: usize) -> Vec<Op> {
    let mut ops = Vec::new();
    for c in 0..chains {
        let mut at = format!("/k{cell}_{c:02}");
        for level in 0..depth {
            ops.push(Op::Mkdir(at.clone()));
            ops.push(Op::Write(
                format!("{at}/f"),
                tiny(cell, c * depth + level, 0),
            ));
            at.push_str(&format!("/l{level:03}"));
        }
    }
    ops
}

fn one_directory_cells() -> Vec<Cell> {
    let cases: [(usize, &str); 5] = [
        (1_023, "below every bound"),
        (4_095, "READ_OBJECT_LIMIT and hint BATCH 4,096, just below"),
        (4_096, "READ_OBJECT_LIMIT and hint BATCH 4,096, at"),
        (4_097, "READ_OBJECT_LIMIT and hint BATCH 4,096, just above"),
        (8_200, "TRANSACTION_ROW_LIMIT 8,191 in files alone"),
    ];
    cases
        .iter()
        .enumerate()
        .map(|(cell, (files, probe))| {
            Cell::new(
                format!("one-dir-{files}"),
                *probe,
                one_directory(cell, *files),
            )
            // Past TRANSACTION_ROW_LIMIT the finish publishes a batch of
            // inode values alone before the held tail.
            .splits(if *files > 8_191 { BOTH } else { &[] })
        })
        .collect()
}

fn many_directories_cells() -> Vec<Cell> {
    let cases: [(usize, usize, &str); 3] = [
        (
            64,
            64,
            "4,096 files and 64 directories: hint BATCH crossed by directories",
        ),
        (
            512,
            8,
            "512 directory tables (BATCH_OBJECT_LIMIT 512 of tables)",
        ),
        (
            4_200,
            1,
            "4,200 directories of one file: READ_OBJECT_LIMIT in tables, rows past 8,191",
        ),
    ];
    cases
        .iter()
        .enumerate()
        .map(|(cell, (directories, files, probe))| {
            Cell::new(
                format!("spread-{directories}x{files}"),
                *probe,
                spread(cell, *directories, *files),
            )
            .splits(if directories * files > 4_100 {
                BOTH
            } else {
                &[]
            })
        })
        .collect()
}

fn chain_cells() -> Vec<Cell> {
    let cases: [(usize, usize, &str); 3] = [
        (1, 128, "one chain 128 deep, a file at every level"),
        (1, 400, "one chain 400 deep"),
        (64, 16, "64 chains 16 deep: 1,024 directories"),
    ];
    cases
        .iter()
        .enumerate()
        .map(|(cell, (count, depth, probe))| {
            Cell::new(
                format!("chains-{count}x{depth}"),
                *probe,
                chains(cell, *count, *depth),
            )
        })
        .collect()
}

/// `count` new symlinks with distinct 4,000-byte targets in one directory.
fn symlinks(cell: usize, count: usize) -> Vec<Op> {
    let mut ops = vec![Op::Mkdir(format!("/y{cell}"))];
    ops.extend((0..count).map(|n| {
        let mut target = format!("c{cell}/n{n:05}/").into_bytes();
        target.resize(4_000, b'a' + (n % 26) as u8);
        Op::Symlink(format!("/y{cell}/l{n:05}"), target)
    }));
    ops
}

/// The staging set a Commit keeps of what it constructed is an eighth of
/// the Store service's cache (`CONSTRUCTED_SHARE`; 64 MiB here and in the
/// daemon's default, so 8 MiB, charged bytes + 256 per object). Symlink
/// targets are staged whole, so a few thousand long ones cross it.
fn staging_cells() -> Vec<Cell> {
    vec![
        Cell::new(
            "symlinks-1500x4000",
            "staging set 8 MiB, below: 6.1 MB of symlink targets",
            symlinks(0, 1_500),
        ),
        Cell::new(
            "symlinks-2400x4000",
            "staging set 8 MiB, above: 9.6 MB of symlink targets",
            symlinks(1, 2_400),
        )
        .separate(
            "the staging set dropped objects (ConstructedObjects::covers is false): \
             validating the root would read this Commit's own objects from the Store, \
             which does not hold the held publication yet, so the Commit is not fused \
             (commit.rs, fused_eligible && built.covers)",
        ),
    ]
}

#[test]
fn staging_set_sqlite() {
    run(
        Kind::Sqlite,
        "tiny4",
        &one_file_seed(),
        &staging_cells(),
        Stores::PerCell,
    );
}

#[test]
fn staging_set_postgresql() {
    run(
        Kind::Postgres,
        "tiny4",
        &one_file_seed(),
        &staging_cells(),
        Stores::PerCell,
    );
}

#[test]
fn one_directory_sqlite() {
    run(
        Kind::Sqlite,
        "tiny1",
        &one_file_seed(),
        &one_directory_cells(),
        Stores::PerCell,
    );
}

#[test]
fn one_directory_postgresql() {
    run(
        Kind::Postgres,
        "tiny1",
        &one_file_seed(),
        &one_directory_cells(),
        Stores::PerCell,
    );
}

#[test]
fn many_directories_sqlite() {
    run(
        Kind::Sqlite,
        "tiny2",
        &one_file_seed(),
        &many_directories_cells(),
        Stores::PerCell,
    );
}

#[test]
fn many_directories_postgresql() {
    run(
        Kind::Postgres,
        "tiny2",
        &one_file_seed(),
        &many_directories_cells(),
        Stores::PerCell,
    );
}

#[test]
fn deep_chains_sqlite() {
    run(
        Kind::Sqlite,
        "tiny3",
        &one_file_seed(),
        &chain_cells(),
        Stores::PerCell,
    );
}

#[test]
fn deep_chains_postgresql() {
    run(
        Kind::Postgres,
        "tiny3",
        &one_file_seed(),
        &chain_cells(),
        Stores::PerCell,
    );
}
