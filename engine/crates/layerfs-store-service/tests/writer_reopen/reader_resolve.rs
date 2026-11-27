//! Second independent round of writer-reopen tests, T4 (fix round
//! 1d0a121012): Store service readers reopen. A reader reopen during an
//! uncertain Commit's resolution must never turn the unknown outcome into a
//! wrong "not committed" (a fold-back), and the change must be installed
//! and published once.
//!
//! A server-restart-style fault: every proxied backend of the service is
//! terminated, then the server refuses connections, then accepts again. It
//! is applied with the resolution held (the proxy's `Pause`) at the first
//! reply of its control-reader lookup, and at the last message of that
//! lookup (after it, the resolution's install runs; calibration shows
//! whether the install sends anything to the server at all: it reads the
//! candidate root's objects through the Workspace's reader).
//!
//! Both must decide the lost Commit as committed, never fold it back, and
//! never publish it again. With the restart after the lookup, the install's
//! reads fail and the resolution answers `Unresolved` with the known
//! publication; the custody must stay retained with it, so a later
//! resolution installs it and a new change lands on the lost Commit. Before
//! review round 2 (V5; also at 7c18d93b57) the resolution took the custody
//! and the Workspace refused every later Commit (`InFlight`), which this
//! printed as a finding; it is now asserted.
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/verify/reopen_rig.rs"]
#[allow(dead_code)]
mod testerwr;

use adversary::{commit, verdict, write, Verdict};
use fault_proxy::Fault;
use layerfs_history::{BranchId, CommitOutcome};
use layerfs_store_service::store::{Resolution, Unresolved};
use std::time::Duration;
use testerwr::*;

/// Where in the resolution's flight the restart hits.
#[derive(Clone, Copy, Debug)]
enum At {
    LookupFirstReply,
    LookupLastMessage,
}

/// Per connection of a window, in first-seen order: its messages.
fn by_connection(window: &fault_proxy::Window) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for (connection, kind) in &window.log {
        match out.last_mut() {
            Some((last, kinds)) if last == connection => kinds.push(*kind),
            _ => out.push((*connection, kind.to_string())),
        }
    }
    out
}

fn restart_during_resolution(label: &str, at: At) {
    let Some(rig) = rig(label) else {
        return;
    };
    let _dog = Watchdog::arm("t4", Duration::from_secs(100));
    let branch: BranchId = rig.store.fork("work");
    let workspace = rig.service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    committed(&workspace, "warm-up");
    // Calibration, twice: the first resolution opens the control reader
    // inside its flight; the second is the shape the armed one has.
    for n in 0..2 {
        lose_commit(&rig, &workspace, branch, &format!("/cal{n}"));
        rig.proxy.arm(None);
        let calibrated = workspace.resolve_retained();
        let window = rig.proxy.disarm();
        assert!(
            matches!(&calibrated, Ok(Some(r)) if r.outcome.is_some()),
            "{calibrated:?}"
        );
        assert_eq!(verdict(&commit(&workspace)), Verdict::UpToDate);
        println!(
            "TESTERWR2_T4 {at:?} calibration {n}: {:?}",
            by_connection(&window)
        );
        if n == 0 {
            continue;
        }
        let runs = by_connection(&window);
        // The lane pass (its own connection), then the lookup.
        let lane = runs[0].1.len() as u64;
        let lookup = runs[1].1.len() as u64;
        let target = match at {
            At::LookupFirstReply => lane,
            At::LookupLastMessage => lane + lookup - 1,
        };
        let after: Vec<&(usize, String)> = runs.iter().skip(2).collect();
        println!("TESTERWR2_T4 {at:?} lane {lane} lookup {lookup}; after the lookup: {after:?}");
        // The armed resolution.
        let (old, _) = lose_commit(&rig, &workspace, branch, "/lost");
        let lost = rig.store.chain(branch)[0].id;
        assert_eq!(rig.store.chain(branch)[0].parent, Some(old), "committed");
        rig.proxy.arm(Some((target, Fault::Pause)));
        let first = std::thread::scope(|scope| {
            let resolving = scope.spawn(|| workspace.resolve_retained());
            assert!(rig.proxy.wait_fired(Duration::from_secs(20)), "held");
            let mut admin = rig.store.admin();
            for pid in live_proxied(&rig) {
                // One may exit on its own meanwhile: not proven ended here.
                let _ = admin.query_one("SELECT pg_terminate_backend($1, 10000)", &[&pid]);
            }
            assert_eq!(live_proxied(&rig), Vec::<i32>::new(), "every backend ended");
            rig.gate.refuse(true);
            rig.proxy.release();
            resolving.join().unwrap()
        });
        let window = rig.proxy.disarm();
        let first_text = format!("{first:?}");
        println!(
            "TESTERWR2_T4 {at:?} fired {:?}; with the server down: {}",
            window.fired.as_ref().map(|f| (f.at, f.connection, f.kind)),
            first_text.chars().take(300).collect::<String>()
        );
        // While the server is down: never a fold-back.
        if let Ok(Some(resolution)) = &first {
            assert!(
                resolution.outcome.is_some(),
                "a reader reopen turned the lost Commit into 'not committed'"
            );
        }
        let down_commit = verdict(&commit(&workspace));
        println!("TESTERWR2_T4 {at:?} a Commit while down: {down_commit:?}");
        rig.gate.refuse(false);
        let second = workspace.resolve_retained();
        let second_text = format!("{second:?}");
        println!(
            "TESTERWR2_T4 {at:?} server back, resolution: {}",
            second_text.chars().take(300).collect::<String>()
        );
        // What each attempt decided: the installed outcome, or the known
        // publication an install failure reported.
        let decided = |answer: &Result<Option<Resolution>, Box<Unresolved>>| match answer {
            Ok(Some(resolution)) => Some(resolution.outcome.clone()),
            Err(unresolved) => unresolved.published.clone().map(Some),
            Ok(None) => None,
        };
        let known = decided(&first).or_else(|| decided(&second));
        match &known {
            Some(Some(CommitOutcome::Committed { record, .. })) => assert_eq!(record.id, lost),
            other => panic!("the lost Commit was decided {other:?}"),
        }
        // Published once, whatever the resolutions answered: the next
        // Commits publish nothing of it again.
        let follow = verdict(&commit(&workspace));
        let again = verdict(&commit(&workspace));
        let after = rig.store.chain(branch);
        println!(
            "TESTERWR2_T4 {at:?} follow-ups {follow:?} then {again:?}; retained {}",
            workspace.has_retained()
        );
        assert_eq!(after[0].id, lost, "published a second time: {follow:?}");
        write(&workspace, "/after", b"after\n");
        let next = verdict(&commit(&workspace));
        let chain = rig.store.chain(branch);
        println!(
            "TESTERWR2_T4 {at:?} a new change: {next:?}; head's parent is the lost one: {}",
            chain[0].parent == Some(lost)
        );
        match next {
            Verdict::Committed(_) => {
                assert_eq!(
                    chain[0].parent,
                    Some(lost),
                    "a new change lands on the lost one"
                )
            }
            // Review round 2, V5: a resolution whose install failed
            // definitely after it knew the publication keeps the custody,
            // so the Workspace is never left refusing every later Commit.
            other => panic!(
                "TESTERWR2_T4_FINDING {at:?}: the Workspace refuses every Commit after the \
                 resolution (first resolved {}, second resolved {}): {other:?}",
                first.is_ok(),
                second.is_ok()
            ),
        }
    }
    rig.service.unmount(workspace);
    rig.service.stop();
}

#[test]
fn t4_a_restart_during_the_lookup_never_folds_back() {
    restart_during_resolution("wr2_t4_lookup", At::LookupFirstReply);
}

#[test]
fn t4_a_restart_between_the_lookup_and_the_install_never_installs_twice() {
    restart_during_resolution("wr2_t4_install", At::LookupLastMessage);
}
