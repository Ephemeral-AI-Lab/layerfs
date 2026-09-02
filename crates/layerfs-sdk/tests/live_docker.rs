use layerfs_sdk::{
    Client, ContainerCreate, ContainerLimits, ContainerManager, CreateWorkspaceSession,
    EndWorkspaceMode, EntityName, ExecutionId, ExecutionTransport, LayerStackInitialization,
    LayerStackStore, LocalForkSource, NonEmpty, OutputPage, SdkError, WorkspaceCommitResult,
    WorkspaceError, WorkspaceId, WorkspacePlacement, WorkspaceProjection,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn managed_container_lifecycle_and_disconnect_cleanup_are_exact() {
    if std::env::var_os("LAYERFS_LIVE_DOCKER").is_none() {
        return;
    }
    let image = std::env::var("LAYERFS_LIVE_DOCKER_IMAGE")
        .expect("LAYERFS_LIVE_DOCKER_IMAGE must name a prepared LayerFS runtime image");
    let root = temp();
    let manager = ContainerManager::open(root.join("containers")).unwrap();
    let name = format!(
        "layerfs-live-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let proof = managed_proof(&manager, &name, &image, &root);
    let cleanup = cleanup_container(&manager, &name);
    match (proof, cleanup) {
        (Ok(()), Ok(())) => std::fs::remove_dir_all(root).unwrap(),
        (Err(proof), Ok(())) => panic!("managed lifecycle proof failed: {proof}"),
        (Ok(()), Err(cleanup)) => panic!("managed container cleanup failed: {cleanup}"),
        (Err(proof), Err(cleanup)) => {
            panic!("managed lifecycle proof failed: {proof}; cleanup also failed: {cleanup}")
        }
    }
}

fn managed_proof(
    manager: &ContainerManager,
    name: &str,
    image: &str,
    root: &Path,
) -> AnyResult<()> {
    require(
        Command::new("docker")
            .arg("info")
            .output()?
            .status
            .success(),
        "docker info",
    )?;
    manager.create(ContainerCreate {
        name: name.to_owned(),
        image: image.to_owned(),
        limits: ContainerLimits::default(),
    })?;
    let running = manager.start(name)?;
    let status = manager.status(name)?;
    require(
        status.running
            && !status.privileged
            && status.fuse_device
            && status.sys_admin
            && status.host_binds == 0,
        "managed container isolation",
    )?;

    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect_with_container(store.clone(), running.binding())?;
    let initialized = client
        .initialize_layerstack(EntityName::new("project")?, LayerStackInitialization::Empty)?;
    let attachment_branch = client.fork_branch(
        EntityName::new("attachment-failure")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let attachment_root = format!("/workspace/layerfs-live-{}-attachment", std::process::id());
    let previous_attachment_failure =
        std::env::var_os("LAYERFS_WORKSPACE_INJECT_POST_ATTACH_FAILURE");
    std::env::set_var("LAYERFS_WORKSPACE_INJECT_POST_ATTACH_FAILURE", "1");
    let attachment = client.create_workspace_session(container_request(
        attachment_branch,
        &running.id,
        &attachment_root,
    ));
    match previous_attachment_failure {
        Some(value) => std::env::set_var("LAYERFS_WORKSPACE_INJECT_POST_ATTACH_FAILURE", value),
        None => std::env::remove_var("LAYERFS_WORKSPACE_INJECT_POST_ATTACH_FAILURE"),
    }
    require(
        matches!(
            attachment,
            Err(SdkError::Workspace(WorkspaceError::InvalidPlacement))
        ),
        "injected post-attachment failure",
    )?;
    require(
        client.active_workspace_count()? == 0,
        "post-attachment active Workspace cleanup",
    )?;
    wait_for(Duration::from_secs(5), || {
        container_clean(name, &attachment_root, "no-attachment-process").unwrap_or(false)
    })?;
    let lifecycle_branch = client.fork_branch(
        EntityName::new("lifecycle")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let lifecycle_root = format!("/workspace/layerfs-live-{}-lifecycle", std::process::id());
    let lifecycle = client.create_workspace_session(container_request(
        lifecycle_branch,
        &running.id,
        &lifecycle_root,
    ))?;
    require(mounted(name, &lifecycle_root)?, "lifecycle FUSE mount")?;
    let (normal_execution, page) = execute(
        &client,
        lifecycle.id,
        ["/bin/sh", "-c", "printf managed > payload"],
    )?;
    require(
        page.receipt.as_ref().is_some_and(|receipt| {
            receipt.exit_code == Some(0) && receipt.transport == ExecutionTransport::Daemon
        }),
        "daemon execution receipt",
    )?;
    require(
        matches!(
            client.commit_workspace_session(lifecycle.id)?,
            WorkspaceCommitResult::Created { .. }
        ),
        "managed Commit",
    )?;
    client.end_workspace_session(lifecycle.id, EndWorkspaceMode::Clean)?;
    require(
        container_clean(name, &lifecycle_root, "no-lifecycle-process")?,
        "lifecycle cleanup",
    )?;

    let failure_branch = client.fork_branch(
        EntityName::new("disconnect")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let failure_root = format!("/workspace/layerfs-live-{}-disconnect", std::process::id());
    let failure = client.create_workspace_session(container_request(
        failure_branch,
        &running.id,
        &failure_root,
    ))?;
    require(mounted(name, &failure_root)?, "disconnect FUSE mount")?;
    let spool = runtime_artifact("workspaces", &failure.id.to_string())?;
    require(spool.len() == 1, "Workspace spool evidence")?;
    let marker = format!("layerfs-disconnect-{}", failure.id);
    let previous = std::env::var_os("LAYERFS_EXEC_INJECT_DISCONNECT");
    std::env::set_var("LAYERFS_EXEC_INJECT_DISCONNECT", "1");
    let disconnected = execute_disconnected(&client, failure.id, &marker);
    match previous {
        Some(value) => std::env::set_var("LAYERFS_EXEC_INJECT_DISCONNECT", value),
        None => std::env::remove_var("LAYERFS_EXEC_INJECT_DISCONNECT"),
    }
    let (failed_execution, output_file) = disconnected?;
    wait_for(Duration::from_secs(5), || {
        matches!(client.active_execution_count(), Ok(0))
    })?;
    client.end_workspace_session(failure.id, EndWorkspaceMode::Discard)?;
    require(
        client.active_workspace_count()? == 0,
        "active Workspace cleanup",
    )?;
    require(!spool[0].exists(), "Workspace spool cleanup")?;
    wait_for(Duration::from_secs(5), || {
        container_clean(name, &failure_root, &marker).unwrap_or(false)
    })?;

    drop(client);
    require(!output_file.exists(), "output reader cleanup")?;
    let client = Client::connect_with_container(store.clone(), running.binding())?;
    let lease = client.create_workspace_session(container_request(
        failure_branch,
        &running.id,
        &failure_root,
    ))?;
    client.end_workspace_session(lease.id, EndWorkspaceMode::Discard)?;
    require(
        client.active_workspace_count()? == 0,
        "Branch lease release",
    )?;
    require(client.active_execution_count()? == 0, "execution cleanup")?;
    require(
        container_clean(name, &failure_root, &marker)?,
        "mount/process cleanup",
    )?;

    drop(client);
    drop(store);
    require(
        runtime_artifact("output", &format!("{normal_execution}.frames"))?.is_empty(),
        "successful output reader cleanup",
    )?;
    require(
        runtime_artifact("output", &format!("{failed_execution}.frames"))?.is_empty(),
        "failed output reader cleanup",
    )?;
    Ok(())
}

fn container_request(
    branch_id: layerfs_sdk::BranchId,
    container_id: &layerfs_sdk::ContainerId,
    root: &str,
) -> CreateWorkspaceSession {
    CreateWorkspaceSession {
        branch_id,
        placement: WorkspacePlacement::Container {
            container_id: container_id.clone(),
            root: PathBuf::from(root),
        },
        projection: Some(WorkspaceProjection::Fuse),
    }
}

fn execute<const N: usize>(
    client: &Client,
    workspace: WorkspaceId,
    argv: [&str; N],
) -> AnyResult<(ExecutionId, OutputPage)> {
    let execution = client.exec_workspace_session(
        workspace,
        NonEmpty::new(argv.into_iter().map(OsString::from).collect())?,
    )?;
    let reader = client.workspace_output(execution.id)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut after = 0;
    loop {
        let page = reader.read(after, true)?;
        if page.exited {
            return Ok((execution.id, page));
        }
        if Instant::now() >= deadline {
            return Err("execution output timeout".into());
        }
        after = page.next_sequence;
    }
}

fn execute_disconnected(
    client: &Client,
    workspace: WorkspaceId,
    marker: &str,
) -> AnyResult<(ExecutionId, PathBuf)> {
    let execution = client.exec_workspace_session(
        workspace,
        NonEmpty::new(vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from("while :; do sleep 30; done"),
            OsString::from(marker),
        ])?,
    )?;
    let output = runtime_artifact("output", &format!("{}.frames", execution.id))?;
    require(output.len() == 1, "output spool evidence")?;
    let reader = client.workspace_output(execution.id)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match reader.read(0, true) {
            Err(WorkspaceError::InfrastructureLost) => {
                drop(reader);
                return Ok((execution.id, output[0].clone()));
            }
            Err(error) => return Err(format!("unexpected output failure: {error}").into()),
            Ok(_) if Instant::now() < deadline => {}
            Ok(_) => return Err("disconnect output timeout".into()),
        }
    }
}

fn runtime_artifact(directory: &str, name: &str) -> std::io::Result<Vec<PathBuf>> {
    let root = std::env::temp_dir().join("layerfs-runtime");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    Ok(std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(directory).join(name))
        .filter(|path| path.exists())
        .collect())
}

fn mounted(container: &str, root: &str) -> std::io::Result<bool> {
    docker_status(container, ["findmnt", "-rn", "-M", root])
}

fn container_clean(container: &str, root: &str, marker: &str) -> std::io::Result<bool> {
    let mount = mounted(container, root)?;
    let pid_output = Command::new("docker")
        .args([
            "exec",
            container,
            "find",
            "/tmp",
            "-maxdepth",
            "1",
            "-name",
            "layerfs-execution-*.pid",
            "-print",
        ])
        .output()?;
    let pid_files = pid_output.status.success() && pid_output.stdout.is_empty();
    let processes = Command::new("docker")
        .args(["top", container, "-eo", "pid,args"])
        .output()?;
    if !processes.status.success() {
        return Err(std::io::Error::other("docker top"));
    }
    let execution = pid_files
        && !String::from_utf8_lossy(&processes.stdout)
            .split_ascii_whitespace()
            .any(|argument| argument == marker);
    let helper = docker_status(
        container,
        [
            "/bin/sh",
            "-c",
            "for file in /proc/[0-9]*/comm; do [ \"$(cat \"$file\" 2>/dev/null)\" = layerfs-fuse ] && exit 1; done; exit 0",
        ],
    )?;
    if mount || !execution || !helper {
        eprintln!(
            "container cleanup residue: mount={mount} process_or_pid={} pid_status={} pid_files={:?} pid_error={:?} helper={} processes={:?}",
            !execution,
            pid_output.status,
            String::from_utf8_lossy(&pid_output.stdout),
            String::from_utf8_lossy(&pid_output.stderr),
            !helper,
            String::from_utf8_lossy(&processes.stdout),
        );
    }
    Ok(!mount && execution && helper)
}

fn docker_status<const N: usize>(container: &str, args: [&str; N]) -> std::io::Result<bool> {
    Ok(Command::new("docker")
        .arg("exec")
        .arg(container)
        .args(args)
        .output()?
        .status
        .success())
}

fn wait_for(timeout: Duration, mut condition: impl FnMut() -> bool) -> AnyResult<()> {
    let deadline = Instant::now() + timeout;
    while !condition() {
        if Instant::now() >= deadline {
            return Err("cleanup timeout".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn cleanup_container(manager: &ContainerManager, name: &str) -> AnyResult<()> {
    let inspect = Command::new("docker")
        .args([
            "inspect",
            "-f",
            "{{index .Config.Labels \"dev.layerfs.managed\"}}",
            name,
        ])
        .output()?;
    if !inspect.status.success() {
        return Ok(());
    }
    require(
        String::from_utf8_lossy(&inspect.stdout).trim() == "true",
        "refusing to remove an unmanaged container",
    )?;
    manager.stop(name)?;
    manager.remove(name)?;
    require(
        !Command::new("docker")
            .args(["inspect", name])
            .output()?
            .status
            .success(),
        "container removal",
    )
}

fn require(condition: bool, message: &'static str) -> AnyResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn temp() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-sdk-v4-docker-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
