//! A3: the daemon's control Service with NFS serving (the native route's
//! mount entry). Every step is a control request through `execute_control`,
//! the function the daemon's control connection answers with: Mount (bind),
//! Attach (export + client mount helper), Status, Commit (client flush, then
//! the unchanged product Commit), Unmount (dirty refusal, reversible busy,
//! terminal) and ForceUnmount. The client mount itself is a recording helper
//! here (this host's kernel has no NFSv3 client); the protocol client of the
//! tests plays the kernel. The kernel-client run is in the A3 VM scripts.
#[allow(dead_code)]
#[path = "support/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
use client::{Client, NOENT};
use layerfs_bridge::control::{
    CommitControl, MountPhase, ReadyMount, Reply, Request, UnmountControl, WorkspaceToken,
};
use layerfs_daemon::control::{
    ExportServing, ExportServingConfig, Failure, Helper, Mounter, Service,
};
use layerfs_history::WorkspaceId;
use std::{
    fs,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const UID: u32 = 501;
const GID: u32 = 20;

fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-nfs-d{}-{label}", std::process::id()))
}
/// A helper that appends `<what> <port> <dir>` to `log`; it answers "busy"
/// (exit 32) while `busy` exists.
fn recording(what: &str, log: &Path, busy: Option<&Path>) -> Helper {
    let refuse = busy
        .map(|flag| {
            format!(
                "if [ -e {} ]; then echo 'umount: target is busy' >&2; exit 32; fi; ",
                flag.display()
            )
        })
        .unwrap_or_default();
    Helper(vec![
        "sh".into(),
        "-c".into(),
        format!("{refuse}echo {what} {{port}} {{dir}} >> {}", log.display()),
    ])
}
fn bind(service: &Service, tag: u8, branch: layerfs_history::BranchId) -> WorkspaceToken {
    let workspace = WorkspaceId::from_authority([tag; 32]).unwrap();
    match service
        .execute_control(&Request::Mount { workspace, branch })
        .unwrap()
        .reply
    {
        Reply::Bound { token, .. } => token,
        other => panic!("{other:?}"),
    }
}
fn attach(service: &Service, token: WorkspaceToken) -> ReadyMount {
    match service
        .execute_control(&Request::Attach(token))
        .unwrap()
        .reply
    {
        Reply::Ready(ready) => *ready,
        other => panic!("{other:?}"),
    }
}
fn phase(service: &Service, token: WorkspaceToken) -> MountPhase {
    match service
        .execute_control(&Request::Status(token))
        .unwrap()
        .reply
    {
        Reply::Status(status) => status.kernel.expect("kernel block").phase,
        other => panic!("{other:?}"),
    }
}
fn commit(service: &Service, token: WorkspaceToken) -> Reply {
    service
        .execute_control(&Request::Commit(CommitControl {
            token,
            force: false,
            message: Some("over nfs".into()),
        }))
        .unwrap()
        .reply
}
fn unmount(service: &Service, token: WorkspaceToken, discard: bool) -> Result<Reply, Failure> {
    service
        .execute_control(&Request::Unmount(UnmountControl { token, discard }))
        .map(|success| success.reply)
}
fn connect(ready: &ReadyMount) -> Client {
    let port = u16::try_from(ready.receipt.mount_id).unwrap();
    Client::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)), UID, GID)
}

#[test]
fn workspace_control_over_the_nfs_route() {
    let source = temp("src");
    let work = temp("work");
    let mounts = temp("mounts");
    let log = temp("helpers.log");
    let busy = temp("busy");
    for path in [&source, &work, &mounts] {
        drop(fs::remove_dir_all(path));
    }
    drop(fs::remove_file(&log));
    fs::create_dir_all(source.join("src")).unwrap();
    fs::write(source.join("README.md"), b"base readme\n").unwrap();
    fs::write(source.join("src/lib.rs"), b"pub fn base() {}\n").unwrap();
    fs::write(source.join("gone.txt"), b"to be removed\n").unwrap();
    fs::create_dir_all(&mounts).unwrap();
    let rig = rig::Rig::new(&source, &work, Duration::from_secs(60));
    let mounter = Mounter {
        mount: recording("mount", &log, None),
        unmount: recording("unmount", &log, Some(&busy)),
        force: recording("force", &log, None),
        flush: Some(recording("flush", &log, None)),
    };
    let export = Arc::new(ExportServing::start(ExportServingConfig::new(
        mounts.clone(),
        UID,
        GID,
        mounter,
    )));
    let service = rig.service(export);
    assert!(service.serves_nfs());

    // Mount + Attach: Ready with an NFS receipt; the helper mounted it.
    let token = bind(&service, 31, rig.branch);
    assert_eq!(phase(&service, token), MountPhase::Unattached);
    let ready = attach(&service, token);
    assert_eq!(ready.receipt.abi_major, 0);
    assert_eq!(
        ready.directory,
        mounts.join(token.namespace.to_string()).to_str().unwrap()
    );
    assert!(Path::new(&ready.directory).is_dir());
    assert_eq!(phase(&service, token), MountPhase::Ready);
    let port = ready.receipt.mount_id;
    let logged = fs::read_to_string(&log).unwrap();
    assert_eq!(logged, format!("mount {port} {}\n", ready.directory));
    // A second Attach of the same Workspace is refused before effects.
    assert!(service.execute_control(&Request::Attach(token)).is_err());

    // Writes through the export.
    let mut nfs = connect(&ready);
    let root = nfs.mnt("/");
    let made = nfs.touch(&root, b"made.txt", 0o644).unwrap();
    nfs.write_all(&made.handle, 0, b"made over nfs\n", 8);
    let dir = nfs.mkdir(&root, b"newdir", 0o755).unwrap();
    let inner = nfs.touch(&dir.handle, b"inner.bin", 0o600).unwrap();
    nfs.write_all(&inner.handle, 0, &vec![7u8; 300_000], 65536);
    nfs.remove(&root, b"gone.txt").unwrap();
    let (src, _) = nfs.lookup(&root, b"src").unwrap();
    nfs.rename(&src, b"lib.rs", &dir.handle, b"lib.rs").unwrap();
    nfs.symlink(&root, b"link", b"newdir/lib.rs").unwrap();
    match service
        .execute_control(&Request::Status(token))
        .unwrap()
        .reply
    {
        Reply::Status(status) => {
            let block = status.kernel.unwrap();
            assert!(block.work.unwrap().served > 10, "{block:?}");
            assert!(status.local.unwrap().dirty_inodes > 0);
        }
        other => panic!("{other:?}"),
    }
    // Unmount refuses uncommitted changes, naming them; nothing closes.
    match unmount(&service, token, false).unwrap() {
        Reply::Dirty(report) => {
            assert!(report.paths.iter().any(|p| p == "/made.txt"), "{report:?}")
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(phase(&service, token), MountPhase::Ready);

    // Commit: the flush helper ran first, then the product Commit.
    match commit(&service, token) {
        Reply::Committed(_) => {}
        other => panic!("{other:?}"),
    }
    assert!(fs::read_to_string(&log)
        .unwrap()
        .contains(&format!("flush {port} ")));
    // The export keeps serving after the Commit.
    assert_eq!(nfs.read_all(&made.handle, 4096), b"made over nfs\n");

    // An independent view of the committed Branch.
    let view = rig.view([92; 32]);
    let viewer = rig.serve_view(
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
        &view,
        UID,
        GID,
    );
    let mut reader = Client::connect(viewer.local_addr(), UID, GID);
    let view_root = reader.mnt("/");
    let (handle, _) = reader.walk_to(&view_root, "made.txt").unwrap();
    assert_eq!(reader.read_all(&handle, 4096), b"made over nfs\n");
    let (handle, attr) = reader.walk_to(&view_root, "newdir/inner.bin").unwrap();
    assert_eq!((attr.size, attr.mode & 0o777), (300_000, 0o600));
    assert_eq!(reader.read_all(&handle, 65536), vec![7u8; 300_000]);
    let (handle, _) = reader.walk_to(&view_root, "newdir/lib.rs").unwrap();
    assert_eq!(reader.read_all(&handle, 4096), b"pub fn base() {}\n");
    assert_eq!(
        reader.walk_to(&view_root, "gone.txt").map(|_| ()),
        Err(NOENT)
    );
    assert_eq!(
        reader.walk_to(&view_root, "src/lib.rs").map(|_| ()),
        Err(NOENT)
    );
    let (handle, _) = reader.walk_to(&view_root, "link").unwrap();
    assert_eq!(reader.readlink(&handle).unwrap(), b"newdir/lib.rs");
    drop(reader);
    viewer.stop();
    rig.close_view(view);

    // A busy client mount is the reversible answer: still Ready, serving.
    fs::write(&busy, b"").unwrap();
    let refused = unmount(&service, token, true).unwrap_err();
    assert!(format!("{refused:?}").contains("busy"), "{refused:?}");
    assert_eq!(phase(&service, token), MountPhase::Ready);
    assert_eq!(nfs.read_all(&made.handle, 4096), b"made over nfs\n");
    fs::remove_file(&busy).unwrap();

    // Terminal unmount: client unmount, export stopped, engine mount
    // revoked, directory removed, Workspace closed.
    let held = service.export_held(token).unwrap();
    assert_eq!(
        unmount(&service, token, true).unwrap(),
        Reply::Unmounted(token)
    );
    assert!(!Path::new(&ready.directory).exists());
    let released = service.export_released(token).unwrap();
    assert_eq!(released.lookups, held.lookups);
    assert_eq!(released.retained, 0);
    drop(nfs);
    assert!(service.execute_control(&Request::Status(token)).is_err());

    // ForceUnmount of a second Workspace with uncommitted changes.
    let second = bind(&service, 32, rig.branch);
    let ready2 = attach(&service, second);
    let mut nfs = connect(&ready2);
    let root = nfs.mnt("/");
    nfs.touch(&root, b"lost.txt", 0o644).unwrap();
    match service
        .execute_control(&Request::ForceUnmount {
            token: second,
            relinquish_unknown: false,
        })
        .unwrap()
        .reply
    {
        Reply::ForceUnmounted(done) => assert_eq!(done.token, second),
        other => panic!("{other:?}"),
    }
    assert!(!Path::new(&ready2.directory).exists());
    drop(nfs);
    let logged = fs::read_to_string(&log).unwrap();
    println!("HELPERS\n{logged}");
    assert_eq!(
        logged.lines().filter(|l| l.starts_with("mount ")).count(),
        2
    );
    assert_eq!(
        logged.lines().filter(|l| l.starts_with("unmount ")).count(),
        1
    );
    assert_eq!(
        logged.lines().filter(|l| l.starts_with("force ")).count(),
        1
    );

    drop(service);
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    for path in [&source, &mounts] {
        fs::remove_dir_all(path).unwrap();
    }
    fs::remove_file(&log).unwrap();
}
