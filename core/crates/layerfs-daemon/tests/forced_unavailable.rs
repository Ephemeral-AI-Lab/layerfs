//! Real kernel mount with no control filesystem: a connection attached without
//! an abort control refuses forced teardown before any effect. This binary
//! never mounts fusectl, so that it stays absent for its one test.
#![cfg(target_os = "linux")]
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
use layerfs_bridge::control::{Activity, ControlCode, NativePhase};
use layerfs_daemon::control::Failure;
use mounted::{mount_entry, COMMAND};
use rig::{bash_in, passed, root, Rig};
use std::fs;

#[test]
fn force_without_an_abort_control_is_refused_and_the_workspace_stays_ready() {
    let table = fs::read_to_string("/proc/self/mountinfo").unwrap();
    assert!(
        !table.lines().any(|line| line.contains(" - fusectl ")),
        "the control filesystem must not be mounted here"
    );
    let rig = Rig::new("forced-unavailable");
    let ready = rig.mount(1);
    assert!(!ready.receipt.abort_bound, "{:?}", ready.receipt);
    let mount = root(&ready).to_owned();
    passed(
        &bash_in(COMMAND, &mount, "printf 'kept\\n' > kept.txt"),
        "external write",
    );
    let before = rig.harness.status(ready.token);
    for relinquish_unknown in [false, true] {
        match rig.harness.try_force(ready.token, relinquish_unknown) {
            Err(Failure::Native(refusal)) => {
                assert_eq!(refusal.code, ControlCode::Failed, "{refusal:?}");
                assert_eq!(refusal.phase, "force:capability");
                assert!(refusal.completions.is_empty() && refusal.evidence.is_none());
            }
            other => panic!("{other:?}"),
        }
    }
    // No effect: the same serving connection, activity and epoch.
    let after = rig.harness.status(ready.token);
    let native = after.native.as_ref().unwrap();
    assert_eq!(native.phase, NativePhase::Ready);
    assert!(!native.detached);
    assert_eq!(after.activity, Activity::Idle);
    assert_eq!(after.epoch, before.epoch);
    assert!(mount_entry(&ready.directory).is_some());
    // The mount still serves an ordinary process and this one.
    assert_eq!(fs::read(mount.join("kept.txt")).unwrap(), b"kept\n");
    let script = "cat kept.txt && printf 'more\\n' > later.txt";
    let read = bash_in(COMMAND, &mount, script);
    passed(&read, "read and write after the refusal");
    assert_eq!(read.stdout, b"kept\n");
    println!(
        "FORCED_UNAVAILABLE abort_bound=false code=Failed phase=force:capability relinquish_unknown=both native_phase={:?} activity={:?} epoch_unchanged={} mount_row=present read_after=exact write_after=ok",
        native.phase,
        after.activity,
        after.epoch == before.epoch
    );
    // The normal path is unaffected: complete drain and Close.
    let receipt = rig.unmount(&ready);
    assert!(receipt.contains("Drained"), "{receipt}");
    rig.finish();
}
