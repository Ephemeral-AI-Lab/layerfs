use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_materialization::materialize;
use layerfs_storage_core::Change;
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;

#[test]
fn snapshot_materializes_without_owning_storage() {
    let run = std::env::temp_dir().join(format!(
        "layerfs-materialize-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&run).unwrap();
    let layer = Arc::new(LayerStore::open(run.join("layer.sqlite")).unwrap());
    let (history, genesis) = layer.provision().unwrap();
    let branch = BranchStore::open(run.join("branch.sqlite"), layer.clone()).unwrap();
    let created = branch
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    branch
        .commit(
            created.id,
            created.head_commit_id,
            &[
                Change::Mkdir {
                    path: "dir".into(),
                    mode: 0o755,
                },
                Change::Write {
                    path: "dir/file".into(),
                    bytes: b"materialized".to_vec(),
                    mode: 0o640,
                },
                Change::HardLink {
                    source: "dir/file".into(),
                    target: "hard".into(),
                },
                Change::Symlink {
                    path: "link".into(),
                    target: b"dir/file".to_vec(),
                },
            ],
        )
        .unwrap();
    let output = run.join("output");
    materialize(&branch, created.id, &output).unwrap();
    assert_eq!(
        std::fs::read(output.join("dir/file")).unwrap(),
        b"materialized"
    );
    assert_eq!(
        std::fs::metadata(output.join("dir/file")).unwrap().ino(),
        std::fs::metadata(output.join("hard")).unwrap().ino()
    );
    assert_eq!(
        std::fs::read_link(output.join("link")).unwrap(),
        std::path::Path::new("dir/file")
    );
    drop(branch);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}
