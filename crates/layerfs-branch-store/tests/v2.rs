use layerfs_branch_store::{BranchStore, CommitOutcome};
use layerfs_layerstack_store::LayerStackStore;
use layerfs_storage::{
    EntityName, FactKind, LayerStackInitialization, LocalForkSource, PullBranchResult,
    PullLayerResult, PushResult, RemotePlacement,
};
use std::sync::Arc;

fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-v2-branch-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn reference_commit_push_add_and_exact_forks_share_objects() {
    let authority_path = path("authority");
    let branch_path = path("local");
    let replica_path = path("replica");
    let authority = Arc::new(LayerStackStore::create(&authority_path).unwrap());
    let genesis = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let branches = BranchStore::create(&branch_path, authority.store_id()).unwrap();
    assert!(matches!(
        branches
            .pull_layer(authority.clone(), genesis, RemotePlacement::Reference)
            .unwrap(),
        PullLayerResult::Created { .. }
    ));
    let objects_before = branches.inventory_page(None, 512).unwrap().entries.len();
    let branch = branches
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    assert_eq!(
        branches.inventory_page(None, 512).unwrap().entries.len(),
        objects_before
    );
    assert!(branches
        .fact_page(FactKind::Commit, None, 512)
        .unwrap()
        .0
        .is_empty());

    let CommitOutcome::Created { commit_id, .. } = branches
        .commit_changes(
            authority.clone(),
            branch,
            None,
            &[layerfs_content::filesystem::ContentChange::Write {
                path: "answer".into(),
                bytes: b"42".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    else {
        panic!("Commit")
    };
    let local_bytes = branches.inventory_page(None, 512).unwrap().entries.len();
    assert!(matches!(
        branches.push_branch(authority.clone(), branch).unwrap(),
        PushResult::Created { .. }
    ));
    assert_eq!(
        branches.inventory_page(None, 512).unwrap().entries.len(),
        local_bytes
    );
    let layer = authority.add_layer(branch).unwrap();
    assert!(matches!(
        layer,
        layerfs_storage::AuthorityAddResult::Added { .. }
    ));

    let rollout_path = path("rollout");
    let rollout = BranchStore::create(&rollout_path, authority.store_id()).unwrap();
    assert!(matches!(
        rollout
            .pull_branch(
                authority.clone(),
                branch,
                commit_id,
                RemotePlacement::Reference,
            )
            .unwrap(),
        PullBranchResult::Created { .. }
    ));
    assert_eq!(rollout.branch(branch).unwrap().unwrap().id, branch);
    let local_child = rollout
        .fork_branch(
            EntityName::new("child").unwrap(),
            LocalForkSource::Branch {
                branch_id: branch,
                commit_id,
            },
        )
        .unwrap();
    assert_ne!(branch, local_child);
    assert_eq!(
        rollout
            .branch(local_child)
            .unwrap()
            .unwrap()
            .forked_from_commit_id,
        Some(commit_id)
    );

    let replica = BranchStore::create(&replica_path, authority.store_id()).unwrap();
    replica
        .pull_branch(
            authority.clone(),
            branch,
            commit_id,
            RemotePlacement::Replica,
        )
        .unwrap();
    let replica_root = replica.branch_root(branch).unwrap();
    assert!(replica.root_complete(replica_root).unwrap());
    assert!(replica
        .root_complete(authority.layer(genesis).unwrap().unwrap().root_id)
        .unwrap());

    drop(replica);
    drop(rollout);
    drop(branches);
    drop(authority);
    std::fs::remove_file(replica_path).unwrap();
    std::fs::remove_file(rollout_path).unwrap();
    std::fs::remove_file(branch_path).unwrap();
    std::fs::remove_file(authority_path).unwrap();
}
