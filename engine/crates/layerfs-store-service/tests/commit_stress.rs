//! #768: multi-session Commit stress with bounded counts, on SQLite and
//! PostgreSQL.
//!
//! Several Workspaces on two Branches, each on its own thread (on two Store
//! services on PostgreSQL), run a fixed number of short change-and-Commit
//! iterations without any pacing: create their own files, overwrite and
//! delete earlier ones, append to their own log. A Commit lands, answers
//! UpToDate, or returns a documented refusal (`LaneMerge`, `LiveConflict`,
//! `Busy`); an unknown outcome fails the test. After the threads end each
//! Workspace Commits once more, sequentially, so everything it holds is
//! published. Then, from fresh sessions: each Branch holds exactly the union
//! of its Workspaces' final files, its first-parent chain is exactly the
//! Commits answered `Committed` on it, each once, no object is located
//! twice and the Store's audit is clean. The PostgreSQL half needs
//! `LAYERFS_TEST_POSTGRES_URL` and reports NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{
    apply, assert_linear, commit, random_bytes, seed_tree, verdict, Gate, Kind, Node, Op, Service,
    TestStore, Tree, Verdict,
};
use layerfs_history::{BranchId, CommitId};
use layerfs_store_service::store::BoundWorkspace;
use std::collections::BTreeMap;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];
const ITERATIONS: usize = 10;

fn documented(text: &str) -> bool {
    ["LaneMerge", "LiveConflict", "Busy"]
        .iter()
        .any(|code| text.contains(code))
}

/// One Workspace's run: its changes and their Commits; its own files at the
/// end and the Commits it landed.
fn churn(index: usize, workspace: &BoundWorkspace, gate: &Gate) -> (Tree, Vec<CommitId>, usize) {
    let mut own = Tree::new();
    let mut landed = Vec::new();
    let mut refused = 0;
    let log = format!("/log{index}");
    assert!(gate.wait(), "every Workspace started");
    for k in 0..ITERATIONS {
        let mut ops = vec![Op::Write(
            format!("/w{index}-{k}"),
            random_bytes((index * 100 + k) as u64, 200 + 37 * k),
        )];
        if k == 0 {
            ops.push(Op::Write(log.clone(), Vec::new()));
        }
        ops.push(Op::Append(
            log.clone(),
            format!("{index}:{k}\n").into_bytes(),
        ));
        if k >= 2 && k % 3 == 2 {
            ops.push(Op::Unlink(format!("/w{index}-{}", k - 2)));
        }
        if k >= 1 && k % 4 == 1 {
            ops.push(Op::Write(
                format!("/w{index}-{}", k - 1),
                b"rewritten\n".to_vec(),
            ));
        }
        for op in &ops {
            apply(workspace, op).unwrap_or_else(|e| panic!("workspace {index} {op:?}: {e}"));
            match op {
                Op::Write(path, bytes) => {
                    own.insert(path.clone(), Node::File(bytes.clone()));
                }
                Op::Append(path, bytes) => {
                    if let Some(Node::File(old)) = own.get_mut(path) {
                        old.extend_from_slice(bytes);
                    }
                }
                Op::Unlink(path) => {
                    own.remove(path);
                }
                _ => unreachable!(),
            }
        }
        match verdict(&commit(workspace)) {
            Verdict::Committed(id) => landed.push(id),
            Verdict::UpToDate => {}
            Verdict::Refused(text) if documented(&text) => refused += 1,
            other => panic!("workspace {index} iteration {k}: {other:?}"),
        }
    }
    (own, landed, refused)
}

fn stress(store: &TestStore) {
    let services: Vec<Service> = match store.kind {
        Kind::Postgres => vec![store.open(), store.open()],
        Kind::Sqlite => vec![store.open()],
    };
    let per_branch = if store.kind == Kind::Postgres { 3 } else { 2 };
    let branches: Vec<BranchId> = (0..2).map(|n| store.fork(&format!("s{n}"))).collect();
    let bound: Vec<(usize, BranchId, BoundWorkspace)> = (0..2 * per_branch)
        .map(|n| {
            let at = n % services.len();
            let branch = branches[n % 2];
            (at, branch, services[at].bind(branch))
        })
        .collect();
    let gate = Gate::new(bound.len());
    let results: Vec<(Tree, Vec<CommitId>, usize)> = std::thread::scope(|scope| {
        let handles: Vec<_> = bound
            .iter()
            .enumerate()
            .map(|(n, (_, _, workspace))| {
                let gate = &gate;
                scope.spawn(move || churn(n, workspace, gate))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut landed: BTreeMap<BranchId, Vec<CommitId>> = BTreeMap::new();
    let mut expected: BTreeMap<BranchId, Tree> =
        branches.iter().map(|b| (*b, seed_tree(SEED))).collect();
    let mut refused = 0;
    for ((_, branch, workspace), (own, ids, refusals)) in bound.iter().zip(results) {
        refused += refusals;
        let list = landed.entry(*branch).or_default();
        list.extend(ids);
        match verdict(&commit(workspace)) {
            Verdict::Committed(id) => list.push(id),
            Verdict::UpToDate => {}
            other => panic!("{:?} flush: {other:?}", store.kind),
        }
        expected.get_mut(branch).unwrap().extend(own);
    }
    println!(
        "STRESS {{\"backend\":\"{:?}\",\"workspaces\":{},\"landed\":{},\"refused\":{refused}}}",
        store.kind,
        bound.len(),
        landed.values().map(Vec::len).sum::<usize>()
    );
    for (at, _, workspace) in bound {
        services[at].unmount(workspace);
    }
    for service in services {
        service.stop();
    }
    let mut rows = 0;
    for branch in &branches {
        let chain = store.chain(*branch);
        rows += chain.len() as i64;
        assert_linear(&chain);
        let mut ids: Vec<_> = chain.iter().map(|r| r.id).collect();
        ids.sort();
        let mut mine = landed.remove(branch).unwrap_or_default();
        mine.sort();
        assert_eq!(
            ids, mine,
            "{:?}: the chain is exactly the landed Commits",
            store.kind
        );
        assert_eq!(&store.tree(*branch), &expected[branch], "{:?}", store.kind);
    }
    assert_eq!(store.commit_rows(), rows, "no Commit row off the chains");
    assert_eq!(store.duplicate_locators(), 0);
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

#[test]
fn several_sessions_commit_in_short_loops() {
    for store in TestStore::both("stress", SEED) {
        stress(&store);
    }
}
