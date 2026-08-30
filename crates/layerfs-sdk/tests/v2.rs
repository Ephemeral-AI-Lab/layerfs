use layerfs_branch_store::CommitOutcome;
use layerfs_sdk::{
    AddLayerResult, BranchStore, Client, ConnectionContext, CreateWorkspaceSession, DiffRequest,
    EndWorkspaceMode, EntityName, LayerStackEndpoint, LayerStackInitialization, LayerStackStore,
    LocalForkSource, PullLayerResult, Query, QueryKind, RemotePlacement, SdkError,
    WorkspaceCommitResult, WorkspacePlacement, WorkspaceProjection,
};
use std::sync::Arc;

fn root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-v2-sdk-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn name(value: &str) -> EntityName {
    value.parse().unwrap()
}

fn initialize(client: &Client) -> layerfs_sdk::LayerId {
    client
        .initialize_layerstack(name("project"), LayerStackInitialization::Empty)
        .unwrap()
        .genesis_layer_id
}

fn fork_layer(
    client: &Client,
    branch_name: &str,
    layer_id: layerfs_sdk::LayerId,
) -> layerfs_sdk::BranchId {
    client
        .fork_branch(name(branch_name), LocalForkSource::Layer { layer_id })
        .unwrap()
}

#[test]
fn one_verified_pair_runs_the_v2_lifecycle() {
    let root = root("lifecycle");
    std::fs::create_dir_all(&root).unwrap();
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), layerstack.store_id()).unwrap();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches,
    })
    .unwrap();
    let genesis = initialize(&client);
    assert!(matches!(
        client
            .pull_layer(genesis, RemotePlacement::Reference)
            .unwrap(),
        PullLayerResult::Created { .. }
    ));
    let pull = client.monitor_snapshot().unwrap().operations.pop().unwrap();
    assert_eq!(
        pull.operation.family,
        layerfs_sdk::OperationFamily::LayerStackPull
    );
    assert_eq!(pull.operation.through_layer_id, Some(genesis));
    assert_eq!(pull.operation.placement, Some(RemotePlacement::Reference));
    assert_eq!(
        pull.operation
            .layer_stack_name
            .as_ref()
            .map(|name| name.as_str()),
        Some("project")
    );
    let branch = fork_layer(&client, "main", genesis);
    let mount = root.join("workspace");
    let workspace = client
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Host {
                root: mount.clone(),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    std::fs::write(mount.join("answer"), b"42").unwrap();
    let commit = client.commit_workspace_session(workspace).unwrap();
    assert!(matches!(commit, WorkspaceCommitResult::Created { .. }));
    client.push_branch(branch).unwrap();
    let added = client.add_layer(branch).unwrap();
    let AddLayerResult::Added { layer_id } = added else {
        panic!("Add")
    };
    client
        .pull_layer(layer_id, RemotePlacement::Reference)
        .unwrap();
    assert!(!client
        .diff(DiffRequest::Layers {
            from_layer_id: genesis,
            to_layer_id: layer_id,
        })
        .unwrap()
        .next_diff_page()
        .unwrap()
        .unwrap()
        .entries
        .is_empty());
    assert!(!client
        .query(Query::new(QueryKind::Branches))
        .unwrap()
        .items
        .is_empty());
    assert!(matches!(
        client.add_layer(branch).unwrap(),
        AddLayerResult::UpToDate { .. }
    ));
    client
        .end_workspace_session(workspace, EndWorkspaceMode::Clean)
        .unwrap();
    drop(client);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn mismatched_parent_is_refused_without_reparenting() {
    let root = root("parent");
    std::fs::create_dir_all(&root).unwrap();
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let wrong = layerfs_sdk::StoreId::random().unwrap();
    let branches = BranchStore::create(root.join("branch.sqlite"), wrong).unwrap();
    assert!(matches!(
        Client::connect(ConnectionContext {
            layerstack: LayerStackEndpoint::local(layerstack.clone()),
            branches,
        }),
        Err(SdkError::InvalidContext)
    ));
    let reopened = BranchStore::connect(root.join("branch.sqlite"), wrong).unwrap();
    assert_eq!(reopened.parent_store_id(), wrong);
    drop(reopened);
    assert!(matches!(
        BranchStore::connect(root.join("branch.sqlite"), layerstack.store_id()),
        Err(layerfs_storage::StorageError::WrongParent)
    ));
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn semantic_operation_receipts_survive_client_reopen() {
    let root = root("monitor-retention");
    std::fs::create_dir_all(&root).unwrap();
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let branch_path = root.join("branch.sqlite");
    let branches = BranchStore::create(&branch_path, layerstack.store_id()).unwrap();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches,
    })
    .unwrap();

    let initialized = client
        .initialize_layerstack(name("project"), LayerStackInitialization::Empty)
        .unwrap();
    let snapshot = client.monitor_snapshot().unwrap();
    assert_eq!(snapshot.operations.len(), 1);
    assert_eq!(
        snapshot.operations[0].operation.family,
        layerfs_monitor::OperationFamily::LayerStackInitialize
    );
    assert_eq!(
        snapshot.operations[0]
            .operation
            .layer_stack_name
            .as_ref()
            .map(|name| name.as_str()),
        Some("project")
    );
    assert!(!snapshot.operations[0].storage.is_empty());
    let receipt = snapshot.operations[0].to_json();
    assert!(receipt.contains(&initialized.layer_stack_id.to_string()));
    assert!(receipt.contains(&initialized.genesis_layer_id.to_string()));
    drop(client);

    let reopened = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches: BranchStore::connect(&branch_path, layerstack.store_id()).unwrap(),
    })
    .unwrap();
    let snapshot = reopened.monitor_snapshot().unwrap();
    assert_eq!(snapshot.operations.len(), 1);
    assert_eq!(
        snapshot.operations[0].operation.layer_stack_id,
        Some(initialized.layer_stack_id)
    );

    drop(reopened);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn monitor_persistence_failure_is_returned_to_the_caller() {
    let root = root("monitor-failure");
    std::fs::create_dir_all(&root).unwrap();
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let branch_path = root.join("branch.sqlite");
    let branches = BranchStore::create(&branch_path, layerstack.store_id()).unwrap();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches,
    })
    .unwrap();
    let mut runtime = branch_path.as_os_str().to_owned();
    runtime.push(".runtime");
    std::fs::create_dir(std::path::PathBuf::from(runtime).join("monitor/operations.jsonl"))
        .unwrap();
    assert!(matches!(
        client.initialize_layerstack(name("project"), LayerStackInitialization::Empty),
        Err(SdkError::Monitor(layerfs_sdk::MonitorError::Io(_)))
    ));
    drop(client);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_add_requires_pull_resolution_commit_push_and_repeat() {
    let root = root("stale-add");
    std::fs::create_dir_all(&root).unwrap();
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), layerstack.store_id()).unwrap();
    let branch_api = branches.clone();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches,
    })
    .unwrap();
    let genesis = initialize(&client);
    client
        .pull_layer(genesis, RemotePlacement::Replica)
        .unwrap();
    let accepted = fork_layer(&client, "accepted", genesis);
    let stale = fork_layer(&client, "stale", genesis);
    commit_logical(
        &branch_api,
        layerstack.clone(),
        accepted,
        "accepted",
        b"one",
    );
    commit_logical(&branch_api, layerstack.clone(), stale, "candidate", b"two");
    client.push_branch(accepted).unwrap();
    let AddLayerResult::Added { layer_id: current } = client.add_layer(accepted).unwrap() else {
        panic!("accepted Add")
    };
    client.push_branch(stale).unwrap();
    assert_eq!(
        client.add_layer(stale).unwrap(),
        AddLayerResult::LayerNotPulled { layer_id: current }
    );
    client
        .pull_layer(current, RemotePlacement::Replica)
        .unwrap();
    let AddLayerResult::NeedsResolution {
        workspace_id,
        conflict_count,
        ..
    } = client.add_layer(stale).unwrap()
    else {
        panic!("resolution")
    };
    let conflicts = client.workspace_conflicts(workspace_id, None).unwrap();
    assert_eq!(conflict_count, 0);
    assert!(conflicts.conflicts.is_empty());
    assert!(matches!(
        client.commit_workspace_session(workspace_id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    assert!(branch_api
        .root_complete(branch_api.branch_root(stale).unwrap())
        .unwrap());
    client
        .end_workspace_session(workspace_id, EndWorkspaceMode::Clean)
        .unwrap();
    client.push_branch(stale).unwrap();
    assert!(matches!(
        client.add_layer(stale).unwrap(),
        AddLayerResult::Added { .. }
    ));
    drop(client);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn typed_stale_conflict_blocks_commit_and_accepts_layer_choice() {
    let root = root("stale-conflict");
    std::fs::create_dir_all(&root).unwrap();
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), layerstack.store_id()).unwrap();
    let branch_api = branches.clone();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches,
    })
    .unwrap();
    let genesis = initialize(&client);
    client
        .pull_layer(genesis, RemotePlacement::Replica)
        .unwrap();
    let accepted = fork_layer(&client, "accepted", genesis);
    let stale = fork_layer(&client, "stale", genesis);
    commit_logical(&branch_api, layerstack.clone(), accepted, "same", b"layer");
    commit_logical(&branch_api, layerstack.clone(), stale, "same", b"branch");
    client.push_branch(accepted).unwrap();
    let AddLayerResult::Added { layer_id: current } = client.add_layer(accepted).unwrap() else {
        panic!("accepted Add")
    };
    client.push_branch(stale).unwrap();
    client
        .pull_layer(current, RemotePlacement::Replica)
        .unwrap();
    let AddLayerResult::NeedsResolution {
        workspace_id,
        conflict_count,
        ..
    } = client.add_layer(stale).unwrap()
    else {
        panic!("resolution")
    };
    assert!(conflict_count > 0);
    assert!(client.commit_workspace_session(workspace_id).is_err());
    let page = client.workspace_conflicts(workspace_id, None).unwrap();
    let conflict = page.conflicts.first().unwrap();
    client
        .resolve_workspace_conflict(
            workspace_id,
            conflict.conflict_id,
            layerfs_sdk::ResolveChoice::Layer,
        )
        .unwrap();
    assert!(matches!(
        client.commit_workspace_session(workspace_id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    assert!(branch_api
        .root_complete(branch_api.branch_root(stale).unwrap())
        .unwrap());
    client
        .end_workspace_session(workspace_id, EndWorkspaceMode::Clean)
        .unwrap();
    client.push_branch(stale).unwrap();
    let final_add = client.add_layer(stale).unwrap();
    assert!(matches!(final_add, AddLayerResult::NoChanges { .. }));
    drop(client);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_lease_and_head_cas_preserve_losing_final_state() {
    let root = root("workspace-cas");
    std::fs::create_dir_all(&root).unwrap();
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), layerstack.store_id()).unwrap();
    let branch_api = branches.clone();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches,
    })
    .unwrap();
    let genesis = initialize(&client);
    client
        .pull_layer(genesis, RemotePlacement::Reference)
        .unwrap();
    let branch = fork_layer(&client, "main", genesis);
    let mount = root.join("workspace");
    let request = || CreateWorkspaceSession {
        branch_id: branch,
        placement: WorkspacePlacement::Host {
            root: mount.clone(),
        },
        projection: Some(WorkspaceProjection::Materialize),
    };
    let workspace = client.create_workspace_session(request()).unwrap();
    assert!(matches!(
        client.create_workspace_session(request()),
        Err(SdkError::Workspace(
            layerfs_sdk::WorkspaceError::WorkspaceBusy
        ))
    ));
    std::fs::write(mount.join("losing"), b"preserved").unwrap();
    commit_logical(&branch_api, layerstack.clone(), branch, "winner", b"head");
    assert!(matches!(
        client.commit_workspace_session(workspace).unwrap(),
        WorkspaceCommitResult::HeadMoved { .. }
    ));
    assert_eq!(std::fs::read(mount.join("losing")).unwrap(), b"preserved");
    client
        .end_workspace_session(workspace, EndWorkspaceMode::Discard)
        .unwrap();
    drop(client);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn all_diff_forms_share_read_only_paged_path_diffs() {
    let root = root("diff-pages");
    std::fs::create_dir_all(&root).unwrap();
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), layerstack.store_id()).unwrap();
    let branch_api = branches.clone();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack.clone()),
        branches,
    })
    .unwrap();
    let genesis = initialize(&client);
    client
        .pull_layer(genesis, RemotePlacement::Reference)
        .unwrap();
    let branch = fork_layer(&client, "main", genesis);
    let changes = (0..140)
        .map(|index| layerfs_content::filesystem::ContentChange::Write {
            path: format!("file-{index:03}"),
            bytes: index.to_string().into_bytes(),
            mode: 0o644,
        })
        .collect::<Vec<_>>();
    let CommitOutcome::Created {
        commit_id: first, ..
    } = branch_api
        .commit_changes(layerstack.clone(), branch, None, &changes)
        .unwrap()
    else {
        panic!("first Commit")
    };
    let objects_before = branch_api.inventory_page(None, 512).unwrap().entries.len();
    let branch_layer_handle = client
        .diff(DiffRequest::BranchLayer {
            branch_id: branch,
            layer_id: genesis,
        })
        .unwrap();
    let branch_layer_operation = branch_layer_handle.id();
    let branch_layer = collect_diff(branch_layer_handle);
    assert!(branch_layer.len() > 128);
    assert_eq!(
        client
            .monitor_snapshot()
            .unwrap()
            .operations
            .last()
            .unwrap()
            .id,
        branch_layer_operation
    );
    assert_eq!(
        branch_api.inventory_page(None, 512).unwrap().entries.len(),
        objects_before
    );

    client.push_branch(branch).unwrap();
    let AddLayerResult::Added { layer_id } = client.add_layer(branch).unwrap() else {
        panic!("Add")
    };
    client
        .pull_layer(layer_id, RemotePlacement::Reference)
        .unwrap();
    let layers = collect_diff(
        client
            .diff(DiffRequest::Layers {
                from_layer_id: genesis,
                to_layer_id: layer_id,
            })
            .unwrap(),
    );
    assert_eq!(layers, branch_layer);

    let CommitOutcome::Created {
        commit_id: second, ..
    } = branch_api
        .commit_changes(
            layerstack.clone(),
            branch,
            Some(first),
            &[layerfs_content::filesystem::ContentChange::Write {
                path: "file-000".into(),
                bytes: b"changed".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    else {
        panic!("second Commit")
    };
    let commit_diff = collect_diff(
        client
            .diff(DiffRequest::BranchCommits {
                branch_id: branch,
                from_commit_id: first,
                to_commit_id: second,
            })
            .unwrap(),
    );
    assert!(commit_diff.iter().any(|entry| matches!(
        entry,
        layerfs_sdk::DiffEntry::Modify { path, aspects, .. }
            if path.as_str() == "file-000" && aspects.content
    )));

    drop(client);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

fn collect_diff(handle: layerfs_sdk::OperationHandle) -> Vec<layerfs_sdk::DiffEntry> {
    let mut entries = Vec::new();
    let mut pages = 0;
    while let Some(page) = handle.next_diff_page().unwrap() {
        assert!(page.entries.len() <= 128);
        pages += 1;
        entries.extend(page.entries);
    }
    assert!(pages > 0);
    entries
}

fn commit_logical(
    branches: &BranchStore,
    layerstack: Arc<LayerStackStore>,
    branch: layerfs_sdk::BranchId,
    path: &str,
    bytes: &[u8],
) {
    assert!(matches!(
        branches
            .commit_changes(
                layerstack,
                branch,
                None,
                &[layerfs_content::filesystem::ContentChange::Write {
                    path: path.to_owned(),
                    bytes: bytes.to_vec(),
                    mode: 0o644,
                }],
            )
            .unwrap(),
        layerfs_branch_store::CommitOutcome::Created { .. }
    ));
}
