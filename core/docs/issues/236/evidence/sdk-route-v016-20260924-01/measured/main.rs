use layerfs_sdk::{Client, ContainerCreate, ContainerLimits, ContainerManager, CreateWorkspaceSession,
    EndWorkspaceMode, EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    NonEmpty, WorkspaceCommitResult, WorkspacePlacement, WorkspaceProjection};
use layerfs_telemetry::{output::{Identity, OutputConfig}, runtime::{Configuration, MonitorConfig, Runtime}};
use std::{env, error::Error, ffi::OsString, path::PathBuf, sync::Arc};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn observed<T>(telemetry: &Runtime, key: u64, name: &'static str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let (result, diagnostic) = telemetry.recorder().run(key, name, |_| f());
    telemetry.publish(diagnostic);
    result
}

fn main() -> Result<()> {
    let image = env::var("LAYERFS_V016_IMAGE")?;
    let root = PathBuf::from(env::var("LAYERFS_V016_OUTPUT")?);
    let run: u128 = env::var("LAYERFS_V016_RUN")?.parse()?;
    std::fs::create_dir_all(&root)?;
    let source = root.join("source");
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("note"), b"base")?;
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
    let setup = Client::connect(store.clone())?;
    let layer = setup.initialize_layerstack(EntityName::new("agent-route")?,
        LayerStackInitialization::Directory(source))?.genesis_layer_id;
    let branch = setup.fork_branch(EntityName::new("main")?, LocalForkSource::Layer { layer_id: layer })?;
    drop(setup);
    let manager = ContainerManager::open(root.join("container-control"))?;
    let name = format!("layerfs-v016-route-{}", std::process::id());
    let telemetry = Runtime::start(Configuration {
        enabled: true, timing: true,
        monitor: MonitorConfig { cpu: true, memory: true, interval_ms: 10, history: 600, windows: 32 },
        output: OutputConfig::forward(),
        identity: Identity { run, pid: std::process::id(), role: 1, namespace: 1 },
    })?;
    let (route, diagnostic) = telemetry.recorder().run(1000, "legacy.route", |_| -> Result<(Client, layerfs_sdk::ContainerId)> {
        let (running, client) = observed(&telemetry, 1001, "legacy.sandbox.create_start_connect", || {
            manager.create(ContainerCreate { name: name.clone(), image: image.clone(),
                limits: ContainerLimits { memory_bytes: 512 * 1024 * 1024, cpus: 2, pids: 64 } })?;
            let running = manager.start(&name)?;
            let client = Client::connect_with_container(store.clone(), running.binding())?;
            Ok((running, client))
        })?;
        let location = PathBuf::from(format!("/workspace/layerfs-route-{}", std::process::id()));
        let session = observed(&telemetry, 1002, "legacy.workspace.mount", || {
            Ok(client.create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: WorkspacePlacement::Container { container_id: running.id.clone(), root: location },
                projection: Some(WorkspaceProjection::Fuse),
            })?)
        })?;
        observed(&telemetry, 1003, "legacy.workspace.exec_and_drain", || {
            let execution = client.exec_workspace_session(session.id,
                NonEmpty::new(vec![OsString::from("/bin/sh"), OsString::from("-c"),
                    OsString::from("printf first > note")])?)?;
            let reader = client.workspace_output(execution.id)?;
            let mut page = reader.read(0, true)?;
            while !page.exited {
                let next = reader.read(page.next_sequence, true)?;
                page.chunks.extend(next.chunks);
                page.next_sequence = next.next_sequence;
                page.truncated |= next.truncated;
                page.exited = next.exited;
                page.receipt = next.receipt;
            }
            if page.truncated || page.receipt.as_ref().and_then(|r| r.exit_code) != Some(0) {
                return Err("Exec did not finish cleanly".into());
            }
            Ok(())
        })?;
        observed(&telemetry, 1004, "legacy.workspace.commit", || {
            match client.commit_workspace_session(session.id)? {
                WorkspaceCommitResult::Created { .. } => Ok(()),
                other => Err(format!("Commit outcome: {other:?}").into()),
            }
        })?;
        observed(&telemetry, 1005, "legacy.workspace.end_clean", || {
            client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
            Ok(())
        })?;
        Ok((client, running.id))
    });
    telemetry.publish(diagnostic);
    let verification = (|| -> Result<()> {
        let (client, container_id) = route?;
        if store.pin_branch(branch)?.branch.head_commit_id.is_none() {
            return Err("Branch head was not published".into());
        }
        let reopened = client.create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Container { container_id,
                root: PathBuf::from(format!("/workspace/layerfs-route-{}-reopen", std::process::id())) },
            projection: Some(WorkspaceProjection::Fuse),
        })?;
        let execution = client.exec_workspace_session(reopened.id,
            NonEmpty::new(vec![OsString::from("/bin/sh"), OsString::from("-c"),
                OsString::from("test \"$(cat note)\" = first")])?)?;
        let reader = client.workspace_output(execution.id)?;
        let mut page = reader.read(0, true)?;
        while !page.exited {
            let next = reader.read(page.next_sequence, true)?;
            page.chunks.extend(next.chunks);
            page.next_sequence = next.next_sequence;
            page.truncated |= next.truncated;
            page.exited = next.exited;
            page.receipt = next.receipt;
        }
        if page.receipt.as_ref().and_then(|r| r.exit_code) != Some(0) {
            return Err("reopened note bytes did not match first".into());
        }
        client.end_workspace_session(reopened.id, EndWorkspaceMode::Clean)?;
        Ok(())
    })();
    let cleanup = (|| -> Result<()> {
        manager.stop(&name)?;
        manager.remove(&name)?;
        Ok(())
    })();
    verification?;
    cleanup?;
    eprintln!("V016_ROUTE_PASS image={image} run={run}");
    Ok(())
}
