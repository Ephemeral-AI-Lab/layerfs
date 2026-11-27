//! Independent writer-reopen tests: a second fault during the reopen itself.
//!
//! Each step quarantines the Store service's writer with a mount whose
//! reply is lost after its intent's `COMMIT` (an orphaned row the reopen
//! must delete), while two Workspaces stay mounted (keys the reopen must
//! take again). The next call, a Commit, reopens; one fault is injected at
//! server message `N` of that call's whole flight (the new connection's
//! startup, every settle step, then the Commit's own messages), for every
//! `N` (each test covers one half of the flight, to stay inside the
//! 2-minute ceiling on a loaded host). After every step, from an
//! independent admin connection:
//!
//! * the call opened at most one connection, and reopened at most once;
//! * both live mounts keep their rows, and `branch.remove` and compaction
//!   from another session answer `MountsActive`;
//! * at most one proxied backend holds session keys, and it holds all four
//!   (Store, project and both Branches): never a partial set;
//! * a Commit answered `Committed` is the head; a refused one moved
//!   nothing; an unknown one resolves (without a resend) to installed once
//!   or folded back, and its change lands exactly once.
//!
//! At the end every orphaned row is gone, the chain holds exactly the
//! Commits that were reported or resolved committed, the Store holds no
//! other Commit row, and the tree holds every change.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/verify/reopen_rig.rs"]
#[allow(dead_code)]
mod testerwr;

use adversary::{assert_linear, commit, verdict, write, Node, Verdict};
use fault_proxy::Fault;
use layerfs_history::{CommitId, CommitOutcome};
use layerfs_store_service::store::BoundWorkspace;
use testerwr::*;

fn settle(workspace: &BoundWorkspace, reported: &mut Vec<CommitId>, label: &str) {
    if workspace.has_retained() {
        let resolution = workspace
            .resolve_retained()
            .unwrap_or_else(|unresolved| panic!("{label}: resolve: {unresolved:?}"))
            .expect("a retained Commit");
        match resolution.outcome {
            Some(CommitOutcome::Committed { record, .. }) => reported.push(record.id),
            Some(other) => panic!("{label}: resolved as {other:?}"),
            None => {}
        }
    }
    match verdict(&commit(workspace)) {
        Verdict::Committed(id) => reported.push(id),
        Verdict::UpToDate => {}
        other => panic!("{label}: settling Commit: {other:?}"),
    }
}

fn sweep(tag: &str, fault: Fault, half: usize) {
    let Some(rig) = rig(tag) else {
        return;
    };
    let twin = rig.store.fork("twin");
    let (held_branch, commit_branch) = (rig.store.fork("held"), rig.store.fork("work"));
    let k_mount = mount_flight(&rig, twin);
    let held = rig.service.bind(held_branch);
    let workspace = rig.service.bind(commit_branch);
    write(&workspace, "/warm", b"warm\n");
    let mut reported: Vec<CommitId> = vec![committed(&workspace, "warm-up")];
    let mut orphans = Vec::new();

    // Calibration: the same shape, unfaulted.
    let orphan = rig.store.fork("o_cal");
    orphans.push(orphan);
    lose_mount(&rig, twin, orphan, k_mount);
    assert_eq!(
        rows(&rig.store, orphan),
        (1, 0),
        "the orphan's row committed"
    );
    write(&workspace, "/cal", b"cal\n");
    let attempts = rig.gate.attempts();
    rig.proxy.arm(None);
    let first = verdict(&commit(&workspace));
    let window = rig.proxy.disarm();
    let Verdict::Committed(id) = first else {
        panic!("calibration: {first:?}");
    };
    reported.push(id);
    assert_eq!(reopens(&rig.service), 1, "calibration reopened once");
    assert_eq!(rig.gate.attempts() - attempts, 1, "one connection");
    assert_eq!(rows(&rig.store, orphan), (0, 0), "the orphan's row is gone");
    let n = window.seen;
    let kinds: Vec<char> = window.log.iter().map(|(_, kind)| *kind).collect();
    let flight: Vec<String> = (window.log.iter())
        .map(|(c, kind)| format!("{c}{kind}"))
        .collect();
    println!("TESTERWR_FLIGHT {tag} N={n} {}", flight.join(" "));

    let mut states = std::collections::BTreeMap::<String, u64>::new();
    // One half of the flight per test, so each stays well inside the
    // 2-minute ceiling on a loaded host.
    let steps = match half {
        0 => 0..n / 2,
        _ => n / 2..n,
    };
    for step in steps.clone() {
        let label = format!("{tag} step {step}");
        let orphan = rig.store.fork(&format!("o{step}"));
        orphans.push(orphan);
        lose_mount(&rig, twin, orphan, k_mount);
        let head = rig.store.chain(commit_branch)[0].id;
        let (reopened, attempts) = (reopens(&rig.service), rig.gate.attempts());
        write(
            &workspace,
            &format!("/f{step}"),
            format!("f{step}\n").as_bytes(),
        );
        rig.proxy.arm(Some((step, fault)));
        let first = verdict(&commit(&workspace));
        let window = rig.proxy.disarm();
        let fired = window.fired.expect("the fault fired");
        let reopened = reopens(&rig.service) - reopened;
        let attempts = rig.gate.attempts() - attempts;
        // Recorded, not asserted here: see the attempts tally printed below.
        assert!(attempts <= 2, "{label}: {attempts} connections in one call");
        // A Commit takes the writer's turn several times; a backend killed
        // after a turn's reply arrived is found dead at the next turn,
        // before anything is sent, and reopened again (recorded below).
        assert!(reopened <= 2, "{label}: {reopened} reopens in one call");
        let chain = rig.store.chain(commit_branch);
        assert_linear(&chain);
        let state = match &first {
            Verdict::Committed(id) => {
                assert!(reopened >= 1, "{label}: served only after a reopen");
                assert_eq!((chain[0].id, chain[0].parent), (*id, Some(head)));
                reported.push(*id);
                "committed"
            }
            Verdict::Refused(_) => {
                assert_eq!(chain[0].id, head, "{label}: a refusal moved the head");
                if reopened == 1 {
                    "refused after reopen"
                } else {
                    "refused, still quarantined"
                }
            }
            Verdict::Unknown(_) => {
                assert!(workspace.has_retained(), "{label}: retained");
                assert!(chain[0].id == head || chain[0].parent == Some(head));
                "unknown"
            }
            other => panic!("{label}: {other:?}"),
        };
        *states
            .entry(format!("{state} @{}{}", fired.connection, fired.kind))
            .or_default() += 1;
        println!(
            "TESTERWR_STEP {label} at {}{} attempts={attempts} reopened={reopened} orphan={:?}: {first:?}",
            fired.connection,
            fired.kind,
            rows(&rig.store, orphan)
        );
        // Custody, from the admin side.
        assert_eq!(rows(&rig.store, held_branch), (1, 0), "{label}: held row");
        assert_eq!(rows(&rig.store, commit_branch), (1, 0), "{label}: work row");
        assert!(rows(&rig.store, orphan).0 <= 1, "{label}: orphan");
        let holders = key_holders(&rig);
        assert!(
            holders.is_empty() || (holders.len() == 1 && holders[0].1 == 4),
            "{label}: key custody {holders:?}"
        );
        assert_eq!(
            remove(&rig.store, "held"),
            "MountsActive",
            "{label}: remove"
        );
        assert_eq!(compact(&rig.store), "MountsActive", "{label}: compaction");
        // Settle unarmed, so every step faults the same flight: resolve a
        // retained Commit (never resent), then one Commit carries whatever
        // the faulted one did not land.
        settle(&workspace, &mut reported, &label);
    }
    println!("TESTERWR_STATES {tag} N={n} {states:?}");
    assert_eq!(verdict(&commit(&workspace)), Verdict::UpToDate);
    // Steps whose orphaned mount row outlived every later reopen.
    let leftover: Vec<String> = (orphans.iter().enumerate())
        .filter(|(_, orphan)| rows(&rig.store, **orphan) != (0, 0))
        .map(|(n, _)| match n {
            0 => "calibration".to_owned(),
            n => {
                let step = steps.start as usize + n - 1;
                format!("{step} ({})", kinds[step])
            }
        })
        .collect();
    let chain = rig.store.chain(commit_branch);
    assert_linear(&chain);
    let ids: std::collections::HashSet<CommitId> = chain.iter().map(|c| c.id).collect();
    for id in &reported {
        assert!(ids.contains(id), "reported Commit {id} is not on the chain");
    }
    assert_eq!(
        chain.len(),
        reported.len(),
        "the chain holds exactly the reported Commits (no double install)"
    );
    let twin_chain = rig.store.chain(twin);
    assert_eq!(
        commit_rows(&rig.store) as usize,
        chain.len() + twin_chain.len(),
        "no Commit row outside the chains"
    );
    let tree = rig.store.tree(commit_branch);
    for step in steps {
        assert_eq!(
            tree.get(&format!("/f{step}")),
            Some(&Node::File(format!("f{step}\n").into_bytes())),
            "change {step} landed"
        );
    }
    assert!(
        leftover.is_empty(),
        "orphaned mount rows never deleted, by faulted step (message kind): {leftover:?}"
    );
    rig.service.unmount(held);
    rig.service.unmount(workspace);
    assert_eq!(key_holders(&rig), vec![], "no key left");
    assert_eq!(all_rows(&rig.store), 0, "no row left");
    assert_eq!(compact(&rig.store), "Ok", "compaction admitted at the end");
    rig.service.stop();
    assert!(rig.store.no_open_transaction());
}

#[test]
fn a_drop_at_every_message_of_a_reopen_and_its_commit_first_half() {
    sweep("wr_sweep_drop_a", Fault::Drop, 0);
}

#[test]
fn a_drop_at_every_message_of_a_reopen_and_its_commit_second_half() {
    sweep("wr_sweep_drop_b", Fault::Drop, 1);
}

#[test]
fn a_kill_at_every_message_of_a_reopen_and_its_commit_first_half() {
    sweep("wr_sweep_kill_a", Fault::HoldKill, 0);
}

#[test]
fn a_kill_at_every_message_of_a_reopen_and_its_commit_second_half() {
    sweep("wr_sweep_kill_b", Fault::HoldKill, 1);
}
