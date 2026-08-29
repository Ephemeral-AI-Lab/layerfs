use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_storage_core::{Change, RefOutcome};
use layerfs_workspace::{ResourcePolicy, Workspace, WorkspaceState, ROOT};
use std::sync::Arc;

fn fixture(name: &str) -> (std::path::PathBuf, Workspace) {
    fixture_with_policy(
        name,
        ResourcePolicy {
            max_spool_bytes: 1024 * 1024,
        },
    )
}

fn fixture_with_policy(name: &str, policy: ResourcePolicy) -> (std::path::PathBuf, Workspace) {
    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let layer = Arc::new(LayerStore::open(root.join("layer.sqlite")).unwrap());
    let (history, genesis) = layer.provision().unwrap();
    let store = BranchStore::open(root.join("branch.sqlite"), layer).unwrap();
    let branch = store
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    let workspace =
        Workspace::open_with_policy(store, branch.id, root.join("spool"), policy).unwrap();
    (root, workspace)
}

#[test]
fn overlay_open_unlink_and_commit_are_transient() {
    let (root, mut workspace) = fixture("lifecycle");
    let file = workspace.create_file(ROOT, b"file", 0o644).unwrap();
    workspace.pin(file.node, false).unwrap();
    workspace.write(file.node, 0, b"kept-open").unwrap();
    workspace.unlink(ROOT, b"file", false).unwrap();
    assert_eq!(workspace.read(file.node, 0, 32).unwrap(), b"kept-open");
    workspace.unpin(file.node).unwrap();
    let final_file = workspace.create_file(ROOT, b"final", 0o600).unwrap();
    workspace.write(final_file.node, 0, b"final-bytes").unwrap();
    assert!(matches!(
        workspace.commit().unwrap(),
        RefOutcome::Created(_)
    ));
    assert_eq!(workspace.state(), WorkspaceState::Committed);
    assert!(std::fs::read_dir(root.join("spool"))
        .unwrap()
        .all(|entry| !entry.unwrap().path().to_string_lossy().contains("sqlite")));
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn spool_limit_is_reclaimed_after_open_unlink_release() {
    let (root, mut workspace) =
        fixture_with_policy("spool-limit", ResourcePolicy { max_spool_bytes: 4 });
    let file = workspace.create_file(ROOT, b"first", 0o600).unwrap();
    workspace.pin(file.node, false).unwrap();
    workspace.write(file.node, 0, b"1234").unwrap();
    assert!(workspace.write(file.node, 4, b"5").is_err());
    workspace.unlink(ROOT, b"first", false).unwrap();
    workspace.unpin(file.node).unwrap();
    let next = workspace.create_file(ROOT, b"next", 0o600).unwrap();
    workspace.write(next.node, 0, b"5678").unwrap();
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn append_then_truncate_finalizes_from_the_final_overlay_ranges() {
    let (root, mut workspace) = fixture("append-truncate");
    let file = workspace.create_file(ROOT, b"file", 0o600).unwrap();
    workspace.write(file.node, 0, b"alpha").unwrap();
    workspace.write(file.node, 5, b"-beta").unwrap();
    workspace.truncate(file.node, 7).unwrap();
    assert_eq!(workspace.read(file.node, 0, 32).unwrap(), b"alpha-b");
    assert!(matches!(
        workspace.commit().unwrap(),
        RefOutcome::Created(_)
    ));
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sparse_overlay_commits_a_64_mib_base_without_hydrating_it() {
    use std::os::unix::fs::MetadataExt;

    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-large-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let layer = Arc::new(LayerStore::open(root.join("layer.sqlite")).unwrap());
    let (history, genesis) = layer.provision().unwrap();
    let store = BranchStore::open(root.join("branch.sqlite"), layer).unwrap();
    let branch = store
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    let RefOutcome::Created(_head) = store
        .commit(
            branch.id,
            branch.head_commit_id,
            &[Change::Write {
                path: "large".into(),
                bytes: vec![b'a'; 64 * 1024 * 1024],
                mode: 0o644,
            }],
        )
        .unwrap()
    else {
        panic!("expected Commit")
    };
    let mut workspace = Workspace::open_with_policy(
        store.clone(),
        branch.id,
        root.join("spool"),
        ResourcePolicy {
            max_spool_bytes: 1024 * 1024,
        },
    )
    .unwrap();
    let file = workspace.lookup(ROOT, b"large").unwrap();
    workspace.write(file.node, 32 * 1024 * 1024, b"z").unwrap();
    let spool = std::fs::read_dir(root.join("spool"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let metadata = std::fs::metadata(spool).unwrap();
    assert_eq!(metadata.len(), 64 * 1024 * 1024);
    assert!(metadata.blocks() * 512 < 1024 * 1024);
    assert!(matches!(
        workspace.commit().unwrap(),
        RefOutcome::Created(_)
    ));
    let mut reopened = Workspace::open(store, branch.id, root.join("reopen-spool")).unwrap();
    let reopened_file = reopened.lookup(ROOT, b"large").unwrap();
    assert_eq!(
        reopened
            .read(reopened_file.node, 32 * 1024 * 1024 - 1, 3)
            .unwrap(),
        b"aza"
    );
    drop(reopened);
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn directory_parent_and_descendant_rename_are_correct() {
    let (root, mut workspace) = fixture("directory-parent");
    let parent = workspace.mkdir(ROOT, b"parent", 0o755).unwrap();
    let child = workspace.mkdir(parent.node, b"child", 0o755).unwrap();
    let entries = workspace.readdir(child.node).unwrap();
    assert!(entries
        .iter()
        .any(|(node, _, name)| *node == parent.node && name == b".."));
    assert!(workspace
        .rename(ROOT, b"parent", child.node, b"loop", false)
        .is_err());
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}
