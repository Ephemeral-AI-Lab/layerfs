use layerfs_branch_store::BranchStore;
use layerfs_content::filesystem::ContentChange;
use layerfs_layer_store::LayerStore;
use layerfs_storage::{BranchSource, RefOutcome, StorageError};
use std::sync::Arc;

#[test]
fn pull_preserves_global_identity_and_rejects_divergence() {
    let run = std::env::temp_dir().join(format!(
        "layerfs-pull-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&run).unwrap();
    let layer = Arc::new(LayerStore::create(run.join("layer.sqlite")).unwrap());
    let (_history, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let source = BranchStore::create(run.join("source.sqlite"), layer.clone()).unwrap();
    let remote = source
        .create_branch(BranchSource::Layer(genesis.id))
        .unwrap();
    source.push_branch(remote.id).unwrap();
    let local = BranchStore::create(run.join("local.sqlite"), layer.clone()).unwrap();
    assert_eq!(
        local.pull_branch(remote.id).unwrap().1,
        RefOutcome::Created(remote.head_commit_id)
    );

    let source_head =
        created(source.commit(remote.id, remote.head_commit_id, &[write("source", b"one")]));
    source.push_branch(remote.id).unwrap();
    assert_eq!(
        local.pull_branch(remote.id).unwrap().1,
        RefOutcome::FastForwarded(source_head)
    );

    let local_head = created(local.commit(remote.id, source_head, &[write("local", b"ahead")]));
    assert_eq!(
        local.pull_branch(remote.id).unwrap().1,
        RefOutcome::UpToDate(local_head)
    );

    let next_source =
        created(source.commit(remote.id, source_head, &[write("remote", b"diverged")]));
    source.push_branch(remote.id).unwrap();
    assert!(matches!(
        local.pull_branch(remote.id),
        Err(StorageError::CommitHeadMoved(_))
    ));
    assert_eq!(
        local.branch(remote.id).unwrap().unwrap().head_commit_id,
        local_head
    );

    assert_ne!(next_source, local_head);
    drop(local);
    drop(source);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

fn created(
    result: layerfs_storage::Result<RefOutcome<layerfs_storage::CommitId>>,
) -> layerfs_storage::CommitId {
    match result.unwrap() {
        RefOutcome::Created(id) => id,
        other => panic!("expected created Commit, got {other:?}"),
    }
}

fn write(path: &str, bytes: &[u8]) -> ContentChange {
    ContentChange::Write {
        path: path.into(),
        bytes: bytes.to_vec(),
        mode: 0o644,
    }
}
