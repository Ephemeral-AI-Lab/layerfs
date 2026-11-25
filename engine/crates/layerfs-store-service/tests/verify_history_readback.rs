//! #768 independent verification: every version a run of fused Commits
//! made reads back exactly, through the pooled delta chains (prefetched
//! since #768) at every depth, on SQLite and PostgreSQL.
//!
//! One Workspace commits 20 rounds over a seed of a 600 KiB file, a
//! directory of 300 small files and a log: small edits at three places of
//! the large file (chunk deltas), an append to the log, a rewrite of one
//! small file and a rename in the directory (the directory's inode leaf and
//! its pooled PREFIX chain, past the depth bound of 8 and its restart).
//! Every Commit's view is recorded. Then a Branch is forked at each Commit
//! and read back: all of them through one long-lived service in a shuffled
//! order (warm caches and chain prefetches crossing versions), and every
//! third through a fresh service. Each equals the recorded view; the
//! Stores pass their audit, and both backends give equal roots.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{
    apply, commit, history, text_bytes, tree_of, verdict, Op, TestStore, Tree, Verdict,
};
use layerfs_history::{ForkRequest, ForkSource, HistoryName};

const ROUNDS: usize = 20;

fn seed() -> Vec<(String, Vec<u8>)> {
    let mut seed = vec![
        ("/big".to_owned(), text_bytes(600, 600 * 1024)),
        ("/log".to_owned(), b"log\n".to_vec()),
    ];
    for n in 0..300 {
        seed.push((format!("/dir/f{n}"), format!("file {n}\n").into_bytes()));
    }
    seed
}

fn run(store: &TestStore) -> Vec<[u8; 32]> {
    let branch = store.fork("main-line");
    let service = store.open();
    let workspace = service.bind(branch);
    let mut big = text_bytes(600, 600 * 1024);
    let mut versions: Vec<(layerfs_history::CommitId, Tree)> = Vec::new();
    for round in 0..ROUNDS {
        for k in 0..3 {
            let at = (round * 7919 + k * 200_003) % (big.len() - 40);
            big[at..at + 40].copy_from_slice(&text_bytes((round * 3 + k) as u64, 40));
        }
        let ops = vec![
            Op::Write("/big".into(), big.clone()),
            Op::Append("/log".into(), format!("round {round}\n").into_bytes()),
            Op::Write(
                format!("/dir/f{round}"),
                format!("rewritten {round}\n").into_bytes(),
            ),
            Op::Rename(format!("/dir/f{}", 100 + round), format!("/dir/g{round}")),
        ];
        for op in &ops {
            apply(&workspace, op).unwrap_or_else(|e| panic!("{op:?}: {e}"));
        }
        let view = tree_of(&workspace);
        match verdict(&commit(&workspace)) {
            Verdict::Committed(id) => versions.push((id, view)),
            other => panic!("{:?} round {round}: {other:?}", store.kind),
        }
    }
    service.unmount(workspace);
    service.stop();
    // A Branch at every version.
    let handles = store.handles();
    let forks: Vec<_> = versions
        .iter()
        .enumerate()
        .map(|(n, (commit, view))| {
            let name = format!("v{n}");
            let id = store.branch(&name);
            history(&handles)
                .fork(&ForkRequest {
                    stack: store.stack,
                    branch: id,
                    name: HistoryName::new(&name).unwrap(),
                    source: ForkSource::Commit {
                        branch,
                        commit: *commit,
                    },
                })
                .unwrap();
            (id, view.clone())
        })
        .collect();
    handles.close();
    // One long-lived reader, shuffled order.
    let reader = store.open();
    let mut order: Vec<usize> = (0..forks.len()).collect();
    let mut state = 0x2545_f491_4f6c_dd1du64;
    for i in (1..order.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        order.swap(i, (state % (i as u64 + 1)) as usize);
    }
    for &n in &order {
        let (id, view) = &forks[n];
        let workspace = reader.bind(*id);
        assert_eq!(
            &tree_of(&workspace),
            view,
            "{:?}: version {n} (warm reader)",
            store.kind
        );
        reader.unmount(workspace);
    }
    reader.stop();
    for (n, (id, view)) in forks.iter().enumerate().filter(|(n, _)| n % 3 == 2) {
        assert_eq!(
            &store.tree(*id),
            view,
            "{:?}: version {n} (fresh reader)",
            store.kind
        );
    }
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "{:?}: audit {failures:#?}", store.kind);
    store
        .chain(branch)
        .iter()
        .rev()
        .map(|r| *r.root.as_bytes())
        .collect()
}

#[test]
fn every_fused_version_reads_back_through_its_chains() {
    let seed = seed();
    let seed: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let roots: Vec<_> = TestStore::both("v768-history", &seed)
        .iter()
        .map(run)
        .collect();
    if let [sqlite, pg] = &roots[..] {
        assert_eq!(sqlite, pg, "SQLite and PostgreSQL roots");
    }
}
