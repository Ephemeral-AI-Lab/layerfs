//! #768 stage 2b size boundaries, part four: renames, deletions, appends
//! and seeded mixes over a 4,000-file seed.
//!
//! The seed is 4,000 tiny files, 1,000 per directory (`/s00`..`/s03`);
//! SQLite cells get copies of one seeded Store, PostgreSQL cells share one
//! pair per group, each on its own Branch (`support/sizes.rs`). The shapes:
//!
//! * Renames of large trees (one binding moves, every descendant stays):
//!   one directory of 1,000 files, three directories under a new parent.
//! * 2,000 spread files renamed into one new directory: new bindings over
//!   the whole serial range, removals from every seed directory.
//! * Deletions: 1,000 spread, one whole directory, 3,000 spread
//!   (`READ_OBJECT_LIMIT` 4,096 in touched entries with the rewritten
//!   tables).
//! * 1,000 spread appends (whole-file PREFIX deltas against the seed).
//! * 60 spread rewrites to 120 KiB: 7 MB of payload beside a leaf rebuild
//!   spread over the tree (SQLite: two 4 MiB publications).
//! * Seeded mixes of every change kind, from a listed seed list.
//!
//! The PostgreSQL tests need `LAYERFS_TEST_POSTGRES_URL` and report
//! NOT_RUN without it.
use crate::adversary;
use crate::verify_sizes as sizes;

use adversary::{seed_tree, text_bytes, Kind, Node, Op, Tree};
use sizes::{model, noise, run, seed_files, seed_path, spread, tiny, Cell, Rng, Stores};

const SEEDED: usize = 4_000;

fn moves_cells() -> Vec<Cell> {
    let spread = |count| spread(count, SEEDED);
    vec![
        Cell::new(
            "rename-dir-1000",
            "one directory of 1,000 files renamed",
            vec![Op::Rename("/s00".into(), "/moved00".into())],
        ),
        Cell::new(
            "rename-3-dirs",
            "three directories, 3,000 files, renamed under a new parent",
            std::iter::once(Op::Mkdir("/r".into()))
                .chain((1..=3).map(|d| Op::Rename(format!("/s{d:02}"), format!("/r/s{d:02}"))))
                .collect(),
        ),
        Cell::new(
            "rename-2000-files",
            "2,000 spread files renamed into one new directory",
            std::iter::once(Op::Mkdir("/n".into()))
                .chain(spread(2_000).map(|n| Op::Rename(seed_path(n), format!("/n/f{n:05}"))))
                .collect(),
        ),
        Cell::new(
            "unlink-1000-spread",
            "1,000 spread deletions",
            spread(1_000).map(|n| Op::Unlink(seed_path(n))).collect(),
        ),
        Cell::new(
            "unlink-dir-1000",
            "one whole directory deleted (1,000 files and the directory)",
            (3_000..4_000)
                .map(|n| Op::Unlink(seed_path(n)))
                .chain(std::iter::once(Op::Rmdir("/s03".into())))
                .collect(),
        ),
        Cell::new(
            "unlink-3000-spread",
            "3,000 spread deletions",
            spread(3_000).map(|n| Op::Unlink(seed_path(n))).collect(),
        ),
        Cell::new(
            "append-1000",
            "1,000 spread appends of 100 bytes",
            spread(1_000)
                .map(|n| Op::Append(seed_path(n), text_bytes(n as u64, 100)))
                .collect(),
        ),
        Cell::new(
            "rewrite-60-large",
            "60 spread rewrites to 120 KiB beside a spread leaf rebuild",
            spread(60)
                .map(|n| Op::Write(seed_path(n), noise(7, n, 120 * 1024)))
                .collect(),
        ),
    ]
}

#[test]
fn renames_deletions_appends_sqlite() {
    run(
        Kind::Sqlite,
        "moves",
        &seed_files(SEEDED),
        &moves_cells(),
        Stores::Shared,
    );
}

#[test]
fn renames_deletions_appends_postgresql() {
    run(
        Kind::Postgres,
        "moves",
        &seed_files(SEEDED),
        &moves_cells(),
        Stores::Shared,
    );
}

/// `count` seeded changes over `base`: creates (mostly tiny, some text,
/// a few large), rewrites, appends, deletions, file and directory renames,
/// mkdir, symlinks and rmdir of empty directories.
fn mixed(cell: usize, seed: u64, base: &Tree, count: usize) -> Vec<Op> {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut tree = base.clone();
    let lists = |tree: &Tree| -> (Vec<String>, Vec<String>) {
        let files = tree
            .iter()
            .filter(|(_, node)| matches!(node, Node::File(_)))
            .map(|(path, _)| path.clone())
            .collect();
        let mut dirs = vec![String::new()];
        dirs.extend(
            tree.iter()
                .filter(|(_, node)| matches!(node, Node::Dir))
                .map(|(path, _)| path.clone()),
        );
        (files, dirs)
    };
    let (mut files, mut dirs) = lists(&tree);
    let mut ops = Vec::with_capacity(count);
    for k in 0..count {
        let dir = dirs[rng.below(dirs.len())].clone();
        let roll = rng.below(100);
        let op = match roll {
            _ if roll < 30 || files.is_empty() => {
                let bytes = match rng.below(20) {
                    0 => noise(cell, k, 131_072 + rng.below(200 * 1024)),
                    1..=3 => text_bytes(((cell as u64) << 32) | k as u64, 1 + rng.below(64 * 1024)),
                    _ => tiny(cell, k, 2),
                };
                let path = format!("{dir}/n{cell}_{k}");
                files.push(path.clone());
                Op::Write(path, bytes)
            }
            30..=49 => Op::Write(files[rng.below(files.len())].clone(), tiny(cell, k, 3)),
            50..=59 => Op::Append(
                files[rng.below(files.len())].clone(),
                text_bytes(k as u64, 1 + rng.below(2_000)),
            ),
            60..=69 => Op::Unlink(files.swap_remove(rng.below(files.len()))),
            70..=79 => {
                let from = files.swap_remove(rng.below(files.len()));
                let to = format!("{dir}/v{cell}_{k}");
                files.push(to.clone());
                Op::Rename(from, to)
            }
            80..=86 => {
                let path = format!("{dir}/d{cell}_{k}");
                dirs.push(path.clone());
                Op::Mkdir(path)
            }
            87..=90 => Op::Symlink(format!("{dir}/y{cell}_{k}"), format!("t{k}").into_bytes()),
            91..=93 if !dir.is_empty() => Op::Rename(dir, format!("/top{cell}_{k}")),
            _ => {
                let prefix = format!("{dir}/");
                let empty = !dir.is_empty()
                    && tree
                        .range(prefix.clone()..)
                        .next()
                        .is_none_or(|(path, _)| !path.starts_with(&prefix));
                if empty {
                    dirs.retain(|d| *d != dir);
                    Op::Rmdir(dir)
                } else {
                    let path = format!("{dir}/e{cell}_{k}");
                    files.push(path.clone());
                    Op::Write(path, tiny(cell, k, 4))
                }
            }
        };
        // A directory rename moves every path below it: list them again.
        let moves_directory =
            matches!(&op, Op::Rename(from, _) if matches!(tree.get(from), Some(Node::Dir)));
        model(&mut tree, &op);
        if moves_directory {
            (files, dirs) = lists(&tree);
        }
        ops.push(op);
    }
    ops
}

/// The listed seeds of the mixed cells.
const SEEDS: [u64; 4] = [1, 7, 768, 65_537];

fn mixed_cells() -> Vec<Cell> {
    let base = seed_files(SEEDED);
    let refs: Vec<(&str, &[u8])> = base
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let tree = seed_tree(&refs);
    SEEDS
        .iter()
        .enumerate()
        .map(|(cell, seed)| {
            Cell::new(
                format!("mix-seed{seed}"),
                "1,500 seeded mixed changes over the 4,000-file seed",
                mixed(cell, *seed, &tree, 1_500),
            )
        })
        .collect()
}

#[test]
fn seeded_mixes_sqlite() {
    run(
        Kind::Sqlite,
        "mixes",
        &seed_files(SEEDED),
        &mixed_cells(),
        Stores::Shared,
    );
}

#[test]
fn seeded_mixes_postgresql() {
    run(
        Kind::Postgres,
        "mixes",
        &seed_files(SEEDED),
        &mixed_cells(),
        Stores::Shared,
    );
}
