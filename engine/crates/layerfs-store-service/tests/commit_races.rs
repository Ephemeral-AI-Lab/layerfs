//! #768: Workspace Commits racing each other on SQLite and PostgreSQL.
//!
//! * Several Workspaces bound on one Branch (on two Store services where
//!   the backend allows it) each write their own file and Commit at the same
//!   moment, round after round. Every Commit lands (merging with whatever
//!   landed first) or returns a documented refusal (`LaneMerge`,
//!   `LiveConflict`, `Busy`); an unknown outcome is a failure here. After a
//!   final sequential flush the Branch holds every file, its first-parent
//!   chain is exactly the landed Commits, each once.
//! * The same path written with different bytes by every Workspace: one
//!   Commit lands, the rest answer a conflicted merge (nothing published);
//!   the head's bytes are its own writer's.
//! * Identical new content published by several sessions at once: three
//!   Workspaces on their own Branches, two Workspaces on one Branch writing
//!   the same path with the same bytes, and a direct `fs.write` (a Save of
//!   the same bytes) on another service. Every one succeeds, every tree reads
//!   the bytes back, `object_location` has no object twice (read from the
//!   private test database), and the Store's audit is clean.
//!
//! The PostgreSQL halves need `LAYERFS_TEST_POSTGRES_URL` and report
//! NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{
    assert_linear, commit, seed_tree, text_bytes, verdict, write, Gate, Kind, Node, Service,
    TestStore, Verdict,
};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_history::CommitId;
use layerfs_store_service::store::BoundWorkspace;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n"), ("/dir/inner", b"inner\n")];
const ROUNDS: usize = 5;

/// The Store services a test spreads its Workspaces over: two on
/// PostgreSQL (two daemons on one Store); one on SQLite, whose writer
/// sessions in one process share a single lane.
fn services(store: &TestStore) -> Vec<Service> {
    match store.kind {
        Kind::Postgres => vec![store.open(), store.open()],
        Kind::Sqlite => vec![store.open()],
    }
}

fn documented(text: &str) -> bool {
    ["LaneMerge", "LiveConflict", "Busy"]
        .iter()
        .any(|code| text.contains(code))
}

/// `count` Workspaces on `branch`, spread round-robin over `services`.
fn bind_all(
    services: &[Service],
    branch: layerfs_history::BranchId,
    count: usize,
) -> Vec<(usize, BoundWorkspace)> {
    (0..count)
        .map(|n| {
            let at = n % services.len();
            (at, services[at].bind(branch))
        })
        .collect()
}

fn finish(bound: Vec<(usize, BoundWorkspace)>, services: Vec<Service>) {
    for (at, workspace) in bound {
        services[at].unmount(workspace);
    }
    for service in services {
        service.stop();
    }
}

/// Runs `step(index, workspace)` on every Workspace at once (each on its
/// own thread, released together by a bounded gate) and returns the
/// answers in Workspace order.
fn at_once(
    bound: &[(usize, BoundWorkspace)],
    step: impl Fn(usize, &BoundWorkspace) -> Verdict + Sync,
) -> Vec<Verdict> {
    let gate = Gate::new(bound.len());
    std::thread::scope(|scope| {
        let handles: Vec<_> = bound
            .iter()
            .enumerate()
            .map(|(n, (_, workspace))| {
                let (gate, step) = (&gate, &step);
                scope.spawn(move || {
                    assert!(gate.wait(), "every Workspace reached the gate");
                    step(n, workspace)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

fn distinct_files(store: &TestStore) {
    let services = services(store);
    let branch = store.fork("race");
    let bound = bind_all(&services, branch, 4);
    let mut expected = seed_tree(SEED);
    let mut landed: Vec<CommitId> = Vec::new();
    let mut refused = Vec::new();
    for round in 0..ROUNDS {
        for (n, (_, workspace)) in bound.iter().enumerate() {
            let path = format!("/w{n}-r{round}");
            let bytes = format!("workspace {n} round {round}\n").into_bytes();
            write(workspace, &path, &bytes);
            expected.insert(path, Node::File(bytes));
        }
        for answer in at_once(&bound, |_, workspace| verdict(&commit(workspace))) {
            match answer {
                Verdict::Committed(id) => landed.push(id),
                Verdict::Refused(text) if documented(&text) => refused.push(text),
                other => panic!("{:?} round {round}: {other:?}", store.kind),
            }
        }
    }
    // Refused changes stay in their Workspace; one sequential Commit each
    // publishes them.
    for (_, workspace) in &bound {
        match verdict(&commit(workspace)) {
            Verdict::Committed(id) => landed.push(id),
            Verdict::UpToDate => {}
            other => panic!("{:?} flush: {other:?}", store.kind),
        }
    }
    println!(
        "RACE_DISTINCT {{\"backend\":\"{:?}\",\"landed\":{},\"refused\":{}}}",
        store.kind,
        landed.len(),
        refused.len()
    );
    let chain = store.chain(branch);
    assert_linear(&chain);
    let mut ids: Vec<_> = chain.iter().map(|r| r.id).collect();
    ids.sort();
    landed.sort();
    assert_eq!(ids, landed, "the chain is exactly the landed Commits");
    assert_eq!(
        store.commit_rows(),
        chain.len() as i64,
        "no Commit row off the chain"
    );
    assert_eq!(store.tree(branch), expected, "every landed change is there");
    finish(bound, services);
    assert_eq!(store.duplicate_locators(), 0);
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

fn one_path(store: &TestStore) {
    let services = services(store);
    let branch = store.fork("same");
    let bound = bind_all(&services, branch, 4);
    let content = |n: usize| format!("written by {n}\n").into_bytes();
    for (n, (_, workspace)) in bound.iter().enumerate() {
        write(workspace, "/shared", &content(n));
    }
    let answers = at_once(&bound, |_, workspace| verdict(&commit(workspace)));
    let mut landed = Vec::new();
    for (n, answer) in answers.iter().enumerate() {
        match answer {
            Verdict::Committed(id) => landed.push((*id, n)),
            Verdict::Conflicted => {}
            Verdict::Refused(text) if documented(text) => {}
            other => panic!("{:?}: {other:?}", store.kind),
        }
    }
    println!(
        "RACE_ONE_PATH {{\"backend\":\"{:?}\",\"answers\":\"{answers:?}\"}}",
        store.kind
    );
    assert!(!landed.is_empty(), "one Commit lands");
    let chain = store.chain(branch);
    assert_linear(&chain);
    assert_eq!(
        chain.len(),
        landed.len(),
        "only landed Commits are recorded"
    );
    let (_, writer) = landed
        .iter()
        .find(|(id, _)| *id == chain[0].id)
        .expect("the head is a landed Commit");
    assert_eq!(
        store.tree(branch).get("/shared"),
        Some(&Node::File(content(*writer))),
        "the head's bytes are its writer's"
    );
    finish(bound, services);
}

fn identical_objects(store: &TestStore) {
    let services = services(store);
    let big = text_bytes(768, 300 * 1024);
    let small = b"the same small file\n".to_vec();
    let own: Vec<_> = (0..3).map(|n| store.fork(&format!("own{n}"))).collect();
    let twin = store.fork("twin");
    let direct = store.fork("direct");
    let mut bound: Vec<(usize, BoundWorkspace)> = own
        .iter()
        .enumerate()
        .map(|(n, branch)| {
            let at = n % services.len();
            (at, services[at].bind(*branch))
        })
        .collect();
    for n in 0..2 {
        let at = n % services.len();
        bound.push((at, services[at].bind(twin)));
    }
    for (_, workspace) in &bound {
        write(workspace, "/same-big", &big);
        write(workspace, "/same-small", &small);
    }
    // One more party: a direct Save of the same bytes through `fs.write` on
    // the last service, in the same moment.
    let writer = services.last().unwrap();
    let gate = Gate::new(bound.len() + 1);
    let (answers, direct_answer) = std::thread::scope(|scope| {
        let handles: Vec<_> = bound
            .iter()
            .map(|(_, workspace)| {
                let gate = &gate;
                scope.spawn(move || {
                    assert!(gate.wait(), "every party reached the gate");
                    verdict(&commit(workspace))
                })
            })
            .collect();
        let (gate, big) = (&gate, &big);
        let direct_handle = scope.spawn(move || {
            assert!(gate.wait(), "every party reached the gate");
            writer.call(
                "fs.write",
                vec![
                    ("branch", SdkValue::text("direct")),
                    ("path", SdkValue::text("/same-big")),
                    ("content", SdkValue::text(std::str::from_utf8(big).unwrap())),
                ],
            )
        });
        let answers: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        (answers, direct_handle.join().unwrap())
    });
    println!(
        "RACE_IDENTICAL {{\"backend\":\"{:?}\",\"answers\":\"{answers:?}\",\"direct\":\"{:?}\"}}",
        store.kind,
        direct_answer.as_ref().map(|_| "ok")
    );
    direct_answer.expect("the direct Save of the same bytes succeeds");
    for (n, answer) in answers.iter().enumerate() {
        let twin_side = n >= own.len();
        assert!(
            matches!(answer, Verdict::Committed(_))
                || (twin_side && matches!(answer, Verdict::UpToDate)),
            "{:?} party {n}: {answer:?}",
            store.kind
        );
    }
    let mut expected = seed_tree(SEED);
    expected.insert("/same-big".into(), Node::File(big.clone()));
    expected.insert("/same-small".into(), Node::File(small.clone()));
    for branch in own.iter().chain([&twin]) {
        assert_eq!(&store.tree(*branch), &expected);
        assert_linear(&store.chain(*branch));
    }
    let direct_tree = store.tree(direct);
    assert_eq!(direct_tree.get("/same-big"), Some(&Node::File(big.clone())));
    finish(bound, services);
    assert_eq!(store.duplicate_locators(), 0, "no object located twice");
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

#[test]
fn concurrent_commits_of_distinct_files_on_one_head() {
    for store in TestStore::both("race-distinct", SEED) {
        distinct_files(&store);
    }
}

#[test]
fn concurrent_commits_of_one_path_land_once() {
    for store in TestStore::both("race-one-path", SEED) {
        one_path(&store);
    }
}

#[test]
fn identical_new_objects_published_by_several_sessions() {
    for store in TestStore::both("race-identical", SEED) {
        identical_objects(&store);
    }
}
