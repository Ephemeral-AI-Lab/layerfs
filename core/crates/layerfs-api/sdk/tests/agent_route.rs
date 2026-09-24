//! Optional live Docker proof: set LAYERFS_TEST_IMAGE to an immutable image with
//! `/layerfs-daemon` and `/bin/sh`.
//!
//! Every product operation in this route goes through the public SDK:
//! `Server::create`, `ProjectApi::init`, `ProjectApi::fork`,
//! `SandboxApi::{create,list,delete}` and
//! `WorkspaceApi::{mount,exec,commit,status,unmount}`. `docker` appears only as
//! read-only supervision (publishing port observation, absence checks) and as
//! fault injection for the stale-daemon outcome; it performs no cleanup and no
//! edit.
use layerfs_api_core::{SandboxId, SandboxStatus, WorkspaceError};
use layerfs_bridge::contract::{Code, CommitOutcomeWire};
use layerfs_sdk::{HistoryMode, ProjectApi, SandboxApi, Server, ServerConfig, WorkspaceApi};
use layerfs_telemetry::{
    output::{Identity, OutputConfig},
    runtime::{Configuration, MonitorConfig, Runtime},
};
use std::{fs::OpenOptions, io::Write, path::PathBuf, process::Command, thread, time::Duration};

const BRANCH: [u8; 16] = [36; 16];
const RETIRED_IMAGE: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// Product-route cleanup: every admitted sandbox is deleted through the SDK.
/// A cleanup failure is recorded, never replaced by a direct Docker removal.
struct Cleanup<'a> {
    root: PathBuf,
    owner: &'a layerfs_sandbox::SandboxOwner,
    sandboxes: Vec<SandboxId>,
    failures: Vec<String>,
    diagnostics: Option<PathBuf>,
}
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let api = SandboxApi::new(self.owner);
        for id in std::mem::take(&mut self.sandboxes) {
            if let Err(error) = api.delete(id) {
                self.failures.push(format!(
                    "sandbox {id}: {:?} container_removed={} volume_removed={}",
                    error.cause, error.container_removed, error.volume_removed
                ));
            }
        }
        if let Some(output) = &self.diagnostics {
            if !self.failures.is_empty() {
                if let Ok(mut file) = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output.join("cleanup-failures.txt"))
                {
                    let _ = file.write_all(self.failures.join("\n").as_bytes());
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn observed<T, E>(
    runtime: &Runtime,
    key: u64,
    label: &'static str,
    call: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let (result, diagnostic) = runtime.recorder().run(key, label, |_| call());
    runtime.publish(diagnostic);
    result
}

fn published_port(id: &SandboxId) -> String {
    let output = Command::new("docker")
        .args(["port", &format!("layerfs-{id}"), "23456/tcp"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

/// Read-only host observation used by the test's cleanup assertions.
fn docker_object_present(args: &[&str]) -> bool {
    Command::new("docker")
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn create(
    cleanup: &mut Cleanup<'_>,
    image: &str,
    name: &str,
) -> Result<SandboxId, layerfs_sandbox::CreateError> {
    match SandboxApi::new(cleanup.owner).create(image, name) {
        Ok(id) => {
            cleanup.sandboxes.push(id);
            Ok(id)
        }
        Err(error) => {
            // A retained create still names an owned sandbox the caller must be
            // able to delete; it is recorded here so cleanup reaches it.
            if let Some(id) = error.sandbox {
                cleanup.sandboxes.push(id);
            }
            Err(error)
        }
    }
}

fn delete(cleanup: &mut Cleanup<'_>, id: SandboxId) {
    SandboxApi::new(cleanup.owner)
        .delete(id)
        .unwrap_or_else(|error| panic!("sandbox delete {id}: {error:?}"));
    cleanup.sandboxes.retain(|held| *held != id);
    assert!(!docker_object_present(&[
        "inspect",
        &format!("layerfs-{id}")
    ]));
    assert!(!docker_object_present(&[
        "volume",
        "inspect",
        &format!("layerfs-{id}-root")
    ]));
}

#[test]
fn sdk_only_lifecycle_edit_commit_readback_history_conflict_and_cleanup() {
    let Ok(image) = std::env::var("LAYERFS_TEST_IMAGE") else {
        return;
    };
    let telemetry_run = std::env::var("LAYERFS_TEST_TELEMETRY_RUN")
        .ok()
        .map(|value| value.parse::<u128>().unwrap());
    let telemetry_output = std::env::var_os("LAYERFS_TEST_TELEMETRY_OUTPUT").map(PathBuf::from);
    assert_eq!(telemetry_run.is_some(), telemetry_output.is_some());
    let telemetry = match telemetry_run {
        Some(run) => Runtime::start(Configuration {
            enabled: true,
            timing: true,
            monitor: MonitorConfig {
                cpu: true,
                memory: true,
                interval_ms: 10,
                history: 600,
                windows: 32,
            },
            output: OutputConfig::forward(),
            identity: Identity {
                run,
                pid: std::process::id(),
                role: 1,
                namespace: 1,
            },
        })
        .unwrap(),
        None => Runtime::disabled(),
    };
    let root = std::env::temp_dir().join(format!("layerfs-agent-route-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("note"), b"base").unwrap();
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"agent-route".to_vec(),
        incarnation: 1,
        cursor_key: [35; 32],
        history: HistoryMode::Create,
        service_host: "host.docker.internal".into(),
        runtime: telemetry.clone(),
        telemetry_run,
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let mut cleanup = Cleanup {
        root: root.clone(),
        owner: &owner,
        sandboxes: Vec::new(),
        failures: Vec::new(),
        diagnostics: telemetry_output.clone(),
    };
    let projects = ProjectApi::new(&server);
    let sandboxes = SandboxApi::new(&owner);
    let workspaces = WorkspaceApi::new(&owner);
    let project = projects.init("agent-route", &source).unwrap();
    let branch = projects.fork(&project, BRANCH, "main").unwrap();
    assert_eq!(branch.name, "main");
    assert!(branch.head_root.is_none());

    // One live sandbox: edit, see the uncommitted edit in the mount, commit.
    let primary = create(&mut cleanup, &image, "agent-one").unwrap();
    let mount = observed(&telemetry, 1002, "sdk.workspace.mount", || {
        workspaces.mount(primary, &project, branch.id, None)
    })
    .unwrap();
    let first_edit = observed(&telemetry, 1003, "sdk.workspace.exec", || {
        workspaces.exec(&mount.id, "printf first > note")
    })
    .unwrap();
    assert_eq!(first_edit.exit_status, Some(0));
    // Edit visibility before publication: the mount already serves the write.
    assert_eq!(
        workspaces.exec(&mount.id, "cat note").unwrap().stdout,
        b"first"
    );
    let first_commit = observed(&telemetry, 1004, "sdk.workspace.commit", || {
        workspaces.commit(&mount.id)
    })
    .unwrap();
    let CommitOutcomeWire::Committed(first_record) = first_commit.outcome else {
        panic!("first commit");
    };
    // Post-acknowledgement status: bounded projection counts from the real route.
    let status = workspaces.status(&mount.id).unwrap();
    assert!(status.mounted);
    assert!(
        status.projection_count("write").unwrap() >= 1,
        "projection counts: {:?}",
        status.projection
    );
    assert!(status.projection_count("setattr").is_some());
    assert!(status.projection_count("rename").is_some());
    assert!(status.upstream_calls > 0);
    workspaces.unmount(&mount.id).unwrap();
    assert_eq!(sandboxes.list().unwrap()[0].id, primary);

    // A restarted daemon is a stale, uncertain route: refused, never replayed.
    let original_port = published_port(&primary);
    assert!(Command::new("docker")
        .args(["restart", &format!("layerfs-{primary}")])
        .output()
        .unwrap()
        .status
        .success());
    let until = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let status = sandboxes
            .list()
            .unwrap()
            .into_iter()
            .find(|item| item.id == primary)
            .unwrap()
            .status;
        if status == SandboxStatus::Stale {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "daemon restart status {status:?}"
        );
        thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(published_port(&primary), original_port);
    assert!(matches!(
        workspaces.exec(&mount.id, "cat note"),
        Err(WorkspaceError::Stale)
    ));

    // Fresh sandbox on the same Branch: the committed content is published.
    let reader = create(&mut cleanup, &image, "agent-readback").unwrap();
    let readback = workspaces.mount(reader, &project, branch.id, None).unwrap();
    assert_eq!(
        workspaces.exec(&readback.id, "cat note").unwrap().stdout,
        b"first"
    );
    let output = workspaces
        .exec(&readback.id, "yes x | head -c 100000")
        .unwrap();
    assert_eq!(output.stdout.len(), 8192);
    assert!(output.stdout_truncated);
    // Second edit and Commit on the fresh mount.
    assert_eq!(
        workspaces
            .exec(&readback.id, "printf second > note")
            .unwrap()
            .exit_status,
        Some(0)
    );
    assert_eq!(
        workspaces.exec(&readback.id, "cat note").unwrap().stdout,
        b"second"
    );
    assert!(matches!(
        workspaces.commit(&readback.id).unwrap().outcome,
        CommitOutcomeWire::Committed(_)
    ));
    workspaces.unmount(&readback.id).unwrap();

    // A third fresh mount reads the second commit.
    let latest = create(&mut cleanup, &image, "agent-latest").unwrap();
    let fresh = workspaces.mount(latest, &project, branch.id, None).unwrap();
    assert_eq!(
        workspaces.exec(&fresh.id, "cat note").unwrap().stdout,
        b"second"
    );
    workspaces.unmount(&fresh.id).unwrap();

    // The retained historical root still resolves the first commit.
    let historical = create(&mut cleanup, &image, "agent-two").unwrap();
    let older = workspaces
        .mount(historical, &project, branch.id, Some(first_record.commit))
        .unwrap();
    assert_eq!(
        workspaces.exec(&older.id, "cat note").unwrap().stdout,
        b"first"
    );
    workspaces.exec(&older.id, "printf stale > note").unwrap();
    match workspaces.commit(&older.id).unwrap_err() {
        WorkspaceError::Commit(failure) => assert_eq!(failure.cause.code, Code::HeadMoved),
        other => panic!("unexpected conflict: {other:?}"),
    }
    workspaces.unmount(&older.id).unwrap();

    // Normal deletion of every admitted sandbox, through the SDK only.
    for id in [primary, reader, latest, historical] {
        delete(&mut cleanup, id);
    }
    assert!(sandboxes.list().unwrap().is_empty());

    // A retained create that never became ready is still deletable by ID.
    let refused = create(&mut cleanup, RETIRED_IMAGE, "agent-retained").unwrap_err();
    let retained = refused.sandbox.expect("retained create names its sandbox");
    assert!(refused.retained);
    assert_eq!(sandboxes.list().unwrap().len(), 1);
    delete(&mut cleanup, retained);
    assert!(sandboxes.list().unwrap().is_empty());
    // An unknown ID names no container and is refused.
    let unknown = SandboxId([7; 16]);
    let error = sandboxes.delete(unknown).unwrap_err();
    assert_eq!(error.sandbox, unknown);
    assert!(!error.container_removed && !error.volume_removed);

    server.shutdown();
    assert!(cleanup.failures.is_empty(), "{:?}", cleanup.failures);
}
