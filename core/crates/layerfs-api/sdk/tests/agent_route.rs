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
use layerfs_bridge::contract::{Code, CommitOutcomeWire, Inspect, Operation, Request, Response};
use layerfs_sdk::{HistoryMode, ProjectApi, SandboxApi, Server, ServerConfig, WorkspaceApi};
use layerfs_telemetry::{
    output::{Identity, OutputConfig},
    runtime::{Configuration, MonitorConfig, Runtime},
};
use std::{
    fs::OpenOptions,
    io::Write,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    process::Command,
    thread,
    time::Duration,
};

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
    assert_eq!(status.projection_count("range_state"), None);
    assert_eq!(status.projection_count("range_edit"), None);
    assert!(status.upstream_calls > 0);
    workspaces.unmount(&mount.id).unwrap();
    assert_eq!(sandboxes.list().unwrap()[0].id, primary);

    // A restarted daemon is a stale, uncertain route: refused, never replayed.
    let original_port: SocketAddr = published_port(&primary).trim().parse().unwrap();
    assert_eq!(original_port.ip(), Ipv4Addr::LOCALHOST);
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
    let restarted_port: SocketAddr = published_port(&primary).trim().parse().unwrap();
    assert_eq!(restarted_port.ip(), Ipv4Addr::LOCALHOST);
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

#[test]
fn sdk_exec_reaches_inherited_descendant_beyond_4096_then_commits() {
    let Ok(image) = std::env::var("LAYERFS_TEST_IMAGE") else {
        return;
    };
    let root = std::env::temp_dir().join(format!("layerfs-phase45-exec-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source");
    let deep = "d".repeat(250);
    let leaf_component = "e".repeat(210);
    std::fs::create_dir_all(source.join("dst")).unwrap();
    std::fs::create_dir_all(source.join("src/subtree")).unwrap();
    std::fs::write(source.join("src/subtree/shallow"), b"old").unwrap();
    let mut leaf_dir = source.join("src/subtree");
    for _ in 0..4 {
        leaf_dir.push(&leaf_component);
    }
    std::fs::create_dir_all(&leaf_dir).unwrap();
    std::fs::write(leaf_dir.join("leaf"), b"deep-base").unwrap();
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"phase45-deep-exec".to_vec(),
        incarnation: 1,
        cursor_key: [45; 32],
        history: HistoryMode::Create,
        service_host: "host.docker.internal".into(),
        runtime: Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let mut cleanup = Cleanup {
        root: root.clone(),
        owner: &owner,
        sandboxes: Vec::new(),
        failures: Vec::new(),
        diagnostics: None,
    };
    let project = ProjectApi::new(&server)
        .init("phase45-deep-exec", &source)
        .unwrap();
    let branch = ProjectApi::new(&server)
        .fork(&project, [47; 16], "main")
        .unwrap();
    let sandbox = create(&mut cleanup, &image, "phase45-deep-exec").unwrap();
    let workspaces = WorkspaceApi::new(&owner);
    let mount = workspaces
        .mount(sandbox, &project, branch.id, None)
        .unwrap();
    let command = format!(
        "start=$PWD; d={deep}; e={leaf_component}; cd dst || exit; i=0; while [ $i -lt 13 ]; do mkdir \"$d\" || exit; cd \"$d\" || exit; i=$((i+1)); done; mv \"$start/src/subtree\" subtree || exit; cd subtree || exit; exec 4<. || exit; i=0; while [ $i -lt 4 ]; do exec 3<. || exit; cd \"/proc/self/fd/3/$e\" || exit; i=$((i+1)); done; test \"$(cat leaf)\" = deep-base || exit; cd /proc/self/fd/4 || exit; printf new > shallow.next && mv -f shallow.next shallow"
    );
    assert!("dst".len() + 13 * 251 + "/subtree".len() + 4 * 211 + "/leaf".len() > 4096);
    let edit = workspaces.exec(&mount.id, &command).unwrap();
    assert_eq!(edit.exit_status, Some(0), "{:?}", edit.stderr);
    let CommitOutcomeWire::Committed(after) = workspaces.commit(&mount.id).unwrap().outcome else {
        panic!("move commit")
    };
    let inspect = |root, query| {
        server
            .service()
            .handle(
                &server.peer().unwrap(),
                &Request {
                    id: 11,
                    generation: 1,
                    store: server.store(),
                    profile: 1,
                    deadline_ms: 10000,
                    response_bytes: 16384,
                    operation: Operation::Inspect { root, query },
                },
                &mut std::io::empty(),
                &mut std::io::sink(),
            )
            .0
    };
    let child = |root, parent, name: &[u8]| {
        let Response::Attributes { serial, .. } = inspect(
            root,
            Inspect::ChildAttributes {
                parent,
                name: name.to_vec(),
            },
        )
        .unwrap() else {
            panic!("child attributes")
        };
        serial
    };
    let old = child(
        project.root,
        child(project.root, project.root_serial, b"src"),
        b"subtree",
    );
    let mut before_leaf = old;
    for _ in 0..4 {
        before_leaf = child(project.root, before_leaf, leaf_component.as_bytes());
    }
    before_leaf = child(project.root, before_leaf, b"leaf");
    let mut moved = child(after.root, project.root_serial, b"dst");
    for _ in 0..13 {
        moved = child(after.root, moved, deep.as_bytes());
    }
    moved = child(after.root, moved, b"subtree");
    assert_eq!(moved, old);
    let mut after_leaf = moved;
    for _ in 0..4 {
        after_leaf = child(after.root, after_leaf, leaf_component.as_bytes());
    }
    after_leaf = child(after.root, after_leaf, b"leaf");
    assert_eq!(after_leaf, before_leaf);
    let before_attr = inspect(
        project.root,
        Inspect::InodeAttributes {
            serial: before_leaf,
        },
    )
    .unwrap();
    let after_attr = inspect(after.root, Inspect::InodeAttributes { serial: after_leaf }).unwrap();
    let (
        Response::Attributes {
            content: before_content,
            mode: before_mode,
            ..
        },
        Response::Attributes {
            content: after_content,
            mode: after_mode,
            ..
        },
    ) = (before_attr, after_attr)
    else {
        panic!("leaf attributes")
    };
    assert_eq!((after_content, after_mode), (before_content, before_mode));
    for content in [before_content, after_content] {
        let mut bytes = Vec::new();
        let (response, _) = server.service().handle(
            &server.peer().unwrap(),
            &Request {
                id: 13,
                generation: 1,
                store: server.store(),
                profile: 1,
                deadline_ms: 10000,
                response_bytes: 9,
                operation: Operation::ReadFile {
                    root: content,
                    start: 0,
                    end: 9,
                },
            },
            &mut std::io::empty(),
            &mut bytes,
        );
        assert_eq!(response.unwrap(), Response::Read { length: 9 });
        assert_eq!(bytes, b"deep-base");
    }
    assert!(inspect(
        after.root,
        Inspect::ChildAttributes {
            parent: child(after.root, project.root_serial, b"src"),
            name: b"subtree".to_vec(),
        }
    )
    .is_err());
    assert!(inspect(
        after.root,
        Inspect::ChildAttributes {
            parent: moved,
            name: b"shallow.next".to_vec(),
        }
    )
    .is_err());
    let replacement = child(after.root, moved, b"shallow");
    let Response::Attributes { content, size, .. } = inspect(
        after.root,
        Inspect::InodeAttributes {
            serial: replacement,
        },
    )
    .unwrap() else {
        panic!("replacement")
    };
    let mut output = Vec::new();
    let (response, _) = server.service().handle(
        &server.peer().unwrap(),
        &Request {
            id: 12,
            generation: 1,
            store: server.store(),
            profile: 1,
            deadline_ms: 10000,
            response_bytes: size,
            operation: Operation::ReadFile {
                root: content,
                start: 0,
                end: size,
            },
        },
        &mut std::io::empty(),
        &mut output,
    );
    assert_eq!(response.unwrap(), Response::Read { length: size });
    assert_eq!(output, b"new");
    workspaces.unmount(&mount.id).unwrap();
    delete(&mut cleanup, sandbox);
    server.shutdown();
    assert!(cleanup.failures.is_empty());
}

#[test]
fn sdk_exec_moves_inherited_directory_and_commits_complete_tree() {
    let Ok(image) = std::env::var("LAYERFS_TEST_IMAGE") else {
        return;
    };
    let root = std::env::temp_dir().join(format!("layerfs-inherited-exec-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source");
    std::fs::create_dir_all(source.join("packages/old/subtree/child")).unwrap();
    std::fs::create_dir(source.join("packages/new")).unwrap();
    std::fs::write(
        source.join("packages/old/subtree/child/grand.txt"),
        b"grand-base",
    )
    .unwrap();
    std::fs::write(
        source.join("packages/old/subtree/sibling.txt"),
        b"sibling-base",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        source.join("packages/old/subtree/child/grand.txt"),
        std::fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    std::fs::set_permissions(
        source.join("packages/old/subtree/sibling.txt"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"inherited-exec".to_vec(),
        incarnation: 1,
        cursor_key: [43; 32],
        history: HistoryMode::Create,
        service_host: "host.docker.internal".into(),
        runtime: Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let mut cleanup = Cleanup {
        root: root.clone(),
        owner: &owner,
        sandboxes: Vec::new(),
        failures: Vec::new(),
        diagnostics: None,
    };
    let project = ProjectApi::new(&server)
        .init("inherited-exec", &source)
        .unwrap();
    let branch = ProjectApi::new(&server)
        .fork(&project, [46; 16], "main")
        .unwrap();
    let sandbox = create(&mut cleanup, &image, "inherited-exec").unwrap();
    let workspaces = WorkspaceApi::new(&owner);
    let mount = workspaces
        .mount(sandbox, &project, branch.id, None)
        .unwrap();
    assert_eq!(
        workspaces
            .exec(&mount.id, "printf baseline > .marker")
            .unwrap()
            .exit_status,
        Some(0)
    );
    let CommitOutcomeWire::Committed(before) = workspaces.commit(&mount.id).unwrap().outcome else {
        panic!("base commit")
    };
    let edit = workspaces
        .exec(
            &mount.id,
            "umask 022 && mv packages/old/subtree packages/new/subtree && printf grand-new > packages/new/subtree/child/grand.txt.next && mv -f packages/new/subtree/child/grand.txt.next packages/new/subtree/child/grand.txt",
        )
        .unwrap();
    assert_eq!(edit.exit_status, Some(0));
    let CommitOutcomeWire::Committed(after) = workspaces.commit(&mount.id).unwrap().outcome else {
        panic!("move commit")
    };
    let inspect = |root, query| {
        server
            .service()
            .handle(
                &server.peer().unwrap(),
                &Request {
                    id: 9,
                    generation: 1,
                    store: server.store(),
                    profile: 1,
                    deadline_ms: 10000,
                    response_bytes: 16384,
                    operation: Operation::Inspect { root, query },
                },
                &mut std::io::empty(),
                &mut std::io::sink(),
            )
            .0
    };
    let names = |root, path: &[u8]| {
        let Response::List {
            entries,
            continuation: None,
        } = inspect(
            root,
            Inspect::List {
                path: path.to_vec(),
                after: Vec::new(),
                entries: 128,
                bytes: 16384,
            },
        )
        .unwrap()
        else {
            panic!("complete listing")
        };
        entries
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(before.root, b""), names(after.root, b""));
    assert_eq!(
        names(after.root, b""),
        vec![b".marker".to_vec(), b"packages".to_vec()]
    );
    assert_eq!(
        names(before.root, b"packages/old"),
        vec![b"subtree".to_vec()]
    );
    assert_eq!(names(before.root, b"packages/new"), Vec::<Vec<u8>>::new());
    assert_eq!(
        names(after.root, b"packages"),
        vec![b"new".to_vec(), b"old".to_vec()]
    );
    assert_eq!(names(after.root, b"packages/old"), Vec::<Vec<u8>>::new());
    assert_eq!(
        names(after.root, b"packages/new"),
        vec![b"subtree".to_vec()]
    );
    assert_eq!(
        names(after.root, b"packages/new/subtree"),
        vec![b"child".to_vec(), b"sibling.txt".to_vec()]
    );
    assert_eq!(
        names(after.root, b"packages/new/subtree/child"),
        vec![b"grand.txt".to_vec()]
    );
    assert_eq!(
        names(before.root, b"packages/old/subtree"),
        vec![b"child".to_vec(), b"sibling.txt".to_vec()]
    );
    let attributes = |root, path: &[u8]| {
        inspect(
            root,
            Inspect::Attributes {
                path: path.to_vec(),
            },
        )
        .unwrap()
    };
    let Response::Attributes {
        serial: old_dir,
        mode: old_mode,
        ..
    } = attributes(before.root, b"packages/old/subtree")
    else {
        panic!("old directory")
    };
    let Response::Attributes {
        serial: new_dir,
        mode: new_mode,
        ..
    } = attributes(after.root, b"packages/new/subtree")
    else {
        panic!("new directory")
    };
    assert_eq!((old_dir, old_mode), (new_dir, new_mode));
    let Response::Attributes {
        serial: old_child,
        mode: old_child_mode,
        ..
    } = attributes(before.root, b"packages/old/subtree/child")
    else {
        panic!("old child")
    };
    let Response::Attributes {
        serial: new_child,
        mode: new_child_mode,
        ..
    } = attributes(after.root, b"packages/new/subtree/child")
    else {
        panic!("new child")
    };
    assert_eq!((old_child, old_child_mode), (new_child, new_child_mode));
    let Response::Attributes {
        content: grand_root,
        mode: grand_mode,
        ..
    } = attributes(after.root, b"packages/new/subtree/child/grand.txt")
    else {
        panic!("new grandchild")
    };
    let Response::Attributes {
        content: sibling_root,
        mode: sibling_mode,
        ..
    } = attributes(after.root, b"packages/new/subtree/sibling.txt")
    else {
        panic!("sibling")
    };
    let Response::Attributes {
        content: old_grand_root,
        mode: old_grand_mode,
        ..
    } = attributes(before.root, b"packages/old/subtree/child/grand.txt")
    else {
        panic!("old grandchild")
    };
    assert_eq!((sibling_mode, old_grand_mode), (0o600, 0o640));
    assert_eq!(grand_mode, 0o644);
    let read = |content, length| {
        let mut output = Vec::new();
        let (response, _) = server.service().handle(
            &server.peer().unwrap(),
            &Request {
                id: 10,
                generation: 1,
                store: server.store(),
                profile: 1,
                deadline_ms: 10000,
                response_bytes: length,
                operation: Operation::ReadFile {
                    root: content,
                    start: 0,
                    end: length,
                },
            },
            &mut std::io::empty(),
            &mut output,
        );
        assert_eq!(response.unwrap(), Response::Read { length });
        output
    };
    assert_eq!(read(grand_root, 9), b"grand-new");
    assert_eq!(read(sibling_root, 12), b"sibling-base");
    assert_eq!(read(old_grand_root, 10), b"grand-base");
    let Response::Attributes {
        content: marker_root,
        ..
    } = attributes(after.root, b".marker")
    else {
        panic!("marker")
    };
    assert_eq!(read(marker_root, 8), b"baseline");
    for (root, path) in [
        (before.root, b"packages/new/subtree".as_slice()),
        (after.root, b"packages/old/subtree".as_slice()),
        (
            after.root,
            b"packages/new/subtree/child/grand.txt.next".as_slice(),
        ),
    ] {
        assert_eq!(
            inspect(
                root,
                Inspect::Attributes {
                    path: path.to_vec()
                }
            )
            .unwrap_err()
            .code,
            Code::PathNotFound
        );
    }
    let status = workspaces.status(&mount.id).unwrap();
    assert!(status.projection_count("rename").unwrap_or(0) >= 2);
    println!(
        "INHERITED_EXEC rename={} create={} write={} commits=2 fuse={:?}",
        status.projection_count("rename").unwrap_or(0),
        status.projection_count("create").unwrap_or(0),
        status.projection_count("write").unwrap_or(0),
        status.projection
    );
    workspaces.unmount(&mount.id).unwrap();
    delete(&mut cleanup, sandbox);
    server.shutdown();
    assert!(cleanup.failures.is_empty());
}
