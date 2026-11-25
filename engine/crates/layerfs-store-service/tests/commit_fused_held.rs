//! #768 stage 2: a fused Workspace Commit whose held last publication is
//! larger than the read caches of its Storage handle.
//!
//! While the Save's last publication is held for the fused transaction, the
//! Commit's own reads (the logical count's file lengths, the binding hints)
//! must find every held location, pack body and inode value although the
//! Store does not have them yet, however many there are. Each case Commits
//! fused (one fused Commit, no fallback, `Store::fused_counts`), and a fresh
//! Store service reads the Branch back with every file's bytes.
//!
//! * A new directory of 8,000 small files and a later directory: more held
//!   locations than the locator cache keeps (`READ_OBJECT_LIMIT`, 4,096).
//!   The binding hints also cross the 4,096-row transaction boundary.
//! * A 20,000-file seed, then 400 files rewritten across it: held inode
//!   values and metadata packs spread over many leaves.
//! * New files of 120 KiB that compress well (40 of them beside 300 small
//!   files, and 200 alone): many canonical bytes per pack, so the last
//!   publication splits at the transaction byte bound and the held batch
//!   names objects whose pack an earlier batch of the same finish already
//!   published.
//!
//! The PostgreSQL halves need `LAYERFS_TEST_POSTGRES_URL` and report
//! NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{apply, commit, Op, TestStore, Verdict};
use layerfs_history::BindingHint;
use layerfs_store_service::write::mutation::Mutation;

/// 40 bytes naming `n`, distinct per file.
fn small(n: usize, round: u8) -> Vec<u8> {
    format!("{round}:{n:06}:abcdefghijklmnopqrstuvwxyz0123")
        .into_bytes()
        .into_iter()
        .take(40)
        .collect()
}

/// Applies `ops`, Commits once and asserts it was fused and committed.
fn commit_fused(store: &TestStore, ops: &[Op]) -> layerfs_history::BranchId {
    let branch = store.fork("work");
    let service = store.open();
    let workspace = service.bind(branch);
    for op in ops {
        apply(&workspace, op).unwrap_or_else(|error| panic!("{:?}: {error}", store.kind));
    }
    let before = service.service.store.fused_counts();
    let result = commit(&workspace);
    let verdict = adversary::verdict(&result);
    assert!(
        matches!(verdict, Verdict::Committed(_)),
        "{:?}: {verdict:?}",
        store.kind
    );
    let after = service.service.store.fused_counts();
    assert_eq!(
        (after.0 - before.0, after.1 - before.1),
        (1, 0),
        "{:?}: one fused Commit, no fallback",
        store.kind
    );
    service.unmount(workspace);
    service.stop();
    branch
}

/// What a fresh Store service reads back from `branch`: the bytes of
/// `files` and the number of entries in each of `directories`.
fn read_back(
    store: &TestStore,
    branch: layerfs_history::BranchId,
    files: &[&str],
    directories: &[&str],
) -> (Vec<Vec<u8>>, Vec<usize>) {
    let service = store.open();
    let workspace = service.bind(branch);
    let m = Mutation::new(&workspace).unwrap();
    let bytes = files
        .iter()
        .map(|path| {
            let serial = m.resolve(path).unwrap().serial;
            let mut bytes = Vec::new();
            loop {
                let piece = m.read(serial, bytes.len() as u64, 64 * 1024).unwrap();
                if piece.is_empty() {
                    break bytes;
                }
                bytes.extend_from_slice(&piece);
            }
        })
        .collect();
    let counts = directories
        .iter()
        .map(|path| {
            let serial = m.resolve(path).unwrap().serial;
            let (mut count, mut after) = (0, None::<Vec<u8>>);
            loop {
                let page = m
                    .view
                    .list(m.op.overlay(), serial, after.as_deref())
                    .unwrap();
                count += page.entries.len();
                after = page.continuation;
                if after.is_none() {
                    break count;
                }
            }
        })
        .collect();
    m.finish().unwrap();
    service.unmount(workspace);
    service.stop();
    (bytes, counts)
}

fn assert_bindings(store: &TestStore, branch: layerfs_history::BranchId, paths: &[&str]) {
    let service = store.open();
    let workspace = service.bind(branch);
    let m = Mutation::new(&workspace).unwrap();
    for path in paths {
        let (parent, name) = path.rsplit_once('/').unwrap();
        let parent = m
            .resolve(if parent.is_empty() { "/" } else { parent })
            .unwrap();
        let child = m.resolve(path).unwrap();
        assert_eq!(
            service
                .service
                .store
                .history()
                .bindings_of(child.serial)
                .unwrap(),
            [BindingHint {
                child: child.serial,
                parent: parent.serial,
                name: name.as_bytes().to_vec(),
            }],
            "{:?}: exactly one hint for {path}",
            store.kind
        );
    }
    m.finish().unwrap();
    service.unmount(workspace);
    service.stop();
}

#[test]
fn a_new_directory_of_many_small_files_commits_fused() {
    const FILES: usize = 8_000;
    for store in TestStore::both("held-many", &[("/seed", b"seed\n")]) {
        let mut ops = vec![Op::Mkdir("/d0".into()), Op::Mkdir("/d1".into())];
        ops.extend((0..FILES).map(|n| Op::Write(format!("/d0/f{n:05}"), small(n, 1))));
        ops.push(Op::Write("/d1/later".into(), b"later".to_vec()));
        let branch = commit_fused(&store, &ops);
        let checked = [0, 1, FILES / 2, FILES - 1];
        let paths: Vec<String> = checked.iter().map(|n| format!("/d0/f{n:05}")).collect();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let (bytes, counts) = read_back(&store, branch, &paths, &["/d0"]);
        assert_eq!(counts, [FILES], "{:?}", store.kind);
        for (n, bytes) in checked.iter().zip(bytes) {
            assert_eq!(bytes, small(*n, 1), "{:?}", store.kind);
        }
        // The two root bindings precede /d0's names, making f04093 the
        // last hint in the prefix and f04094 the first hint after it.
        assert_bindings(
            &store,
            branch,
            &[
                "/d0",
                "/d1",
                "/d0/f00000",
                "/d0/f04093",
                "/d0/f04094",
                "/d0/f04095",
                "/d0/f07999",
                "/d1/later",
            ],
        );
    }
}

#[test]
fn spread_rewrites_over_a_large_tree_commit_fused() {
    const SEEDED: usize = 20_000;
    let paths: Vec<String> = (0..SEEDED)
        .map(|n| format!("/s{:02}/f{n:05}", n / 1_000))
        .collect();
    let bytes: Vec<Vec<u8>> = (0..SEEDED).map(|n| small(n, 0)).collect();
    let seed: Vec<(&str, &[u8])> = paths
        .iter()
        .zip(&bytes)
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    for store in TestStore::both("held-spread", &seed) {
        let ops: Vec<Op> = (0..SEEDED)
            .step_by(50)
            .map(|n| Op::Write(paths[n].clone(), small(n, 2)))
            .collect();
        assert_eq!(ops.len(), 400);
        let branch = commit_fused(&store, &ops);
        // Rewritten, then untouched.
        let checked = [(0, 2), (50, 2), (9_950, 2), (19_950, 2)]
            .into_iter()
            .chain([(1, 0), (49, 0), (10_001, 0), (19_999, 0)]);
        let checked: Vec<(usize, u8)> = checked.collect();
        let files: Vec<&str> = checked.iter().map(|(n, _)| paths[*n].as_str()).collect();
        let directories: Vec<String> = (0..20).map(|d| format!("/s{d:02}")).collect();
        let directories: Vec<&str> = directories.iter().map(String::as_str).collect();
        let (bytes, counts) = read_back(&store, branch, &files, &directories);
        for ((n, round), bytes) in checked.iter().zip(bytes) {
            assert_eq!(bytes, small(*n, *round), "{:?} {}", store.kind, paths[*n]);
        }
        assert_eq!(counts, vec![1_000; 20], "{:?}", store.kind);
    }
}

/// `length` bytes naming `n` in their first eight, zeros after: distinct
/// files that compress to almost nothing.
fn padded(n: usize, length: usize) -> Vec<u8> {
    let mut bytes = vec![0; length];
    bytes[..8].copy_from_slice(&(n as u64).to_le_bytes());
    bytes
}

/// `large` new files of 120 KiB and `small` new files of 40 bytes in one
/// new directory, Committed fused and read back.
fn large_files(label: &str, large: usize, small_files: usize) {
    const LARGE: usize = 120 * 1024;
    for store in TestStore::both(label, &[("/seed", b"seed\n")]) {
        let mut ops = vec![Op::Mkdir("/big".into())];
        ops.extend((0..large).map(|n| Op::Write(format!("/big/l{n:04}"), padded(n, LARGE))));
        ops.extend((0..small_files).map(|n| Op::Write(format!("/big/s{n:04}"), small(n, 3))));
        let branch = commit_fused(&store, &ops);
        let mut checked: Vec<(String, Vec<u8>)> = [0, large / 2, large - 1]
            .into_iter()
            .map(|n| (format!("/big/l{n:04}"), padded(n, LARGE)))
            .collect();
        if small_files > 0 {
            checked.extend(
                [0, small_files - 1]
                    .into_iter()
                    .map(|n| (format!("/big/s{n:04}"), small(n, 3))),
            );
        }
        let files: Vec<&str> = checked.iter().map(|(path, _)| path.as_str()).collect();
        let (bytes, counts) = read_back(&store, branch, &files, &["/big"]);
        assert_eq!(counts, [large + small_files], "{:?}", store.kind);
        for ((path, expected), bytes) in checked.iter().zip(bytes) {
            assert!(bytes == *expected, "{:?}: {path} differs", store.kind);
        }
    }
}

#[test]
fn large_files_beside_small_ones_commit_fused() {
    large_files("held-large-mixed", 40, 300);
}

#[test]
fn many_large_files_commit_fused() {
    large_files("held-large", 200, 0);
}
