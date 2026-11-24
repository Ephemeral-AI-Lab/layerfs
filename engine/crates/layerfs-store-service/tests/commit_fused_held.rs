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
//! * One new directory of 8,000 small files: more held locations than the
//!   locator cache keeps (`READ_OBJECT_LIMIT`, 4,096).
//! * A 20,000-file seed, then 400 files rewritten across it: held inode
//!   values and metadata packs spread over many leaves.
//!
//! The PostgreSQL halves need `LAYERFS_TEST_POSTGRES_URL` and report
//! NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{apply, commit, Op, TestStore, Verdict};
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

#[test]
fn a_new_directory_of_many_small_files_commits_fused() {
    const FILES: usize = 8_000;
    for store in TestStore::both("held-many", &[("/seed", b"seed\n")]) {
        let mut ops = vec![Op::Mkdir("/d0".into())];
        ops.extend((0..FILES).map(|n| Op::Write(format!("/d0/f{n:05}"), small(n, 1))));
        let branch = commit_fused(&store, &ops);
        let checked = [0, 1, FILES / 2, FILES - 1];
        let paths: Vec<String> = checked.iter().map(|n| format!("/d0/f{n:05}")).collect();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let (bytes, counts) = read_back(&store, branch, &paths, &["/d0"]);
        assert_eq!(counts, [FILES], "{:?}", store.kind);
        for (n, bytes) in checked.iter().zip(bytes) {
            assert_eq!(bytes, small(*n, 1), "{:?}", store.kind);
        }
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
