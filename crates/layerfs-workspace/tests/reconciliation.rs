use layerfs_content::filesystem::ContentChange;
use layerfs_layerstack_store::{
    apply_changes, AddLayerResult, CommitOutcome, EntityName, LayerStackInitialization,
    LayerStackStore, LocalForkSource,
};
use layerfs_workspace::{
    inject_projection_refresh_failure_once, EndWorkspaceMode, ResolveChoice, WorkspaceCommitResult,
    WorkspacePlacement, WorkspaceState, Workspaces,
};

#[test]
fn materialized_resolution_ignores_unrelated_edits_and_remains_writable_after_commit() {
    let root = temp();
    std::fs::create_dir_all(&root).unwrap();
    let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
    let initialized = store
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let accepted = store
        .fork_branch(
            EntityName::new("accepted").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    let stale = store
        .fork_branch(
            EntityName::new("stale").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    commit(&store, accepted, b"layer");
    let AddLayerResult::Added { layer_id: current } = store.add_layer(accepted).unwrap() else {
        panic!("accepted Add")
    };
    commit(&store, stale, b"branch");

    let workspaces = Workspaces::new(root.join("runtime"), store.clone()).unwrap();
    let (workspace_id, conflicts) = workspaces
        .create_reconciliation_workspace(stale, current)
        .unwrap();
    assert!(conflicts > 0);
    let page = workspaces.workspace_conflicts(workspace_id, None).unwrap();
    let conflict = page.conflicts.first().unwrap();
    workspaces
        .resolve_workspace_conflict(workspace_id, conflict.conflict_id, ResolveChoice::Layer)
        .unwrap();
    let detail = workspaces.session(workspace_id).unwrap();
    let WorkspacePlacement::Host { root: mount } = detail.session.placement else {
        panic!("materialized host Workspace")
    };
    assert_eq!(std::fs::read(mount.join("z")).unwrap(), b"branch");
    std::fs::write(mount.join("a"), b"unrelated").unwrap();

    assert!(matches!(
        workspaces.commit_workspace_session(workspace_id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    assert_eq!(std::fs::read(mount.join("z")).unwrap(), b"layer");
    assert_eq!(std::fs::read(mount.join("a")).unwrap(), b"unrelated");
    std::fs::write(mount.join("late"), b"still-active").unwrap();
    workspaces
        .end_workspace_session(workspace_id, EndWorkspaceMode::Discard)
        .unwrap();

    drop(workspaces);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn published_commit_reports_projection_failure_and_recovers_without_recommit() {
    let root = temp();
    std::fs::create_dir_all(&root).unwrap();
    let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
    let initialized = store
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let accepted = store
        .fork_branch(
            EntityName::new("accepted").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    let stale = store
        .fork_branch(
            EntityName::new("stale").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    commit(&store, accepted, b"layer");
    let AddLayerResult::Added { layer_id: current } = store.add_layer(accepted).unwrap() else {
        panic!("accepted Add")
    };
    commit(&store, stale, b"branch");
    let commits_before = store.store_counts().unwrap().commits;
    let previous_head = store.branch(stale).unwrap().unwrap().head_commit_id;

    let workspaces = Workspaces::new(root.join("runtime"), store.clone()).unwrap();
    let (workspace_id, _) = workspaces
        .create_reconciliation_workspace(stale, current)
        .unwrap();
    let conflict = workspaces
        .workspace_conflicts(workspace_id, None)
        .unwrap()
        .conflicts[0]
        .conflict_id;
    workspaces
        .resolve_workspace_conflict(workspace_id, conflict, ResolveChoice::Layer)
        .unwrap();
    let WorkspacePlacement::Host { root: mount } =
        workspaces.session(workspace_id).unwrap().session.placement
    else {
        panic!("materialized host Workspace")
    };
    std::fs::write(mount.join("a"), b"once").unwrap();
    inject_projection_refresh_failure_once();
    let WorkspaceCommitResult::CreatedPresentationFailed {
        previous_head: observed_previous,
        commit_id,
    } = workspaces.commit_workspace_session(workspace_id).unwrap()
    else {
        panic!("published presentation failure")
    };
    assert_eq!(observed_previous, previous_head);
    assert_eq!(
        store.branch(stale).unwrap().unwrap().head_commit_id,
        Some(commit_id)
    );
    assert_eq!(store.store_counts().unwrap().commits, commits_before + 1);
    assert_eq!(
        workspaces.session(workspace_id).unwrap().session.state,
        WorkspaceState::BrokenPresentation
    );

    let recovered = workspaces
        .recover_workspace_presentation(workspace_id)
        .unwrap();
    assert_eq!(recovered.state, WorkspaceState::Active);
    assert_eq!(std::fs::read(mount.join("z")).unwrap(), b"layer");
    assert_eq!(std::fs::read(mount.join("a")).unwrap(), b"once");
    assert!(matches!(
        workspaces.commit_workspace_session(workspace_id).unwrap(),
        WorkspaceCommitResult::UpToDate { head: Some(head) } if head == commit_id
    ));
    assert_eq!(store.store_counts().unwrap().commits, commits_before + 1);
    workspaces
        .end_workspace_session(workspace_id, EndWorkspaceMode::Clean)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

fn commit(store: &LayerStackStore, branch_id: layerfs_layerstack_store::BranchId, value: &[u8]) {
    let pinned = store.pin_branch(branch_id).unwrap();
    let built = apply_changes(
        &pinned.reader,
        pinned.root,
        &[ContentChange::Write {
            path: "z".to_owned(),
            bytes: value.to_vec(),
            mode: 0o644,
        }],
        [9; 32],
    )
    .unwrap();
    assert!(matches!(
        store
            .commit_candidate(
                &pinned.branch,
                pinned.root,
                pinned.branch.base_layer_id,
                built,
            )
            .unwrap(),
        CommitOutcome::Committed { .. }
    ));
}

fn temp() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-v4-workspace-reconciliation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
