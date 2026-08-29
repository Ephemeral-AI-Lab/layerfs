use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_sdk::{Change, Direct, RefOutcome, RemoteEndpoint};
use layerfs_stack_store::StackStore;
use layerfs_storage_core::internal::{
    reset_transfer_authentication_counts, transfer_authentication_counts,
};
use std::sync::Arc;

#[test]
fn embedded_remote_push_and_remote_pull_hash_only_at_required_trust_boundaries() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-hash-counts-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();

    let embedded_layer = Arc::new(LayerStore::open(root.join("embedded-layer.sqlite")).unwrap());
    let (history, genesis) = embedded_layer.provision().unwrap();
    let embedded_branch =
        BranchStore::open(root.join("embedded-branch.sqlite"), embedded_layer).unwrap();
    let branch = embedded_branch
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    embedded_branch
        .commit(
            branch.id,
            branch.head_commit_id,
            &[Change::Write {
                path: "embedded".into(),
                bytes: b"one hash".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap();
    reset_transfer_authentication_counts();
    embedded_branch.push_branch(branch.id).unwrap();
    let (traversal, receiver) = transfer_authentication_counts();
    assert!(traversal > 0);
    assert_eq!(receiver, 0);
    drop(embedded_branch);

    let layer = Arc::new(LayerStore::open(root.join("remote-layer.sqlite")).unwrap());
    let (history, genesis) = layer.provision().unwrap();
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server_store = layer.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut output = stream.try_clone().unwrap();
        while layerfs_layer_store::serve_once(&server_store, &mut stream, &mut output).is_ok() {}
    });
    let remote = RemoteEndpoint::connect(address).unwrap();
    let parent = Arc::new(remote.clone());
    let branch_store =
        BranchStore::open(root.join("remote-branch.sqlite"), parent.clone()).unwrap();
    let direct = Direct::from_parts(branch_store, remote.clone());
    let branch = direct
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    let RefOutcome::Created(_) = direct
        .commit(
            branch.id,
            branch.head_commit_id,
            &[Change::Write {
                path: "remote-push".into(),
                bytes: b"two hashes".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    else {
        panic!("expected Commit")
    };
    reset_transfer_authentication_counts();
    direct.push_branch(branch.id).unwrap();
    let (sender, receiver) = transfer_authentication_counts();
    assert!(sender > 0);
    assert_eq!(receiver, sender);

    let stack = StackStore::open(root.join("pull-stack.sqlite"), parent.clone()).unwrap();
    reset_transfer_authentication_counts();
    stack.pull_layer_history(history.id, genesis.id).unwrap();
    let (receiver_traversal, destination_rehash) = transfer_authentication_counts();
    assert!(receiver_traversal > 0);
    assert_eq!(destination_rehash, 0);

    drop(stack);
    drop(direct);
    drop(parent);
    drop(remote);
    server.join().unwrap();
    drop(layer);
    std::fs::remove_dir_all(root).unwrap();
}
