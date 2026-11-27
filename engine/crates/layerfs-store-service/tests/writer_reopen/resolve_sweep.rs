//! Independent writer-reopen test: a second fault while an uncertain Commit
//! is resolved.
//!
//! Each step loses a warm Commit's last reply after the server committed it
//! (the writer is quarantined, the Commit retained). Its resolution
//! (`resolve_retained`, as the daemon runs it before the Workspace's next
//! Commit) passes the publication lane on a connection of its own and looks
//! the attempt up on the service's control reader; the next write call
//! reopens the writer. One `Drop` is injected at server message `N` of the
//! resolution, for every `N` up to the control reader's lookup (a lost
//! control-reader reply is `testerwr_control_reader.rs`). The server
//! committed, so the resolution may fail (the Commit stays retained and a
//! later resolution decides it) or answer committed, never "not
//! committed": that would fold the change back and the next Commit would
//! publish it a second time. After every step, from fresh sessions:
//! exactly one Commit past the step's old head, it is the lost Commit, and
//! the Workspace's next Commit is `UpToDate`. Each step remounts the
//! Workspace first (a fault may have ended its reader, which never
//! reopens).
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::adversary;
use crate::fault_proxy;
use crate::testerwr;

use adversary::{assert_linear, commit, verdict, write, Node, Verdict};
use fault_proxy::Fault;
use layerfs_history::{CommitId, CommitOutcome};
use layerfs_store_service::store::BoundWorkspace;
use testerwr::*;

#[test]
fn a_drop_at_every_message_of_an_uncertain_commits_resolution() {
    let Some(rig) = rig("wr_resolve") else {
        return;
    };
    rig.store.fork("twin");
    let branch = rig.store.fork("work");
    let mut workspace = rig.service.bind(branch);
    // One unarmed Commit on the current connection, so the next lost
    // Commit's flight is the warm one.
    let warm_up = |workspace: &BoundWorkspace, label: &str| {
        write(workspace, &format!("/warm-{label}"), b"w\n");
        committed(workspace, label);
    };
    warm_up(&workspace, "start");
    warm_up(&workspace, "start2");
    // Calibration: the resolution's whole flight.
    let (cal_old, _) = lose_commit(&rig, &workspace, branch, "/cal");
    rig.proxy.arm(None);
    let resolution = workspace
        .resolve_retained()
        .unwrap_or_else(|unresolved| panic!("{unresolved:?}"))
        .expect("retained");
    let window = rig.proxy.disarm();
    let Some(CommitOutcome::Committed { record, .. }) = resolution.outcome else {
        panic!("calibration resolved {:?}", resolution.outcome);
    };
    assert_eq!(rig.store.chain(branch)[0].parent, Some(cal_old));
    assert_eq!(rig.store.chain(branch)[0].id, record.id);
    // The resolution's flight: the publication lane's pass on a connection
    // it opens, then the lookup on the service's control reader (opened at
    // its first use). A fault on the control reader quarantines it for the
    // service's life (readers never reopen; `testerwr_control_reader.rs`),
    // so the sweep covers the flight up to the control reader's first
    // message.
    let first = window.log[0].0;
    let lane_end = window
        .log
        .iter()
        .position(|(c, _)| *c != first)
        .unwrap_or(window.log.len());
    let n = lane_end as u64;
    let kinds: String = window.log.iter().map(|(_, kind)| *kind).collect();
    println!("TESTERWR_RESOLVE_FLIGHT N={n} {kinds}");
    let mut states = std::collections::BTreeMap::<String, u64>::new();
    let mut lost_ids: Vec<CommitId> = vec![record.id];
    for step in 0..n {
        let label = format!("resolve step {step}");
        // A fresh mount each step: a fault may have ended the Workspace's
        // reader, which never reopens (a new mount gets a usable one).
        rig.service.unmount(workspace);
        workspace = rig.service.bind(branch);
        warm_up(&workspace, &format!("w{step}"));
        warm_up(&workspace, &format!("x{step}"));
        let path = format!("/f{step}");
        let (old, _) = lose_commit(&rig, &workspace, branch, &path);
        rig.proxy.arm(Some((step, Fault::Drop)));
        let first = workspace.resolve_retained();
        let armed = rig.proxy.disarm();
        // The flight may be shorter than `step` (statements already
        // prepared): then nothing fired and the resolution ran unfaulted.
        let fired_kind = armed.fired.as_ref().map_or('-', |fired| fired.kind);
        let flight: String = (armed.log.iter())
            .map(|(c, kind)| format!("{c}{kind}"))
            .collect::<Vec<_>>()
            .join(" ");
        let (outcome, how) = match first {
            Ok(Some(resolution)) => (resolution.outcome, "resolved"),
            Ok(None) => panic!("{label}: nothing retained"),
            Err(first) => {
                assert!(workspace.has_retained(), "{label}: still retained");
                // Later resolutions, unarmed: the first that answers decides.
                // Each is one new attempt (nothing is resent); bounded.
                let mut errors = vec![format!("{first:?}")];
                let mut decided = None;
                for _ in 0..3 {
                    let (reopened, attempts) = (reopens(&rig.service), rig.gate.attempts());
                    match workspace.resolve_retained() {
                        Ok(resolution) => {
                            decided = Some(resolution.expect("retained").outcome);
                            break;
                        }
                        Err(error) => errors.push(format!(
                            "{error:?} (reopens +{}, attempts +{})",
                            reopens(&rig.service) - reopened,
                            rig.gate.attempts() - attempts
                        )),
                    }
                }
                println!("TESTERWR_RESOLVE_RETRIES {label} {errors:?} flight: {flight}");
                match decided {
                    Some(outcome) => (outcome, "unresolved, then resolved"),
                    None => {
                        // Only a quarantined Workspace reader (readers never
                        // reopen) may keep it unresolved: every error is its
                        // history read. The server's truth stands: the lost
                        // Commit is the head, once. The Workspace is
                        // replaced by a new mount.
                        assert!(
                            errors.iter().all(|e| e.contains("history read failed")),
                            "{label}: never resolves (fault at {}): {errors:?}",
                            fired_kind
                        );
                        let chain = rig.store.chain(branch);
                        assert_linear(&chain);
                        assert_eq!(chain[0].parent, Some(old), "{label}: one Commit");
                        let unmounted = rig.service.try_unmount(workspace);
                        println!("TESTERWR_RESOLVE_STUCK {label}: unmount {unmounted:?}");
                        workspace = rig.service.bind(branch);
                        assert_eq!(verdict(&commit(&workspace)), Verdict::UpToDate);
                        *states
                            .entry(format!("stuck reader, remounted @{}", fired_kind))
                            .or_default() += 1;
                        continue;
                    }
                }
            }
        };
        let follow = verdict(&commit(&workspace));
        let chain = rig.store.chain(branch);
        assert_linear(&chain);
        assert_eq!(
            chain[0].parent,
            Some(old),
            "{label}: exactly one Commit past the old head"
        );
        let state = match outcome {
            Some(CommitOutcome::Committed { record, .. }) => {
                assert_eq!(
                    record.id, chain[0].id,
                    "{label}: the lost Commit is the head"
                );
                assert_eq!(follow, Verdict::UpToDate, "{label}: published once");
                lost_ids.push(record.id);
                "committed"
            }
            None => {
                assert_eq!(
                    follow,
                    Verdict::Committed(chain[0].id),
                    "{label}: folded back, carried once by the next Commit"
                );
                "not committed"
            }
            Some(other) => panic!("{label}: {other:?}"),
        };
        *states
            .entry(format!("{how}: {state} @{}", fired_kind))
            .or_default() += 1;
    }
    println!("TESTERWR_RESOLVE_STATES N={n} {states:?}");
    let chain = rig.store.chain(branch);
    let tree = rig.store.tree(branch);
    for (path, node) in &tree {
        if path.starts_with("/f") || path.starts_with("/cal") {
            assert_eq!(node, &Node::File(path.clone().into_bytes()), "{path}");
        }
    }
    let lost_files = tree.keys().filter(|path| path.starts_with("/f")).count();
    assert!(
        lost_files as u64 >= n,
        "every step's change landed: {lost_files}"
    );
    for id in &lost_ids {
        assert!(chain.iter().any(|c| c.id == *id), "{id} on the chain");
    }
    let twin = rig.store.chain(rig.store.branch("twin")).len();
    assert_eq!(
        commit_rows(&rig.store) as usize,
        chain.len() + twin,
        "no Commit row outside the chains"
    );
    rig.service.unmount(workspace);
    rig.service.stop();
    assert!(rig.store.no_open_transaction());
}
