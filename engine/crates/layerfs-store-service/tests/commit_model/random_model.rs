//! #768: seeded random Workspace workloads, Committed on SQLite and on
//! PostgreSQL and checked against an in-memory model.
//!
//! A fixed list of seeds drives a small generator of create, overwrite,
//! append, rename (files, symlinks and directories, replacing a file),
//! delete, mkdir, rmdir and symlink changes, with a Commit every few
//! changes. The same sequence runs on one Workspace per backend, at one
//! fixed mutation time. After every Commit the Workspace's view equals the
//! model; at the end the Branch read from a fresh session equals it, the
//! two backends recorded the same number of Commits with the same roots
//! (content addressing: equal trees, equal roots) and the same per-Commit
//! logical size (`branch.list_commits`), and each Store's audit is clean.
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and reports
//! NOT_RUN without it (the SQLite half still runs against the model).
use crate::adversary;

use adversary::{
    apply, assert_linear, commit, random_bytes, seed_tree, tree_of, verdict, Node, Op, Service,
    TestStore, Tree, Verdict,
};
use layerfs_protocol::sdk::SdkValue;
use layerfs_store_service::store::BoundWorkspace;

const SEEDS: [u64; 8] = [1, 7, 42, 99, 768, 1024, 4096, 65537];
const OPS: usize = 64;
const SEED_FILES: &[(&str, &[u8])] = &[("/seed", b"seed\n"), ("/d0/inner", b"inner\n")];

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
    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        (!items.is_empty()).then(|| &items[self.below(items.len())])
    }
}

/// One step of a workload: a change, or a Commit.
#[derive(Clone, Debug)]
enum Step {
    Change(Op),
    Commit,
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').unwrap().0
}

fn child(dir: &str, name: &str) -> String {
    format!("{dir}/{name}")
}

/// Applies `op` to the model; the generator only makes valid ones.
fn model_apply(tree: &mut Tree, op: &Op) {
    match op {
        Op::Write(path, bytes) => {
            tree.insert(path.clone(), Node::File(bytes.clone()));
        }
        Op::Append(path, bytes) => match tree.get_mut(path) {
            Some(Node::File(old)) => old.extend_from_slice(bytes),
            other => panic!("model append {path}: {other:?}"),
        },
        Op::Mkdir(path) => {
            tree.insert(path.clone(), Node::Dir);
        }
        Op::Symlink(path, target) => {
            tree.insert(path.clone(), Node::Link(target.clone()));
        }
        Op::Rename(from, to) => {
            let moved: Vec<(String, Node)> = tree
                .iter()
                .filter(|(p, _)| *p == from || p.starts_with(&format!("{from}/")))
                .map(|(p, n)| (p.clone(), n.clone()))
                .collect();
            for (p, _) in &moved {
                tree.remove(p);
            }
            for (p, n) in moved {
                tree.insert(format!("{to}{}", &p[from.len()..]), n);
            }
        }
        Op::Unlink(path) | Op::Rmdir(path) => {
            tree.remove(path);
        }
    }
}

fn names(tree: &Tree, dir: &str) -> Vec<String> {
    tree.keys()
        .filter(|p| parent(p) == dir)
        .map(|p| p.rsplit_once('/').unwrap().1.to_owned())
        .collect()
}

/// The seeded workload: `OPS` changes with a Commit every 3 to 6 of them,
/// and a last Commit.
fn workload(seed: u64) -> Vec<Step> {
    let mut rng = Rng(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    let mut model = seed_tree(SEED_FILES);
    let mut steps = Vec::new();
    let mut until_commit = 3 + rng.below(4);
    let pool = ["a", "b", "c", "d", "e", "f", "g"];
    let dir_pool = ["d0", "d1", "d2", "d3"];
    while steps
        .iter()
        .filter(|s| matches!(s, Step::Change(_)))
        .count()
        < OPS
    {
        let dirs: Vec<String> = std::iter::once(String::new())
            .chain(
                model
                    .iter()
                    .filter(|(_, n)| **n == Node::Dir)
                    .map(|(p, _)| p.clone()),
            )
            .collect();
        let files: Vec<String> = model
            .iter()
            .filter(|(_, n)| matches!(n, Node::File(_)))
            .map(|(p, _)| p.clone())
            .collect();
        let leaves: Vec<String> = model
            .iter()
            .filter(|(_, n)| !matches!(n, Node::Dir))
            .map(|(p, _)| p.clone())
            .collect();
        let dir = rng.pick(&dirs).unwrap().clone();
        let taken = names(&model, &dir);
        let free: Vec<&str> = pool
            .iter()
            .chain(dir_pool.iter())
            .copied()
            .filter(|n| !taken.iter().any(|t| t == n))
            .collect();
        let size = if rng.below(10) == 0 {
            70_000 + rng.below(40_000)
        } else {
            rng.below(3_000)
        };
        let bytes = random_bytes(rng.next(), size);
        let op = match rng.below(10) {
            0 | 1 => rng
                .pick(&free)
                .map(|name| Op::Write(child(&dir, name), bytes)),
            2 => rng.pick(&files).map(|path| Op::Write(path.clone(), bytes)),
            3 => rng
                .pick(&files)
                .map(|path| Op::Append(path.clone(), bytes[..bytes.len().min(500)].to_vec())),
            4 => rng.pick(&free).map(|name| Op::Mkdir(child(&dir, name))),
            5 => rng.pick(&free).map(|name| {
                Op::Symlink(
                    child(&dir, name),
                    format!("../t{}", rng.below(9)).into_bytes(),
                )
            }),
            6 => {
                // A file or symlink to a free name, or over a file.
                let from = rng.pick(&leaves).cloned();
                let to = if rng.below(3) == 0 {
                    rng.pick(&files).cloned()
                } else {
                    rng.pick(&free).map(|name| child(&dir, name))
                };
                match (from, to) {
                    (Some(from), Some(to)) if from != to => Some(Op::Rename(from, to)),
                    _ => None,
                }
            }
            7 => {
                // A directory to a free name in its own parent.
                let moving: Vec<&String> = dirs.iter().filter(|d| !d.is_empty()).collect();
                rng.pick(&moving).and_then(|from| {
                    let at = parent(from).to_owned();
                    let taken = names(&model, &at);
                    let free: Vec<&str> = dir_pool
                        .iter()
                        .chain(pool.iter())
                        .copied()
                        .filter(|n| !taken.iter().any(|t| t == n))
                        .collect();
                    rng.pick(&free)
                        .map(|name| Op::Rename((*from).clone(), child(&at, name)))
                })
            }
            8 => rng.pick(&leaves).map(|path| Op::Unlink(path.clone())),
            _ => {
                let empty: Vec<&String> = dirs
                    .iter()
                    .filter(|d| !d.is_empty() && names(&model, d).is_empty())
                    .collect();
                rng.pick(&empty).map(|path| Op::Rmdir((*path).clone()))
            }
        };
        let Some(op) = op else { continue };
        model_apply(&mut model, &op);
        steps.push(Step::Change(op));
        until_commit -= 1;
        if until_commit == 0 {
            steps.push(Step::Commit);
            until_commit = 3 + rng.below(4);
        }
    }
    steps.push(Step::Commit);
    steps
}

/// What one backend recorded for one workload.
struct Recorded {
    roots: Vec<String>,
    logical: Vec<String>,
}

fn logical_sizes(service: &Service, branch: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut position = 0i64;
    loop {
        let page = service
            .call(
                "branch.list_commits",
                vec![
                    ("branch", SdkValue::text(branch)),
                    ("pageSize", SdkValue::Int(100)),
                    ("pagePos", SdkValue::Int(position)),
                ],
            )
            .unwrap();
        let Some(SdkValue::List(items)) = page.get("items") else {
            panic!("list_commits answers items: {page:?}")
        };
        for item in items {
            out.push(format!("{:?}", item.get("logical")));
        }
        position += items.len() as i64;
        if !page.get_bool("hasMore") || items.is_empty() {
            return out;
        }
    }
}

fn run_on(store: &TestStore, seed: u64, steps: &[Step]) -> Recorded {
    let name = format!("s{seed}");
    let branch = store.fork(&name);
    let service = store.open();
    let workspace: BoundWorkspace = service.bind(branch);
    let mut model = seed_tree(SEED_FILES);
    let mut committed_model = model.clone();
    for (n, step) in steps.iter().enumerate() {
        match step {
            Step::Change(op) => {
                apply(&workspace, op).unwrap_or_else(|e| {
                    panic!("{:?} seed {seed} step {n} {op:?}: {e}", store.kind)
                });
                model_apply(&mut model, op);
            }
            Step::Commit => {
                match verdict(&commit(&workspace)) {
                    Verdict::Committed(_) => {}
                    Verdict::UpToDate => assert_eq!(
                        model, committed_model,
                        "{:?} seed {seed} step {n}: UpToDate with changes",
                        store.kind
                    ),
                    other => panic!("{:?} seed {seed} step {n}: {other:?}", store.kind),
                }
                committed_model = model.clone();
                assert_eq!(
                    tree_of(&workspace),
                    model,
                    "{:?} seed {seed} step {n}: the view after the Commit",
                    store.kind
                );
            }
        }
    }
    let logical = logical_sizes(&service, &name);
    service.unmount(workspace);
    service.stop();
    let chain = store.chain(branch);
    assert_linear(&chain);
    assert_eq!(
        store.tree(branch),
        model,
        "{:?} seed {seed}: the Branch",
        store.kind
    );
    Recorded {
        roots: chain.iter().map(|r| format!("{:?}", r.root)).collect(),
        logical,
    }
}

#[test]
fn seeded_workloads_match_the_model_on_both_backends() {
    let stores = TestStore::both("random-model", SEED_FILES);
    for seed in SEEDS {
        let steps = workload(seed);
        let commits = steps.iter().filter(|s| matches!(s, Step::Commit)).count();
        let recorded: Vec<Recorded> = stores.iter().map(|s| run_on(s, seed, &steps)).collect();
        println!(
            "RANDOM_MODEL {{\"seed\":{seed},\"steps\":{},\"commit_calls\":{commits},\"commits\":{:?}}}",
            steps.len(),
            recorded.iter().map(|r| r.roots.len()).collect::<Vec<_>>()
        );
        if let [sqlite, postgres] = &recorded[..] {
            let differing: Vec<usize> = (0..sqlite.roots.len().max(postgres.roots.len()))
                .filter(|n| sqlite.roots.get(*n) != postgres.roots.get(*n))
                .collect();
            assert!(
                differing.is_empty(),
                "seed {seed}: roots differ at Commits {differing:?} (newest first) of {}",
                sqlite.roots.len()
            );
            assert_eq!(
                sqlite.logical, postgres.logical,
                "seed {seed}: equal logical sizes"
            );
        }
    }
    for store in &stores {
        let rows: usize = SEEDS
            .iter()
            .map(|seed| store.chain(store.branch(&format!("s{seed}"))).len())
            .sum();
        assert_eq!(
            store.commit_rows(),
            rows as i64,
            "no Commit row off the chains"
        );
        assert_eq!(store.duplicate_locators(), 0);
        let failures = store.integrity_failures();
        assert!(failures.is_empty(), "{:?} audit: {failures:#?}", store.kind);
    }
}
