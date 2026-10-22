//! Actual Linux daemon binary, protected config and direct Store installation.
#![cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(dead_code)]
#[path = "support/host_install.rs"]
mod support;
use layerfs_api::internal::Control;
use layerfs_bridge::{
    control::{ControlCode, DaemonPhase, HelloRequest, Reply, Request},
    daemon_setup::{DaemonLimits, DaemonSetup},
    duplex,
};
use layerfs_history::WorkspaceId;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    net::{SocketAddr, TcpStream},
    os::unix::fs::PermissionsExt,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn connect(address: SocketAddr) -> duplex::Connection {
    duplex::initiate(
        support::socket(TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap()),
        &support::CLIENT_PRIVATE,
        duplex::public_key(&support::SERVER_PRIVATE).unwrap(),
    )
    .unwrap()
}
#[test]
fn actual_daemon_install_hello_bind_status_no_constructor_and_session_end() {
    let f = support::Fixture::new("application", None);
    fs::set_permissions(&f.directory, fs::Permissions::from_mode(0o700)).unwrap();
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: support::SERVER_PRIVATE,
        control_peer: duplex::public_key(&support::CLIENT_PRIVATE).unwrap(),
        store: f.project.manifest.locator.clone(),
        overlay: f.directory.join("overlay.sqlite").to_str().unwrap().into(),
        mounts: f.directory.join("mounts").to_str().unwrap().into(),
        command_uid: 65534,
        command_gid: 65534,
        limits: DaemonLimits {
            connections: 2,
            handshake_ms: 3000,
            cache_bytes: 64 * 1024,
            namespaces: 8,
            pager_kib: 1024,
            serial_low_water: 0,
            idle_exit_s: 0,
        },
        existing_store: None,
    };
    let config = f.directory.join("daemon.config");
    fs::write(&config, setup.encode().unwrap()).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let mut child = Process(
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
    let stdout = child.0.stdout.take().unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout.take(256))
            .read_line(&mut line)
            .map(|_| line);
        let _ = send.send(result);
    });
    let line = receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    reader.join().unwrap();
    let address: SocketAddr = line
        .strip_prefix("LAYERFS_DAEMON_LISTEN ")
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let mut pending = Control::new(connect(address));
    let observed = match pending
        .call(Request::Hello(HelloRequest {
            expected_instance: None,
            wait_for_store: false,
        }))
        .unwrap()
    {
        Reply::Hello(v) => v,
        other => panic!("{other:?}"),
    };
    assert_eq!(observed.phase, DaemonPhase::InstallPending);
    assert!(observed.overlay_sqlite.is_some());
    assert!(observed.store_sqlite.is_none());
    assert_eq!(
        pending.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    let mut channel = connect(address);
    let installed = layerfs_api::internal::install(&f.project, &mut channel).unwrap();
    assert!(installed.manifest.daemon_sqlite.is_some());
    let mut control = Control::new(channel);
    let ready = match control
        .call(Request::Hello(HelloRequest {
            expected_instance: Some(observed.instance),
            wait_for_store: true,
        }))
        .unwrap()
    {
        Reply::Hello(v) => v,
        other => panic!("{other:?}"),
    };
    assert_eq!(ready.phase, DaemonPhase::ControlReady);
    assert_eq!(ready.store_sqlite, installed.manifest.daemon_sqlite);
    assert!(
        matches!(control.call(Request::Hello(HelloRequest {expected_instance:Some([99;32]),wait_for_store:false})).unwrap(), Reply::Refused(v) if v.code == ControlCode::Invalid)
    );
    let bound = layerfs_api::internal::workspace(&mut control)
        .bind(
            WorkspaceId::from_authority([88; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    let before = layerfs_api::internal::workspace(&mut control)
        .status(bound.token)
        .unwrap();
    // The real binary's Commit of an unchanged Workspace: the same root, no
    // new Commit record.
    assert!(
        matches!(control.call(Request::Commit(layerfs_bridge::control::CommitControl { token: bound.token, force: false, message: None, })).unwrap(), Reply::Committed(layerfs_history::CommitOutcome::UpToDate { head: None, root }) if root == bound.binding.effective_root)
    );
    let after = layerfs_api::internal::workspace(&mut control)
        .status(bound.token)
        .unwrap();
    assert_eq!(after.binding, before.binding);
    assert_eq!(after.activity, before.activity);
    let history = support::commits(&installed.manifest, bound.binding.branch.id, 1);
    assert!(history.records.is_empty());
    layerfs_api::internal::workspace(&mut control)
        .unmount(bound.token, false)
        .unwrap();
    assert_eq!(
        control.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    assert!(!control.call(Request::EndSession).unwrap_err().attempted);
    let mut ended = layerfs_api::internal::into_connection(control);
    assert!(matches!(
        ended.send.send(b"cannot reset ended session"),
        Err(duplex::ChannelError::Quarantined)
    ));
    println!("DAEMON_APPLICATION direct_store=true overlay_sqlite={} store_sqlite={} logical_bind=true kernel_fuse_ready=false sealed_source_absent=true session_end=true", ready.overlay_sqlite.unwrap(), ready.store_sqlite.unwrap());
    // Explicit test process stop is crash scope; logical Workspace close was proved above.
    drop(child);
    f.cleanup();
}

#[cfg(target_os = "macos")]
#[test]
fn actual_daemon_mounts_fuse_t_workspace_commits_and_unmounts_cleanly() {
    let f = support::Fixture::new("macos-fuse-t", None);
    fs::set_permissions(&f.directory, fs::Permissions::from_mode(0o700)).unwrap();
    let uid = nix::unistd::Uid::effective().as_raw();
    let gid = nix::unistd::Gid::effective().as_raw();
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: support::SERVER_PRIVATE,
        control_peer: duplex::public_key(&support::CLIENT_PRIVATE).unwrap(),
        store: f.project.manifest.locator.clone(),
        overlay: f.directory.join("overlay.sqlite").to_str().unwrap().into(),
        mounts: f.directory.join("mounts").to_str().unwrap().into(),
        command_uid: uid,
        command_gid: gid,
        limits: DaemonLimits {
            connections: 2,
            handshake_ms: 3000,
            cache_bytes: 64 * 1024,
            namespaces: 8,
            pager_kib: 1024,
            serial_low_water: 0,
            idle_exit_s: 0,
        },
        existing_store: None,
    };
    let config = f.directory.join("daemon.config");
    fs::write(&config, setup.encode().unwrap()).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let mut child = Process(
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
    let stdout = child.0.stdout.take().unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout.take(256))
            .read_line(&mut line)
            .map(|_| line);
        let _ = send.send(result);
    });
    let line = receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    reader.join().unwrap();
    let address: SocketAddr = line
        .strip_prefix("LAYERFS_DAEMON_LISTEN ")
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let mut channel = connect(address);
    let installed = layerfs_api::internal::install(&f.project, &mut channel).unwrap();
    let mut control = Control::new(channel);
    let hello = match control
        .call(Request::Hello(HelloRequest {
            expected_instance: None,
            wait_for_store: true,
        }))
        .unwrap()
    {
        Reply::Hello(v) => v,
        other => panic!("{other:?}"),
    };
    assert_eq!(hello.phase, DaemonPhase::ControlReady);

    let bound = layerfs_api::internal::workspace(&mut control)
        .bind(
            WorkspaceId::from_authority([77; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    let ready = match control.call(Request::Attach(bound.token)).unwrap() {
        Reply::Ready(ready) => *ready,
        other => panic!("{other:?}"),
    };
    assert_eq!(ready.receipt.loops, 1);
    assert_eq!(ready.receipt.abi_major, 7);
    assert!(ready.receipt.mount_id > 0);

    let mount_root = std::path::Path::new(&ready.directory);
    assert_eq!(
        fs::read(mount_root.join(".git/index")).unwrap(),
        b"complete index"
    );
    assert_eq!(
        fs::metadata(mount_root.join("file")).unwrap().len(),
        support::FILE_BYTES as u64
    );

    fs::write(
        mount_root.join("created.txt"),
        b"committed through macos fuse-t daemon\n",
    )
    .unwrap();
    fs::write(mount_root.join(".git/index"), b"updated index\n").unwrap();

    let committed = match layerfs_api::internal::workspace(&mut control)
        .commit(bound.token)
        .unwrap()
    {
        layerfs_history::CommitOutcome::Committed { record, .. } => *record,
        other => panic!("{other:?}"),
    };
    assert_ne!(committed.root, bound.binding.effective_root);

    layerfs_api::internal::workspace(&mut control)
        .unmount(bound.token, false)
        .unwrap();
    assert!(!mount_root.exists());
    assert!(!f.directory.join("1.sqlite").exists());

    let second = layerfs_api::internal::workspace(&mut control)
        .bind(
            WorkspaceId::from_authority([78; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    assert_eq!(second.binding.effective_root, committed.root);
    let second_ready = match control.call(Request::Attach(second.token)).unwrap() {
        Reply::Ready(ready) => *ready,
        other => panic!("{other:?}"),
    };
    let second_root = std::path::Path::new(&second_ready.directory);
    assert_eq!(
        fs::read(second_root.join("created.txt")).unwrap(),
        b"committed through macos fuse-t daemon\n"
    );
    assert_eq!(
        fs::read(second_root.join(".git/index")).unwrap(),
        b"updated index\n"
    );
    layerfs_api::internal::workspace(&mut control)
        .unmount(second.token, true)
        .unwrap();
    assert!(!second_root.exists());
    assert!(!f.directory.join("2.sqlite").exists());

    assert_eq!(
        control.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    let history = support::commits(&installed.manifest, bound.binding.branch.id, 4);
    assert_eq!(history.records.len(), 1);
    assert_eq!(history.records[0].id, committed.id);
    drop(child);
    f.cleanup();
}
