//! Real kernel mounts: Ready evidence, complete-root reads through ordinary
//! filesystem calls, remaining refusals, reversible Busy and full normal drain.
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
#[path = "support/mutating.rs"]
mod mutating;
#[allow(dead_code)]
#[path = "support/host_install.rs"]
mod support;
use layerfs_bridge::control::{Activity, ControlCode, MountPhase, Reply, Request};
use layerfs_daemon::control::{Failure, Service};
use layerfs_history::BranchId;
use mounted::{mount_entry, until, Harness, COMMAND};
use nix::{errno::Errno, fcntl::OFlag};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{FileExt, MetadataExt, OpenOptionsExt},
        process::CommandExt,
    },
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
};

fn harness(label: &str) -> (fixture::Fixture, Harness) {
    let f = fixture::Fixture::labeled(fixture::Shape::Mixed, label);
    let branch = BranchId::from_slice(&f.manifest.branch).unwrap();
    let h = Harness::new(f.opened.store.clone(), &f.directory, branch);
    (f, h)
}
fn errno(error: &io::Error) -> Errno {
    Errno::from_raw(error.raw_os_error().unwrap())
}
/// Every path of the kernel source and nothing else, with exact metadata,
/// bytes, raw link targets and identity. The oracle is the fixture's own
/// record of what was initialized; the source tree itself no longer exists.
fn complete_root(f: &fixture::Fixture, root: &Path) -> (usize, u64) {
    let mut children: BTreeMap<String, BTreeSet<OsString>> = BTreeMap::new();
    for (path, _) in &f.metadata {
        if path.is_empty() {
            continue;
        }
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        children
            .entry(parent.to_owned())
            .or_default()
            .insert(name.into());
    }
    let mut regular = 0;
    for (path, expected) in &f.metadata {
        let actual = fs::symlink_metadata(root.join(path)).unwrap();
        assert_eq!(actual.mode() & 0o7777, expected.mode, "{path} mode");
        assert_eq!(
            (actual.mtime(), actual.mtime_nsec() as u32),
            (expected.mtime_seconds, expected.mtime_nanoseconds),
            "{path} mtime"
        );
        assert_eq!((actual.uid(), actual.gid()), (COMMAND, COMMAND), "{path}");
        if actual.is_dir() {
            let listed: BTreeSet<OsString> = fs::read_dir(root.join(path))
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            assert_eq!(
                listed,
                children.get(path).cloned().unwrap_or_default(),
                "{path} complete listing"
            );
        }
    }
    for (path, body) in fixture::FILES
        .into_iter()
        .chain([(".cache/index-alias", fixture::FILES[1].1)])
        .chain([("output/index-copy", fixture::FILES[1].1)])
    {
        assert_eq!(fs::read(root.join(path)).unwrap(), body, "{path}");
        assert_eq!(
            fs::metadata(root.join(path)).unwrap().len(),
            body.len() as u64
        );
        regular += body.len() as u64;
    }
    for (path, target) in fixture::LINKS {
        assert_eq!(
            fs::read_link(root.join(path))
                .unwrap()
                .into_os_string()
                .into_vec(),
            target,
            "{path}"
        );
    }
    // One inode behind both in-root names; an equal-content copy is distinct.
    let index = fs::metadata(root.join(".git/index")).unwrap();
    let alias = fs::metadata(root.join(".cache/index-alias")).unwrap();
    let copy = fs::metadata(root.join("output/index-copy")).unwrap();
    assert_eq!((index.ino(), index.nlink()), (alias.ino(), alias.nlink()));
    assert!(index.nlink() >= 2);
    assert_ne!(index.ino(), copy.ino());
    assert_eq!(fs::metadata(root).unwrap().ino(), 1);
    // Outside-root kernel neighbours never entered the Workspace.
    for absent in ["external", "outside-index", "missing"] {
        assert_eq!(
            errno(&fs::symlink_metadata(root.join(absent)).unwrap_err()),
            Errno::ENOENT
        );
    }
    (f.metadata.len(), regular)
}
/// An ordinary process that was never registered with the daemon: launched
/// here, under the runtime's nonroot command identity, by plain exec.
fn command(identity: u32, program: &str, argument: &Path) -> std::process::Output {
    Command::new(program)
        .arg(argument)
        .uid(identity)
        .gid(identity)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// The kernel lookup entries and file custody the engine keeps in memory.
fn lookups(h: &Harness, through: layerfs_bridge::control::WorkspaceToken) -> u64 {
    let open = h.engine_resources(through).open;
    open.kernel_entries + open.file_custody
}
#[test]
fn ready_mount_serves_the_complete_root_to_unregistered_access_then_drains() {
    let (f, h) = harness("-ready");
    let helper = h.bind(1);
    let token = h.bind(2);
    assert_eq!(h.phase(token), MountPhase::Unattached);
    let before = lookups(&h, token);
    let ready = h.attach(token);
    let root = Path::new(&ready.directory);
    assert_eq!(root, h.mounts.join(token.namespace.to_string()));
    // Ready evidence: kernel mount, negotiated profile and the loop serving.
    let receipt = ready.receipt;
    // The mount's declared receive workers.
    const LOOPS: u16 = layerfs_fuse::RECEIVE_SLOTS as u16;
    assert_eq!(receipt.loops, LOOPS);
    assert_eq!(receipt.abi_major, 7);
    assert_eq!((receipt.max_write, receipt.max_readahead), (131072, 131072));
    assert_eq!(
        (receipt.max_background, receipt.congestion_threshold),
        (1, 1)
    );
    assert_eq!(receipt.selected & !receipt.offered, 0);
    assert!(receipt.mount > 0 && receipt.mount_id > 0 && receipt.root > 0);
    let entry = mount_entry(&ready.directory).expect("kernel mount table entry");
    assert_eq!(entry.filesystem, "fuse");
    assert_eq!(entry.source, "layerfs");
    for option in ["nosuid", "nodev", "noatime"] {
        assert!(entry.options.split(',').any(|o| o == option), "{entry:?}");
    }
    for option in [
        "user_id=0",
        "group_id=0",
        "default_permissions",
        "allow_other",
        "max_read=131072",
    ] {
        assert!(
            entry.super_options.split(',').any(|o| o == option),
            "{entry:?}"
        );
    }
    let status = h.status(token);
    assert_eq!(status.activity, Activity::Idle);
    let kernel = status.kernel.unwrap();
    assert_eq!(kernel.phase, MountPhase::Ready);
    assert_eq!(kernel.ready.as_ref(), Some(&ready));
    let work = kernel.work.unwrap();
    assert_eq!(
        (work.loops_configured, work.loops_entered, work.loops_exited),
        (LOOPS, LOOPS, 0)
    );
    assert_eq!((work.active, work.retained), (0, 0));

    let (paths, regular) = complete_root(&f, root);
    mutating::refusals(root);
    // External access needs no Exec registration: a separate nonroot process
    // reads by ordinary syscalls; kernel permission checks decide access.
    let listed = command(COMMAND, "ls", &root.join("node_modules/pkg"));
    assert!(listed.status.success(), "{listed:?}");
    assert_eq!(listed.stdout, b"index.js\n");
    let read = command(COMMAND, "cat", &root.join(".git/objects/aa/object"));
    assert_eq!(read.stdout, b"git-object", "{read:?}");
    let other = command(COMMAND - 1, "cat", &root.join(".git/index"));
    assert!(!other.status.success(), "mode 0640 is kernel-enforced");
    // The same unregistered process mutates through the same mount.
    let created = command(COMMAND, "touch", &root.join("created-by-command"));
    assert!(created.status.success(), "{created:?}");
    let made = fs::metadata(root.join("created-by-command")).unwrap();
    assert_eq!((made.uid(), made.len()), (COMMAND, 0));
    let removed = command(COMMAND, "rm", &root.join("created-by-command"));
    assert!(removed.status.success(), "{removed:?}");
    // The command identity cannot reach the daemon's backing files.
    let backing = command(COMMAND, "cat", &f.directory.join("overlay.sqlite"));
    println!("BACKING_READ_BY_COMMAND status={:?}", backing.status.code());

    let served = h.status(token).kernel.unwrap().work.unwrap();
    // FLUSH, FSYNC and FSYNCDIR are refused once each with ENOSYS, beside
    // the three refusals of `mutating::refusals`; none is an inline reply.
    assert!(served.served > 0 && served.refused >= 6, "{served:?}");
    assert_eq!((served.terminal, served.retained), (0, 0));
    assert!(lookups(&h, token) > before);
    println!(
        "KERNEL_READY paths={paths} regular_bytes={regular} receipt={receipt:?} entry={entry:?} served={served:?}"
    );

    // Reversible probe: each retained reference makes the kernel answer Busy,
    // the Workspace stays Ready and no terminal error reaches any holder.
    let held = File::open(root.join(".git/index")).unwrap();
    let busy = |what: &str| match h.try_unmount(token) {
        Err(Failure::Kernel(failure)) => {
            assert_eq!(
                (failure.code, failure.phase),
                (ControlCode::Busy, "unmount:kernel"),
                "{what}"
            );
            let status = h.status(token);
            assert_eq!(status.activity, Activity::Idle);
            let kernel = status.kernel.unwrap();
            assert_eq!(kernel.phase, MountPhase::Ready);
            assert!(!kernel.detached);
            assert_eq!(kernel.ready.unwrap().receipt, receipt, "same connection");
            assert_eq!(
                fs::read(root.join("ignored.bin")).unwrap(),
                fixture::FILES[6].1
            );
        }
        other => panic!("{what}: {other:?}"),
    };
    busy("open descriptor");
    let mut bytes = [0; 9];
    held.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes, b"git-index");
    drop(held);
    let path_only = OpenOptions::new()
        .read(true)
        .custom_flags(OFlag::O_PATH.bits())
        .open(root.join("output"))
        .unwrap();
    busy("O_PATH descriptor");
    drop(path_only);
    let mut resident = Command::new("sleep")
        .arg("30")
        .current_dir(root.join("output/build"))
        .uid(COMMAND)
        .gid(COMMAND)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    busy("working directory of a running command");
    // The command is the runtime's to end; the filesystem killed nothing.
    assert!(resident.try_wait().unwrap().is_none());
    resident.kill().unwrap();
    resident.wait().unwrap();

    // Cached kernel lookups and idle receive loops are not busy.
    let held = lookups(&h, token);
    let done = h.unmount(&ready);
    println!(
        "KERNEL_UNMOUNT lookup_owner_details={held} receipt={:?}",
        done.kernel
    );
    for request in [Request::Status(token), Request::Attach(token)] {
        assert!(matches!(
            h.service.execute_control(&request),
            Err(Failure::Rejected(ControlCode::Missing, _))
        ));
    }
    assert!(matches!(
        h.service.execute_control(&Request::Locate(token.workspace)),
        Err(Failure::Rejected(ControlCode::Missing, _))
    ));
    // Indexed retirement and namespace deletion finish as bounded owner work.
    until("kernel ownership retired", || {
        h.engine(helper).namespaces == 1 && lookups(&h, helper) <= before
    });
    assert!(matches!(
        h.try_unmount(helper).unwrap().reply,
        Reply::Unmounted(_)
    ));
    h.stop();
    f.cleanup();
}

#[test]
fn attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached() {
    let (f, h) = harness("-refusal");
    let helper = h.bind(1);
    let token = h.bind(2);
    // A control-only Service has no kernel half to report or attach.
    let plain = Service::new(f.opened.store.clone(), &h.owner);
    assert!(matches!(
        plain.execute_control(&Request::Attach(token)),
        Err(Failure::Rejected(ControlCode::Invalid, _))
    ));
    drop(plain);
    let stale = layerfs_bridge::control::WorkspaceToken {
        namespace: token.namespace + 1000,
        ..token
    };
    assert!(matches!(
        h.try_attach(stale),
        Err(Failure::Rejected(ControlCode::Invalid, _))
    ));
    // The mount directory must be created by this attach; an existing path is
    // refused before any device or mount effect.
    let directory = h.mounts.join(token.namespace.to_string());
    fs::create_dir(&directory).unwrap();
    match h.try_attach(token) {
        Err(Failure::Kernel(failure)) => {
            assert_eq!(
                (failure.code, failure.phase),
                (ControlCode::Failed, "attach:mount"),
                "{failure:?}"
            );

            let evidence = format!("{:?}", failure.evidence);
            assert!(evidence.contains("Directory") && evidence.contains("mounted: false"));
        }
        other => panic!("{other:?}"),
    }
    assert!(mount_entry(directory.to_str().unwrap()).is_none());
    assert!(directory.exists(), "a foreign directory is never removed");
    let status = h.status(token);
    assert_eq!(status.activity, Activity::Idle);
    assert_eq!(status.kernel.unwrap().phase, MountPhase::Unattached);
    fs::remove_dir(&directory).unwrap();
    // The released engine owner retires as bounded maintenance; a later
    // explicit Attach is a new operation with a new mount incarnation.
    until("failed attach owner retired", || {
        h.engine(helper).maintenance_targets == 0
    });
    let ready = h.attach(token);
    assert_eq!(Path::new(&ready.directory), directory);
    match h.try_attach(token) {
        Err(Failure::Kernel(failure)) => assert_eq!(
            (failure.code, failure.phase),
            (ControlCode::Invalid, "attach:admission")
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(h.status(token).kernel.unwrap().ready, Some(ready.clone()));
    assert_eq!(
        fs::read(directory.join("node_modules/pkg/index.js")).unwrap(),
        fixture::FILES[3].1
    );
    assert_eq!(
        OsString::from(directory.join("opaque").read_link().unwrap()).as_bytes(),
        fixture::LINKS[4].1
    );
    h.unmount(&ready);
    h.try_unmount(helper).unwrap();
    h.stop();
    f.cleanup();
}

#[test]
fn two_workspaces_mount_and_unmount_independently() {
    let (f, h) = harness("-shared");
    let first = h.mount(3);
    let second = h.mount(4);
    assert_ne!(first.directory, second.directory);
    assert_ne!(first.receipt.mount_id, second.receipt.mount_id);
    assert_ne!(first.receipt.device_minor, second.receipt.device_minor);
    let roots = [first.directory.clone(), second.directory.clone()];
    let fixture = Arc::new(f);
    let readers: Vec<_> = roots
        .into_iter()
        .map(|root| {
            let fixture = fixture.clone();
            std::thread::spawn(move || {
                for _ in 0..8 {
                    complete_root(&fixture, Path::new(&root));
                }
            })
        })
        .collect();
    for reader in readers {
        reader.join().unwrap();
    }
    // One mount's terminal drain leaves the other fully usable.
    h.unmount(&first);
    assert_eq!(h.phase(second.token), MountPhase::Ready);
    complete_root(&fixture, Path::new(&second.directory));
    h.unmount(&second);
    h.stop();
    Arc::try_unwrap(fixture).ok().unwrap().cleanup();
}
