use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_stack_store::StackStore;
use layerfs_storage_core::{AddLayerSource, Change, RefOutcome, StorageError};
use std::sync::Arc;

#[test]
fn direct_and_stacked_conflicts_move_no_head_or_object_count() {
    direct_conflict();
    stacked_conflict();
}

fn direct_conflict() {
    let run = run_dir("direct");
    let layer_path = run.join("layer.sqlite");
    let layer = Arc::new(LayerStore::open(&layer_path).unwrap());
    let (history, genesis) = layer.provision().unwrap();
    let branch = BranchStore::open(run.join("branch.sqlite"), layer.clone()).unwrap();
    let left = committed(&branch, history.id, genesis.id, None, b"left");
    let right = committed(&branch, history.id, genesis.id, None, b"right");
    branch.push_branch(left.0).unwrap();
    branch.push_branch(right.0).unwrap();
    layer
        .add_layer(
            history.id,
            AddLayerSource::BranchSource {
                branch_id: left.0,
                commit_id: left.1,
            },
        )
        .unwrap();
    let before_head = layer.layer_history(history.id).unwrap().unwrap();
    let before_objects = object_count(&layer_path);
    assert!(matches!(
        layer.add_layer(
            history.id,
            AddLayerSource::BranchSource {
                branch_id: right.0,
                commit_id: right.1,
            }
        ),
        Err(StorageError::Conflict(_))
    ));
    assert_eq!(
        layer.layer_history(history.id).unwrap().unwrap(),
        before_head
    );
    assert_eq!(object_count(&layer_path), before_objects);
    drop(branch);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

fn stacked_conflict() {
    let run = run_dir("stacked");
    let layer = Arc::new(LayerStore::open(run.join("layer.sqlite")).unwrap());
    let (layer_history, genesis) = layer.provision().unwrap();
    let stack_path = run.join("stack.sqlite");
    let stack = Arc::new(StackStore::open(&stack_path, layer.clone()).unwrap());
    stack
        .pull_layer_history(layer_history.id, genesis.id)
        .unwrap();
    let (history, seed) = stack
        .create_stack_history_from_layer(layer_history.id, genesis.id)
        .unwrap();
    let branch = BranchStore::open(run.join("branch.sqlite"), stack.clone()).unwrap();
    let left = committed(
        &branch,
        layer_history.id,
        genesis.id,
        Some((history.id, seed.id)),
        b"left",
    );
    let right = committed(
        &branch,
        layer_history.id,
        genesis.id,
        Some((history.id, seed.id)),
        b"right",
    );
    branch.push_branch(left.0).unwrap();
    branch.push_branch(right.0).unwrap();
    stack.add_stack(history.id, left.0, left.1).unwrap();
    let before_head = stack.stack_history(history.id).unwrap().unwrap();
    let before_objects = object_count(&stack_path);
    assert!(matches!(
        stack.add_stack(history.id, right.0, right.1),
        Err(StorageError::Conflict(_))
    ));
    assert_eq!(
        stack.stack_history(history.id).unwrap().unwrap(),
        before_head
    );
    assert_eq!(object_count(&stack_path), before_objects);
    drop(branch);
    drop(stack);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

fn committed(
    store: &BranchStore,
    layer_history: layerfs_storage_core::LayerHistoryId,
    layer: layerfs_storage_core::LayerId,
    stack: Option<(
        layerfs_storage_core::StackHistoryId,
        layerfs_storage_core::StackId,
    )>,
    bytes: &[u8],
) -> (
    layerfs_storage_core::BranchId,
    layerfs_storage_core::CommitId,
) {
    let branch = match stack {
        Some((history, stack)) => store.create_branch_from_stack(history, stack).unwrap(),
        None => store
            .create_branch_from_layer(layer_history, layer)
            .unwrap(),
    };
    let commit = match store
        .commit(
            branch.id,
            branch.head_commit_id,
            &[Change::Write {
                path: "same".into(),
                bytes: bytes.to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    {
        RefOutcome::Created(id) => id,
        _ => panic!(),
    };
    (branch.id, commit)
}

fn run_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-conflict-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn object_count(path: &std::path::Path) -> u64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("SELECT count(*) FROM objects", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap() as u64
}
