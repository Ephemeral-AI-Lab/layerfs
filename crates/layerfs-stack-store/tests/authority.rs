use layerfs_layer_store::LayerStore;
use layerfs_stack_store::StackStore;
use layerfs_storage::{
    read_value, write_value, BaseId, BranchId, BranchRecord, CommitId, CommitRecord, EndpointReply,
    EndpointRequest, Fact, FactKind, FrameKind, StackPush, StorageError, StorageId, TransferIntent,
};
use std::sync::Arc;

fn run_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-stack-{name}-{}-{}",
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
fn signer_survives_reopen_and_pulled_history_is_read_only() {
    let run = run_dir("authority");
    let layer = Arc::new(LayerStore::create(run.join("layer.sqlite")).unwrap());
    let (_layer_history, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let creator_path = run.join("creator.sqlite");
    let creator = StackStore::create(&creator_path, layer.clone()).unwrap();
    creator.pull_layer(genesis.id).unwrap();
    let (first_history, seed) = creator.create_stack(genesis.id).unwrap();
    creator.push_stack(seed.id).unwrap();
    drop(creator);

    let reopened = StackStore::connect(&creator_path, layer.clone()).unwrap();
    let (second_history, _) = reopened.create_stack(genesis.id).unwrap();
    assert_eq!(
        first_history.id.verification_key_digest(),
        second_history.id.verification_key_digest()
    );
    drop(reopened);

    let pulled = StackStore::create(run.join("pulled.sqlite"), layer.clone()).unwrap();
    pulled.pull_stack(seed.id).unwrap();
    assert!(matches!(
        layerfs_storage::StoreEndpoint::add_stack(
            &pulled,
            first_history.id,
            BranchId::new(),
            CommitId::derive(genesis.root_id, None, None)
        ),
        Err(StorageError::ReadOnlyStackHistory(_))
    ));
    drop(pulled);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn stack_remote_rejects_unbound_transfer_phases_and_mismatched_final() {
    let run = run_dir("remote-session");
    let layer = Arc::new(LayerStore::create(run.join("layer.sqlite")).unwrap());
    let (_layer_history, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let stack_path = run.join("stack.sqlite");
    let store = StackStore::create(&stack_path, layer).unwrap();
    store.pull_layer(genesis.id).unwrap();
    let (history, seed) = store.create_stack(genesis.id).unwrap();
    let commit = CommitRecord {
        id: CommitId::derive(seed.root_id, None, None),
        root_id: seed.root_id,
        parent_id: None,
        merge_parent_id: None,
    };
    let branch_a = BranchRecord {
        id: BranchId::new(),
        head_commit_id: commit.id,
        base_id: BaseId::Stack(seed.id),
    };
    let branch_b = BranchRecord {
        id: BranchId::new(),
        ..branch_a
    };
    let push = StackPush {
        history_id: history.id,
        base_layer_id: genesis.id,
        expected_head: Some(seed.id),
        incoming_head: seed.id,
        fact_count: 0,
        root_count: 0,
        provenance_digest: [0; 32],
        publication_count: 0,
        publication_digest: [0; 32],
        public_key: [0; 32],
        signature: [0; 64],
    };
    let baseline = stack_transfer_rows(&stack_path);
    let cases = vec![
        vec![EndpointRequest::Transfer {
            objects: Vec::new(),
            facts: vec![Fact::Commit(commit)],
            object_ids: Vec::new(),
            fact_kind: None,
            fact_ids: Vec::new(),
        }],
        vec![EndpointRequest::TransferEnd {
            objects: Vec::new(),
            facts: vec![Fact::Commit(commit)],
            intent: Box::new(TransferIntent::None),
        }],
        vec![
            EndpointRequest::TransferBeginBranch {
                branch: branch_a,
                root: seed.root_id,
            },
            EndpointRequest::TransferEnd {
                objects: Vec::new(),
                facts: vec![Fact::Commit(commit)],
                intent: Box::new(TransferIntent::Branch {
                    branch: branch_b,
                    expected: None,
                }),
            },
        ],
        vec![
            EndpointRequest::TransferBeginBranch {
                branch: branch_a,
                root: seed.root_id,
            },
            EndpointRequest::TransferEnd {
                objects: Vec::new(),
                facts: Vec::new(),
                intent: Box::new(TransferIntent::Stack(push.clone())),
            },
        ],
        vec![
            EndpointRequest::TransferBeginBranch {
                branch: branch_a,
                root: seed.root_id,
            },
            EndpointRequest::Transfer {
                objects: Vec::new(),
                facts: vec![Fact::Branch(branch_a)],
                object_ids: Vec::new(),
                fact_kind: Some(FactKind::Branch),
                fact_ids: Vec::new(),
            },
        ],
        vec![
            EndpointRequest::TransferBeginBranch {
                branch: branch_a,
                root: seed.root_id,
            },
            EndpointRequest::TransferEnd {
                objects: Vec::new(),
                facts: vec![Fact::Commit(commit)],
                intent: Box::new(TransferIntent::Branch {
                    branch: branch_a,
                    expected: Some(commit.id),
                }),
            },
        ],
    ];
    for requests in cases {
        assert!(serve_stack_requests(&store, &requests).is_err());
        assert_eq!(stack_transfer_rows(&stack_path), baseline);
    }
    assert!(rusqlite::Connection::open(&stack_path)
        .unwrap()
        .query_row(
            "SELECT NOT EXISTS(SELECT 1 FROM branches WHERE branch_id=?1)",
            [branch_a.id.as_slice()],
            |row| row.get::<_, bool>(0),
        )
        .unwrap());
    drop(store);
    std::fs::remove_dir_all(run).unwrap();
}

fn serve_stack_requests(store: &StackStore, requests: &[EndpointRequest]) -> EndpointReply {
    let mut input = Vec::new();
    for request in requests {
        write_value(&mut input, FrameKind::Command, request).unwrap();
    }
    let mut output = Vec::new();
    layerfs_stack_store::serve_once(store, &mut input.as_slice(), &mut output).unwrap();
    let mut output = output.as_slice();
    let mut reply = read_value(&mut output, FrameKind::Reply).unwrap();
    while !output.is_empty() {
        reply = read_value(&mut output, FrameKind::Reply).unwrap();
    }
    reply
}

fn stack_transfer_rows(path: &std::path::Path) -> (i64, i64, i64, i64) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let count = |table| {
        connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    (
        count("commits"),
        count("branches"),
        count("stacks"),
        count("add_results"),
    )
}
