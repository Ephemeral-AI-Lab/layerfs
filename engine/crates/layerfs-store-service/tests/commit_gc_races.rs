//! #768: Workspace Commits concurrent with the GC calls that change what a
//! Commit may publish, on SQLite and PostgreSQL:
//!
//! * `branch.remove` of other Branches of the project, which bumps the
//!   project epoch a Commit reads at its start and checks again before it
//!   commits: a Commit lands or is refused `Conflict` ("project epoch") with
//!   nothing published; every removal answered `Removed` is gone.
//! * `project.remove_file` while a Workspace is mounted: refused
//!   `MountsActive` (the marker is project-wide), so no mounted Workspace
//!   ever loses a file under it; once unmounted the marker lands, later
//!   Commits do not resurrect the path, and a re-creation after it shows.
//! * Store compaction (with a removed Branch's large file to collect) while
//!   a Workspace is mounted and committing: refused `MountsActive` while the
//!   mount lives. After the unmount it rebuilds (SQLite only once no other
//!   session holds the file: with the old service open it refuses, "sidecar
//!   remains"), and the Commits that follow (on the old service: published
//!   or a documented `Stale`/`Busy` refusal; on a new one: published) lose
//!   nothing.
//!
//! Every case ends with the Branch read from fresh sessions: linear, each
//! landed Commit once, the tree holding exactly the landed changes. The
//! PostgreSQL halves need `LAYERFS_TEST_POSTGRES_URL` and report NOT_RUN
//! without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{
    assert_linear, commit, seed_tree, verdict, write, Gate, Kind, Node, Op, Service, TestStore,
    Verdict, CURSOR,
};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_history::{CommitId, HistoryError};
use layerfs_persistence::{CompactRequest, Handles, SqliteHandles, StoreSelection};

const VICTIMS: usize = 6;
const LOOPS: usize = 8;

fn seed() -> Vec<(String, Vec<u8>)> {
    let mut seed = vec![("/seed".to_owned(), b"seed\n".to_vec())];
    for n in 0..VICTIMS {
        seed.push((format!("/victim{n}"), format!("victim {n}\n").into_bytes()));
    }
    seed
}

fn store_pair(label: &str) -> Vec<TestStore> {
    let seed = seed();
    let seed: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    TestStore::both(label, &seed)
}

fn expected_seed() -> adversary::Tree {
    let seed = seed();
    let seed: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    seed_tree(&seed)
}

/// A second service for the GC caller on PostgreSQL (another daemon); the
/// same one on SQLite.
fn gc_service(store: &TestStore) -> Option<Service> {
    (store.kind == Kind::Postgres).then(|| store.open())
}

fn refusal_code(answer: &Result<SdkValue, layerfs_protocol::provision::install::SdkRefusal>) -> String {
    match answer {
        Ok(_) => "Ok".into(),
        Err(refusal) => refusal.code.clone(),
    }
}

fn epoch_refusal(text: &str) -> bool {
    text.contains("project epoch") || text.contains("Conflict")
}

fn branch_removals(store: &TestStore) {
    let committer = store.open();
    let gc = gc_service(store);
    let remover = gc.as_ref().unwrap_or(&committer);
    let keep = store.fork("keep");
    let gone: Vec<_> = (0..LOOPS).map(|n| format!("gone{n}")).collect();
    for name in &gone {
        store.fork(name);
    }
    let workspace = committer.bind(keep);
    let mut expected = expected_seed();
    let gate = Gate::new(2);
    let (commits, removals) = std::thread::scope(|scope| {
        let (gate, workspace) = (&gate, &workspace);
        let committing = scope.spawn(move || {
            assert!(gate.wait());
            (0..LOOPS)
                .map(|n| {
                    write(workspace, &format!("/k{n}"), format!("k {n}\n").as_bytes());
                    verdict(&commit(workspace))
                })
                .collect::<Vec<_>>()
        });
        let gone = &gone;
        let removing = scope.spawn(move || {
            assert!(gate.wait());
            gone.iter()
                .map(|name| remover.call("branch.remove", vec![("branch", SdkValue::text(name))]))
                .collect::<Vec<_>>()
        });
        (committing.join().unwrap(), removing.join().unwrap())
    });
    for n in 0..LOOPS {
        expected.insert(
            format!("/k{n}"),
            Node::File(format!("k {n}\n").into_bytes()),
        );
    }
    let mut landed: Vec<CommitId> = Vec::new();
    let mut conflicts = 0;
    for answer in &commits {
        match answer {
            Verdict::Committed(id) => landed.push(*id),
            Verdict::Refused(text) if epoch_refusal(text) => conflicts += 1,
            other => panic!("{:?}: Commit {other:?}", store.kind),
        }
    }
    match verdict(&commit(&workspace)) {
        Verdict::Committed(id) => landed.push(id),
        Verdict::UpToDate => {}
        other => panic!("{:?}: flush {other:?}", store.kind),
    }
    let codes: Vec<String> = removals.iter().map(refusal_code).collect();
    println!(
        "GC_BRANCH_REMOVAL {{\"backend\":\"{:?}\",\"landed\":{},\"epoch_conflicts\":{conflicts},\"removals\":\"{codes:?}\"}}",
        store.kind,
        landed.len()
    );
    for code in &codes {
        assert!(
            ["Ok", "Busy"].contains(&code.as_str()),
            "{:?}: branch.remove answered {code}",
            store.kind
        );
    }
    committer.unmount(workspace);
    if let Some(gc) = gc {
        gc.stop();
    }
    committer.stop();
    let chain = store.chain(keep);
    assert_linear(&chain);
    let mut ids: Vec<_> = chain.iter().map(|r| r.id).collect();
    ids.sort();
    landed.sort();
    assert_eq!(ids, landed, "the chain is exactly the landed Commits");
    assert_eq!(store.tree(keep), expected);
    let handles = Handles::open_read_only(&store.selection, &store.binding, CURSOR).unwrap();
    for (name, code) in gone.iter().zip(&codes) {
        let present = adversary::history(&handles)
            .branch(store.branch(name))
            .unwrap()
            .is_some();
        assert_eq!(present, code != "Ok", "{name}: answered {code}");
    }
    handles.close();
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

fn path_removals(store: &TestStore) {
    let committer = store.open();
    let gc = gc_service(store);
    let keep = store.fork("keep");
    let workspace = committer.bind(keep);
    let mut expected = expected_seed();
    let gate = Gate::new(2);
    let (commits, removals) = {
        let remover = gc.as_ref().unwrap_or(&committer);
        std::thread::scope(|scope| {
            let (gate, workspace) = (&gate, &workspace);
            let committing = scope.spawn(move || {
                assert!(gate.wait());
                (0..LOOPS)
                    .map(|n| {
                        write(workspace, &format!("/p{n}"), format!("p {n}\n").as_bytes());
                        verdict(&commit(workspace))
                    })
                    .collect::<Vec<_>>()
            });
            let removing = scope.spawn(move || {
                assert!(gate.wait());
                (0..VICTIMS)
                    .map(|n| {
                        remover.call(
                            "project.remove_file",
                            vec![("path", SdkValue::text(format!("/victim{n}")))],
                        )
                    })
                    .collect::<Vec<_>>()
            });
            (committing.join().unwrap(), removing.join().unwrap())
        })
    };
    let codes: Vec<String> = removals.iter().map(refusal_code).collect();
    println!(
        "GC_PATH_REMOVAL {{\"backend\":\"{:?}\",\"commits\":\"{commits:?}\",\"removals\":\"{codes:?}\"}}",
        store.kind
    );
    let mut landed = Vec::new();
    for answer in &commits {
        match answer {
            Verdict::Committed(id) => landed.push(*id),
            other => panic!("{:?}: Commit {other:?}", store.kind),
        }
    }
    for code in &codes {
        assert!(
            ["MountsActive", "Busy"].contains(&code.as_str()),
            "{:?}: a removal under a live mount answered {code}",
            store.kind
        );
    }
    for n in 0..LOOPS {
        expected.insert(
            format!("/p{n}"),
            Node::File(format!("p {n}\n").into_bytes()),
        );
    }
    // The mounted Workspace lost nothing.
    assert_eq!(adversary::tree_of(&workspace), expected);
    committer.unmount(workspace);
    let chain = store.chain(keep);
    assert_linear(&chain);
    assert_eq!(chain.len(), landed.len());
    assert_eq!(store.tree(keep), expected);

    // No mount now: the marker lands and hides the path on every Branch.
    let remover = gc.as_ref().unwrap_or(&committer);
    remover
        .call(
            "project.remove_file",
            vec![("path", SdkValue::text("/victim0"))],
        )
        .unwrap_or_else(|refusal| {
            panic!(
                "{:?}: a removal with no live mount: {refusal:?}",
                store.kind
            )
        });
    expected.remove("/victim0");
    let workspace = committer.bind(keep);
    assert_eq!(adversary::tree_of(&workspace), expected, "hidden at mount");
    write(&workspace, "/after", b"after\n");
    expected.insert("/after".into(), Node::File(b"after\n".to_vec()));
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    assert_eq!(
        store.tree(keep),
        expected,
        "a later Commit does not resurrect the removed path"
    );
    // A re-creation after the marker is visible again.
    adversary::apply(
        &workspace,
        &Op::Write("/victim0".into(), b"again\n".to_vec()),
    )
    .unwrap();
    expected.insert("/victim0".into(), Node::File(b"again\n".to_vec()));
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    assert_eq!(store.tree(keep), expected, "the re-creation shows");
    committer.unmount(workspace);
    if let Some(gc) = gc {
        gc.stop();
    }
    committer.stop();
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

/// One compaction on a session of its own: "Ok" or the refusal's class.
fn compact(store: &TestStore, op: u8) -> String {
    let request = CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    };
    let result = match &store.selection {
        StoreSelection::Postgres(_) => {
            let Handles::Postgres(handles) = store.handles() else {
                unreachable!()
            };
            let result = handles.compact(&request);
            handles.close();
            result
        }
        StoreSelection::Sqlite(config) => {
            match SqliteHandles::open_writable(config.clone(), &store.binding, CURSOR) {
                Ok(handles) => handles.compact(&request),
                Err(error) => return format!("open: {error:?}"),
            }
        }
    };
    match result {
        Ok(receipt) => format!("Ok({})", receipt.path.as_str()),
        Err(HistoryError::MountsActive(_)) => "MountsActive".into(),
        Err(HistoryError::Busy | HistoryError::BusyOp(_)) => "Busy".into(),
        Err(other) => format!("{other:?}"),
    }
}

fn compaction(store: &TestStore) {
    let committer = store.open();
    // Dead bytes enough for a rebuild: a large file only a removed Branch
    // reached.
    let trash = store.fork("trash");
    let workspace = committer.bind(trash);
    write(&workspace, "/dead", &adversary::random_bytes(41, 4 << 20));
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    committer.unmount(workspace);
    committer
        .call("branch.remove", vec![("branch", SdkValue::text("trash"))])
        .expect("the trash Branch is removed");
    let keep = store.fork("keep");
    let workspace = committer.bind(keep);
    let mut expected = expected_seed();
    let gate = Gate::new(2);
    let (commits, compacted) = std::thread::scope(|scope| {
        let (gate, workspace) = (&gate, &workspace);
        let committing = scope.spawn(move || {
            assert!(gate.wait());
            (0..LOOPS)
                .map(|n| {
                    write(workspace, &format!("/c{n}"), format!("c {n}\n").as_bytes());
                    verdict(&commit(workspace))
                })
                .collect::<Vec<_>>()
        });
        let compacting = scope.spawn(move || {
            assert!(gate.wait());
            (0..3)
                .map(|n| compact(store, n + 1))
                .collect::<Vec<String>>()
        });
        (committing.join().unwrap(), compacting.join().unwrap())
    });
    let codes = compacted;
    println!(
        "GC_COMPACT_MOUNTED {{\"backend\":\"{:?}\",\"compactions\":\"{codes:?}\"}}",
        store.kind
    );
    for code in &codes {
        assert!(
            ["MountsActive", "Busy"].contains(&code.as_str()),
            "{:?}: a compaction under a live mount answered {code}",
            store.kind
        );
    }
    for (n, answer) in commits.iter().enumerate() {
        assert!(
            matches!(answer, Verdict::Committed(_)),
            "{:?}: Commit {n} beside a refused compaction: {answer:?}",
            store.kind
        );
        expected.insert(
            format!("/c{n}"),
            Node::File(format!("c {n}\n").into_bytes()),
        );
    }
    committer.unmount(workspace);

    // No mount: the compaction runs. The still-open service may be fenced
    // off (`Stale`), never silently wrong.
    let after = compact(store, 9);
    println!(
        "GC_COMPACT_UNMOUNTED {{\"backend\":\"{:?}\",\"compaction\":\"{}\"}}",
        store.kind, after
    );
    // SQLite compacts only a Store no other session holds open: with the old
    // service still open it refuses, and runs once that service is gone.
    match store.kind {
        Kind::Postgres => assert_eq!(after, "Ok(rebuild)", "the dead file is collected"),
        Kind::Sqlite => assert!(
            after == "Ok(rebuild)" || after.contains("sidecar remains"),
            "{after}"
        ),
    }
    let old_service = match committer.try_bind(keep) {
        Ok(workspace) => {
            write(&workspace, "/old-service", b"old\n");
            let answer = verdict(&commit(&workspace));
            println!(
                "GC_COMPACT_OLD_SERVICE {{\"backend\":\"{:?}\",\"commit\":\"{answer:?}\"}}",
                store.kind
            );
            match answer {
                Verdict::Committed(_) => {
                    expected.insert("/old-service".into(), Node::File(b"old\n".to_vec()));
                }
                Verdict::Refused(text) => assert!(
                    text.contains("Stale") || text.contains("Busy"),
                    "{:?}: {text}",
                    store.kind
                ),
                other => panic!("{:?}: {other:?}", store.kind),
            }
            let _ = committer.try_unmount(workspace);
            "bound"
        }
        Err(error) => {
            println!(
                "GC_COMPACT_OLD_SERVICE {{\"backend\":\"{:?}\",\"bind\":\"{error}\"}}",
                store.kind
            );
            assert!(
                error.contains("Stale") || error.contains("Busy"),
                "{:?}: {error}",
                store.kind
            );
            "refused"
        }
    };
    committer.abandon();
    if after != "Ok(rebuild)" {
        let again = compact(store, 10);
        println!(
            "GC_COMPACT_CLOSED {{\"backend\":\"{:?}\",\"compaction\":\"{again}\"}}",
            store.kind
        );
        assert_eq!(again, "Ok(rebuild)", "the dead file is collected");
    }
    assert_eq!(store.tree(keep), expected, "compaction lost nothing");
    let fresh = store.open();
    let workspace = fresh.bind(keep);
    write(&workspace, "/fresh", b"fresh\n");
    expected.insert("/fresh".into(), Node::File(b"fresh\n".to_vec()));
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    fresh.unmount(workspace);
    fresh.stop();
    assert_eq!(store.tree(keep), expected);
    assert_linear(&store.chain(keep));
    let failures = store.integrity_failures();
    assert!(
        failures.is_empty(),
        "audit after {old_service}: {failures:#?}"
    );
}

#[test]
fn commits_race_branch_removals_and_their_epoch_bumps() {
    for store in store_pair("gc-branch") {
        branch_removals(&store);
    }
}

#[test]
fn commits_race_path_removals() {
    for store in store_pair("gc-path") {
        path_removals(&store);
    }
}

#[test]
fn commits_race_compaction() {
    for store in store_pair("gc-compact") {
        compaction(&store);
    }
}
