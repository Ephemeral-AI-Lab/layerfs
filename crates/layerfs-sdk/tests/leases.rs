use layerfs_sdk::{
    AddLayerResult, BranchStore, Client, ConnectionContext, CreateWorkspaceSession,
    EndWorkspaceMode, EntityName, LayerStackEndpoint, LayerStackInitialization, LayerStackStore,
    LocalForkSource, Query, QueryItem, QueryKind, RemotePlacement, SdkError, WorkspaceError,
    WorkspacePlacement, WorkspaceProjection, WorkspaceState,
};
use std::sync::Arc;

#[test]
fn writable_workspace_lease_is_shared_across_clients() {
    let root = root("ordinary");
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), layerstack.store_id()).unwrap();
    let client_a = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches: branches.clone(),
    })
    .unwrap();
    let client_b = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches,
    })
    .unwrap();
    let layer = client_a
        .initialize_layerstack(entity("project"), LayerStackInitialization::Empty)
        .unwrap()
        .genesis_layer_id;
    client_a
        .pull_layer(layer, RemotePlacement::Reference)
        .unwrap();
    let branch = client_a
        .fork_branch(entity("main"), LocalForkSource::Layer { layer_id: layer })
        .unwrap();
    let request = |name: &str| CreateWorkspaceSession {
        branch_id: branch,
        placement: WorkspacePlacement::Host {
            root: root.join(name),
        },
        projection: Some(WorkspaceProjection::Materialize),
    };

    let invalid_destination = root.join("not-a-directory");
    std::fs::write(&invalid_destination, b"occupied").unwrap();
    assert!(client_a
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Host {
                root: invalid_destination,
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .is_err());

    let first = client_a
        .create_workspace_session(request("workspace-a"))
        .unwrap();
    assert!(matches!(
        client_b.create_workspace_session(request("workspace-b")),
        Err(SdkError::Workspace(WorkspaceError::WorkspaceBusy))
    ));
    std::fs::write(root.join("workspace-a/dirty"), b"dirty").unwrap();
    assert!(matches!(
        client_a.end_workspace_session(first, EndWorkspaceMode::Clean),
        Err(SdkError::Workspace(WorkspaceError::WorkspaceDirty))
    ));
    assert!(matches!(
        client_b.create_workspace_session(request("workspace-b")),
        Err(SdkError::Workspace(WorkspaceError::WorkspaceBusy))
    ));
    client_a
        .end_workspace_session(first, EndWorkspaceMode::Discard)
        .unwrap();

    let second = client_b
        .create_workspace_session(request("workspace-b"))
        .unwrap();
    client_b
        .end_workspace_session(second, EndWorkspaceMode::Discard)
        .unwrap();
    let third = client_a
        .create_workspace_session(request("workspace-c"))
        .unwrap();
    client_a
        .end_workspace_session(third, EndWorkspaceMode::Discard)
        .unwrap();
    let first_page = client_a
        .query(Query::new(QueryKind::Workspaces).limit(1))
        .unwrap();
    assert_eq!(first_page.items.len(), 1);
    let continuation = first_page.continuation.unwrap();
    let second_page = client_a
        .query(
            Query::new(QueryKind::Workspaces)
                .limit(1)
                .after(continuation),
        )
        .unwrap();
    assert_eq!(second_page.items.len(), 1);
    assert!(second_page.continuation.is_none());
    let queried = first_page
        .items
        .into_iter()
        .chain(second_page.items)
        .map(|item| match item {
            QueryItem::Workspace(value) => {
                assert_eq!(value.layer_stack_name.as_str(), "project");
                assert_eq!(value.branch_name.as_str(), "main");
                value.summary.id
            }
            _ => panic!("Workspace query item"),
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(queried, std::collections::BTreeSet::from([first, third]));

    let broken = client_a
        .create_workspace_session(request("workspace-broken"))
        .unwrap();
    let broken_path = root.join("workspace-broken");
    std::fs::remove_dir_all(&broken_path).unwrap();
    std::fs::write(&broken_path, b"not a projection directory").unwrap();
    assert!(client_a
        .end_workspace_session(broken, EndWorkspaceMode::Discard)
        .is_err());
    assert!(matches!(
        client_a.end_workspace_session(broken, EndWorkspaceMode::Discard),
        Err(SdkError::Workspace(WorkspaceError::InvalidPlacement))
    ));
    assert!(matches!(
        client_b.create_workspace_session(request("blocked-by-broken-cleanup")),
        Err(SdkError::Workspace(WorkspaceError::WorkspaceBusy))
    ));
    let broken_state = client_a
        .query(Query::new(QueryKind::Workspaces).limit(512))
        .unwrap()
        .items
        .into_iter()
        .find_map(|item| match item {
            QueryItem::Workspace(value) if value.summary.id == broken => Some(value.summary.state),
            _ => None,
        });
    assert_eq!(broken_state, Some(WorkspaceState::BrokenCleanup));

    drop(client_b);
    drop(client_a);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn reconciliation_workspace_holds_the_shared_lease_until_end() {
    let root = root("reconciliation");
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), layerstack.store_id()).unwrap();
    let branch_api = branches.clone();
    let client_a = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches: branches.clone(),
    })
    .unwrap();
    let client_b = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches,
    })
    .unwrap();
    let genesis = client_a
        .initialize_layerstack(entity("project"), LayerStackInitialization::Empty)
        .unwrap()
        .genesis_layer_id;
    client_a
        .pull_layer(genesis, RemotePlacement::Reference)
        .unwrap();
    let accepted = client_a
        .fork_branch(
            entity("accepted"),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    let stale = client_a
        .fork_branch(
            entity("stale"),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    commit(&branch_api, layerstack.clone(), accepted, "accepted");
    commit(&branch_api, layerstack.clone(), stale, "stale");
    client_a.push_branch(accepted).unwrap();
    let AddLayerResult::Added { layer_id: current } = client_a.add_layer(accepted).unwrap() else {
        panic!("accepted Layer")
    };
    client_a.push_branch(stale).unwrap();
    client_a
        .pull_layer(current, RemotePlacement::Reference)
        .unwrap();
    let AddLayerResult::NeedsResolution { workspace_id, .. } = client_a.add_layer(stale).unwrap()
    else {
        panic!("reconciliation Workspace")
    };
    assert!(matches!(
        client_b.add_layer(stale),
        Err(SdkError::Workspace(WorkspaceError::WorkspaceBusy))
    ));
    assert!(matches!(
        client_b.create_workspace_session(CreateWorkspaceSession {
            branch_id: stale,
            placement: WorkspacePlacement::Host {
                root: root.join("blocked"),
            },
            projection: Some(WorkspaceProjection::Materialize),
        }),
        Err(SdkError::Workspace(WorkspaceError::WorkspaceBusy))
    ));
    assert!(matches!(
        client_a.add_layer(stale).unwrap(),
        AddLayerResult::NeedsResolution {
            workspace_id: existing,
            ..
        } if existing == workspace_id
    ));

    client_a
        .end_workspace_session(workspace_id, EndWorkspaceMode::Discard)
        .unwrap();
    let ordinary = client_b
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: stale,
            placement: WorkspacePlacement::Host {
                root: root.join("after-end"),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    client_b
        .end_workspace_session(ordinary, EndWorkspaceMode::Discard)
        .unwrap();

    drop(client_b);
    drop(client_a);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

fn commit(
    branches: &BranchStore,
    layerstack: Arc<LayerStackStore>,
    branch: layerfs_sdk::BranchId,
    path: &str,
) {
    branches
        .commit_changes(
            layerstack,
            branch,
            None,
            &[layerfs_content::filesystem::ContentChange::Write {
                path: path.into(),
                bytes: path.as_bytes().to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap();
}

fn root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "layerfs-v2-shared-lease-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn entity(value: &str) -> EntityName {
    value.parse().unwrap()
}
