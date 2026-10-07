//! Actual Linux daemon binary, protected config and direct Store installation.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::{
    control::{ControlCode, DaemonPhase, HelloRequest, Reply, Request},
    daemon_setup::{DaemonLimits, DaemonSetup},
    native,
};
use layerfs_history::{CommitHistoryRequest, WorkspaceId};
use layerfs_sdk::{control::Control, ProjectApi, WorkspaceApi};
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
fn connect(address: SocketAddr) -> native::Connection {
    native::initiate(
        support::socket(TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap()),
        &support::CLIENT_PRIVATE,
        native::public_key(&support::SERVER_PRIVATE).unwrap(),
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
        control_peer: native::public_key(&support::CLIENT_PRIVATE).unwrap(),
        store: f.project.manifest.locator.clone(),
        overlay: f.directory.join("overlay.sqlite").to_str().unwrap().into(),
        mounts: f.directory.join("mounts").to_str().unwrap().into(),
        command_uid: 65534,
        command_gid: 65534,
        limits: DaemonLimits {
            connections: 2,
            read_handles: 2,
            handshake_ms: 3000,
            cache_bytes: 64 * 1024,
            owner_bytes: 16 * 1024 * 1024,
            lifecycle_reserve: 2 * 1024 * 1024,
            namespaces: 8,
            ordinary_jobs: 8,
            lifecycle_jobs: 4,
            pager_kib: 1024,
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
    let installed = ProjectApi::new().install(&f.project, &mut channel).unwrap();
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
    let bound = WorkspaceApi::new(&mut control)
        .bind(
            WorkspaceId::from_authority([88; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    let before = WorkspaceApi::new(&mut control).status(bound.token).unwrap();
    assert!(
        matches!(control.call(Request::Commit(bound.token)).unwrap(), Reply::Refused(v) if v.code == ControlCode::Invalid)
    );
    assert_eq!(
        WorkspaceApi::new(&mut control).status(bound.token).unwrap(),
        before
    );
    let history = ProjectApi::new()
        .history(
            &mut control,
            CommitHistoryRequest {
                branch: bound.binding.branch.id,
                start: None,
                cursor: None,
                limit: 1,
            },
        )
        .unwrap();
    assert!(history.records.is_empty());
    WorkspaceApi::new(&mut control)
        .unmount(bound.token)
        .unwrap();
    assert_eq!(
        control.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    assert!(!control.call(Request::EndSession).unwrap_err().attempted);
    let mut ended = control.into_connection();
    assert!(matches!(
        ended.send.send(b"cannot reset ended session"),
        Err(native::ChannelError::Quarantined)
    ));
    println!("DAEMON_APPLICATION direct_store=true overlay_sqlite={} store_sqlite={} logical_bind=true native_fuse_ready=false sealed_source_absent=true session_end=true", ready.overlay_sqlite.unwrap(), ready.store_sqlite.unwrap());
    // Explicit test process stop is crash scope; logical Workspace close was proved above.
    drop(child);
    f.cleanup();
}
