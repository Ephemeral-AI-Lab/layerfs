//! #768 independent verification: seeded random Workspace workloads run on
//! SQLite (no faults) and on PostgreSQL with server-side faults injected
//! into the fused Commit, compared Commit by Commit.
//!
//! Before about half of the PostgreSQL Commits a one-shot trigger
//! (`support/verify768.rs`) is armed on a random table the fused message
//! writes, raising a random SQLSTATE: `40001` (`Busy`) or `P0001`
//! (`Integrity`), definite refusals, or `22012`, which the message reads as
//! a race at most steps (the call then continues on the separate path). A
//! refused Commit is followed at once by another Commit of the same
//! Workspace, which must land. An unknown outcome fails the test.
//!
//! The Workspace's live view equals an in-memory model after every change
//! batch, on both backends. At the end the two Branches hold the same
//! number of Commits with the same roots and logical sizes in the same
//! order (a refused attempt and its retry leave exactly what one clean
//! Commit leaves), the same tree, which equals the model, and both Stores
//! pass their audit; PostgreSQL also passes amcheck and the allocation
//! checks. Needs `LAYERFS_TEST_POSTGRES_URL` for the PostgreSQL half (the
//! comparison is NOT_RUN without it).
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;

use adversary::{
    apply, assert_linear, commit, random_bytes, text_bytes, tree_of, verdict, Kind, Node, Op,
    TestStore, Tree, Verdict,
};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n"), ("/d/a", b"a\n")];
const SEEDS: &[u64] = &[11, 29, 47, 53, 71, 89, 101, 131, 151, 173];
const COMMITS: usize = 14;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn parent(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some(("", _)) | None => "/",
        Some((p, _)) => p,
    }
}

fn children(tree: &Tree, dir: &str) -> usize {
    let prefix = if dir == "/" {
        "/".to_owned()
    } else {
        format!("{dir}/")
    };
    tree.keys()
        .filter(|k| k.starts_with(&prefix) && !k[prefix.len()..].contains('/'))
        .count()
}

fn model_apply(tree: &mut Tree, op: &Op) {
    match op {
        Op::Write(p, b) => {
            tree.insert(p.clone(), Node::File(b.clone()));
        }
        Op::Append(p, b) => match tree.get_mut(p) {
            Some(Node::File(bytes)) => bytes.extend_from_slice(b),
            other => panic!("append to {other:?}"),
        },
        Op::Mkdir(p) => {
            tree.insert(p.clone(), Node::Dir);
        }
        Op::Symlink(p, t) => {
            tree.insert(p.clone(), Node::Link(t.clone()));
        }
        Op::Rename(from, to) => {
            let node = tree.remove(from).unwrap();
            tree.insert(to.clone(), node);
        }
        Op::Unlink(p) | Op::Rmdir(p) => {
            tree.remove(p);
        }
    }
}

/// The change batches of one seed, each committed after it.
fn workload(seed: u64) -> Vec<Vec<Op>> {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut tree = adversary::seed_tree(SEED);
    let mut made = 0u64;
    let mut batches = Vec::new();
    for _ in 0..COMMITS {
        let mut batch = Vec::new();
        for _ in 0..2 + rng.below(5) {
            let dirs: Vec<String> = std::iter::once("/".to_owned())
                .chain(
                    tree.iter()
                        .filter(|(_, n)| **n == Node::Dir)
                        .map(|(k, _)| k.clone()),
                )
                .collect();
            let files: Vec<String> = tree
                .iter()
                .filter(|(_, n)| matches!(n, Node::File(_)))
                .map(|(k, _)| k.clone())
                .collect();
            let leaves: Vec<String> = tree
                .iter()
                .filter(|(_, n)| !matches!(n, Node::Dir))
                .map(|(k, _)| k.clone())
                .collect();
            made += 1;
            let dir = dirs[rng.below(dirs.len())].clone();
            let fresh = if dir == "/" {
                format!("/n{made}")
            } else {
                format!("{dir}/n{made}")
            };
            let op = match rng.below(10) {
                0 | 1 => {
                    let length = [0, 17, 4000, 131_072, 200_000][rng.below(5)];
                    Op::Write(fresh, random_bytes(seed * 1000 + made, length))
                }
                2 if !files.is_empty() => {
                    // A rewrite close to the old bytes (a delta), or new text.
                    let path = files[rng.below(files.len())].clone();
                    let Some(Node::File(old)) = tree.get(&path) else {
                        unreachable!()
                    };
                    let mut bytes = old.clone();
                    if bytes.len() > 64 {
                        let at = rng.below(bytes.len() - 32);
                        bytes[at..at + 32].copy_from_slice(&text_bytes(made, 32));
                    } else {
                        bytes = text_bytes(made, 300 + rng.below(70_000));
                    }
                    Op::Write(path, bytes)
                }
                3 if !files.is_empty() => {
                    let path = files[rng.below(files.len())].clone();
                    Op::Append(path, text_bytes(made, 1 + rng.below(9000)))
                }
                4 if !leaves.is_empty() => {
                    let from = leaves[rng.below(leaves.len())].clone();
                    if tree.contains_key(&fresh) || parent(&fresh) == from {
                        continue;
                    }
                    Op::Rename(from, fresh)
                }
                5 if !leaves.is_empty() => Op::Unlink(leaves[rng.below(leaves.len())].clone()),
                6 => Op::Mkdir(fresh),
                7 => {
                    let empty: Vec<&String> = dirs
                        .iter()
                        .filter(|d| d.as_str() != "/" && children(&tree, d) == 0)
                        .collect();
                    if empty.is_empty() {
                        continue;
                    }
                    Op::Rmdir(empty[rng.below(empty.len())].clone())
                }
                8 => Op::Symlink(fresh, format!("target-{made}").into_bytes()),
                _ => Op::Write(fresh, format!("small {made}\n").into_bytes()),
            };
            model_apply(&mut tree, &op);
            batch.push(op);
        }
        batches.push(batch);
    }
    batches
}

/// What one Store recorded: per landed Commit (oldest first) its root and
/// logical size; the final tree; the fault log.
struct Recorded {
    commits: Vec<([u8; 32], u64)>,
    tree: Tree,
    log: Vec<String>,
}

fn run_on(store: &TestStore, seed: u64, batches: &[Vec<Op>]) -> Recorded {
    let mut rng = Rng(seed ^ 0xfeed_beef);
    let branch = store.fork(&format!("s{seed}"));
    let service = store.open();
    let workspace = service.bind(branch);
    let mut model = adversary::seed_tree(SEED);
    let mut log = Vec::new();
    let targets: Vec<_> = verify768::TARGETS
        .iter()
        .filter(|t| t.table != "store_policy")
        .copied()
        .collect();
    for (n, batch) in batches.iter().enumerate() {
        for op in batch {
            apply(&workspace, op).unwrap_or_else(|e| panic!("seed {seed} {op:?}: {e}"));
            model_apply(&mut model, op);
        }
        assert_eq!(
            tree_of(&workspace),
            model,
            "{:?} seed {seed}: view after batch {n}",
            store.kind
        );
        let armed = (store.kind == Kind::Postgres && rng.below(2) == 0).then(|| {
            let target = targets[rng.below(targets.len())];
            let code = ["40001", "P0001", "22012"][rng.below(3)];
            verify768::arm(store, target, code);
            (target, code)
        });
        let first = verdict(&commit(&workspace));
        if let Some((target, code)) = armed {
            let fired = verify768::fired(store);
            verify768::disarm(store, target);
            log.push(format!(
                "{n}: {}:{code} fired={fired} -> {first:?}",
                target.table
            ));
        }
        match first {
            Verdict::Committed(_) | Verdict::UpToDate => {}
            Verdict::Refused(_) => match verdict(&commit(&workspace)) {
                Verdict::Committed(_) | Verdict::UpToDate => {}
                other => panic!(
                    "{:?} seed {seed} batch {n}: the retry answered {other:?}",
                    store.kind
                ),
            },
            other => panic!("{:?} seed {seed} batch {n}: {other:?}", store.kind),
        }
    }
    service.unmount(workspace);
    service.stop();
    let chain = store.chain(branch);
    assert_linear(&chain);
    let commits = chain
        .iter()
        .rev()
        .map(|r| (*r.root.as_bytes(), r.meta.statistics.logical))
        .collect();
    let tree = store.tree(branch);
    assert_eq!(
        tree, model,
        "{:?} seed {seed}: the Branch is the model",
        store.kind
    );
    Recorded { commits, tree, log }
}

#[test]
fn faulted_postgresql_matches_sqlite_commit_by_commit() {
    let stores = TestStore::both("v768-diff", SEED);
    for seed in SEEDS {
        let batches = workload(*seed);
        let recorded: Vec<Recorded> = stores.iter().map(|s| run_on(s, *seed, &batches)).collect();
        for line in recorded.iter().flat_map(|r| &r.log) {
            println!("VERIFY_DIFF seed {seed} {line}");
        }
        if let [sqlite, pg] = &recorded[..] {
            assert_eq!(sqlite.tree, pg.tree, "seed {seed}: trees");
            assert_eq!(
                sqlite.commits.len(),
                pg.commits.len(),
                "seed {seed}: Commit counts"
            );
            for (n, (a, b)) in sqlite.commits.iter().zip(&pg.commits).enumerate() {
                assert_eq!(a, b, "seed {seed}: Commit {n} root/logical differ");
            }
        }
    }
    for store in &stores {
        let failures = store.integrity_failures();
        assert!(failures.is_empty(), "{:?} audit: {failures:#?}", store.kind);
        if store.kind == Kind::Postgres {
            let failures = verify768::amcheck(store);
            assert!(failures.is_empty(), "amcheck: {failures:#?}");
            let failures = verify768::duplicate_allocations(store);
            assert!(failures.is_empty(), "allocation: {failures:#?}");
        }
    }
}
