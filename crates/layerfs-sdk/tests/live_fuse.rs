#[cfg(target_os = "linux")]
use layerfs_sdk::{
    BranchStore, Client, ConnectionContext, CreateWorkspaceSession, EndWorkspaceMode, EntityName,
    LayerStackEndpoint, LayerStackInitialization, LayerStackStore, LocalForkSource,
    RemotePlacement, WorkspaceCommitResult, WorkspacePlacement, WorkspaceProjection,
};
#[cfg(target_os = "linux")]
use std::sync::Arc;

#[cfg(target_os = "linux")]
fn root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-v2-live-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[cfg(target_os = "linux")]
#[test]
fn host_fuse_reads_and_commits_reference_and_replica() {
    if std::env::var_os("LAYERFS_LIVE_FUSE").is_none() {
        return;
    }
    let root = root("host");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority.clone()),
        branches,
    })
    .unwrap();

    for (name, placement) in [
        ("reference", RemotePlacement::Reference),
        ("replica", RemotePlacement::Replica),
    ] {
        let source = root.join(format!("source-{name}"));
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("base"), name).unwrap();
        let layer = client
            .initialize_layerstack(
                EntityName::new(name).unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap()
            .genesis_layer_id;
        client.pull_layer(layer, placement).unwrap();
        let branch = client
            .fork_branch(
                EntityName::new(format!("{name}-main")).unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let mount = root.join(format!("mount-{name}"));
        let started = std::time::Instant::now();
        let workspace = client
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: WorkspacePlacement::Host {
                    root: mount.clone(),
                },
                projection: Some(WorkspaceProjection::Fuse),
            })
            .unwrap();
        println!("HOST_MOUNT {name} {}", started.elapsed().as_nanos());
        assert_eq!(std::fs::read_to_string(mount.join("base")).unwrap(), name);
        std::fs::write(mount.join("created"), format!("{name}-fuse")).unwrap();
        std::fs::hard_link(mount.join("created"), mount.join("hard-link")).unwrap();
        std::os::unix::fs::symlink("created", mount.join("symlink")).unwrap();
        assert_eq!(
            std::fs::read_to_string(mount.join("hard-link")).unwrap(),
            format!("{name}-fuse")
        );
        assert!(matches!(
            client.commit_workspace_session(workspace).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        client
            .end_workspace_session(workspace, EndWorkspaceMode::Clean)
            .unwrap();
    }

    drop(client);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(not(target_os = "linux"))]
#[test]
fn host_fuse_reads_and_commits_reference_and_replica() {
    assert!(std::env::var_os("LAYERFS_LIVE_FUSE").is_none());
}
