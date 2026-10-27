//! The daemon's control Service with NFS serving, driven by control requests
//! (A3 kernel-client runs).
//!
//! usage: daemon_nfs <source-dir> <work-dir> <mounts-dir> <view-port> <uid> <gid>
//!
//! Initializes a Store from `<source-dir>` (Branch `main`), then runs the
//! Workspace control requests the daemon's control connection answers:
//! Mount and Attach (`BOUND`, `READY <port> <dir>`); the client mount, flush
//! and unmount are the helpers named by `LAYERFS_NFS_MOUNT`,
//! `LAYERFS_NFS_FLUSH`, `LAYERFS_NFS_UNMOUNT` and `LAYERFS_NFS_FORCE`
//! (`{port}`, `{dir}` replaced), as in the daemon. `LAYERFS_NFS_LOOKUPS` sets
//! the export's reference bound. Then, whenever a trigger file appears:
//! `<work-dir>.status` prints the Status kernel block and the export's
//! holdings (`STATUS …`); `<work-dir>.commit` runs a control Commit
//! (`COMMITTED …`) and serves an independent view of the committed Branch
//! read-only on `<view-port>` (`VIEW …`); `<work-dir>.unmount` runs a
//! control Unmount without discard (`UNMOUNT …`), and on success stops.
#[allow(dead_code)]
#[path = "../tests/support/rig.rs"]
mod rig;
use layerfs_bridge::control::{CommitControl, Reply, Request, UnmountControl};
use layerfs_daemon::control::{ExportServing, ExportServingConfig, Helper, Mounter};
use layerfs_history::WorkspaceId;
use std::{net::TcpListener, path::PathBuf, sync::Arc, time::Duration};

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() != 7 {
        eprintln!("usage: daemon_nfs <source-dir> <work-dir> <mounts-dir> <view-port> <uid> <gid>");
        std::process::exit(2);
    }
    let source = PathBuf::from(&arguments[1]);
    let work = PathBuf::from(&arguments[2]);
    let mounts = PathBuf::from(&arguments[3]);
    let view_port: u16 = arguments[4].parse().expect("view port");
    let uid: u32 = arguments[5].parse().expect("uid");
    let gid: u32 = arguments[6].parse().expect("gid");
    let trigger = |name: &str| PathBuf::from(format!("{}.{name}", work.display()));
    let rig = rig::Rig::new(&source, &work, Duration::from_secs(600));
    println!("INIT entries={}", rig.entries);
    let mut mounter = Mounter::host();
    let helper = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|line| Helper::words(&line))
    };
    if let Some(mount) = helper("LAYERFS_NFS_MOUNT") {
        mounter.mount = mount;
    }
    if let Some(unmount) = helper("LAYERFS_NFS_UNMOUNT") {
        mounter.unmount = unmount;
    }
    if let Some(force) = helper("LAYERFS_NFS_FORCE") {
        mounter.force = force;
    }
    mounter.flush = helper("LAYERFS_NFS_FLUSH");
    let mut config = ExportServingConfig::new(mounts, uid, gid, mounter);
    if let Some(bound) = std::env::var("LAYERFS_NFS_LOOKUPS")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        config.lookups_held = bound;
    }
    let service = rig.service(Arc::new(ExportServing::start(config)));
    let workspace = WorkspaceId::from_authority([41; 32]).unwrap();
    let token = match service
        .execute_control(&Request::Mount {
            workspace,
            branch: rig.branch,
        })
        .expect("Mount")
        .reply
    {
        Reply::Bound { token, .. } => token,
        other => panic!("{other:?}"),
    };
    println!("BOUND {token:?}");
    let ready = match service.execute_control(&Request::Attach(token)) {
        Ok(success) => match success.reply {
            Reply::Ready(ready) => *ready,
            other => panic!("{other:?}"),
        },
        Err(failure) => {
            println!("ATTACH_FAILED {failure:?}");
            std::process::exit(1);
        }
    };
    println!("READY {} {}", ready.receipt.mount_id, ready.directory);
    let mut views = Vec::new();
    loop {
        if trigger("status").exists() {
            match service
                .execute_control(&Request::Status(token))
                .map(|s| s.reply)
            {
                Ok(Reply::Status(status)) => println!(
                    "STATUS phase={:?} work={:?} dirty_inodes={:?} held={:?}",
                    status.kernel.as_ref().map(|k| k.phase),
                    status.kernel.as_ref().and_then(|k| k.work),
                    status.local.as_ref().map(|l| l.dirty_inodes),
                    service.export_held(token)
                ),
                other => println!("STATUS {other:?}"),
            }
            drop(std::fs::remove_file(trigger("status")));
        }
        if trigger("commit").exists() {
            let at = std::time::Instant::now();
            let done = service.execute_control(&Request::Commit(CommitControl {
                token,
                force: false,
                message: Some("a3 over nfs".into()),
            }));
            let ms = at.elapsed().as_millis();
            match done {
                Ok(success) => {
                    let kind = match &success.reply {
                        Reply::Committed(_) => "Committed",
                        Reply::MergeCommitted(_) => "MergeCommitted",
                        Reply::Conflict(_) => "Conflict",
                        _ => "Other",
                    };
                    println!("COMMITTED {kind} ms={ms}");
                    let mut authority = [90; 32];
                    authority[0] = views.len() as u8;
                    let view = rig.view(authority);
                    let port = view_port + views.len() as u16;
                    let viewer = rig.serve_view(
                        TcpListener::bind(("127.0.0.1", port)).expect("bind view"),
                        &view,
                        uid,
                        gid,
                    );
                    println!("VIEW {}", viewer.local_addr());
                    views.push((viewer, view));
                }
                Err(failure) => println!("COMMIT_FAILED ms={ms} {failure:?}"),
            }
            drop(std::fs::remove_file(trigger("commit")));
        }
        if trigger("unmount").exists() {
            drop(std::fs::remove_file(trigger("unmount")));
            let held = service.export_held(token);
            match service.execute_control(&Request::Unmount(UnmountControl {
                token,
                discard: false,
            })) {
                Ok(success) => {
                    println!("UNMOUNT {:?} held_before={held:?}", success.reply);
                    if matches!(success.reply, Reply::Unmounted(_)) {
                        break;
                    }
                }
                Err(failure) => println!("UNMOUNT_FAILED {failure:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let released = service.export_released(token).unwrap_or_default();
    println!("RELEASED {released:?}");
    println!(
        "DIRECTORY_GONE {}",
        !std::path::Path::new(&ready.directory).exists()
    );
    for (viewer, view) in views {
        viewer.stop();
        rig.close_view(view);
    }
    drop(service);
    let (_, outstanding) = rig.stop(released);
    println!("STOPPED outstanding_readers={outstanding}");
}
