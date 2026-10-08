//! Real kernel mount: the first execution of forced teardown. The control
//! filesystem is mounted by the test before Attach, so the connection's abort
//! control is bound; one idle Workspace is then forced. Counted work only.
//!
//! The safety rule of `support/fusectl.rs` binds this file: nothing here
//! writes under `/sys/fs/fuse/connections` or reads another connection.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/fusectl.rs"]
mod fusectl;
// Compiled here so the support builds with the product; track P4 executes it.
#[allow(dead_code)]
#[path = "support/history_gate.rs"]
mod history_gate;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
use layerfs_bridge::control::{
    AbortDisposition, Activity, CommitKnowledge, ControlCode, DetachDisposition, ForcedCleanup,
    NativePhase, Reply, Request,
};
use layerfs_daemon::control::Failure;
use mounted::{mount_entry, until, COMMAND};
use rig::{bash_in, passed, root, Rig};
use std::fs;

#[test]
fn an_idle_workspace_is_forced_with_one_abort_and_one_detach() {
    fusectl::mount();
    let rig = Rig::new("forced-smoke");
    let ready = rig.mount(1);
    assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
    let connection = fusectl::own(&ready);
    assert!(connection.exists());
    let mount = root(&ready).to_owned();
    // An ordinary process, never registered with the daemon, writes one file
    // and exits before the teardown: the Workspace is idle and unreferenced.
    let script = "printf 'forced\\n' > smoke.txt && cat smoke.txt";
    passed(&bash_in(COMMAND, &mount, script), "external write");
    assert_eq!(fs::read(mount.join("smoke.txt")).unwrap(), b"forced\n");
    let before = rig.harness.status(ready.token);
    assert_eq!(before.activity, Activity::Idle);
    let native = before.native.as_ref().unwrap();
    assert_eq!(native.phase, NativePhase::Ready);
    println!(
        "FORCED_SMOKE before: abort_bound={} minor={} waiting={} work={:?}",
        ready.receipt.abort_bound,
        connection.minor,
        connection.waiting(),
        native.work
    );
    let done = rig
        .harness
        .try_force(ready.token, false)
        .unwrap_or_else(|failure| panic!("forced unmount: {failure:?}"));
    let outcome = match &done.reply {
        Reply::ForceUnmounted(closed) => {
            assert_eq!(closed.token, ready.token);
            closed.outcome.clone()
        }
        other => panic!("{other:?}"),
    };
    println!("FORCED_SMOKE outcome={outcome:?}");
    // The drain receipt carries the loops' original exit and the one removal.
    println!("FORCED_SMOKE receipt={:?}", done.native);
    println!(
        "FORCED_SMOKE completions: close={} earlier={} cleanup_observation_failure={:?}",
        done.completion.is_some(),
        done.earlier.len(),
        done.observation_failure
    );
    assert_eq!(outcome.facts.abort, AbortDisposition::Written);
    assert_eq!(outcome.facts.detach, DetachDisposition::Detached);
    assert_eq!(outcome.facts.commit, CommitKnowledge::Absent);
    // Which cleanup state is read depends on maintenance turns; only that the
    // one observation was made is asserted.
    assert_ne!(outcome.cleanup, ForcedCleanup::Unobserved);
    assert!(done.observation_failure.is_none());
    assert!(done.completion.is_some(), "logical Close receipt");
    let work = outcome.work;
    assert_eq!((work.received, work.admitted, work.retained), (0, 0, 0));
    assert_eq!(work.loops_joined, work.loops_configured, "{work:?}");
    let receipt = format!("{:?}", done.native.as_ref().expect("drain receipt"));
    assert!(receipt.contains("Drained"), "{receipt}");
    // Independent kernel observations: no mount row, no directory, and the
    // test's own connection is gone from the control filesystem.
    assert!(mount_entry(&ready.directory).is_none());
    assert!(!mount.exists());
    until("own connection directory removed", || !connection.exists());
    // The incarnation left routing: every later call finds nothing.
    for request in [
        Request::ForceUnmount {
            token: ready.token,
            relinquish_unknown: false,
        },
        Request::Unmount(ready.token),
        Request::Status(ready.token),
        Request::Attach(ready.token),
    ] {
        assert!(
            matches!(
                rig.harness.service.execute_control(&request),
                Err(Failure::Rejected(ControlCode::Missing, _))
            ),
            "{request:?}"
        );
    }
    println!(
        "FORCED_SMOKE after: mount_row=None directory=absent own_connection=absent token=Missing cleanup={:?} fenced={}",
        outcome.cleanup, outcome.facts.fenced
    );
    drop(done);
    // Asserts no mount lane, queued, running or retained request remains.
    rig.finish();
}
