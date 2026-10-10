//! The actual `layerfs-daemon` executable through a changed Commit (the note
//! of row R5-1 and row R2-STEP-3): protected configuration, direct Store
//! installation, a logical bind, a native Attach, a change made by an
//! ordinary external bash process of the command identity in the reported
//! mount, one control Commit over the authenticated connection, the terminal
//! Unmount, and then a fresh Workspace of the same Branch attached and
//! compared with an independent oracle.
//!
//! Nothing here composes a product `Service` inside the test process: every
//! product step is the executable's own, reached over its control endpoint.
//!
//! The oracle is a native replay. A plain directory outside the daemon holds
//! the same base tree the fixture initialized the Store from, and the same
//! script is run in it by the same kind of process. The mounted tree must
//! equal it in names, kinds, permission bits, link counts, alias classes,
//! whole bytes and symlink targets, and in every modification time the
//! script set explicitly; a time the clock chose is compared only as
//! "chosen by the clock" (`rig::clock_masked`). Owner, `st_nlink`, `st_size`
//! and `st_ino` sharing are checked against the kernel's own table.
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
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::{
    control::{Activity, DaemonPhase, HelloRequest, ReadyMount, Reply, Request, WorkspaceToken},
    daemon_setup::{DaemonLimits, DaemonSetup},
    native,
};
use layerfs_history::{
    BranchId, CommitHistoryRequest, CommitRecord, CommitStagedOutcome, WorkspaceId,
};
use layerfs_sdk::{control::Control, ProjectApi, WorkspaceApi};
use model::{assert_same, Flat};
use mounted::COMMAND;
use rig::{assert_kernel, bash_in, clock_masked, clocked, kernel, native as tree, passed};
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    net::{SocketAddr, TcpStream},
    os::unix::{
        ffi::OsStrExt,
        fs::{lchown, symlink, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// The executable is killed and reaped when the test ends, on every path.
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
/// Never leaves a kernel mount behind: if the test unwinds before the
/// product's own Unmount, one lazy umount(8) launched and reaped here
/// detaches what is still there.
struct MountGuard(String);
impl Drop for MountGuard {
    fn drop(&mut self) {
        if mounted_at(&self.0) {
            let done = Command::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "R5-1-BINARY guard: lazy umount of {}: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// The kernel's own mount table, with no daemon cooperation.
fn mounted_at(directory: &str) -> bool {
    fs::read_to_string("/proc/self/mountinfo")
        .unwrap()
        .lines()
        .any(|line| line.split(' ').nth(4) == Some(directory))
}
fn connect(address: SocketAddr) -> native::Connection {
    native::initiate(
        support::socket(TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap()),
        &support::CLIENT_PRIVATE,
        native::public_key(&support::SERVER_PRIVATE).unwrap(),
    )
    .unwrap()
}
/// Starts the actual executable with its private configuration record and
/// returns it with the address it reported.
fn start(f: &support::Fixture, mounts: &Path) -> (Process, SocketAddr) {
    fs::set_permissions(&f.directory, fs::Permissions::from_mode(0o700)).unwrap();
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: support::SERVER_PRIVATE,
        control_peer: native::public_key(&support::CLIENT_PRIVATE).unwrap(),
        store: f.project.manifest.locator.clone(),
        overlay: f.directory.join("overlay.sqlite").to_str().unwrap().into(),
        mounts: mounts.to_str().unwrap().into(),
        command_uid: COMMAND,
        command_gid: COMMAND,
        limits: DaemonLimits {
            connections: 4,
            read_handles: 2,
            handshake_ms: 3000,
            cache_bytes: 64 * 1024,
            owner_bytes: 16 * 1024 * 1024,
            lifecycle_reserve: 2 * 1024 * 1024,
            namespaces: 8,
            ordinary_jobs: 8,
            lifecycle_jobs: 4,
            pager_kib: 1024,
            serial_low_water: 0,
        },
        existing_store: None,
    };
    let config = f.directory.join("daemon.config");
    fs::write(&config, setup.encode().unwrap()).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_layerfs-daemon"))
            .arg("--config")
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(
                File::create(f.directory.join("daemon.stderr")).unwrap(),
            ))
            .spawn()
            .unwrap(),
    );
    let stdout = process.0.stdout.take().unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout.take(256))
            .read_line(&mut line)
            .map(|_| line);
        let _ = send.send(result);
    });
    // If startup stops, Process Drop closes the child's pipe: this reader's
    // exit never depends on another thread finishing without panicking.
    let line = receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    reader.join().unwrap();
    let address = line
        .strip_prefix("LAYERFS_DAEMON_LISTEN ")
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    (process, address)
}
/// The fixture's base tree again, as a plain native directory of the command
/// identity. `support::Fixture::new` builds exactly these five names.
fn replay(root: &Path) {
    fs::create_dir(root).unwrap();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join("file"), base_file()).unwrap();
    fs::hard_link(root.join("file"), root.join("alias")).unwrap();
    fs::write(root.join(".git/index"), b"complete index").unwrap();
    symlink(OsStr::from_bytes(b"../opaque-\xff"), root.join(".git/link")).unwrap();
    for path in ["", ".git", "file", ".git/index", ".git/link"] {
        lchown(root.join(path), Some(COMMAND), Some(COMMAND)).unwrap();
    }
}
fn base_file() -> Vec<u8> {
    (0..support::FILE_BYTES).map(|n| (n % 251) as u8).collect()
}
/// The change, by an ordinary process: in the tree and inside `.git`; new,
/// changed, renamed, linked and removed names; explicit times.
const CHANGE: &str = r#"
set -euo pipefail
umask 022
mkdir -p made/sub
printf 'created through the real daemon\n' > made/new.txt
printf '\nappended by bash' >> .git/index
printf 'OVERWRITTEN' | dd of=file bs=1 seek=100000 conv=notrunc status=none
ln made/new.txt made/alias.txt
ln -s ../new.txt made/sub/link
mv -T .git/link made/moved-link
chmod 0600 made/new.txt
head -c 200000 /dev/zero | tr '\0' 'z' > made/large.bin
truncate -s 140001 made/large.bin
rm -f alias
touch -d '2001-02-03 04:05:06.123456789 UTC' made/large.bin
touch -d '2002-03-04 05:06:07 UTC' made/sub
"#;
/// Two consecutive observations of a connection with nothing received or
/// admitted and no request completed in between. A readiness wait before the
/// one Unmount, bounded; never a repeated attempt.
fn quiet(control: &mut Control, token: WorkspaceToken) {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last = None;
    loop {
        let work = WorkspaceApi::new(control)
            .status(token)
            .unwrap()
            .native
            .unwrap()
            .work
            .unwrap();
        let now = (work.received, work.admitted, work.completed);
        if work.received == 0 && work.admitted == 0 && last == Some(now) {
            return;
        }
        last = Some(now);
        assert!(
            Instant::now() < deadline,
            "the connection did not become quiet: {work:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
/// The terminal Unmount through the executable, and the kernel's table after.
fn unmount(control: &mut Control, ready: &ReadyMount) {
    quiet(control, ready.token);
    WorkspaceApi::new(control).unmount(ready.token).unwrap();
    assert!(
        !mounted_at(&ready.directory),
        "the mount is still in the kernel's table"
    );
}
fn history(control: &mut Control, branch: BranchId) -> Vec<CommitRecord> {
    let page = ProjectApi::new()
        .history(
            control,
            CommitHistoryRequest {
                branch,
                start: None,
                cursor: None,
                limit: 4,
            },
        )
        .unwrap();
    assert!(page.continuation.is_none());
    page.records
}
/// The comparison of a mounted tree with the replay: everything exactly,
/// except which instant the clock chose.
fn same(replayed: &Flat, mounted: &Flat, started: i64, what: &str) {
    assert_same(
        &clock_masked(replayed, started),
        &clock_masked(mounted, started),
        what,
    );
}

/// R5-1 (note) and R2-STEP-3.
#[test]
fn r5_1_the_actual_daemon_commits_a_change_by_external_bash_and_a_fresh_mount_is_exact() {
    let name = "R5-1-BINARY";
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 2;
    let f = support::Fixture::new("r5-1-binary", None);
    // The fixture directory is private to the daemon; the command identity
    // must reach the mount home and the replay, so both are beside it.
    let beside = |what: &str| -> PathBuf {
        std::env::temp_dir().join(format!("layerfs-r5-1-binary-{what}-{}", std::process::id()))
    };
    let (mounts, replayed) = (beside("mounts"), beside("replay"));
    let (process, address) = start(&f, &mounts);

    // Direct Store installation into the running executable, then readiness.
    let mut channel = connect(address);
    let installed = ProjectApi::new().install(&f.project, &mut channel).unwrap();
    assert!(installed.manifest.daemon_sqlite.is_some());
    let mut control = Control::new(channel);
    let hello = match control
        .call(Request::Hello(HelloRequest {
            expected_instance: None,
            wait_for_store: true,
        }))
        .unwrap()
    {
        Reply::Hello(hello) => hello,
        other => panic!("{other:?}"),
    };
    assert_eq!(hello.phase, DaemonPhase::ControlReady);
    assert_eq!(hello.store_sqlite, installed.manifest.daemon_sqlite);
    let branch = f.project.branch.branch.id;

    // Logical bind, then the native Attach.
    let bound = WorkspaceApi::new(&mut control)
        .bind(WorkspaceId::from_authority([150; 32]).unwrap(), branch)
        .unwrap();
    assert_eq!(bound.binding.branch.head_commit, None);
    let ready = WorkspaceApi::new(&mut control).attach(bound.token).unwrap();
    let guard = MountGuard(ready.directory.clone());
    assert!(mounted_at(&ready.directory));
    let mount = Path::new(&ready.directory);
    println!(
        "{name} STARTED executable={} pid={} listen={address} phase={:?} store_sqlite={} mount={} command_identity={COMMAND}:{COMMAND}",
        env!("CARGO_BIN_EXE_layerfs-daemon"),
        process.0.id(),
        hello.phase,
        hello.store_sqlite.as_deref().unwrap(),
        ready.directory
    );

    // The base, before any change: the mount is the replay of the base.
    replay(&replayed);
    let base = tree(mount);
    same(
        &tree(&replayed),
        &base,
        started,
        "the base on the first mount",
    );
    assert_eq!(base.len(), 6, "the root and five names");

    // The change, by an ordinary external bash process of the command
    // identity whose working directory is the reported mount; and the same
    // script in the replay.
    passed(&bash_in(COMMAND, mount, CHANGE), "the change in the mount");
    passed(
        &bash_in(COMMAND, &replayed, CHANGE),
        "the change in the replay",
    );
    let expected = tree(&replayed);
    assert_ne!(
        clock_masked(&expected, started),
        clock_masked(&base, started)
    );
    let live = tree(mount);
    same(&expected, &live, started, "the live mount after the change");
    // Bytes the test computes itself, not through the replay.
    let mut file = base_file();
    file[100_000..100_011].copy_from_slice(b"OVERWRITTEN");
    assert_eq!(live[b"file".as_slice()].payload, file);
    assert_eq!(
        live[b".git/index".as_slice()].payload,
        b"complete index\nappended by bash"
    );
    assert_eq!(
        live[b"made/large.bin".as_slice()].payload,
        vec![b'z'; 140_001]
    );
    assert_eq!(
        live[b"made/large.bin".as_slice()].mtime,
        (981_173_106, 123_456_789)
    );
    assert_eq!(live[b"made/sub".as_slice()].mtime, (1_015_218_367, 0));
    assert!(!live.contains_key(b"alias".as_slice()));

    // One control Commit over the authenticated connection.
    let before = WorkspaceApi::new(&mut control).status(bound.token).unwrap();
    assert_eq!(before.activity, Activity::Idle);
    let record = match WorkspaceApi::new(&mut control).commit(bound.token).unwrap() {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("{name}: the Commit replied {other:?}"),
    };
    assert_eq!(record.parent, None);
    assert_ne!(record.root, bound.binding.effective_root);
    let after = WorkspaceApi::new(&mut control).status(bound.token).unwrap();
    assert_eq!(
        (
            after.activity,
            after.binding.effective_root,
            after.binding.branch.head_commit
        ),
        (Activity::Idle, record.root, Some(record.id))
    );
    assert_eq!(history(&mut control, branch), [record.clone()]);
    same(
        &expected,
        &tree(mount),
        started,
        "the same mount after install",
    );
    println!(
        "{name} COMMITTED commit={:?} root={:?} parent=None history=1 same_mount=exact paths={}",
        record.id,
        record.root,
        expected.len()
    );

    // The terminal Unmount, by the executable.
    unmount(&mut control, &ready);
    drop(guard);

    // A fresh Workspace of the Branch selects the committed root; its mount
    // is the replay, and the kernel's own table agrees with the model.
    let fresh = WorkspaceApi::new(&mut control)
        .bind(WorkspaceId::from_authority([151; 32]).unwrap(), branch)
        .unwrap();
    assert_eq!(
        (
            fresh.binding.effective_root,
            fresh.binding.branch.head_commit
        ),
        (record.root, Some(record.id))
    );
    let again = WorkspaceApi::new(&mut control).attach(fresh.token).unwrap();
    let guard = MountGuard(again.directory.clone());
    assert_ne!(again.token, ready.token);
    let mount = Path::new(&again.directory);
    let seen = tree(mount);
    same(&expected, &seen, started, "the fresh mount of the Branch");
    assert_eq!(seen[b"file".as_slice()].payload, file);
    assert_kernel(&expected, &kernel(mount), "R5-1-BINARY fresh mount");
    let explicit = expected.len() - clocked(&expected, started);
    println!(
        "{name} FRESH_MOUNT workspace=new root=committed oracle=native_replay names_kinds_modes_links_aliases_bytes=exact explicit_times={explicit} clock_times={} kernel_table=owner,nlink,size,inode_sharing paths={}",
        clocked(&expected, started),
        expected.len()
    );
    // An unchanged fresh Workspace commits nothing new.
    assert!(matches!(
        WorkspaceApi::new(&mut control).commit(fresh.token).unwrap(),
        CommitStagedOutcome::UpToDate { head, root } if head == Some(record.id) && root == record.root
    ));
    assert_eq!(history(&mut control, branch), [record.clone()]);
    unmount(&mut control, &again);
    drop(guard);
    assert_eq!(
        control.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    println!(
        "{name} RESULT actual_executable=true attach=2 external_bash_change=true control_commit=Committed terminal_unmount=2 fresh_mount=exact up_to_date_after=true session_end=true"
    );
    // The process stop is by the test and is crash scope; both Workspaces
    // were closed by their own Unmounts above.
    drop(process);
    fs::remove_dir_all(&replayed).unwrap();
    fs::remove_dir(&mounts).unwrap();
    f.cleanup();
}
