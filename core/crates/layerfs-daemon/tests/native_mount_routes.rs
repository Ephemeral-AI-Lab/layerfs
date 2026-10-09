//! Real kernel mounts through the authenticated control channel: lost
//! acknowledgements observed without replay, the SDK's two-step mount,
//! frequent fresh mounts and sustained work on one mount.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::control::{
    Call, ControlCode, NativePhase, ReadyMount, Reply, Request, WorkspaceToken,
};
use layerfs_daemon::{
    control::{Failure, Success},
    install::receive_install,
    install_types::{InstalledStore, StoreSettings},
};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_sdk::{control::Control, OperationCause, WorkspaceApi};
use mounted::{until, Harness};
use nix::fcntl::OFlag;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, OpenOptions},
    os::unix::{
        ffi::OsStringExt,
        fs::{FileExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

const BYTES: usize = support::FILE_BYTES;
fn installed(label: &str) -> (support::Fixture, InstalledStore, Harness) {
    let native = support::Fixture::new(label, None);
    let target = PathBuf::from(&native.project.manifest.locator);
    let (mut client, worker) = support::pair(move |connection| {
        receive_install(connection, &target, StoreSettings::default())
    });
    layerfs_sdk::install(&native.project, &mut client).unwrap();
    let installed = worker.join().unwrap();
    let branch = BranchId::from_bytes(installed.manifest.branch).unwrap();
    let harness = Harness::new(installed.opened.store.clone(), &native.directory, branch);
    (native, installed, harness)
}
fn pattern(range: std::ops::Range<usize>) -> Vec<u8> {
    range.map(|n| (n % 251) as u8).collect()
}
fn names(directory: &Path) -> BTreeSet<OsString> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect()
}
/// The whole small root: every name, both hard-link names, the raw link
/// target and every byte of a file larger than one read window.
fn whole_root(root: &Path) {
    assert_eq!(
        names(root),
        [".git", "alias", "file"].map(OsString::from).into()
    );
    assert_eq!(
        names(&root.join(".git")),
        ["index", "link"].map(OsString::from).into()
    );
    assert_eq!(fs::read(root.join("file")).unwrap(), pattern(0..BYTES));
    let (file, alias) = (
        fs::metadata(root.join("file")).unwrap(),
        fs::metadata(root.join("alias")).unwrap(),
    );
    assert_eq!(
        (file.ino(), file.nlink(), file.len()),
        (alias.ino(), 2, BYTES as u64)
    );
    assert_eq!(
        fs::read(root.join(".git/index")).unwrap(),
        b"complete index"
    );
    assert_eq!(
        fs::read_link(root.join(".git/link"))
            .unwrap()
            .into_os_string()
            .into_vec(),
        b"../opaque-\xff"
    );
}
/// Every entry's kernel-visible identity and attributes, and this mount's device.
fn identity(root: &Path) -> (u64, Vec<String>) {
    let entries = ["", ".git", ".git/index", ".git/link", "alias", "file"].map(|path| {
        let m = fs::symlink_metadata(root.join(path)).unwrap();
        format!(
            "{path} ino={} size={} mtime={}.{} ctime={}.{} mode={:o} owner={}:{} nlink={}",
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
            m.mode(),
            m.uid(),
            m.gid(),
            m.nlink()
        )
    });
    (fs::symlink_metadata(root).unwrap().dev(), entries.to_vec())
}
/// Uncached work: a fresh open, direct reads that cannot be answered from the
/// page cache, a listing and both releases reach the daemon on every round.
fn uncached_round(root: &Path, round: usize) {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(OFlag::O_DIRECT.bits())
        .open(root.join(if round % 2 == 0 { "file" } else { "alias" }))
        .unwrap();
    // Across the 128 KiB window boundary, the first byte and the last byte.
    for (start, end) in [
        (131_000, 131_200),
        (0, 1),
        (BYTES - 1, BYTES),
        (65_000, 70_000),
    ] {
        let mut actual = vec![0; end - start];
        file.read_exact_at(&mut actual, start as u64).unwrap();
        assert_eq!(actual, pattern(start..end), "round {round} at {start}");
    }
    let mut end = [0; 16];
    assert_eq!(file.read_at(&mut end, BYTES as u64).unwrap(), 0);
    assert_eq!(file.read_at(&mut end, BYTES as u64 - 5).unwrap(), 5);
    assert_eq!(names(&root.join(".git")).len(), 2);
}
/// Executes one request whose acknowledgement the caller never reads, and
/// returns what the daemon actually did.
fn lost(h: &Harness, request: Request) -> Result<Success, Failure> {
    let service = h.service.clone();
    let (mut channel, worker) =
        support::pair(move |connection| service.serve_one_control(connection));
    channel
        .send
        .send(&Call { id: 1, request }.encode().unwrap())
        .unwrap();
    drop(channel);
    match worker.join() {
        Ok(served) => served.outcome,
        Err(failure) => failure.original.expect("request was executed").outcome,
    }
}
fn session(h: &Harness) -> (Control, support::Worker<usize>) {
    let service = h.service.clone();
    let (channel, worker) = support::pair(move |connection| {
        let mut served = 0;
        loop {
            let call = service.serve_one_control(connection).unwrap().call;
            served += 1;
            if matches!(call.request, Request::EndSession) {
                return served;
            }
        }
    });
    (Control::new(channel), worker)
}
fn refusal(
    result: Result<impl std::fmt::Debug, Box<layerfs_sdk::OperationFailure>>,
) -> (ControlCode, String) {
    match result.unwrap_err().cause {
        OperationCause::Remote(refusal) => (refusal.code, refusal.phase),
        other => panic!("{other:?}"),
    }
}

#[test]
fn lost_bind_and_attach_acknowledgements_are_located_without_replay() {
    let (native, installed, h) = installed("routes");
    let workspace = WorkspaceId::from_authority([7; 32]).unwrap();
    let (mut control, worker) = session(&h);
    let mut api = WorkspaceApi::new(&mut control);
    assert_eq!(
        refusal(api.locate(workspace)).0,
        ControlCode::Missing,
        "nothing bound yet"
    );
    // The daemon bound the Workspace; the caller never learned its namespace.
    let bound = match lost(
        &h,
        Request::Mount {
            workspace,
            branch: h.branch,
        },
    )
    .unwrap()
    .reply
    {
        Reply::Bound { token, .. } => token,
        other => panic!("{other:?}"),
    };
    let located = api.locate(workspace).unwrap();
    assert_eq!(located.token, bound);
    assert_eq!(located.native.unwrap().phase, NativePhase::Unattached);
    // Observation replayed nothing, and a guessed re-bind is refused.
    assert_eq!(refusal(api.bind(workspace, h.branch)).0, ControlCode::Busy);
    // The daemon attached; the caller never learned the mount directory.
    let ready: ReadyMount = match lost(&h, Request::Attach(bound)).unwrap().reply {
        Reply::Ready(ready) => *ready,
        other => panic!("{other:?}"),
    };
    let located = api.locate(workspace).unwrap().native.unwrap();
    assert_eq!(located.phase, NativePhase::Ready);
    assert_eq!(located.ready.as_ref(), Some(&ready));
    let root = PathBuf::from(&located.ready.unwrap().directory);
    whole_root(&root);
    // An explicit second Attach is a refused new operation, never a second
    // connection: the same mount incarnation is still the one serving.
    assert_eq!(
        refusal(api.attach(bound)),
        (ControlCode::Invalid, "attach:admission".into())
    );
    assert_eq!(
        api.status(bound).unwrap().native.unwrap().ready,
        Some(ready)
    );
    api.unmount(bound).unwrap();
    assert_eq!(refusal(api.locate(workspace)).0, ControlCode::Missing);
    assert!(!root.exists());

    // SDK mount: two acknowledgements, bind then attach.
    let second = WorkspaceId::from_authority([8; 32]).unwrap();
    let mounted = api.mount(second, h.branch).unwrap();
    assert_eq!(mounted.bound.token, mounted.ready.token);
    whole_root(Path::new(&mounted.ready.directory));
    // A failed second acknowledgement leaves the Workspace bound, unattached
    // and usable for a later explicit attach.
    let next = WorkspaceToken {
        workspace: WorkspaceId::from_authority([9; 32]).unwrap(),
        namespace: mounted.bound.token.namespace + 1,
    };
    let occupied = h.mounts.join(next.namespace.to_string());
    fs::create_dir(&occupied).unwrap();
    let failed = api.mount(next.workspace, h.branch).unwrap_err();
    assert_eq!(failed.bound.as_ref().map(|bound| bound.token), Some(next));
    assert_eq!(
        refusal(Err::<(), _>(failed.failure)),
        (ControlCode::Failed, "attach:mount".into())
    );
    let located = api.locate(next.workspace).unwrap();
    assert_eq!(located.native.unwrap().phase, NativePhase::Unattached);
    fs::remove_dir(&occupied).unwrap();
    until("failed attach owner retired", || {
        h.engine(next).maintenance_targets == 0
    });
    let attached = api.attach(next).unwrap();
    whole_root(Path::new(&attached.directory));
    api.unmount(next).unwrap();
    api.unmount(mounted.bound.token).unwrap();
    assert_eq!(
        control.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    println!("NATIVE_ROUTES served={}", worker.join());
    h.stop();
    drop(installed);
    native.cleanup();
}

#[test]
fn fresh_mounts_repeat_and_one_mount_sustains_uncached_work() {
    let (native, installed, h) = installed("sustained");
    let helper = h.bind(1);
    let idle = h.engine(helper);
    // Frequent fresh mounts: each binds the complete root, serves it, and
    // drains fully before the next. No connection, lane or directory is reused.
    let (mut seen, mut stable) = (BTreeSet::new(), None);
    for tag in 10..18 {
        let ready = h.mount(tag);
        // A namespace is never reused, so neither is its mount directory.
        assert!(seen.insert(ready.token.namespace), "{ready:?}");
        assert_eq!(
            Path::new(&ready.directory),
            h.mounts.join(ready.token.namespace.to_string())
        );
        whole_root(Path::new(&ready.directory));
        // Identity is a property of the committed root, not of one mount.
        let (_, entries) = identity(Path::new(&ready.directory));
        assert_eq!(*stable.get_or_insert(entries.clone()), entries, "{ready:?}");
        h.unmount(&ready);
    }
    until("fresh mounts retired", || {
        let counts = h.engine(helper);
        counts.namespaces == 1 && counts.owner_rows <= idle.owner_rows
    });
    // Sustained same-mount work from two concurrent callers.
    const ROUNDS: usize = 150;
    let ready = h.mount(30);
    let root = PathBuf::from(&ready.directory);
    whole_root(&root);
    // Two live mounts of the same root: equal identity, separate devices.
    let other = h.mount(31);
    let (first, second) = (identity(&root), identity(Path::new(&other.directory)));
    assert_eq!(Some(&first.1), stable.as_ref());
    assert_eq!(first.1, second.1);
    assert_ne!(first.0, second.0, "each mount is its own filesystem device");
    h.unmount(&other);
    let before = h.status(ready.token).native.unwrap().work.unwrap();
    let callers: Vec<_> = (0..2)
        .map(|caller| {
            let root = root.clone();
            std::thread::spawn(move || {
                for round in 0..ROUNDS {
                    uncached_round(&root, round + caller);
                }
            })
        })
        .collect();
    for caller in callers {
        caller.join().unwrap();
    }
    until("asynchronous releases served", || {
        let work = h.status(ready.token).native.unwrap().work.unwrap();
        work.received == 0 && work.admitted == 0
    });
    let after = h.status(ready.token).native.unwrap().work.unwrap();
    // Per round: open, four windowed reads, two tail reads, opendir, at least
    // one readdir and both releases.
    assert!(
        after.completed - before.completed >= (2 * ROUNDS * 10) as u64,
        "{before:?} {after:?}"
    );
    assert_eq!(
        (after.retained, after.terminal, after.unadmitted),
        (0, 0, 0)
    );
    assert_eq!(
        (after.loops_entered, after.loops_exited),
        (1, 0),
        "same connection throughout"
    );
    whole_root(&root);
    println!(
        "NATIVE_SUSTAINED fresh=8 rounds={} requests={} before={before:?} after={after:?}",
        2 * ROUNDS,
        after.completed - before.completed
    );
    h.unmount(&ready);
    h.try_unmount(helper).unwrap();
    h.stop();
    drop(installed);
    native.cleanup();
}
