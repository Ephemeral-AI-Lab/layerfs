use layerfs_sdk::{
    Client, CreateWorkspaceSession, EndWorkspaceMode, EntityName, LayerStackInitialization,
    LayerStackStore, LocalForkSource, WorkspacePlacement, WorkspaceProjection,
};
use std::sync::Arc;

#[test]
fn real_host_fuse_mount_is_one_store_and_committable() {
    if std::env::var_os("LAYERFS_LIVE_FUSE").is_none() {
        return;
    }
    let root = temp("fuse");
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite")).unwrap());
    let client = Client::connect(store.clone()).unwrap();
    let initialized = client
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let branch_id = client
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    let mount = root.join("mount");
    let session = client
        .create_workspace_session(CreateWorkspaceSession {
            branch_id,
            placement: WorkspacePlacement::Host {
                root: mount.clone(),
            },
            projection: Some(WorkspaceProjection::Fuse),
        })
        .unwrap();
    std::fs::write(mount.join("payload"), b"fuse").unwrap();
    client.commit_workspace_session(session.id).unwrap();
    assert_eq!(std::fs::read(mount.join("payload")).unwrap(), b"fuse");
    client
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
    drop(client);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn temp(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-sdk-v4-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
