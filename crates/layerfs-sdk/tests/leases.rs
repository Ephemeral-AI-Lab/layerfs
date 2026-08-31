use layerfs_sdk::{
    Client, CreateWorkspaceSession, EndWorkspaceMode, EntityName, LayerStackInitialization,
    LayerStackStore, LocalForkSource, SdkError, WorkspaceError, WorkspacePlacement,
    WorkspaceProjection,
};
use std::sync::Arc;

#[test]
fn writable_workspace_lease_is_shared_and_released_on_end_and_client_drop() {
    let root = temp();
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite")).unwrap());
    let first = Client::connect(store.clone()).unwrap();
    let second = Client::connect(store.clone()).unwrap();
    let initialized = first
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let branch_id = first
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    let request = |name: &str| CreateWorkspaceSession {
        branch_id,
        placement: WorkspacePlacement::Host {
            root: root.join(name),
        },
        projection: Some(WorkspaceProjection::Materialize),
    };
    let session = first.create_workspace_session(request("one")).unwrap();
    assert!(matches!(
        second.create_workspace_session(request("two")),
        Err(SdkError::Workspace(WorkspaceError::WorkspaceBusy))
    ));
    first
        .end_workspace_session(session.id, EndWorkspaceMode::Discard)
        .unwrap();
    let session = second.create_workspace_session(request("two")).unwrap();
    second
        .end_workspace_session(session.id, EndWorkspaceMode::Discard)
        .unwrap();

    let third = Client::connect(store.clone()).unwrap();
    let session = third.create_workspace_session(request("three")).unwrap();
    drop(third);
    let session_after_drop = first.create_workspace_session(request("four")).unwrap();
    first
        .end_workspace_session(session_after_drop.id, EndWorkspaceMode::Discard)
        .unwrap();
    drop(session);
    drop(first);
    drop(second);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn temp() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-sdk-v4-leases-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
