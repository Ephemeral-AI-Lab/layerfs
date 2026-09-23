//! Optional live Docker proof: set LAYERFS_TEST_IMAGE to an immutable image with /layerfs-daemon and /bin/sh.
use layerfs_api_core::{SandboxId, SandboxStatus, WorkspaceError};
use layerfs_bridge::{
    adapters::native::{
        connection::{accept, Peer, VerifiedPeer},
        server::serve,
    },
    contract::{
        Code, CommitOutcomeWire, HistoryCommand, HistoryForkSource, HistoryResult, Operation,
        Request, Response, HISTORY_PROFILE, HISTORY_RESULT_BYTES, MAX_OPERATION_MS,
    },
};
use layerfs_history::{sqlite, HistoryCatalog, HistoryCatalogConfig};
use layerfs_sandbox::{OwnerConfig, SandboxOwner};
use layerfs_sdk::{ProjectApi, SandboxApi, WorkspaceApi};
use layerfs_service::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{
    io::Cursor,
    net::TcpListener,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

struct Cleanup {
    root: PathBuf,
    containers: Vec<String>,
    stop: Arc<AtomicBool>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for name in &self.containers {
            let _ = Command::new("docker").args(["rm", "-f", name]).output();
            let _ = Command::new("docker")
                .args(["volume", "rm", &format!("{name}-root")])
                .output();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn create(api: &SandboxApi<'_>, cleanup: &mut Cleanup, image: &str, name: &str) -> SandboxId {
    match api.create(image, name) {
        Ok(id) => {
            cleanup.containers.push(format!("layerfs-{id}"));
            id
        }
        Err(error) => {
            if let Some(id) = error.sandbox {
                cleanup.containers.push(format!("layerfs-{id}"));
            }
            panic!("sandbox create: {error:?}");
        }
    }
}

#[test]
fn init_mount_exec_commit_unmount_and_historical_conflict() {
    let Ok(image) = std::env::var("LAYERFS_TEST_IMAGE") else {
        return;
    };
    let root = std::env::temp_dir().join(format!("layerfs-agent-route-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let mut cleanup = Cleanup {
        root: root.clone(),
        containers: Vec::new(),
        stop: stop.clone(),
    };
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("note"), b"base").unwrap();
    let host_private = [31; 32];
    let daemon_private = [32; 32];
    let service_private = [33; 32];
    let control_private = [34; 32];
    let host_peer = VerifiedPeer::from_private(&host_private).unwrap();
    let daemon_peer = VerifiedPeer::from_private(&daemon_private).unwrap();
    let service_peer = VerifiedPeer::from_private(&service_private).unwrap();
    let store = Timing::disabled("create", |scope| {
        Store::create(
            root.join("store.sqlite"),
            Store::default_policy(),
            scope.child("store"),
        )
    })
    .0
    .unwrap();
    let history: Arc<dyn HistoryCatalog> = Arc::new(
        sqlite::create(
            &root.join("history.sqlite"),
            &HistoryCatalogConfig {
                binding_key: b"agent-route".to_vec(),
                incarnation: 1,
                cursor_key: [35; 32],
            },
        )
        .unwrap(),
    );
    let service = Arc::new(
        Service::new(
            vec![StoreAccess {
                id: 1,
                store,
                history: Some(history),
                grants: vec![&host_peer, &daemon_peer]
                    .into_iter()
                    .map(|peer| Grant {
                        public_key: *peer.public_key(),
                        operations: u8::MAX,
                        expires_unix: u64::MAX,
                    })
                    .collect(),
            }],
            OperationRecorder::disabled(),
        )
        .unwrap(),
    );
    let project = ProjectApi::new(&service, &host_peer, 1)
        .init("agent-route", &source)
        .unwrap();
    let request = Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: HISTORY_PROFILE,
        deadline_ms: MAX_OPERATION_MS,
        response_bytes: HISTORY_RESULT_BYTES as u64,
        operation: Operation::HistoryCommand(HistoryCommand::Fork {
            stack: project.id,
            branch: [36; 16],
            name: b"main".to_vec(),
            source: HistoryForkSource::Layer(project.genesis_layer),
        }),
    };
    let (result, _) = service.handle(
        &host_peer,
        &request,
        &mut Cursor::new([]),
        &mut std::io::sink(),
    );
    let Response::History(result) = result.unwrap() else {
        panic!("fork result");
    };
    let HistoryResult::BranchSnapshot(branch) = *result else {
        panic!("fork snapshot");
    };
    let listener = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let server = service.clone();
    let stopping = stop.clone();
    let peer = Peer {
        selector: 1,
        public: *daemon_peer.public_key(),
        expires_unix: u64::MAX,
    };
    let thread = thread::spawn(move || {
        while !stopping.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let server = server.clone();
                    let peer = peer.clone();
                    thread::spawn(move || {
                        if let Ok(connection) = accept(stream, &service_private, &[peer]) {
                            let _ = serve(connection, |peer, request, input, output, deadline| {
                                server
                                    .handle_until(peer, request, input, output, deadline)
                                    .0
                            });
                        }
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("service accept: {error}"),
            }
        }
    });
    let owner = SandboxOwner::new(OwnerConfig {
        service_endpoint: format!("host.docker.internal:{port}"),
        service_selector: 1,
        service_private: daemon_private,
        service_public: *service_peer.public_key(),
        control_private,
        store: 1,
    })
    .unwrap();
    let sandbox_api = SandboxApi::new(&owner);
    let workspace_api = WorkspaceApi::new(&owner);
    let first = create(&sandbox_api, &mut cleanup, &image, "agent-one");
    assert_eq!(sandbox_api.list().unwrap()[0].id, first);
    let mount = workspace_api
        .mount(first, &project, branch.branch.branch, None)
        .unwrap();
    assert_eq!(
        workspace_api.exec(&mount.id, "cat note").unwrap().stdout,
        b"base"
    );
    let output = workspace_api
        .exec(&mount.id, "yes x | head -c 100000")
        .unwrap();
    assert_eq!(output.stdout.len(), 8192);
    assert!(output.stdout_truncated);
    assert_eq!(
        workspace_api
            .exec(&mount.id, "printf first > note")
            .unwrap()
            .exit_status,
        Some(0)
    );
    let first_commit = workspace_api.commit(&mount.id).unwrap();
    let CommitOutcomeWire::Committed(first_record) = first_commit.outcome else {
        panic!("first commit");
    };
    assert_eq!(
        workspace_api
            .exec(&mount.id, "printf second > note")
            .unwrap()
            .exit_status,
        Some(0)
    );
    let second_commit = workspace_api.commit(&mount.id).unwrap();
    assert!(matches!(
        second_commit.outcome,
        CommitOutcomeWire::Committed(_)
    ));
    workspace_api.unmount(&mount.id).unwrap();
    assert!(Command::new("docker")
        .args(["restart", &format!("layerfs-{first}")])
        .output()
        .unwrap()
        .status
        .success());
    let until = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let status = sandbox_api
            .list()
            .unwrap()
            .into_iter()
            .find(|item| item.id == first)
            .unwrap()
            .status;
        if status == SandboxStatus::Stale {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "daemon restart status {status:?}; lookup: {:?}; port: {}; logs: {}",
            owner.lookup(first).err(),
            String::from_utf8_lossy(
                &Command::new("docker")
                    .args(["port", &format!("layerfs-{first}"), "23456/tcp"])
                    .output()
                    .unwrap()
                    .stdout
            ),
            String::from_utf8_lossy(
                &Command::new("docker")
                    .args(["logs", &format!("layerfs-{first}")])
                    .output()
                    .unwrap()
                    .stderr
            )
        );
        thread::sleep(Duration::from_millis(25));
    }
    assert!(matches!(
        workspace_api.exec(&mount.id, "cat note"),
        Err(WorkspaceError::Stale)
    ));
    let current_sandbox = create(&sandbox_api, &mut cleanup, &image, "agent-current");
    let current = workspace_api
        .mount(current_sandbox, &project, branch.branch.branch, None)
        .unwrap();
    assert_eq!(
        workspace_api.exec(&current.id, "cat note").unwrap().stdout,
        b"second"
    );
    workspace_api.unmount(&current.id).unwrap();
    let second = create(&sandbox_api, &mut cleanup, &image, "agent-two");
    let older = workspace_api
        .mount(
            second,
            &project,
            branch.branch.branch,
            Some(first_record.commit),
        )
        .unwrap();
    assert_eq!(
        workspace_api.exec(&older.id, "cat note").unwrap().stdout,
        b"first"
    );
    workspace_api
        .exec(&older.id, "printf stale > note")
        .unwrap();
    let failure = workspace_api.commit(&older.id).unwrap_err();
    match failure {
        WorkspaceError::Commit(failure) => assert_eq!(failure.cause.code, Code::HeadMoved),
        other => panic!("unexpected conflict: {other:?}"),
    }
    workspace_api.unmount(&older.id).unwrap();
    cleanup.stop.store(true, Ordering::Release);
    thread.join().unwrap();
}
