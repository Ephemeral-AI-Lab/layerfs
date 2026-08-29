use layerfs_branch_store::BranchStore;
use layerfs_content::filesystem::ContentChange;
use layerfs_layer_store::LayerStore;
use layerfs_stack_store::StackStore;
use layerfs_storage::{BranchCommit, BranchSource, LayerSource, MergeOutcome, StorageError};
use std::sync::Arc;

fn run_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-branch-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn merge_falls_back_to_common_layer_without_moving_target_base() {
    let run = run_dir("cross-layer");
    let layer = Arc::new(LayerStore::create(run.join("layer.sqlite")).unwrap());
    let (_history, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let store = BranchStore::create(run.join("branch.sqlite"), layer.clone()).unwrap();

    let accepted = store
        .create_branch(BranchSource::Layer(genesis.id))
        .unwrap();
    let accepted_head = match store
        .commit(
            accepted.id,
            accepted.head_commit_id,
            &[ContentChange::Write {
                path: "accepted".into(),
                bytes: b"layer".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    {
        layerfs_storage::RefOutcome::Created(id) => id,
        _ => panic!(),
    };
    store.push_branch(accepted.id).unwrap();
    let next_layer = layer
        .add_layer(LayerSource::BranchCommit(BranchCommit {
            branch_id: accepted.id,
            commit_id: accepted_head,
        }))
        .unwrap()
        .result_id;

    let target = store
        .create_branch(BranchSource::Layer(next_layer))
        .unwrap();
    let source = store
        .create_branch(BranchSource::Layer(genesis.id))
        .unwrap();
    let source_head = match store
        .commit(
            source.id,
            source.head_commit_id,
            &[ContentChange::Write {
                path: "private".into(),
                bytes: b"branch".to_vec(),
                mode: 0o600,
            }],
        )
        .unwrap()
    {
        layerfs_storage::RefOutcome::Created(id) => id,
        _ => panic!(),
    };
    assert_ne!(source_head, target.head_commit_id);
    let MergeOutcome::Merged(_) = store.merge(source.id, target.id).unwrap() else {
        panic!("expected merge Commit")
    };
    let merged = store.branch(target.id).unwrap().unwrap();
    assert_eq!(merged.base_id, target.base_id);
    assert_eq!(store.read_path(target.id, "accepted").unwrap(), b"layer");
    assert_eq!(store.read_path(target.id, "private").unwrap(), b"branch");
    drop(store);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn merge_rejects_different_layer_histories_without_moving_target() {
    let run = run_dir("no-common-base");
    let layer = Arc::new(LayerStore::create(run.join("layer.sqlite")).unwrap());
    let (_left_history, left_layer) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let (_right_history, right_layer) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let store = BranchStore::create(run.join("branch.sqlite"), layer.clone()).unwrap();
    let left = store
        .create_branch(BranchSource::Layer(left_layer.id))
        .unwrap();
    let right = store
        .create_branch(BranchSource::Layer(right_layer.id))
        .unwrap();
    assert!(matches!(
        store.merge(left.id, right.id),
        Err(StorageError::NoCommonBase)
    ));
    assert_eq!(store.branch(right.id).unwrap().unwrap(), right);
    drop(store);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn merge_falls_back_to_closest_common_stack() {
    let run = run_dir("cross-stack");
    let layer = Arc::new(LayerStore::create(run.join("layer.sqlite")).unwrap());
    let (_layer_history, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let stack = Arc::new(StackStore::create(run.join("stack.sqlite"), layer.clone()).unwrap());
    stack.pull_layer(genesis.id).unwrap();
    let (_stack_history, seed) = stack.create_stack(genesis.id).unwrap();
    let store = BranchStore::create(run.join("branch.sqlite"), stack.clone()).unwrap();

    let accepted = store.create_branch(BranchSource::Stack(seed.id)).unwrap();
    let accepted_head = match store
        .commit(
            accepted.id,
            accepted.head_commit_id,
            &[ContentChange::Write {
                path: "accepted".into(),
                bytes: b"stack".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    {
        layerfs_storage::RefOutcome::Created(id) => id,
        _ => panic!(),
    };
    store.push_branch(accepted.id).unwrap();
    let next_stack = stack
        .add_stack(BranchCommit {
            branch_id: accepted.id,
            commit_id: accepted_head,
        })
        .unwrap()
        .result_id;
    let target = store
        .create_branch(BranchSource::Stack(next_stack))
        .unwrap();
    let source = store.create_branch(BranchSource::Stack(seed.id)).unwrap();
    store
        .commit(
            source.id,
            source.head_commit_id,
            &[ContentChange::Write {
                path: "private".into(),
                bytes: b"branch".to_vec(),
                mode: 0o600,
            }],
        )
        .unwrap();
    let MergeOutcome::Merged(_) = store.merge(source.id, target.id).unwrap() else {
        panic!("expected merge Commit")
    };
    assert_eq!(store.read_path(target.id, "accepted").unwrap(), b"stack");
    assert_eq!(store.read_path(target.id, "private").unwrap(), b"branch");
    drop(store);
    drop(stack);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}
