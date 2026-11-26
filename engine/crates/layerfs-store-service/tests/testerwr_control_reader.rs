//! Independent writer-reopen tests: resolving an uncertain Commit after the
//! writer reopened needs the service's control reader too.
//!
//! * The server restarts (every backend of the service ends) after a
//!   Commit's reply was lost: once the server is back, the Commit must
//!   resolve, installed once.
//! * The control reader loses one reply during a resolution's lookup:
//!   readers never reopen (by design, unchanged by the reopen), so check
//!   whether any later resolution, on this or another Workspace, can still
//!   decide an uncertain Commit.
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/testerwr.rs"]
#[allow(dead_code)]
mod testerwr;

use adversary::{commit, verdict, write, Verdict};
use fault_proxy::Fault;
use layerfs_history::CommitOutcome;
use testerwr::*;

#[test]
fn an_uncertain_commit_resolves_after_the_server_restarts() {
    let Some(rig) = rig("wr_restart_resolve") else {
        return;
    };
    let branch = rig.store.fork("work");
    let workspace = rig.service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    committed(&workspace, "warm-up");
    let (old, _) = lose_commit(&rig, &workspace, branch, "/lost");
    assert!(workspace.has_retained(), "retained before the restart");
    // A restart: every backend of the service ends.
    for pid in live_proxied(&rig) {
        kill(&rig.store, pid);
    }
    assert!(workspace.has_retained(), "retained across the restart");
    let mut answers = Vec::new();
    let mut decided = None;
    for _ in 0..2 {
        match workspace.resolve_retained() {
            Ok(Some(resolution)) => {
                decided = Some(resolution.outcome);
                break;
            }
            Ok(None) => {
                answers.push("nothing retained".into());
                break;
            }
            Err(unresolved) => answers.push(format!("{unresolved:?}")),
        }
    }
    let short: Vec<String> = answers
        .iter()
        .map(|a| a.chars().take(160).collect())
        .collect();
    println!("TESTERWR_RESTART_RESOLVE {short:?} -> {decided:?}");
    let chain = rig.store.chain(branch);
    assert_eq!(chain[0].parent, Some(old), "the server committed it");
    let lost = chain[0].id;
    // Whatever the resolution answered, the change must be published once:
    // the Workspace's next Commits never publish it again.
    let follow = verdict(&commit(&workspace));
    let again = verdict(&commit(&workspace));
    let after = rig.store.chain(branch);
    println!(
        "TESTERWR_RESTART_FOLLOW {follow:?} then {again:?}; head moved: {}",
        after[0].id != lost
    );
    assert_eq!(
        after[0].id, lost,
        "the lost Commit published a second time: {follow:?}"
    );
    match decided {
        Some(Some(CommitOutcome::Committed { record, .. })) => assert_eq!(record.id, lost),
        other => panic!(
            "after a restart the lost Commit is not resolved: {other:?} {short:?}; \
             the Workspace's next Commit: {follow:?}"
        ),
    }
    rig.service.unmount(workspace);
    rig.service.stop();
}

#[test]
fn a_control_reader_reply_lost_during_a_resolution() {
    let Some(rig) = rig("wr_control_reader") else {
        return;
    };
    let branch = rig.store.fork("work");
    let workspace = rig.service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    committed(&workspace, "warm-up");
    // Calibration: a resolution's flight (the lane pass on a connection of
    // its own, then the control reader's lookup).
    lose_commit(&rig, &workspace, branch, "/cal");
    rig.proxy.arm(None);
    let calibrated = workspace.resolve_retained();
    let window = rig.proxy.disarm();
    assert!(matches!(&calibrated, Ok(Some(r)) if r.outcome.is_some()));
    assert_eq!(verdict(&commit(&workspace)), Verdict::UpToDate);
    // The armed run on a second resolution, whose control reader is open:
    // the first message after the lane pass's own connection.
    let (old, _) = lose_commit(&rig, &workspace, branch, "/lost");
    rig.proxy.arm(None);
    let probe = rig.proxy.disarm();
    drop(probe);
    let lane = window.log[0].0;
    let lookup = window.log.iter().position(|(c, _)| *c != lane).unwrap();
    // The calibration opened the control reader inside its flight; this
    // run reuses it, so its lookup starts right after the lane pass.
    let lane_len = window.log.iter().take_while(|(c, _)| *c == lane).count();
    println!("TESTERWR_CONTROL calibration lane {lane_len} messages, lookup at {lookup}");
    rig.proxy.arm(Some((lane_len as u64, Fault::Drop)));
    let first = workspace.resolve_retained();
    let armed = rig.proxy.disarm();
    let fired = armed.fired.clone().expect("fired");
    println!(
        "TESTERWR_CONTROL fired on connection {} ({}): {:?}",
        fired.connection,
        fired.kind,
        first.as_ref().err()
    );
    assert!(first.is_err(), "the lookup's reply was lost");
    let chain = rig.store.chain(branch);
    assert_eq!(chain[0].parent, Some(old), "the server committed it");
    let mut answers = Vec::new();
    let mut decided = None;
    for _ in 0..2 {
        match workspace.resolve_retained() {
            Ok(resolution) => {
                decided = Some(resolution.expect("retained").outcome);
                break;
            }
            Err(unresolved) => answers.push(format!("{unresolved:?}")),
        }
    }
    // Another Workspace of the same service, with its own lost Commit.
    let other_branch = rig.store.fork("other");
    let other = rig.service.bind(other_branch);
    write(&other, "/o", b"o\n");
    committed(&other, "other warm-up");
    let (_, _) = lose_commit(&rig, &other, other_branch, "/other");
    let other_answer = other
        .resolve_retained()
        .map(|r| r.map(|r| r.outcome.is_some()));
    println!(
        "TESTERWR_CONTROL later: {answers:?} -> {decided:?}; other Workspace: {other_answer:?}"
    );
    assert!(
        matches!(decided, Some(Some(CommitOutcome::Committed { .. }))),
        "the lost Commit never resolves after one lost control-reader reply: {answers:?}; \
         another Workspace's resolution: {other_answer:?}"
    );
    rig.service.unmount(workspace);
    rig.service.unmount(other);
    rig.service.stop();
}
