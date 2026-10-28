//! Real kernel mounts with the kernel's FUSE control filesystem mounted by
//! the test before Attach: the fusectl half of FP-2, and FP-9.
//!
//! SAFETY RULE (`support/fusectl.rs`). `/sys/fs/fuse/connections` lists the
//! connections of other, protected containers. Nothing here writes under it,
//! lists it or reads another connection: a `fusectl::Connection` is built
//! from one mount's own Ready receipt after its device was checked against
//! the test's own mount-table row, and only that connection's `waiting`,
//! `max_background` and `congestion_threshold` are read.
//!
//! FP-9 staging, hook-free. The rig's Store has a fixed read set of two
//! readers; the test leases both through the Store's public admission
//! (`support/holds.rs`), so a request that needs the Store is parked by the
//! request service and nothing is blocked inside a provider call. Two
//! external python3 processes of the command identity each open one large
//! base file before the hold and, when told, read it through the page cache
//! (no `O_DIRECT`). The files were never read, so the first read of each is a
//! page-cache miss that the kernel fills by readahead.
//!
//! What is observed, by counts: the connection's own `waiting` (requests the
//! kernel holds for it, in flight or queued), the mount's maintained
//! `handoffs`, `admitted`, `parked`, `received` and `completed` through
//! control Status, the Store's read-admission counters, and each process's
//! own output and exit status. The READ opcode count of the mount is exposed
//! only by the drain receipt, so during the hold "one READ" is one handoff
//! since a quiescent baseline whose only issuer is a process blocked in one
//! `read(2)` inside its file's size, with one request waiting for a Store
//! reader; the receipt's READ count is printed at the end. Nothing is timed.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/fusectl.rs"]
mod fusectl;
#[allow(dead_code)]
#[path = "support/holds.rs"]
mod holds;
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
use layerfs_bridge::control::{MountPhase, MountWork, WorkspaceToken};
use mounted::{mount_entry, COMMAND};
use rig::{bash_in, passed, pattern, root, stamp_tree, Rig};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::Command as Process,
    thread,
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the tests.
const WAIT: Duration = Duration::from_secs(5);
/// Each large base file: several readahead windows of 128 KiB.
const LARGE: usize = 600 * 1024 + 123;
const SEEDS: [u8; 2] = [11, 29];

fn within(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "bounded observation expired: {what}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
/// Never leaves a kernel mount behind: if the test unwinds, one lazy
/// umount(8) launched and reaped by the test detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "FUSECTL_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}

fn work(rig: &Rig, token: WorkspaceToken) -> MountWork {
    rig.harness.status(token).kernel.unwrap().work.unwrap()
}
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between.
fn quiet(rig: &Rig, token: WorkspaceToken) {
    let mut last = None;
    within("connection quiescent", || {
        let now = work(rig, token);
        let settled = now.active == 0 && last == Some(now.completed);
        last = Some(now.completed);
        thread::sleep(Duration::from_millis(10));
        settled
    });
}
/// A base of two large files under `big/`, each with its own bytes.
fn large_rig(label: &str) -> Rig {
    Rig::built(label, |source| {
        fs::create_dir(source.join("big")).unwrap();
        for (n, seed) in SEEDS.into_iter().enumerate() {
            let path = source.join(large(n));
            fs::write(&path, pattern(seed, LARGE)).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        stamp_tree(source);
    })
}
fn large(n: usize) -> String {
    format!("big/file-{n}")
}

/// FP-2, fusectl half: with the control filesystem mounted before Attach the
/// connection's abort control is bound, and the kernel's own values for the
/// test's connection equal the Ready receipt. Two mounts of one daemon are
/// two connections, each agreeing with its own receipt. The observed maximum
/// request size of the row is not measured here.
#[test]
fn fp2_the_own_connection_agrees_with_the_ready_receipt_and_its_abort_control_is_bound() {
    fusectl::mount();
    assert!(fusectl::mounted());
    let rig = large_rig("fp2-fusectl");
    let first = rig.mount(1);
    let second = rig.mount(2);
    let guards = [
        Mounted(first.directory.clone()),
        Mounted(second.directory.clone()),
    ];
    let mut connections = Vec::new();
    for ready in [&first, &second] {
        let receipt = ready.receipt;
        assert!(receipt.abort_bound, "{receipt:?}");
        let entry = mount_entry(&ready.directory).expect("kernel mount table entry");
        assert_eq!(
            (entry.filesystem.as_str(), entry.source.as_str()),
            ("fuse", "layerfs"),
            "{entry:?}"
        );
        // Panics unless the mount-table device of this directory equals the
        // receipt's; only then is `<minor>/` of the control filesystem read.
        let connection = fusectl::own(ready);
        assert_eq!(connection.minor, receipt.device_minor);
        assert!(connection.exists(), "own connection directory");
        let (background, congestion) = (
            connection.max_background(),
            connection.congestion_threshold(),
        );
        assert_eq!(
            (background, congestion),
            (
                u64::from(receipt.max_background),
                u64::from(receipt.congestion_threshold)
            ),
            "the kernel's limits of connection {} against {receipt:?}",
            connection.minor
        );
        assert_eq!(
            (receipt.max_background, receipt.congestion_threshold),
            (1, 1),
            "the negotiated profile"
        );
        let kernel = rig.harness.status(ready.token).kernel.unwrap();
        assert_eq!(kernel.phase, MountPhase::Ready);
        assert_eq!(kernel.ready.as_ref(), Some(ready), "the same receipt");
        quiet(&rig, ready.token);
        println!(
            "FP-2 fusectl: mount={} device={}:{} abort_bound={} receipt(max_background={} congestion_threshold={}) fusectl(max_background={background} congestion_threshold={congestion} waiting={})",
            ready.directory,
            receipt.device_major,
            receipt.device_minor,
            receipt.abort_bound,
            receipt.max_background,
            receipt.congestion_threshold,
            connection.waiting()
        );
        connections.push(connection);
    }
    assert_ne!(
        connections[0].minor, connections[1].minor,
        "two mounts are two connections"
    );
    // A served request leaves the idle connection with nothing waiting.
    for (n, ready) in [&first, &second].into_iter().enumerate() {
        let listed = bash_in(COMMAND, root(ready), "ls big");
        passed(&listed, "list through the mount");
        assert_eq!(listed.stdout, b"file-0\nfile-1\n");
        quiet(&rig, ready.token);
        within("nothing waiting on an idle connection", || {
            connections[n].waiting() == 0
        });
    }
    // Normal unmount of one connection removes its own control directory and
    // leaves the other's values as they were. The abort control is not used.
    rig.unmount(&first);
    within("own connection directory removed", || {
        !connections[0].exists()
    });
    assert!(connections[1].exists());
    assert_eq!(
        (
            connections[1].max_background(),
            connections[1].congestion_threshold()
        ),
        (
            u64::from(second.receipt.max_background),
            u64::from(second.receipt.congestion_threshold)
        )
    );
    rig.unmount(&second);
    within("own connection directory removed", || {
        !connections[1].exists()
    });
    println!(
        "FP-2 fusectl: connections={:?} both_removed_after_normal_unmount=true observed_maximum_request_size=NOT_RUN",
        connections.iter().map(|c| c.minor).collect::<Vec<_>>()
    );
    drop(guards);
    rig.finish();
}
