use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_sdk::{Direct, RemoteEndpoint, Stacked};
use layerfs_stack_store::StackStore;
use layerfs_storage_core::{
    read_frame, write_frame, AddLayerSource, BranchId, Change, CommitId, CommitRecord,
    EndpointRequest, Fact, FactKind, FrameKind, RefOutcome, StorageId, StoreEndpoint, WireValue,
};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Default)]
struct WireCounts {
    object_pages: AtomicU64,
    history_pages: AtomicU64,
    turns: AtomicU64,
    command_frames: AtomicU64,
    payload_batches: AtomicU64,
    payload_frames: AtomicU64,
    reply_frames: AtomicU64,
    bytes: AtomicU64,
    max_frame_bytes: AtomicUsize,
    publication_facts: std::sync::Mutex<Vec<(FactKind, Vec<u8>)>>,
}

fn relay_frames(
    mut input: std::net::TcpStream,
    mut output: std::net::TcpStream,
    client_to_server: bool,
    counts: Arc<WireCounts>,
) {
    while let Ok(frame) = read_frame(&mut input) {
        counts
            .bytes
            .fetch_add((45 + frame.bytes.len()) as u64, Ordering::SeqCst);
        counts
            .max_frame_bytes
            .fetch_max(frame.bytes.len(), Ordering::SeqCst);
        if client_to_server && frame.kind == FrameKind::Command {
            counts.command_frames.fetch_add(1, Ordering::SeqCst);
            let request = EndpointRequest::decode(&frame.bytes).unwrap();
            match request {
                EndpointRequest::TransferBeginBranch { .. }
                | EndpointRequest::TransferBeginStack { .. } => {
                    counts.object_pages.fetch_add(1, Ordering::SeqCst);
                    counts.turns.fetch_add(1, Ordering::SeqCst);
                }
                EndpointRequest::Transfer {
                    objects,
                    facts,
                    object_ids,
                    fact_kind,
                    fact_ids,
                } => {
                    if object_ids.is_empty()
                        && fact_kind.is_none()
                        && fact_ids.is_empty()
                        && (!objects.is_empty() || !facts.is_empty())
                    {
                        counts.payload_batches.fetch_add(1, Ordering::SeqCst);
                    } else if fact_kind.is_some()
                        && fact_ids.is_empty()
                        && objects.is_empty()
                        && !facts.is_empty()
                    {
                        counts
                            .publication_facts
                            .lock()
                            .unwrap()
                            .extend(facts.into_iter().map(|fact| (fact.kind(), fact.id())));
                    } else {
                        counts
                            .object_pages
                            .fetch_add(u64::from(!object_ids.is_empty()), Ordering::SeqCst);
                        counts
                            .history_pages
                            .fetch_add(u64::from(!fact_ids.is_empty()), Ordering::SeqCst);
                        counts.turns.fetch_add(1, Ordering::SeqCst);
                    }
                }
                EndpointRequest::TransferEnd { .. } => {
                    counts.turns.fetch_add(1, Ordering::SeqCst);
                }
                _ => {
                    counts.turns.fetch_add(1, Ordering::SeqCst);
                }
            }
        } else if client_to_server && frame.kind == FrameKind::Payload {
            counts.payload_frames.fetch_add(1, Ordering::SeqCst);
        } else if !client_to_server && frame.kind == FrameKind::Reply {
            counts.reply_frames.fetch_add(1, Ordering::SeqCst);
        }
        if write_frame(&mut output, &frame).is_err() {
            break;
        }
    }
    let _ = output.shutdown(std::net::Shutdown::Write);
}

fn counting_proxy(
    destination: std::net::SocketAddr,
) -> (
    std::net::SocketAddr,
    Arc<WireCounts>,
    std::thread::JoinHandle<()>,
) {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let counts = Arc::new(WireCounts::default());
    let proxy_counts = counts.clone();
    let proxy = std::thread::spawn(move || {
        let (client, _) = listener.accept().unwrap();
        let server = std::net::TcpStream::connect(destination).unwrap();
        let client_to_server = {
            let input = client.try_clone().unwrap();
            let output = server.try_clone().unwrap();
            let counts = proxy_counts.clone();
            std::thread::spawn(move || relay_frames(input, output, true, counts))
        };
        relay_frames(server, client, false, proxy_counts);
        client_to_server.join().unwrap();
    });
    (address, counts, proxy)
}

fn run_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerstack-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn count(path: &std::path::Path, table: &str) -> u64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap() as u64
}

fn commit_root(
    store: &dyn StoreEndpoint,
    branch_id: BranchId,
    commit_id: layerfs_storage_core::CommitId,
) -> layerfs_core::ObjectId {
    let mut root = None;
    store
        .visit_commits(
            branch_id,
            &mut |_, ids| layerfs_storage_core::MissingBitmap::from_missing(ids.len(), |_| true),
            &mut |commits| {
                root = commits
                    .iter()
                    .find(|commit| commit.id == commit_id)
                    .map(|commit| commit.root_id)
                    .or(root);
                Ok(())
            },
        )
        .unwrap();
    root.unwrap()
}

#[test]
fn stacked_three_physical_databases_publish_and_survive_creator_loss() {
    let run = run_dir("stacked");
    let layer_path = run.join("layer.sqlite");
    let stack_path = run.join("stack.sqlite");
    let branch_path = run.join("branch.sqlite");
    let consumer_stack_path = run.join("stack-consumer.sqlite");
    let fresh_branch_path = run.join("branch-fresh.sqlite");
    let layer = Arc::new(LayerStore::open(&layer_path).unwrap());
    let (layer_history, genesis) = layer.provision().unwrap();
    let stack = Arc::new(StackStore::open(&stack_path, layer.clone()).unwrap());
    stack
        .pull_layer_history(layer_history.id, genesis.id)
        .unwrap();
    let (stack_history, seed) = stack
        .create_stack_history_from_layer(layer_history.id, genesis.id)
        .unwrap();
    let branch = BranchStore::open(&branch_path, stack.clone()).unwrap();
    let created = branch
        .create_branch_from_stack(stack_history.id, seed.id)
        .unwrap();
    assert_eq!(count(&branch_path, "objects"), 0);
    let RefOutcome::Created(commit_id) = branch
        .commit(
            created.id,
            created.head_commit_id,
            &[Change::Write {
                path: "stacked.txt".into(),
                bytes: b"LayerStack stacked".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    else {
        panic!("expected Commit")
    };
    branch.push_branch(created.id).unwrap();
    let added_stack = stack
        .add_stack(stack_history.id, created.id, commit_id)
        .unwrap();
    assert!(matches!(
        stack.push_stack(added_stack.result_id).unwrap(),
        RefOutcome::Created(_) | RefOutcome::FastForwarded(_)
    ));
    let added_layer = layer
        .add_layer(
            layer_history.id,
            AddLayerSource::StackSource(added_stack.result_id),
        )
        .unwrap();
    let final_layer = layer.layer(added_layer.result_id).unwrap().unwrap();
    assert!(matches!(
        stack.push_stack(added_stack.result_id).unwrap(),
        RefOutcome::UpToDate(_)
    ));
    let RefOutcome::Created(later_commit) = branch
        .commit(
            created.id,
            commit_id,
            &[Change::Write {
                path: "later.txt".into(),
                bytes: b"not accepted under the frozen Branch ID".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    else {
        panic!("expected later Commit")
    };
    assert_ne!(later_commit, commit_id);
    assert!(branch.push_branch(created.id).is_err());
    assert_eq!(
        stack.branch(created.id).unwrap().unwrap().head_commit_id,
        commit_id
    );

    drop(branch);
    drop(stack);
    let consumer = Arc::new(StackStore::open(&consumer_stack_path, layer.clone()).unwrap());
    consumer
        .pull_stack_history(stack_history.id, added_stack.result_id)
        .unwrap();
    let through = consumer.pull_commit_history(created.id).unwrap();
    assert_eq!(through, commit_id);
    assert!(consumer.branch(created.id).unwrap().is_none());
    let fresh = BranchStore::open(&fresh_branch_path, consumer.clone()).unwrap();
    let local_id = BranchId::new();
    fresh.pull_branch(created.id, local_id).unwrap();
    assert_eq!(
        fresh.read_path(local_id, "stacked.txt").unwrap(),
        b"LayerStack stacked"
    );
    assert_eq!(
        commit_root(layer.as_ref(), created.id, commit_id),
        final_layer.root_id
    );
    assert!(branch_path.exists() && stack_path.exists() && layer_path.exists());
    assert_ne!(branch_path, stack_path);
    assert_ne!(stack_path, layer_path);
    drop(fresh);
    drop(consumer);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn direct_two_physical_databases_publish_and_pull_without_base_copy() {
    let run = run_dir("direct");
    let layer_path = run.join("layer.sqlite");
    let branch_path = run.join("branch.sqlite");
    let fresh_path = run.join("branch-fresh.sqlite");
    let layer = Arc::new(LayerStore::open(&layer_path).unwrap());
    let (history, genesis) = layer.provision().unwrap();
    let branch = BranchStore::open(&branch_path, layer.clone()).unwrap();
    let created = branch
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    assert_eq!(count(&branch_path, "objects"), 0);
    let committed = branch
        .commit(
            created.id,
            created.head_commit_id,
            &[Change::Write {
                path: "hello.txt".into(),
                bytes: b"LayerStack direct".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap();
    let RefOutcome::Created(commit_id) = committed else {
        panic!("expected Commit")
    };
    assert!(matches!(
        branch.push_branch(created.id).unwrap(),
        RefOutcome::Created(_) | RefOutcome::FastForwarded(_)
    ));
    let added = layer
        .add_layer(
            history.id,
            AddLayerSource::BranchSource {
                branch_id: created.id,
                commit_id,
            },
        )
        .unwrap();
    let final_layer = layer.layer(added.result_id).unwrap().unwrap();
    assert!(matches!(
        branch.push_branch(created.id).unwrap(),
        RefOutcome::UpToDate(_)
    ));
    let fresh = BranchStore::open(&fresh_path, layer.clone()).unwrap();
    let local_id = BranchId::new();
    fresh.pull_branch(created.id, local_id).unwrap();
    assert_eq!(count(&fresh_path, "objects"), 0);
    assert_eq!(
        fresh.read_path(local_id, "hello.txt").unwrap(),
        b"LayerStack direct"
    );
    assert_eq!(
        commit_root(layer.as_ref(), created.id, commit_id),
        final_layer.root_id
    );
    assert_ne!(branch_path, layer_path);
    assert!(branch_path.exists() && layer_path.exists() && fresh_path.exists());
    drop(fresh);
    drop(branch);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn direct_loopback_uses_the_same_endpoint_contract() {
    let run = run_dir("direct-loopback");
    let layer = Arc::new(LayerStore::open(run.join("layer.sqlite")).unwrap());
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
    let branch = BranchStore::open(run.join("branch.sqlite"), parent.clone()).unwrap();
    let direct = Direct::from_parts(branch, remote.clone());
    let created = direct
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    let commit_id = match direct
        .commit(
            created.id,
            created.head_commit_id,
            &[Change::Write {
                path: "loopback.txt".into(),
                bytes: b"same contract".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    {
        RefOutcome::Created(id) => id,
        _ => panic!(),
    };
    direct.push_branch(created.id).unwrap();
    let mut transferred_commits = 0;
    parent
        .visit_commits(
            created.id,
            &mut |_, ids| layerfs_storage_core::MissingBitmap::from_missing(ids.len(), |_| false),
            &mut |commits| {
                transferred_commits += commits.len();
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(transferred_commits, 0);
    let added = direct
        .add_layer(
            history.id,
            AddLayerSource::BranchSource {
                branch_id: created.id,
                commit_id,
            },
        )
        .unwrap();
    assert!(matches!(
        direct.push_branch(created.id).unwrap(),
        RefOutcome::UpToDate(_)
    ));
    assert_eq!(
        layer.layer(added.result_id).unwrap().unwrap().root_id,
        commit_root(layer.as_ref(), created.id, commit_id)
    );
    drop(direct);
    drop(parent);
    drop(remote);
    server.join().unwrap();
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn large_loopback_payload_batches_do_not_add_request_reply_turns() {
    let run = run_dir("large-loopback-frontier");
    let layer = Arc::new(LayerStore::open(run.join("layer.sqlite")).unwrap());
    let (history, genesis) = layer.provision().unwrap();
    let branch_path = run.join("branch.sqlite");
    let branch = BranchStore::open(&branch_path, layer.clone()).unwrap();
    let created = branch
        .create_branch_from_layer(history.id, genesis.id)
        .unwrap();
    let mut bytes = vec![0_u8; 8 * 1024 * 1024];
    let mut state = 0x9e37_79b9_u32;
    for byte in &mut bytes {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *byte = state as u8;
    }
    branch
        .commit(
            created.id,
            created.head_commit_id,
            &[Change::Write {
                path: "large.bin".into(),
                bytes,
                mode: 0o644,
            }],
        )
        .unwrap();
    drop(branch);

    let server_listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let server_address = server_listener.local_addr().unwrap();
    let server_store = layer.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = server_listener.accept().unwrap();
        let mut output = stream.try_clone().unwrap();
        while layerfs_layer_store::serve_once(&server_store, &mut stream, &mut output).is_ok() {}
    });
    let (proxy_address, counts, proxy) = counting_proxy(server_address);
    let remote = RemoteEndpoint::connect(proxy_address).unwrap();
    let remote_branch = BranchStore::open(&branch_path, Arc::new(remote.clone())).unwrap();
    remote_branch.push_branch(created.id).unwrap();
    drop(remote_branch);
    drop(remote);
    proxy.join().unwrap();
    server.join().unwrap();

    let object_pages = counts.object_pages.load(Ordering::SeqCst);
    let history_pages = counts.history_pages.load(Ordering::SeqCst);
    let turns = counts.turns.load(Ordering::SeqCst);
    let payload_batches = counts.payload_batches.load(Ordering::SeqCst);
    eprintln!(
        "P_o={object_pages} H={history_pages} J={payload_batches} turns={turns} command_frames={} payload_frames={} reply_frames={} bytes={}",
        counts.command_frames.load(Ordering::SeqCst),
        counts.payload_frames.load(Ordering::SeqCst),
        counts.reply_frames.load(Ordering::SeqCst),
        counts.bytes.load(Ordering::SeqCst),
    );
    assert!(object_pages > 1);
    assert!(payload_batches > 1);
    assert_eq!(
        counts.command_frames.load(Ordering::SeqCst),
        turns + payload_batches
    );
    assert!(counts.payload_frames.load(Ordering::SeqCst) > payload_batches);
    assert!(turns <= object_pages + history_pages + 1);
    assert_eq!(counts.reply_frames.load(Ordering::SeqCst), turns + 1);
    assert!(counts.max_frame_bytes.load(Ordering::SeqCst) <= layerfs_storage_core::MAX_FRAME_BYTES);
    assert!(counts.bytes.load(Ordering::SeqCst) > 8 * 1024 * 1024);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn stacked_loopback_uses_the_same_endpoint_contract() {
    let run = run_dir("stacked-loopback");
    let layer = Arc::new(LayerStore::open(run.join("layer.sqlite")).unwrap());
    let (layer_history, genesis) = layer.provision().unwrap();
    let layer_listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let layer_address = layer_listener.local_addr().unwrap();
    let layer_server_store = layer.clone();
    let layer_server = std::thread::spawn(move || {
        let (mut stream, _) = layer_listener.accept().unwrap();
        let mut output = stream.try_clone().unwrap();
        while layerfs_layer_store::serve_once(&layer_server_store, &mut stream, &mut output).is_ok()
        {
        }
    });
    let layer_remote = RemoteEndpoint::connect(layer_address).unwrap();
    let layer_parent = Arc::new(layer_remote.clone());
    let stack = Arc::new(StackStore::open(run.join("stack.sqlite"), layer_parent.clone()).unwrap());
    stack
        .pull_layer_history(layer_history.id, genesis.id)
        .unwrap();
    let (stack_history, seed) = stack
        .create_stack_history_from_layer(layer_history.id, genesis.id)
        .unwrap();

    let stack_listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let stack_address = stack_listener.local_addr().unwrap();
    let stack_server_store = stack.clone();
    let stack_server = std::thread::spawn(move || {
        let (mut stream, _) = stack_listener.accept().unwrap();
        let mut output = stream.try_clone().unwrap();
        while layerfs_stack_store::serve_once(&stack_server_store, &mut stream, &mut output).is_ok()
        {
        }
    });
    let stack_remote = RemoteEndpoint::connect(stack_address).unwrap();
    let stack_parent = Arc::new(stack_remote.clone());
    let branch = BranchStore::open(run.join("branch.sqlite"), stack_parent.clone()).unwrap();
    let topology = Stacked::from_parts(
        branch,
        stack.clone(),
        stack_remote.clone(),
        layer_remote.clone(),
    );
    let created = topology
        .create_branch_from_stack(stack_history.id, seed.id)
        .unwrap();
    let commit_id = match topology
        .commit(
            created.id,
            created.head_commit_id,
            &[Change::Write {
                path: "loopback-stacked.txt".into(),
                bytes: b"same stacked contract".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    {
        RefOutcome::Created(id) => id,
        _ => panic!(),
    };
    topology.push_branch(created.id).unwrap();
    let stacked = topology
        .add_stack(stack_history.id, created.id, commit_id)
        .unwrap();
    assert!(matches!(
        topology.push_branch(created.id).unwrap(),
        RefOutcome::UpToDate(_)
    ));
    topology.push_stack(stacked.result_id).unwrap();
    assert!(matches!(
        topology.push_stack(stacked.result_id).unwrap(),
        RefOutcome::UpToDate(_)
    ));
    let layered = topology
        .add_layer(
            layer_history.id,
            AddLayerSource::StackSource(stacked.result_id),
        )
        .unwrap();
    assert_eq!(
        layer.layer(layered.result_id).unwrap().unwrap().root_id,
        stack.stack(stacked.result_id).unwrap().unwrap().root_id
    );
    drop(topology);
    drop(stack_parent);
    drop(stack_remote);
    stack_server.join().unwrap();
    drop(stack);
    drop(layer_parent);
    drop(layer_remote);
    layer_server.join().unwrap();
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn remote_stack_publication_sends_exactly_missing_metadata_ids() {
    let run = run_dir("stack-publication-missing-only");
    let layer = Arc::new(LayerStore::open(run.join("layer.sqlite")).unwrap());
    let (layer_history, genesis) = layer.provision().unwrap();
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let destination = listener.local_addr().unwrap();
    let server_store = layer.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut output = stream.try_clone().unwrap();
        while layerfs_layer_store::serve_once(&server_store, &mut stream, &mut output).is_ok() {}
    });
    let (proxy_address, counts, proxy) = counting_proxy(destination);
    let remote = RemoteEndpoint::connect(proxy_address).unwrap();
    let parent = Arc::new(remote.clone());
    let stack = Arc::new(StackStore::open(run.join("stack.sqlite"), parent.clone()).unwrap());
    stack
        .pull_layer_history(layer_history.id, genesis.id)
        .unwrap();
    let (history, seed) = stack
        .create_stack_history_from_layer(layer_history.id, genesis.id)
        .unwrap();
    let branch = BranchStore::open(run.join("branch.sqlite"), stack.clone()).unwrap();
    stack.push_stack(seed.id).unwrap();
    counts.publication_facts.lock().unwrap().clear();
    let anchor = CommitRecord {
        id: CommitId::derive(genesis.root_id, None, None),
        root_id: genesis.root_id,
        parent_id: None,
        merge_parent_id: None,
    };
    layer
        .transfer_exchange_unlocked(&[], &[Fact::Commit(anchor)], &[], None)
        .unwrap();
    let mut current = seed.id;
    let mut branches = Vec::new();
    for index in 0..1_025 {
        let created = branch
            .create_branch_from_stack(history.id, current)
            .unwrap();
        branch.push_branch(created.id).unwrap();
        current = stack
            .add_stack(history.id, created.id, created.head_commit_id)
            .unwrap()
            .result_id;
        if index % 2 == 0 {
            layer
                .transfer_exchange_unlocked(&[], &[Fact::Branch(created)], &[], None)
                .unwrap();
        }
        layer
            .transfer_exchange_unlocked(
                &[],
                &[Fact::Stack(stack.stack(current).unwrap().unwrap())],
                &[],
                None,
            )
            .unwrap();
        branches.push(created.id);
    }
    stack.push_stack(current).unwrap();
    let mut expected = branches
        .iter()
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, id)| (FactKind::Branch, id.as_slice().to_vec()))
        .collect::<Vec<_>>();
    expected.sort_by(|left, right| left.1.cmp(&right.1));
    let mut results = branches
        .iter()
        .map(|id| (FactKind::AddResult, id.as_slice().to_vec()))
        .collect::<Vec<_>>();
    results.sort_by(|left, right| left.1.cmp(&right.1));
    expected.extend(results);
    assert_eq!(*counts.publication_facts.lock().unwrap(), expected);
    assert!(counts.max_frame_bytes.load(Ordering::SeqCst) <= layerfs_storage_core::MAX_FRAME_BYTES);

    drop(branch);
    drop(stack);
    drop(parent);
    drop(remote);
    proxy.join().unwrap();
    server.join().unwrap();
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}
