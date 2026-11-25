//! #768 independent verification: two daemons (two Store services, so two
//! PostgreSQL sessions; one service on SQLite) each with a Workspace bound
//! on the same Branch, committing at the same moment round after round with
//! no pacing, with overlapping and identical content:
//!
//! * every round both write `/shared` with the same bytes, and each its own
//!   file;
//! * every third round both also write the same new 160 KiB file (the same
//!   new objects published by two sessions: the fused message's lost
//!   identity race);
//! * every fifth round they write only `/shared`, identically, so the second
//!   Commit's merged tree is the head's own.
//!
//! An identical change on both sides is not a conflict. Each answer is
//! Committed, UpToDate or a documented refusal; a conflicted merge or an
//! unknown outcome fails. After a sequential flush, from fresh sessions:
//! the first-parent chain is linear, holds every answered Commit exactly
//! once and nothing else (no Commit published without being answered; the
//! Store's Commit rows are exactly the chain), and the tree is the union of
//! everything written. The audit, amcheck and the allocation checks pass.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;

use adversary::{
    assert_linear, commit, random_bytes, seed_tree, verdict, write, Gate, Kind, Node, Service,
    TestStore, Verdict,
};
use layerfs_history::CommitId;
use std::collections::HashMap;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];
const ROUNDS: usize = 24;

fn documented(text: &str) -> bool {
    ["LaneMerge", "LiveConflict", "Busy"]
        .iter()
        .any(|code| text.contains(code))
}

fn shared(round: usize) -> Vec<u8> {
    format!("shared bytes of round {round}\n").into_bytes()
}

fn run(store: &TestStore) {
    let services: Vec<Service> = match store.kind {
        Kind::Postgres => vec![store.open(), store.open()],
        Kind::Sqlite => vec![store.open()],
    };
    let branch = store.fork("both");
    let workspaces: Vec<_> = (0..2)
        .map(|n| services[n % services.len()].bind(branch))
        .collect();
    let mut expected = seed_tree(SEED);
    let mut answers: Vec<(usize, usize, Verdict)> = Vec::new();
    for round in 0..ROUNDS {
        let only_shared = round % 5 == 4;
        let blob = (round % 3 == 0).then(|| random_bytes(round as u64 + 7680, 160 * 1024));
        expected.insert("/shared".into(), Node::File(shared(round)));
        for (n, workspace) in workspaces.iter().enumerate() {
            write(workspace, "/shared", &shared(round));
            if !only_shared {
                let own = format!("/own{n}-{round}");
                let bytes = format!("{n} wrote round {round}\n").into_bytes();
                write(workspace, &own, &bytes);
                expected.insert(own, Node::File(bytes));
            }
            if let Some(blob) = &blob {
                write(workspace, &format!("/blob-{round}"), blob);
            }
        }
        if let Some(blob) = blob {
            expected.insert(format!("/blob-{round}"), Node::File(blob));
        }
        let gate = Gate::new(workspaces.len());
        let round_answers: Vec<Verdict> = std::thread::scope(|scope| {
            let handles: Vec<_> = workspaces
                .iter()
                .map(|workspace| {
                    let gate = &gate;
                    scope.spawn(move || {
                        assert!(gate.wait(), "both parties reached the gate");
                        verdict(&commit(workspace))
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for (n, answer) in round_answers.into_iter().enumerate() {
            answers.push((round, n, answer));
        }
    }
    // Sequential flush: each Workspace publishes what it still holds.
    for pass in 0..2 {
        for (n, workspace) in workspaces.iter().enumerate() {
            answers.push((ROUNDS + pass, n, verdict(&commit(workspace))));
        }
    }
    let mut landed: Vec<CommitId> = Vec::new();
    let mut tally: HashMap<&'static str, usize> = HashMap::new();
    for (round, n, answer) in &answers {
        let kind = match answer {
            Verdict::Committed(id) => {
                landed.push(*id);
                "committed"
            }
            Verdict::UpToDate => "uptodate",
            Verdict::Refused(text) if documented(text) => "refused",
            other => panic!(
                "{:?}: round {round} Workspace {n} answered {other:?}; all: {answers:#?}",
                store.kind
            ),
        };
        *tally.entry(kind).or_default() += 1;
    }
    println!(
        "VERIFY_TWO_DAEMONS {{\"backend\":\"{:?}\",\"tally\":\"{tally:?}\"}}",
        store.kind
    );
    // The last flush pass publishes nothing new.
    for (round, n, answer) in &answers {
        if *round == ROUNDS + 1 {
            assert_eq!(
                *answer,
                Verdict::UpToDate,
                "{:?}: Workspace {n} still published in the last flush",
                store.kind
            );
        }
    }
    for (n, workspace) in workspaces.into_iter().enumerate() {
        services[n % services.len()].unmount(workspace);
    }
    for service in services {
        service.stop();
    }
    let chain = store.chain(branch);
    assert_linear(&chain);
    let mut in_chain: Vec<CommitId> = chain.iter().map(|r| r.id).collect();
    in_chain.sort();
    let mut answered = landed.clone();
    answered.sort();
    assert_eq!(
        in_chain, answered,
        "{:?}: the chain is exactly the answered Commits",
        store.kind
    );
    assert_eq!(
        store.commit_rows(),
        chain.len() as i64,
        "{:?}: a Commit row nothing answered for",
        store.kind
    );
    assert_eq!(store.tree(branch), expected, "{:?}: the union", store.kind);
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "{:?}: audit {failures:#?}", store.kind);
    if store.kind == Kind::Postgres {
        let failures = verify768::amcheck(store);
        assert!(failures.is_empty(), "amcheck: {failures:#?}");
        let failures = verify768::duplicate_allocations(store);
        assert!(failures.is_empty(), "allocation: {failures:#?}");
    }
}

#[test]
fn two_daemons_commit_one_branch_with_identical_overlapping_content() {
    for store in TestStore::both("v768-two", SEED) {
        run(&store);
    }
}

/// A Workspace Commit and a direct Store `fs.write` of another file on the
/// same Branch (on another service where the backend allows it), at the
/// same moment, round after round: each answer is a landed Commit or a
/// documented refusal; the chain is exactly the answered Commits plus one
/// per accepted write; the tree is the union.
fn with_direct_writes(store: &TestStore) {
    let services: Vec<Service> = match store.kind {
        Kind::Postgres => vec![store.open(), store.open()],
        Kind::Sqlite => vec![store.open()],
    };
    let branch = store.fork("mixed");
    let workspace = services[0].bind(branch);
    let writer = services.last().unwrap();
    let mut expected = seed_tree(SEED);
    let mut landed = Vec::new();
    let mut writes = 0usize;
    let mut log = Vec::new();
    for round in 0..16 {
        let own = format!("/ws-{round}");
        write(&workspace, &own, format!("ws {round}\n").as_bytes());
        expected.insert(own, Node::File(format!("ws {round}\n").into_bytes()));
        let direct = format!("/direct-{round}");
        let gate = Gate::new(2);
        let (answer, wrote) = std::thread::scope(|scope| {
            let (gate, workspace) = (&gate, &workspace);
            let a = scope.spawn(move || {
                assert!(gate.wait());
                verdict(&commit(workspace))
            });
            let direct = direct.clone();
            let b = scope.spawn(move || {
                assert!(gate.wait());
                writer.call(
                    "fs.write",
                    vec![
                        ("branch", layerfs_bridge::control::SdkValue::text("mixed")),
                        ("path", layerfs_bridge::control::SdkValue::text(&direct)),
                        (
                            "content",
                            layerfs_bridge::control::SdkValue::text("direct\n"),
                        ),
                    ],
                )
            });
            (a.join().unwrap(), b.join().unwrap())
        });
        match &wrote {
            Ok(_) => {
                writes += 1;
                expected.insert(direct, Node::File(b"direct\n".to_vec()));
            }
            Err(refusal) => assert!(
                documented(&refusal.code) || refusal.code.contains("Conflict"),
                "{:?}: fs.write refused {refusal:?}",
                store.kind
            ),
        }
        match &answer {
            Verdict::Committed(id) => landed.push(*id),
            Verdict::Refused(text) if documented(text) => {}
            other => panic!("{:?}: round {round} Commit {other:?}", store.kind),
        }
        log.push(format!("{round}: {answer:?} / {}", wrote.is_ok()));
    }
    // Flush: the Workspace publishes what it still holds.
    match verdict(&commit(&workspace)) {
        Verdict::Committed(id) => landed.push(id),
        Verdict::UpToDate => {}
        other => panic!("{:?}: flush {other:?}", store.kind),
    }
    println!(
        "VERIFY_DIRECT {{\"backend\":\"{:?}\",\"landed\":{},\"writes\":{writes}}}",
        store.kind,
        landed.len()
    );
    services[0].unmount(workspace);
    for service in services {
        service.stop();
    }
    let chain = store.chain(branch);
    assert_linear(&chain);
    for id in &landed {
        assert!(
            chain.iter().any(|r| r.id == *id),
            "{:?}: {id} off the chain",
            store.kind
        );
    }
    assert_eq!(
        chain.len(),
        landed.len() + writes,
        "{:?}: one Commit per landed Commit and per accepted write; log {log:#?}",
        store.kind
    );
    assert_eq!(
        store.commit_rows(),
        chain.len() as i64,
        "{:?}: unanswered Commit rows",
        store.kind
    );
    assert_eq!(store.tree(branch), expected, "{:?}: the union", store.kind);
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "{:?}: audit {failures:#?}", store.kind);
}

#[test]
fn a_workspace_commit_races_direct_writes_on_its_branch() {
    for store in TestStore::both("v768-direct", SEED) {
        with_direct_writes(&store);
    }
}
