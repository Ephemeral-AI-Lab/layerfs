use layerfs_core::content::rope::{build, ObjectStore};
use layerfs_core::inode::{inode_table_from_root, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{
    encode_inode_record, encode_namespace_root, profile_id as namespace_profile_id,
};
use layerfs_core::{encode_bytes_object, ObjectId};
use layerfs_service::{RemoteEndpoint, MAX_WIRE_BYTES};
use layerfs_sync::client::{
    fetch_branch, fetch_objects, push_branch, push_layer_stack_genesis, push_objects,
};
use layerfs_sync::ResumeToken;
use layerfs_working_store::{
    BranchId, CommitResult, IntegrityMode, LayerId, LayerStackId, WorkingCandidate, WorkingStore,
};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn authenticated_service_restart_preserves_durable_identity_and_objects() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-service-restart-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let bearer = [0x31_u8; 32];
    let working =
        WorkingStore::open(&base.join("working-a"), IntegrityMode::TrustedLocalDev).unwrap();
    let canonical = encode_bytes_object(b"service-owned durable object").unwrap();
    let id = ObjectId::for_bytes(&canonical);
    let mut writer = working.begin_candidate_write().unwrap();
    assert_eq!(writer.put(&canonical).unwrap(), id);
    writer.commit_objects().unwrap();

    let (mut service, address, session) = start_service(&base.join("service"), &bearer);
    assert!(RemoteEndpoint::connect(address, &[0x32; 32]).is_err());
    let mut oversized = std::net::TcpStream::connect(address).unwrap();
    oversized
        .write_all(&((MAX_WIRE_BYTES + 1) as u32).to_be_bytes())
        .unwrap();
    let mut response_len = [0; 4];
    oversized.read_exact(&mut response_len).unwrap();
    let response_len = u32::from_be_bytes(response_len) as usize;
    assert!((1..=MAX_WIRE_BYTES).contains(&response_len));
    let mut response = vec![0; response_len];
    oversized.read_exact(&mut response).unwrap();
    assert!(String::from_utf8(response)
        .unwrap()
        .contains("request limit"));
    let durable_id = layerfs_sync::DurableEndpoint::durable_storage_id(&session);
    let pushed =
        push_objects(&working, &session, [0x33; 32], [id], ResumeToken::default()).unwrap();
    assert_eq!(pushed.transferred_objects, 1);
    let root = empty_root(&working);
    let stack = working
        .create_layer_stack(
            LayerStackId::from_bytes([0x41; 32]),
            LayerId::from_bytes([0x42; 32]),
            "main",
            root,
        )
        .unwrap();
    let branch = working
        .create_top_level_branch(BranchId::from_bytes([0x43; 32]), Some("main"), stack)
        .unwrap();
    let mut accepted = branch;
    for _ in 0..257 {
        let begin = working.begin_operation(accepted).unwrap();
        match working
            .operation_commit(
                begin,
                WorkingCandidate {
                    operation_id: begin.operation_id,
                    expected_branch_generation: accepted.generation,
                    base_root: root,
                    candidate_root: root,
                    normalized_transition: Vec::new(),
                },
            )
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, record, .. } => {
                working.acknowledge_operation(record).unwrap();
                accepted = head;
            }
            CommitResult::Conflict { .. } => panic!("unexpected Working conflict"),
        }
    }
    push_layer_stack_genesis(
        &working,
        &session,
        [0x40; 32],
        branch.branch_id,
        stack,
        "main",
        ResumeToken::default(),
    )
    .unwrap();
    let branch_push = push_branch(
        &working,
        &session,
        [0x44; 32],
        branch.branch_id,
        None,
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(branch_push.pages, 5);
    assert!(matches!(
        branch_push.outcome,
        layerfs_sync::BranchPushOutcome::DurablyAccepted { head, .. } if head == accepted
    ));
    service.kill().unwrap();
    service.wait().unwrap();

    let (mut wrong_store, wrong_address, _) =
        start_service_at(&base.join("wrong-service"), &bearer, address);
    assert_eq!(wrong_address, address);
    assert!(layerfs_sync::DurableEndpoint::contains_object(&session, id).is_err());
    wrong_store.kill().unwrap();
    wrong_store.wait().unwrap();

    let (mut restarted, _, session) = start_service(&base.join("service"), &bearer);
    assert_eq!(
        layerfs_sync::DurableEndpoint::durable_storage_id(&session),
        durable_id
    );
    let working_b = WorkingStore::open(&base.join("working-b"), IntegrityMode::Verified).unwrap();
    let fetched = fetch_objects(
        &session,
        &working_b,
        [0x34; 32],
        [id],
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(fetched.transferred_objects, 1);
    assert!(working_b.sync_has_object(id).unwrap());
    let fetched_branch = fetch_branch(
        &session,
        &working_b,
        [0x45; 32],
        branch.branch_id,
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(fetched_branch.head, accepted);
    assert_eq!(fetched_branch.pages, 5);
    assert_eq!(
        working_b.layer_stack_head(stack.layer_stack_id).unwrap(),
        Some(stack)
    );

    drop(working_b);
    restarted.kill().unwrap();
    restarted.wait().unwrap();
    drop(working);
    fs::remove_dir_all(base).unwrap();
}

fn empty_root(working: &WorkingStore) -> ObjectId {
    let mut writer = working.begin_candidate_write().unwrap();
    let root_inode = writer.allocate_inode_id().unwrap();
    let (mode, _) = build(&mut writer, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = Vec::new();
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut writer, mtime.as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut writer,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_file_root: mtime.0,
            },
        ],
    )
    .unwrap();
    let directory = empty_directory(&mut writer).unwrap();
    let record = writer
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let table = inode_table_from_root(&mut writer, root_inode, record).unwrap();
    let root = writer
        .put(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: namespace_profile_id(),
                root_directory_inode: root_inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    writer.commit_candidate(root).unwrap()
}

fn start_service(
    root: &std::path::Path,
    bearer: &[u8],
) -> (Child, std::net::SocketAddr, RemoteEndpoint) {
    start_service_at(root, bearer, "127.0.0.1:0".parse().unwrap())
}

fn start_service_at(
    root: &std::path::Path,
    bearer: &[u8],
    listen: std::net::SocketAddr,
) -> (Child, std::net::SocketAddr, RemoteEndpoint) {
    fs::create_dir_all(root).unwrap();
    let bearer_file = root.join("service.bearer");
    fs::write(&bearer_file, bearer).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bearer_file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_layerfs-service"))
        .args([
            "--root",
            root.to_str().unwrap(),
            "--bearer-file",
            bearer_file.to_str().unwrap(),
            "--listen",
            &listen.to_string(),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut address = String::new();
    BufReader::new(child.stdout.as_mut().unwrap())
        .read_line(&mut address)
        .unwrap();
    let address = address.trim().parse().unwrap();
    let endpoint = RemoteEndpoint::connect(address, bearer).unwrap();
    (child, address, endpoint)
}
