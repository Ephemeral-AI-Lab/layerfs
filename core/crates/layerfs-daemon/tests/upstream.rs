//! Portable authenticated protocol fixtures exercise real consumer/overlay owners.
//! These fixtures are not a global Store/runtime or mounted-workload qualification.
use layerfs_bridge::{
    codec::{ReassemblyConfig, ReceiveBudget},
    contract::{MessageClass, MessageKind},
    native::{self, RecordReceiver, RecordSender},
};
use layerfs_content::{
    filesystem::{profile_id, scope_for_seed, FilesystemRoot},
    AuthenticatedObjects, ContentError, ObjectId,
};
use layerfs_daemon::{upstream::*, Command, Owner, OwnerConfig, Response};
use layerfs_history::{
    BranchId, BranchRecord, BranchSnapshot, CatalogId, CommitId, HistoryName, LayerId,
    LayerStackId, WorkspaceId,
};
use layerfs_overlay::ProfileConfig;
use layerfs_persistence::SqlitePersistenceProfile;
use layerfs_sdk::{
    client::{Attachment, FailureOrigin, Operation, ReplyView},
    runtime::{encode_grant, encode_refusal},
    RuntimeError,
};
use layerfs_storage::{StorageError, StoragePolicy};
use std::{
    collections::BTreeMap,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-upstream-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn socket(stream: TcpStream) -> TcpStream {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
}
fn budget() -> ReceiveBudget {
    ReceiveBudget::new(ReassemblyConfig {
        kind: MessageKind::Reply,
        messages: 8,
        demand_messages: 1,
        control_messages: 1,
        message_bytes: 34 << 20,
        bytes: 96 << 20,
        demand_reserve: 34 << 20,
        control_reserve: 64 << 10,
    })
    .unwrap()
}
fn fixture() -> (
    ExpectedBinding,
    PersistenceBootstrap,
    BTreeMap<ObjectId, Vec<u8>>,
    ObjectId,
) {
    let scope = scope_for_seed([8; 32]);
    let root = FilesystemRoot::new(
        profile_id(),
        scope,
        1,
        ObjectId::for_bytes(b"protocol-fixture-table"),
    )
    .unwrap()
    .encode()
    .unwrap();
    let root_id = ObjectId::for_bytes(&root);
    let payload =
        layerfs_content::encode_whole_file_payload(b"independent healthy demand").unwrap();
    let payload_id = ObjectId::for_bytes(&payload);
    let stack = LayerStackId::from_authority([3; 16]);
    let snapshot = BranchSnapshot {
        branch: BranchRecord {
            id: BranchId::from_authority([4; 16]),
            stack,
            name: HistoryName::new("main").unwrap(),
            base_layer: LayerId::derive(stack, None, root_id),
            head_commit: None,
        },
        head_root: None,
        base_root: root_id,
        effective_root: root_id,
        scope: scope.object(),
        profile: profile_id(),
    };
    let expected = ExpectedBinding {
        host_peer: native::public_key(&[33; 32]).unwrap(),
        local_peer: native::public_key(&[1; 32]).unwrap(),
        runtime: [7; 32],
        catalog: CatalogId::derive(b"upstream").unwrap(),
        provider_incarnation: 9,
        workspace: WorkspaceId::from_authority([2; 32]).unwrap(),
        snapshot,
        root_serial: 1,
        policy: StoragePolicy::frozen_default(),
        persistence: SqlitePersistenceProfile::Disposable,
    };
    let bootstrap = PersistenceBootstrap {
        host_peer: expected.host_peer,
        runtime: expected.runtime,
        catalog: expected.catalog,
        provider_incarnation: expected.provider_incarnation,
        profile: expected.persistence,
    };
    (
        expected,
        bootstrap,
        BTreeMap::from([(root_id, root), (payload_id, payload)]),
        payload_id,
    )
}
fn binding_reply(expected: &ExpectedBinding) -> Vec<u8> {
    let mut bytes = b"LRP1\x00\x0a\x00\x00".to_vec();
    for value in [
        expected.runtime.as_slice(),
        expected.local_peer.as_slice(),
        expected.workspace.to_bytes().as_slice(),
        expected.catalog.as_slice(),
    ] {
        bytes.extend_from_slice(value);
    }
    bytes.extend_from_slice(&expected.provider_incarnation.to_be_bytes());
    bytes.extend_from_slice(&expected.root_serial.to_be_bytes());
    let s = &expected.snapshot;
    bytes.extend_from_slice(&s.branch.id.to_bytes());
    bytes.extend_from_slice(&s.branch.stack.to_bytes());
    blob(&mut bytes, s.branch.name.as_str().as_bytes());
    bytes.extend_from_slice(&s.branch.base_layer.to_bytes());
    bytes.push(u8::from(s.branch.head_commit.is_some()));
    if let Some(head) = s.branch.head_commit {
        bytes.extend_from_slice(&head.to_bytes());
    }
    bytes.push(u8::from(s.head_root.is_some()));
    if let Some(root) = s.head_root {
        bytes.extend_from_slice(root.as_bytes());
    }
    for id in [s.base_root, s.effective_root, s.scope, s.profile] {
        bytes.extend_from_slice(id.as_bytes());
    }
    assert!(matches!(
        ReplyView::decode(&bytes).unwrap(),
        ReplyView::Binding(_)
    ));
    bytes
}
fn blob(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(value);
}
struct Server {
    worker: JoinHandle<()>,
    requests: Arc<AtomicU64>,
}
fn start(expected: &ExpectedBinding, objects: BTreeMap<ObjectId, Vec<u8>>) -> (Attachment, Server) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let binding = binding_reply(expected);
    let policy = expected.policy;
    let requests = Arc::new(AtomicU64::new(0));
    let observed = requests.clone();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => panic!("accept: {e}"),
            }
        };
        let host = native::accept(
            socket(stream),
            &[33; 32],
            native::public_key(&[1; 32]).unwrap(),
        )
        .unwrap();
        let mut receive = RecordReceiver::new(host.receive);
        let mut send = RecordSender::new(host.send).map_err(|(e, _)| e).unwrap();
        while let Ok(first) = receive.receive() {
            let envelope = first.envelope;
            let header = layerfs_sdk::runtime::check_header(envelope, first.bytes).unwrap();
            observed.fetch_add(1, Ordering::AcqRel);
            let grant = encode_grant();
            send.begin(
                MessageKind::Reply,
                MessageClass::Control,
                envelope.correlation,
                grant.bytes.len() as u64,
                &grant.bytes,
            )
            .unwrap();
            let mut body = Vec::new();
            let mut received = layerfs_sdk::client::REQUEST_HEADER_BYTES as u64;
            while received < envelope.total_bytes {
                let fragment = match receive.receive() {
                    Ok(fragment) => fragment,
                    Err(_) => return,
                };
                assert_eq!(fragment.envelope, envelope);
                body.extend_from_slice(fragment.bytes);
                received += fragment.bytes.len() as u64;
            }
            let reply = match header.operation {
                Operation::Binding => binding.clone(),
                Operation::Policy => {
                    let mut bytes = b"LRP1\x00\x01\x00\x00".to_vec();
                    bytes.push(policy.format_profile());
                    bytes.extend_from_slice(&policy.small_file_threshold_bytes().to_be_bytes());
                    bytes.extend_from_slice(&[
                        policy.whole_file_delta_max_depth(),
                        policy.chunk_delta_max_depth(),
                        policy.metadata_delta_max_depth(),
                    ]);
                    bytes
                }
                Operation::Objects => {
                    let ids: Vec<_> = body
                        .chunks_exact(32)
                        .map(|b| ObjectId::from_bytes(b).unwrap())
                        .collect();
                    if let Some(missing) = ids.iter().find(|id| !objects.contains_key(id)) {
                        let mut bytes = encode_refusal(
                            &RuntimeError::Storage(Arc::new(StorageError::ObjectMissing(*missing))),
                            64 << 10,
                        )
                        .unwrap()
                        .bytes;
                        bytes[4] = 1; // External protocol fixture's dispatched owning failure.
                        bytes
                    } else {
                        let mut bytes = b"LRP1\x00\x05\x00\x00".to_vec();
                        bytes.extend_from_slice(&(ids.len() as u32).to_be_bytes());
                        for id in ids {
                            bytes.extend_from_slice(id.as_bytes());
                            blob(&mut bytes, &objects[&id]);
                        }
                        bytes
                    }
                }
                _ => panic!("unexpected fixture request"),
            };
            assert!(ReplyView::decode(&reply).is_ok());
            send.begin(
                MessageKind::Reply,
                header.operation.class(),
                envelope.correlation,
                reply.len() as u64,
                &reply,
            )
            .unwrap();
        }
    });
    let client = native::initiate(
        socket(TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap()),
        &[1; 32],
        native::public_key(&[33; 32]).unwrap(),
    )
    .unwrap();
    (
        Attachment::new(client, budget())
            .map_err(|_| "attachment")
            .unwrap(),
        Server { worker, requests },
    )
}
fn joined(attachment: &mut Attachment) {
    attachment.fence();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if attachment.try_join().unwrap().is_some() {
            return;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn mismatched_host_profile_and_same_root_history_context_refuse_before_local_open() {
    let (expected, bootstrap, objects, _) = fixture();
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    for scenario in 0..3 {
        let (attachment, server) = start(&expected, objects.clone());
        let mut wanted = expected.clone();
        let mut supplied = bootstrap;
        let mismatch = match scenario {
            0 => {
                wanted.host_peer = [11; 32];
                BindingMismatch::HostPeer
            }
            1 => {
                supplied.profile = SqlitePersistenceProfile::Durable;
                BindingMismatch::Persistence
            }
            _ => {
                wanted.snapshot.branch.head_commit = Some(CommitId::derive(
                    wanted.snapshot.effective_root,
                    None,
                    wanted.snapshot.branch.base_layer,
                ));
                wanted.snapshot.head_root = Some(wanted.snapshot.effective_root);
                BindingMismatch::Snapshot
            }
        };
        let mut refused = Upstream::attach(attachment, wanted, supplied, owner.client(), 8192)
            .err()
            .unwrap();
        assert!(matches!(refused.error, UpstreamError::Mismatch(actual) if actual == mismatch));
        assert_eq!(owner.client().diagnostics().unwrap().admitted, 0);
        if scenario < 2 {
            assert_eq!(refused.phase, AttachPhase::Provision);
            assert!(refused.binding_reply.is_none());
        } else {
            assert_eq!(refused.phase, AttachPhase::Binding);
            assert!(refused.binding_reply.is_some());
            assert!(refused.policy_reply.is_none());
        }
        joined(&mut refused.attachment);
        server.worker.join().unwrap();
        assert_eq!(
            server.requests.load(Ordering::Acquire),
            u64::from(scenario == 2)
        );
    }
    owner.stop().unwrap();
}
#[test]
fn policy_mismatch_retains_both_original_replies_and_does_not_open_overlay() {
    let (expected, bootstrap, objects, _) = fixture();
    let (attachment, server) = start(&expected, objects);
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let mut wanted = expected;
    wanted.policy =
        wanted
            .policy
            .with_metadata_depth(if wanted.policy.metadata_delta_max_depth() == 0 {
                1
            } else {
                0
            });
    let mut refused = Upstream::attach(attachment, wanted, bootstrap, owner.client(), 8192)
        .err()
        .unwrap();
    assert_eq!(refused.phase, AttachPhase::Policy);
    assert!(matches!(
        refused.error,
        UpstreamError::Mismatch(BindingMismatch::Policy)
    ));
    assert!(matches!(
        ReplyView::decode(refused.binding_reply.as_ref().unwrap().bytes()).unwrap(),
        ReplyView::Binding(_)
    ));
    assert!(matches!(
        ReplyView::decode(refused.policy_reply.as_ref().unwrap().bytes()).unwrap(),
        ReplyView::Policy(_)
    ));
    assert_eq!(owner.client().diagnostics().unwrap().admitted, 0);
    joined(&mut refused.attachment);
    server.worker.join().unwrap();
    owner.stop().unwrap();
}
#[test]
fn original_owner_failure_and_failed_object_operation_keep_custody_without_poisoning_independent_operation(
) {
    let (expected, bootstrap, objects, payload) = fixture();
    let (attachment, server) = start(&expected, objects.clone());
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let mut upstream = Upstream::attach(
        attachment,
        expected.clone(),
        bootstrap,
        owner.client(),
        8192,
    )
    .unwrap();
    let missing = ObjectId::for_bytes(b"missing demand");
    let failed = upstream
        .operation()
        .unwrap()
        .run(|operation| operation.client().read_canonical(missing))
        .err()
        .unwrap();
    assert_eq!(failed.error, ContentError::MissingObject);
    let first_envelope;
    {
        let cause = failed.operation.failure().unwrap();
        let reply = cause.as_ref().unwrap().reply().unwrap();
        first_envelope = reply.envelope();
        assert!(matches!(
            ReplyView::decode(reply.bytes()).unwrap(),
            ReplyView::Failure {
                origin: FailureOrigin::Dispatched,
                ..
            }
        ));
    }
    let requests = server.requests.load(Ordering::Acquire);
    assert!(matches!(
        failed.operation.client().read_canonical(missing),
        Err(ContentError::ResourceUnavailable { .. })
    ));
    assert_eq!(server.requests.load(Ordering::Acquire), requests);
    assert_eq!(
        failed
            .operation
            .failure()
            .unwrap()
            .as_ref()
            .unwrap()
            .reply()
            .unwrap()
            .envelope(),
        first_envelope
    );
    let healthy = upstream
        .operation()
        .unwrap()
        .run(|operation| operation.client().read_canonical(payload))
        .unwrap();
    assert_eq!(healthy.value, objects[&payload]);
    assert!(healthy.operation.failure().unwrap().is_none());
    assert!(failed.operation.failure().unwrap().is_some());
    // A second explicit attach with the already-owned namespace is an attempted
    // Open failure; its original boxed Completion/credit survives, with no close.
    let (attachment, duplicate_server) = start(&expected, objects);
    let mut duplicate = Upstream::attach(attachment, expected, bootstrap, owner.client(), 8192)
        .err()
        .unwrap();
    assert_eq!(duplicate.phase, AttachPhase::Open);
    assert!(matches!(&duplicate.error, UpstreamError::Completion(done) if done.result().is_err()));
    assert!(owner.client().diagnostics().unwrap().outstanding > 0);
    joined(&mut duplicate.attachment);
    duplicate_server.worker.join().unwrap();
    drop(duplicate);
    assert_eq!(owner.client().diagnostics().unwrap().outstanding, 0);
    upstream.fence();
    let stopped = upstream
        .operation()
        .unwrap()
        .run(|operation| {
            operation
                .client()
                .read_canonical(ObjectId::for_bytes(b"uncached after fence"))
        })
        .err()
        .unwrap();
    assert!(matches!(
        stopped.error,
        ContentError::ProviderFailure { .. }
    ));
    assert!(stopped.operation.failure().unwrap().is_some());
    let deadline = Instant::now() + Duration::from_secs(3);
    while upstream.try_join().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    server.worker.join().unwrap();
    let done = owner
        .client()
        .try_submit(Some(upstream.route()), Command::Close)
        .map_err(|(e, _)| e)
        .unwrap()
        .wait()
        .unwrap();
    assert!(matches!(done.result(), Ok(Response::Done)));
    drop(done);
    drop((failed, healthy, stopped, upstream));
    owner.stop().unwrap();
}
