use layerfs_sdk::{
    AddLayerResult, Client, CreateWorkspaceSession, EndWorkspaceMode, EntityName,
    LayerStackInitialization, LayerStackStore, LocalForkSource, Query, QueryItem, QueryKind,
    WorkspaceCommitResult, WorkspacePlacement, WorkspaceProjection,
};
use std::sync::Arc;

#[test]
fn public_sdk_runs_one_store_workspace_commit_and_add_lifecycle() {
    let root = temp("lifecycle");
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
    let mount = root.join("view");
    let session = client
        .create_workspace_session(CreateWorkspaceSession {
            branch_id,
            placement: WorkspacePlacement::Host {
                root: mount.clone(),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    std::fs::write(mount.join("hello"), b"world").unwrap();
    let commit_id = match client.commit_workspace_session(session.id).unwrap() {
        WorkspaceCommitResult::Created { commit_id, .. } => commit_id,
        result => panic!("unexpected Commit: {result:?}"),
    };
    let branches = client.query(Query::new(QueryKind::Branches)).unwrap().items;
    assert!(branches.iter().any(|item| {
        matches!(item, QueryItem::Branch(branch) if branch.id == branch_id && branch.head_commit_id == Some(commit_id))
    }));
    assert!(matches!(
        client.add_layer(branch_id).unwrap(),
        AddLayerResult::Added { .. }
    ));
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
