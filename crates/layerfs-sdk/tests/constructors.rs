use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_stack_store::StackStore;
use layerfs_storage::StorageError;
use std::sync::Arc;

#[test]
fn stack_and_branch_connect_reject_a_different_parent_without_writes() {
    let root = run_dir("parent-binding");
    let first = Arc::new(LayerStore::create(root.join("first-layer.sqlite")).unwrap());
    let second = Arc::new(LayerStore::create(root.join("second-layer.sqlite")).unwrap());
    let stack_path = root.join("stack.sqlite");
    let stack = Arc::new(StackStore::create(&stack_path, first.clone()).unwrap());
    let branch_path = root.join("branch.sqlite");
    let branch = BranchStore::create(&branch_path, stack.clone()).unwrap();
    drop(branch);
    drop(stack);

    assert!(matches!(
        StackStore::connect(&stack_path, second.clone()),
        Err(StorageError::WrongParent)
    ));
    let stack = Arc::new(StackStore::connect(&stack_path, first.clone()).unwrap());
    assert!(matches!(
        BranchStore::connect(&branch_path, second),
        Err(StorageError::WrongParent)
    ));
    BranchStore::connect(&branch_path, stack).unwrap();

    drop(first);
    std::fs::remove_dir_all(root).unwrap();
}

fn run_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-sdk-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
