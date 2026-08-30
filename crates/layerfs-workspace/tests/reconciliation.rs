use layerfs_branch_store::{BranchStore, CommitOutcome};
use layerfs_content::filesystem::ContentChange;
use layerfs_layerstack_store::LayerStackStore;
use layerfs_storage::{
    AuthorityAddResult, EntityName, LayerStackInitialization, LocalForkSource, RemotePlacement,
};
use layerfs_workspace::{
    EndWorkspaceMode, ResolveChoice, WorkspaceCommitResult, WorkspacePlacement, Workspaces,
};
use std::sync::Arc;

#[test]
fn materialized_reference_resolution_ignores_unrelated_edits_and_refreshes_to_committed_root() {
    let root = temp();
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let initialized = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let producer = BranchStore::create(root.join("producer.sqlite"), authority.store_id()).unwrap();
    producer
        .pull_layer(
            authority.clone(),
            initialized.genesis_layer_id,
            RemotePlacement::Reference,
        )
        .unwrap();
    let accepted = producer
        .fork_branch(
            EntityName::new("accepted").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    commit(&producer, authority.clone(), accepted, b"layer");
    producer.push_branch(authority.clone(), accepted).unwrap();
    let AuthorityAddResult::Added { layer_id: current } = authority.add_layer(accepted).unwrap()
    else {
        panic!("accepted Add")
    };

    let branches = BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();
    branches
        .pull_layer(
            authority.clone(),
            initialized.genesis_layer_id,
            RemotePlacement::Reference,
        )
        .unwrap();
    let stale = branches
        .fork_branch(
            EntityName::new("stale").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    commit(&branches, authority.clone(), stale, b"branch");
    branches.push_branch(authority.clone(), stale).unwrap();
    branches
        .pull_layer(authority.clone(), current, RemotePlacement::Reference)
        .unwrap();
    let current_root = authority.layer(current).unwrap().unwrap().root_id;
    assert!(!branches.root_complete(current_root).unwrap());

    let branch_api = branches.clone();
    let workspaces = Workspaces::new(root.join("runtime"), branches, authority.clone()).unwrap();
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
    assert!(std::fs::write(mount.join("late"), b"rejected").is_err());
    assert!(!branch_api
        .root_complete(branch_api.branch_root(stale).unwrap())
        .unwrap());
    workspaces
        .end_workspace_session(workspace_id, EndWorkspaceMode::Clean)
        .unwrap();

    drop(workspaces);
    drop(producer);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

fn commit(
    branches: &BranchStore,
    authority: Arc<LayerStackStore>,
    branch: layerfs_storage::BranchId,
    value: &[u8],
) {
    assert!(matches!(
        branches
            .commit_changes(
                authority,
                branch,
                None,
                &[ContentChange::Write {
                    path: "z".to_owned(),
                    bytes: value.to_vec(),
                    mode: 0o644,
                }],
            )
            .unwrap(),
        CommitOutcome::Created { .. }
    ));
}

fn temp() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-v2-workspace-reconciliation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
