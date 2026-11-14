//! The native macOS client must not send atime updates the export ignores,
//! and a Commit with the default export flush (the export volume's own
//! `sync_volume_np` on macOS) runs when that flush succeeds and is refused,
//! with nothing committed, when it fails.
#![cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "support/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
use client::Client;
use layerfs_bridge::control::{CommitControl, ControlCode, Reply, Request, UnmountControl};
use layerfs_daemon::control::{ExportServing, ExportServingConfig, Failure, Helper, Mounter};
use layerfs_history::WorkspaceId;
use std::{
    fs,
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    sync::Arc,
    time::Duration,
};

#[test]
fn host_mount_disables_access_time_updates() {
    let mounter = Mounter::host();
    let options = mounter
        .mount
        .0
        .windows(2)
        .find(|pair| pair[0] == "-o")
        .expect("mount options");
    assert!(options[1].split(',').any(|option| option == "noatime"));
    // Keep the existing attribute-cache contract; no global noac tradeoff.
    assert!(options[1].split(',').any(|option| option == "actimeo=60"));
}

#[test]
fn default_flush_commits_with_the_export_volume_and_refuses_without_it() {
    // What this observes, and what it cannot: the Workspace is driven through
    // this test's own NFS RPC client, so the host's mount client holds no
    // writeback for the flush to push and the syscall leaves no trace here.
    // The two Commits below therefore check the contract that is observable:
    // a flush that succeeds (`sync_volume_np` of the live directory) lets
    // the Commit run, and one that fails (`ENOENT` after the directory is
    // removed) refuses it as `Failed` at `commit:flush`, before it runs, and
    // leaves the Workspace idle. `sync_volume_wait`'s `Ok`/error is covered
    // where it lives, by `layerfs-bridge`'s `process_identity` test.
    let base = std::env::temp_dir().join(format!("layerfs-nfs-mounter-{}", std::process::id()));
    let source = base.join("src");
    let work = base.join("work");
    let mounts = base.join("mounts");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("README.md"), b"base\n").unwrap();
    fs::create_dir_all(&mounts).unwrap();

    let rig = rig::Rig::new(&source, &work, Duration::from_secs(60));
    let noop = Helper(vec!["sh".into(), "-c".into(), "true".into()]);
    let mounter = Mounter {
        mount: noop.clone(),
        unmount: noop.clone(),
        force: noop,
        flush: None,
    };
    let export = Arc::new(ExportServing::start(ExportServingConfig::new(
        mounts.clone(),
        501,
        20,
        mounter,
    )));
    let service = rig.service(export);
    let workspace = WorkspaceId::from_authority([41; 32]).unwrap();
    let token = match service
        .execute_control(&Request::Mount {
            workspace,
            branch: rig.branch,
        })
        .unwrap()
        .reply
    {
        Reply::Bound { token, .. } => token,
        other => panic!("{other:?}"),
    };
    let ready = match service
        .execute_control(&Request::Attach(token))
        .unwrap()
        .reply
    {
        Reply::Ready(ready) => *ready,
        other => panic!("{other:?}"),
    };
    let port = u16::try_from(ready.receipt.mount_id).unwrap();
    let mut nfs = Client::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)), 501, 20);
    let root = nfs.mnt("/");

    // 1. Live export directory: the default flush is the volume's own
    // `sync_volume_np`, which succeeds; the Commit runs.
    let f1 = nfs.touch(&root, b"one.txt", 0o644).unwrap();
    nfs.write_all(&f1.handle, 0, b"first\n", 6);
    assert!(Path::new(&ready.directory).is_dir());
    match service
        .execute_control(&Request::Commit(CommitControl {
            token,
            force: false,
            message: Some("first".into()),
        }))
        .unwrap()
        .reply
    {
        Reply::Committed(_) => {}
        other => panic!("{other:?}"),
    }

    // 2. Missing export directory: `sync_volume_np` answers `ENOENT`; the
    // Commit is refused before it runs instead of skipping the writeback.
    let f2 = nfs.touch(&root, b"two.txt", 0o644).unwrap();
    nfs.write_all(&f2.handle, 0, b"second\n", 7);
    fs::remove_dir_all(&ready.directory).unwrap();
    assert!(!Path::new(&ready.directory).exists());
    match service.execute_control(&Request::Commit(CommitControl {
        token,
        force: false,
        message: Some("second".into()),
    })) {
        Err(Failure::Kernel(failure)) => {
            assert_eq!(
                (failure.code, failure.phase),
                (ControlCode::Failed, "commit:flush"),
                "{}",
                failure.detail
            );
            assert!(
                failure.detail.contains("sync_volume_np"),
                "{}",
                failure.detail
            );
        }
        Ok(outcome) => panic!("the Commit ran without its flush: {:?}", outcome.reply),
        Err(other) => panic!("{other:?}"),
    }

    fs::create_dir_all(&ready.directory).unwrap();
    drop(nfs);
    match service
        .execute_control(&Request::Unmount(UnmountControl {
            token,
            discard: true,
        }))
        .unwrap()
        .reply
    {
        Reply::Unmounted(unmounted) => assert_eq!(unmounted, token),
        other => panic!("{other:?}"),
    }
    let released = service.export_released(token).unwrap();
    drop(service);
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(&base).unwrap();
}
