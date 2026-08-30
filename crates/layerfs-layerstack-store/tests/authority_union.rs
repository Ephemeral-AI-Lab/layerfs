#![cfg(feature = "test-instrumentation")]

use layerfs_layerstack_store::LayerStackStore;
use layerfs_storage::{
    BranchId, BranchRecord, CommitId, CommitRecord, EntityName, Fact, LayerStackEndpoint,
    LayerStackInitialization, PushResult,
};

#[test]
fn authority_walks_all_owned_suffix_pages_and_prunes_equal_roots() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-v2-authority-union-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = LayerStackStore::create(&path).unwrap();
    let initialized = store
        .initialize_layerstack(
            EntityName::new("api-server").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let root_id = store
        .layer(initialized.genesis_layer_id)
        .unwrap()
        .unwrap()
        .root_id;
    let closure_len = layerfs_storage::dependency_order(&store, root_id)
        .unwrap()
        .len();
    let mut parent = None;
    for _ in 0..270 {
        let commit = CommitRecord {
            id: CommitId::derive(root_id, parent, initialized.genesis_layer_id),
            root_id,
            parent_commit_id: parent,
            base_layer_id: initialized.genesis_layer_id,
        };
        LayerStackEndpoint::admit_facts(&store, &[Fact::Commit(commit)]).unwrap();
        parent = Some(commit.id);
    }
    let branch = BranchRecord {
        id: BranchId::new(),
        layer_stack_id: initialized.layer_stack_id,
        name: EntityName::new("main").unwrap(),
        base_layer_id: initialized.genesis_layer_id,
        head_commit_id: parent,
        forked_from_layer_id: Some(initialized.genesis_layer_id),
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    };

    layerfs_storage::reset_sql_trace();
    assert!(matches!(
        LayerStackEndpoint::publish_branch(&store, &branch, None).unwrap(),
        PushResult::Created { .. }
    ));
    assert!(closure_len > 0);
    assert_eq!(
        layerfs_storage::sql_trace()
            .iter()
            .filter(|sql| sql.contains("SELECT bytes FROM objects WHERE object_id="))
            .count(),
        0,
        "equal positional roots must not re-read their complete payload closure",
    );

    drop(store);
    std::fs::remove_file(path).unwrap();
}
