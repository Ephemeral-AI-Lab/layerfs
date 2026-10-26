//! Real kernel mounts: FORGET delivered on a live connection with its exact
//! lookup decrement, and a terminal unmount that stops after effects and keeps
//! `Retained` custody instead of reporting a guessed success.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/host_install.rs"]
mod support;
use layerfs_bridge::control::{
    Activity, MountPhase, Reply, Request, TeardownStage, WorkspaceToken,
};
use layerfs_daemon::control::Failure;
use layerfs_history::BranchId;
use layerfs_workspace::Route;
use mounted::{mount_entry, until, Harness};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::Path,
    process::Command as Process,
};

/// Reclaim scoped to this container's own memory cgroup: the kernel evicts
/// unreferenced dentries and inodes charged to it, which is what makes it send
/// FORGET on a connection that stays mounted. No system-wide cache is dropped.
const RECLAIM: &str = "/sys/fs/cgroup/memory.reclaim";

fn harness(label: &str) -> (fixture::Fixture, Harness) {
    let f = fixture::Fixture::labeled(fixture::Shape::Mixed, label);
    let branch = BranchId::from_slice(&f.manifest.branch).unwrap();
    let h = Harness::new(f.opened.store.clone(), &f.directory, branch);
    (f, h)
}
/// The public owner route of one bound Workspace, taken while it is usable.
fn route(h: &Harness, token: WorkspaceToken) -> Route {
    h.service.operation(token).unwrap().workspace().route()
}
/// The kernel and descriptor ownership the engine keeps in memory: kernel
/// lookup entries, file custody, retained directory parents and open handles.
fn details(h: &Harness, through: WorkspaceToken) -> u64 {
    let open = h.engine_resources(through).open;
    open.kernel_entries + open.file_custody + open.directory_parents + open.file_handles
}
/// Every non-root path once by ordinary `lstat`: one positive LOOKUP each.
/// The names of each inode, and how many of the inodes are directories.
fn walk(f: &fixture::Fixture, root: &Path) -> (BTreeMap<u64, u64>, u64) {
    let mut references = BTreeMap::new();
    let mut directories = 0;
    for (path, _) in &f.metadata {
        if path.is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(root.join(path)).unwrap();
        let names = references.entry(metadata.ino()).or_insert(0);
        directories += u64::from(metadata.is_dir() && *names == 0);
        *names += 1;
    }
    (references, directories)
}

#[test]
fn kernel_forget_arrives_on_a_live_connection_with_its_exact_decrement() {
    // Declared precondition, not a product effect: the cgroup interface of
    // this container must be writable so reclaim can be requested for it.
    let remount = Process::new("mount")
        .args(["-o", "remount,rw", "/sys/fs/cgroup"])
        .output()
        .unwrap();
    assert!(
        remount.status.success() && Path::new(RECLAIM).exists(),
        "precondition: container-scoped {RECLAIM} is required: {remount:?}"
    );
    let (f, h) = harness("-forget");
    let helper = h.bind(1);
    let ready = h.mount(2);
    let token = ready.token;
    let root = Path::new(&ready.directory);
    let work = || h.status(token).kernel.unwrap().work.unwrap();
    let quiet = || {
        let work = work();
        work.active == 0
    };
    until("attach quiescent", quiet);
    let baseline = details(&h, token);
    assert_eq!(work().forget_units, 0);

    // 27 names over 26 inodes: the in-root hard link gives one inode two
    // kernel references, which the kernel must return in one FORGET unit.
    let (references, directories) = walk(&f, root);
    let names: u64 = references.values().sum();
    let inodes = references.len() as u64;
    // Each referenced inode has its kernel lookup entry and its file custody
    // in memory, and a directory its retained parent. No stored row stands
    // for a lookup reference.
    let custody = 2 * inodes + directories;
    assert_eq!((names, inodes), (27, 26));
    assert!(references.values().any(|count| *count == 2));
    until("lookups quiescent", quiet);
    let looked = details(&h, token);
    assert_eq!(
        looked - baseline,
        custody,
        "one kernel lookup entry per live kernel inode: {baseline:?} {looked:?} {:?}",
        h.engine_resources(token).open
    );

    // Stimulus, bounded: ask for more than the cgroup can give back so the
    // kernel walks its dentry and inode lists to the end. Each write is one
    // reclaim request to the kernel; no daemon operation is repeated.
    let mut requests = 0;
    until("every lookup owner returned by FORGET", || {
        requests += 1;
        let _ = fs::OpenOptions::new()
            .write(true)
            .open(RECLAIM)
            .and_then(|mut file| file.write_all(b"1G"));
        quiet() && details(&h, token) == baseline
    });
    let after = work();
    let returned = details(&h, token);
    println!(
        "KERNEL_FORGET names={names} inodes={inodes} reclaim_requests={requests} forget_units={} work={after:?} baseline={baseline:?} looked={looked:?} returned={returned:?}",
        after.forget_units
    );
    // Exactness: a short decrement leaves its row (the count above would not
    // return), an excess one is an underflow failure retained on the lane.
    assert_eq!(after.forget_units, inodes, "one unit per evicted inode");
    assert_eq!((after.retained, after.terminal), (0, 0));
    assert_eq!(returned, baseline);
    assert_eq!(h.phase(token), MountPhase::Ready);
    assert!(mount_entry(&ready.directory).is_some());

    // The connection is unchanged: the same names bind the same inodes again,
    // each through a new LOOKUP that reacquires exactly one reference.
    assert_eq!(walk(&f, root), (references, directories));
    until("second lookups quiescent", quiet);
    assert_eq!(
        details(&h, token) - baseline,
        custody,
        "reacquired after FORGET"
    );
    assert_eq!(
        fs::read(root.join(".cache/index-alias")).unwrap(),
        fixture::FILES[1].1
    );
    h.unmount(&ready);
    until("kernel ownership retired", || {
        h.engine(helper).namespaces == 1 && details(&h, helper) <= baseline
    });
    assert!(matches!(
        h.try_unmount(helper).unwrap().reply,
        Reply::Unmounted(_)
    ));
    h.stop();
    f.cleanup();
}

#[test]
fn an_unmount_of_an_already_revoked_mount_keeps_exact_retained_custody() {
    let (f, h) = harness("-retained");
    let ready = h.mount(2);
    let token = ready.token;
    let root = Path::new(&ready.directory);
    let mut file = fs::File::open(root.join("ignored.bin")).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, fixture::FILES[6].1);
    // The kernel sends the read's RELEASE after `close` returns. Its arrival
    // is observed as the descriptor leaving the engine, read through another
    // namespace.
    let open = details(&h, token);
    drop(file);
    until("the read's RELEASE disposed", || {
        let work = h.status(token).kernel.unwrap().work.unwrap();
        details(&h, token) < open && work.active == 0
    });
    let route = route(&h, token);
    // Stage an inconsistent lifecycle through the public owner port, as the
    // FORGET proof does for lookup counts. The real kernel connection remains
    // attached; its terminal teardown must retain the original stale Revoke.
    let mount = h
        .owner
        .client()
        .kernel_retained_mount(route)
        .unwrap()
        .unwrap();
    h.owner.client().kernel_revoke(route, mount).unwrap();

    let custody = match h.try_unmount(token) {
        Err(Failure::Retained(custody)) => *custody,
        other => panic!("{other:?}"),
    };
    println!("KERNEL_RETAINED custody={custody:?}");
    assert_eq!(custody.token, token);
    assert_eq!(custody.stage, TeardownStage::Revoke);
    assert!(custody.detached, "the kernel detach is known");
    let work = custody.work.expect("counters at the stopping boundary");
    assert_eq!(
        (work.loops_configured, work.loops_joined),
        (
            layerfs_fuse::RECEIVE_SLOTS as u16,
            layerfs_fuse::RECEIVE_SLOTS as u16
        ),
        "{work:?}"
    );
    assert_eq!((work.active, work.retained), (0, 0));
    assert!(!custody.detail.is_empty());
    // Effects that happened stay happened; nothing is reported as unmounted.
    assert!(mount_entry(&ready.directory).is_none());
    let status = h.status(token);
    let kernel = status.kernel.unwrap();
    assert_eq!(kernel.phase, MountPhase::Retained);
    assert!(kernel.detached);
    assert_eq!(kernel.ready.as_ref(), Some(&ready));
    assert_eq!(status.activity, Activity::Closing);
    let kept = h.service.retained_kernel(token).unwrap().expect("owner");
    assert!(
        kept.contains("Revoke") && kept.contains("Drained"),
        "{kept}"
    );
    // Later operations answer with the same custody and attempt nothing.
    for request in [
        Request::Unmount(layerfs_bridge::control::UnmountControl {
            token,
            discard: true,
        }),
        Request::Attach(token),
    ] {
        match h.service.execute_control(&request) {
            Err(Failure::Retained(again)) => assert_eq!(*again, custody, "{request:?}"),
            other => panic!("{request:?}: {other:?}"),
        }
    }
    assert!(h
        .service
        .execute_control(&Request::Locate(token.workspace))
        .is_ok());
    // The original revoked state remains; retained control custody performs
    // no guessed replay or additional revocation.
    assert_eq!(
        h.owner.client().kernel_state(route, mount).unwrap(),
        layerfs_overlay::KernelMountState::Revoked
    );
    assert_eq!(h.phase(token), MountPhase::Retained);
    match h.try_unmount(token) {
        Err(Failure::Retained(again)) => assert_eq!(*again, custody),
        other => panic!("{other:?}"),
    }
    h.stop_retained();
    f.cleanup();
}
