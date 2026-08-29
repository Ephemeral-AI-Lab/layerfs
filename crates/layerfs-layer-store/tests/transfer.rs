use layerfs_core::ObjectId;
use layerfs_layer_store::LayerStore;
use layerfs_storage_core::{
    read_value, write_frame_bytes, write_value, BaseId, BranchId, BranchRecord, CommitId,
    CommitRecord, EndpointReply, EndpointRequest, Fact, FactKind, FrameKind, StackHistoryId,
    StackId, StackPush, StorageId, TransferIntent,
};

#[test]
fn incomplete_object_frame_admits_nothing() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-layer-incomplete-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store = LayerStore::open(root.join("layer.sqlite")).unwrap();
    let (_, base) = store.provision().unwrap();
    let commit = CommitRecord {
        id: CommitId::derive(base.root_id, None, None),
        root_id: base.root_id,
        parent_id: None,
        merge_parent_id: None,
    };
    let branch = BranchRecord {
        id: BranchId::new(),
        head_commit_id: commit.id,
        base_id: BaseId::Layer(base.id),
    };
    let baseline = table_count(&root.join("layer.sqlite"), "objects");
    let payload = b"incomplete";
    let mut input = Vec::new();
    write_value(
        &mut input,
        FrameKind::Command,
        &EndpointRequest::TransferBeginBranch {
            branch,
            root: base.root_id,
        },
    )
    .unwrap();
    write_value(
        &mut input,
        FrameKind::Command,
        &EndpointRequest::TransferEnd {
            objects: vec![(ObjectId::for_bytes(payload), payload.len() as u64)],
            facts: vec![Fact::Commit(commit)],
            intent: Box::new(TransferIntent::Branch {
                branch,
                expected: None,
            }),
        },
    )
    .unwrap();
    write_frame_bytes(&mut input, FrameKind::Payload, payload).unwrap();
    input.truncate(input.len() - 1);
    let mut output = Vec::new();
    layerfs_layer_store::serve_once(&store, &mut input.as_slice(), &mut output).unwrap();
    let reply = last_reply(&output);
    assert!(reply.is_err());
    assert_eq!(table_count(&root.join("layer.sqlite"), "objects"), baseline);
    assert_eq!(table_count(&root.join("layer.sqlite"), "branches"), 0);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn complete_wrong_identity_is_rejected_by_receiver_admission() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-layer-identity-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store = LayerStore::open(root.join("layer.sqlite")).unwrap();
    let (_, base) = store.provision().unwrap();
    let commit = CommitRecord {
        id: CommitId::derive(base.root_id, None, None),
        root_id: base.root_id,
        parent_id: None,
        merge_parent_id: None,
    };
    let branch = BranchRecord {
        id: BranchId::new(),
        head_commit_id: commit.id,
        base_id: BaseId::Layer(base.id),
    };
    let baseline = table_count(&root.join("layer.sqlite"), "objects");
    let payload = layerfs_core::encode_bytes_object(b"authenticated once").unwrap();
    let mut input = Vec::new();
    write_value(
        &mut input,
        FrameKind::Command,
        &EndpointRequest::TransferBeginBranch {
            branch,
            root: base.root_id,
        },
    )
    .unwrap();
    write_value(
        &mut input,
        FrameKind::Command,
        &EndpointRequest::TransferEnd {
            objects: vec![(ObjectId::for_bytes(b"wrong"), payload.len() as u64)],
            facts: vec![Fact::Commit(commit)],
            intent: Box::new(TransferIntent::Branch {
                branch,
                expected: None,
            }),
        },
    )
    .unwrap();
    write_frame_bytes(&mut input, FrameKind::Payload, &payload).unwrap();
    let mut output = Vec::new();
    layerfs_layer_store::serve_once(&store, &mut input.as_slice(), &mut output).unwrap();
    let reply = last_reply(&output);
    assert!(reply.is_err());
    assert_eq!(table_count(&root.join("layer.sqlite"), "objects"), baseline);
    assert_eq!(table_count(&root.join("layer.sqlite"), "branches"), 0);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn remote_transfer_requires_pinned_begin_and_exact_final_intent() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-layer-session-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let database = root.join("layer.sqlite");
    let store = LayerStore::open(&database).unwrap();
    let (_, base) = store.provision().unwrap();
    let commit = CommitRecord {
        id: CommitId::derive(base.root_id, None, None),
        root_id: base.root_id,
        parent_id: None,
        merge_parent_id: None,
    };
    let branch_a = BranchRecord {
        id: BranchId::new(),
        head_commit_id: commit.id,
        base_id: BaseId::Layer(base.id),
    };
    let branch_b = BranchRecord {
        id: BranchId::new(),
        ..branch_a
    };
    let history_id = StackHistoryId::new(&[7; 32]);
    let incoming = StackId::derive(history_id, None, base.root_id);
    let push = StackPush {
        history_id,
        base_layer_id: base.id,
        expected_head: None,
        incoming_head: incoming,
        fact_count: 0,
        root_count: 0,
        provenance_digest: [0; 32],
        publication_count: 0,
        publication_digest: [0; 32],
        public_key: [7; 32],
        signature: [0; 64],
    };
    let baseline = transfer_rows(&database);
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
                root: base.root_id,
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
                root: base.root_id,
            },
            EndpointRequest::TransferEnd {
                objects: Vec::new(),
                facts: Vec::new(),
                intent: Box::new(TransferIntent::Stack(push.clone())),
            },
        ],
        vec![
            EndpointRequest::TransferBeginStack {
                history_id,
                base_layer_id: base.id,
                incoming,
                root: base.root_id,
            },
            EndpointRequest::TransferEnd {
                objects: Vec::new(),
                facts: vec![Fact::Commit(commit)],
                intent: Box::new(TransferIntent::Branch {
                    branch: branch_a,
                    expected: None,
                }),
            },
        ],
        vec![
            EndpointRequest::TransferBeginBranch {
                branch: branch_a,
                root: base.root_id,
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
                root: base.root_id,
            },
            EndpointRequest::TransferEnd {
                objects: Vec::new(),
                facts: vec![Fact::Commit(commit)],
                intent: Box::new(TransferIntent::Branch {
                    branch: branch_a,
                    expected: Some(CommitId::derive(base.root_id, Some(commit.id), None)),
                }),
            },
        ],
    ];
    for requests in cases {
        assert!(serve_requests(&store, &requests).is_err());
        assert_eq!(transfer_rows(&database), baseline);
    }
    assert!(rusqlite::Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT NOT EXISTS(SELECT 1 FROM branches WHERE branch_id=?1)",
            [branch_a.id.as_slice()],
            |row| row.get::<_, bool>(0),
        )
        .unwrap());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn serve_requests(store: &LayerStore, requests: &[EndpointRequest]) -> EndpointReply {
    let mut input = Vec::new();
    for request in requests {
        write_value(&mut input, FrameKind::Command, request).unwrap();
    }
    let mut output = Vec::new();
    layerfs_layer_store::serve_once(store, &mut input.as_slice(), &mut output).unwrap();
    last_reply(&output)
}

fn last_reply(bytes: &[u8]) -> EndpointReply {
    let mut output = bytes;
    let mut reply = read_value(&mut output, FrameKind::Reply).unwrap();
    while !output.is_empty() {
        reply = read_value(&mut output, FrameKind::Reply).unwrap();
    }
    reply
}

fn transfer_rows(path: &std::path::Path) -> (i64, i64, i64, i64) {
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

fn table_count(path: &std::path::Path, table: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
