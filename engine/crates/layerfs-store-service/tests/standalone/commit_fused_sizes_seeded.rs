//! #768 stage 2b size boundaries, part three: spread rewrites over a large
//! seed.
//!
//! The seed is a tree of tiny files, 1,000 per directory; each cell
//! rewrites files spread over all of it, so the Save rebuilds inode leaves
//! and assigns new inode values across the whole tree (`support/sizes.rs`;
//! SQLite cells get copies of one seeded Store, PostgreSQL cells share
//! one pair per group, each on its own Branch). The bounds (values at
//! efcd412dcb):
//!
//! * `PUBLISHED_VALUES` (`POOLED_VALUE_CACHE_BYTES` / `INODE_VALUE_BYTES`
//!   = 524,288 / 73 = 7,182): the published inode values a Storage handle
//!   keeps, oldest first. 7,200 spread rewrites assign one new value each
//!   and evict the earliest.
//! * `READ_OBJECT_LIMIT` (4,096) and `TRANSACTION_ROW_LIMIT` (8,191) in
//!   rewritten files and their values (4,100 rewrites: 8,200 rows).
//! * `DEPENDENCY_PACK_CACHE_BYTES` (2 MiB): metadata packs the Save reads
//!   back while it rebuilds leaves spread over the whole tree.
//!
//! On both backends the seed is 8,000 files; a PostgreSQL Init of 20,000
//! took 35 s per Store on the test server. The 20,000-file shape runs on
//! SQLite only (its review case, 400 spread rewrites, also runs on both
//! backends in `commit_fused_held.rs`).
//!
//! The PostgreSQL tests need `LAYERFS_TEST_POSTGRES_URL` and report
//! NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/verify/sizes.rs"]
#[allow(dead_code)]
mod sizes;

use adversary::{Kind, Op};
use sizes::{run, seed_files, seed_path, spread, tiny, Cell, Stores};

fn rewrite(cell: usize, count: usize, seeded: usize) -> Vec<Op> {
    spread(count, seeded)
        .map(|n| Op::Write(seed_path(n), tiny(cell, n, 1)))
        .collect()
}

fn rewrite_cells(seeded: usize) -> Vec<Cell> {
    vec![
        Cell::new(
            "rewrite-4100",
            "4,100 spread rewrites: READ_OBJECT_LIMIT, 8,200 rows",
            rewrite(0, 4_100, seeded),
        ),
        Cell::new(
            "rewrite-7200",
            "7,200 spread rewrites: PUBLISHED_VALUES 7,182 just above",
            rewrite(1, 7_200, seeded),
        )
        // 7,200 new inode values and their files pass TRANSACTION_ROW_LIMIT:
        // the finish publishes a batch before the held tail.
        .splits(&[Kind::Sqlite, Kind::Postgres]),
    ]
}

#[test]
fn spread_rewrites_sqlite() {
    let cells = rewrite_cells(8_000);
    run(
        Kind::Sqlite,
        "seed1",
        &seed_files(8_000),
        &cells,
        Stores::Shared,
    );
}

#[test]
fn spread_rewrites_postgresql() {
    let cells = rewrite_cells(8_000);
    run(
        Kind::Postgres,
        "seed1",
        &seed_files(8_000),
        &cells,
        Stores::Shared,
    );
}

#[test]
fn spread_rewrites_over_20000_files_sqlite() {
    let mut cells = vec![Cell::new(
        "rewrite-400",
        "400 spread rewrites over 20,000 files: the value FIFO review case",
        rewrite(2, 400, 20_000),
    )];
    cells.extend(rewrite_cells(20_000));
    run(
        Kind::Sqlite,
        "seed20k",
        &seed_files(20_000),
        &cells,
        Stores::Shared,
    );
}
