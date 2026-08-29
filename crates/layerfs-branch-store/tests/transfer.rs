use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_storage_core::{BranchId, Change, RefOutcome, StorageError};
use std::sync::Arc;

#[test]
fn two_id_pull_preserves_local_ahead_and_rejects_divergence() {
    let run = std::env::temp_dir().join(format!(
        "layerfs-pull-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&run).unwrap();
    let layer = Arc::new(LayerStore::open(run.join("layer.sqlite")).unwrap());
    let (history, genesis) = layer.provision().unwrap();
    let source = BranchStore::open(run.join("source.sqlite"), layer.clone()).unwrap();
    let remote = source
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    source.push_branch(remote.id).unwrap();
    let local = BranchStore::open(run.join("local.sqlite"), layer.clone()).unwrap();
    assert_eq!(
        local.pull_branch(remote.id, remote.id).unwrap(),
        RefOutcome::Created(remote.head_commit_id)
    );

    let source_head =
        created(source.commit(remote.id, remote.head_commit_id, &[write("source", b"one")]));
    source.push_branch(remote.id).unwrap();
    assert_eq!(
        local.pull_branch(remote.id, remote.id).unwrap(),
        RefOutcome::FastForwarded(source_head)
    );

    let local_head = created(local.commit(remote.id, source_head, &[write("local", b"ahead")]));
    assert_eq!(
        local.pull_branch(remote.id, remote.id).unwrap(),
        RefOutcome::UpToDate(local_head)
    );

    let next_source =
        created(source.commit(remote.id, source_head, &[write("remote", b"diverged")]));
    source.push_branch(remote.id).unwrap();
    assert!(matches!(
        local.pull_branch(remote.id, remote.id),
        Err(StorageError::CommitHeadMoved(_))
    ));
    assert_eq!(
        local.branch(remote.id).unwrap().unwrap().head_commit_id,
        local_head
    );

    let fresh_id = BranchId::new();
    assert_eq!(
        local.pull_branch(remote.id, fresh_id).unwrap(),
        RefOutcome::Created(next_source)
    );
    let occupied = local
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    assert!(matches!(
        local.pull_branch(remote.id, occupied.id),
        Err(StorageError::CommitHeadMoved(_))
    ));
    assert_eq!(local.branch(occupied.id).unwrap().unwrap(), occupied);
    drop(local);
    drop(source);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

fn created(
    result: layerfs_storage_core::Result<RefOutcome<layerfs_storage_core::CommitId>>,
) -> layerfs_storage_core::CommitId {
    match result.unwrap() {
        RefOutcome::Created(id) => id,
        other => panic!("expected created Commit, got {other:?}"),
    }
}

fn write(path: &str, bytes: &[u8]) -> Change {
    Change::Write {
        path: path.into(),
        bytes: bytes.to_vec(),
        mode: 0o644,
    }
}
