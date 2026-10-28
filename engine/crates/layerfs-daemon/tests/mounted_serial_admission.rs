//! Initial serial reservation is prepared before Ready, without a create-time
//! wait. Two ordinary processes create while an independent Store writer is
//! held. Commit Begin must still answer Busy. All observations are counts;
//! subprocesses and the writer have independent finite lifetimes.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/held_writer.rs"]
mod held_writer;
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
use held_writer::HeldWriter;
use layerfs_daemon::{
    control::Failure,
    store::{CommitError, CommitPhase},
};
use layerfs_storage::StorageError;
use mounted::COMMAND;
use rig::{root, Rig};
use std::{
    fs,
    os::unix::{fs::MetadataExt, process::CommandExt},
    process::{Command, Stdio},
};

#[test]
fn two_ready_workspaces_create_with_no_store_write_demand() {
    let rig = Rig::new("issue319-initial-serials");
    rig.store.set_serial_low_water(256);
    let a = rig.mount(1);
    let b = rig.mount(2);
    let after_ready = rig.store.work().serial_reservations;
    let held = HeldWriter::acquire(&rig.fixture.config.path);
    // timeout(1) bounds each child independently; no child waits for its peer.
    let spawn = |directory: &std::path::Path| {
        Command::new("timeout").args(["2", "python3", "-c",
            "import os; f=os.open('created',os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o644); os.write(f,b'new'); os.close(f)"])
            .current_dir(directory).uid(COMMAND).gid(COMMAND)
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().unwrap()
    };
    let ca = spawn(root(&a));
    let cb = spawn(root(&b));
    let oa = ca.wait_with_output().unwrap();
    let ob = cb.wait_with_output().unwrap();
    let after_creates = rig.store.work().serial_reservations;
    let ia = fs::metadata(root(&a).join("created")).map(|m| m.ino()).ok();
    let ib = fs::metadata(root(&b).join("created")).map(|m| m.ino()).ok();
    let commit = rig.commit(a.token);
    let begin_busy = matches!(&commit, Err(Failure::Commit(f))
        if f.phase == CommitPhase::Begin && matches!(f.error, CommitError::Storage(StorageError::Busy)));
    println!("INITIAL_SERIALS ready_reservations={after_ready} create_reservations={} a={} b={} inode_a={ia:?} inode_b={ib:?} begin_busy={begin_busy}",
        after_creates-after_ready, oa.status, ob.status);
    held.release();
    drop(commit);
    rig.unmount(&a);
    rig.unmount(&b);
    rig.finish();
    assert!(
        oa.status.success(),
        "first create: {}",
        String::from_utf8_lossy(&oa.stderr)
    );
    assert!(
        ob.status.success(),
        "second create: {}",
        String::from_utf8_lossy(&ob.stderr)
    );
    assert_eq!(
        after_ready, 2,
        "one original reservation per Ready Workspace"
    );
    assert_eq!(
        after_creates, after_ready,
        "first creates consume local serials"
    );
    assert!(
        ia.is_some() && ib.is_some() && ia != ib,
        "distinct authoritative serials"
    );
    assert!(
        begin_busy,
        "Commit Begin keeps its one-attempt Busy refusal"
    );
}

#[test]
fn kernel_create_reservations_are_windows_not_per_file() {
    let rig = Rig::new("issue319-serial-counts");
    rig.store.set_serial_low_water(256);
    let ready = rig.mount(1);
    let start = rig.store.work().serial_reservations;
    let output = Command::new("timeout").args(["25", "python3", "-c",
        "import os\nfor i in range(2050):\n f=os.open('serial-%04d'%i,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o644); os.close(f)\n"])
        .uid(COMMAND).gid(COMMAND).current_dir(root(&ready)).output().unwrap();
    let end = rig.store.work().serial_reservations;
    println!("SERIAL_WINDOW_COUNTS creates=2050 ready_reservations={start} create_reservations={} total_reservations={end} refill=1024 low_water=256",end-start);
    rig.unmount(&ready);
    rig.finish();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Before the repair: 0 at Ready and 3 during creates. After it: 1 and 2.
    assert_eq!(end, 3, "unchanged allocation count and window size");
}

#[test]
fn serial_writer_hold_has_a_real_inclusive_upper_observation() {
    use layerfs_history::{HistoryCatalog, ReserveRequest};
    use layerfs_persistence::{Handles, SqlitePersistenceProfile};
    let rig = Rig::new("issue319-writer-upper");
    assert_eq!(
        rig.fixture.config.sqlite_profile,
        SqlitePersistenceProfile::Disposable
    );
    let handles = Handles::open_writable(
        rig.fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
    )
    .unwrap();
    let scope = handles
        .history
        .branch_snapshot(rig.fixture.branch)
        .unwrap()
        .unwrap()
        .scope;
    let mut maximum = 0;
    for original in 1..=8 {
        let before = handles.diagnostics().unwrap();
        let range = handles
            .history
            .reserve_inodes(&ReserveRequest { scope, count: 1024 })
            .unwrap();
        let after = handles.diagnostics().unwrap();
        let upper = after.transaction_ns - before.transaction_ns;
        maximum = maximum.max(upper);
        assert_eq!(after.write_transactions - before.write_transactions, 1);
        assert_eq!(range.count, 1024);
        println!("SERIAL_WRITER_OBSERVATION original={original} write_transactions=1 inclusive_upper_ns={upper}");
    }
    println!("SERIAL_WRITER_OBSERVATION maximum_inclusive_upper_ns={maximum} exclusive_hold_ns=UNAVAILABLE bound_used=false admission=earlier_reservation");
    drop(handles);
    rig.finish();
}

#[test]
fn busy_initial_reservation_has_no_kernel_mount_effect() {
    use layerfs_history::HistoryError;
    let rig = Rig::new("issue319-attach-busy");
    let token = rig.harness.bind(1);
    let held = HeldWriter::acquire(&rig.fixture.config.path);
    let attach = rig.harness.try_attach(token);
    let busy = matches!(&attach, Err(Failure::History(error)) if matches!(error.cause(),HistoryError::Busy));
    let status = rig.harness.status(token);
    let unattached = status
        .kernel
        .as_ref()
        .is_some_and(|value| value.phase == layerfs_bridge::control::MountPhase::Unattached);
    held.release();
    drop(attach);
    rig.harness.try_unmount(token).unwrap();
    rig.finish();
    assert!(busy, "initial reservation preserves exact Busy");
    assert!(unattached, "no engine/kernel mount after initial Busy");
}
