use layerfs_branch_store::BranchStore;
use layerfs_layerstack_store::LayerStackStore;
use layerfs_monitor::{
    ExactOrUnavailable, Monitor, OperationFamily, OperationId, OperationOutcome, OperationReceipt,
    SemanticOperation, TimingFragment,
};
use layerfs_storage::{
    reset_sql_trace, sql_trace, EntityName, LayerStackInitialization, LocalForkSource,
};
use layerfs_workspace::Workspaces;
use std::sync::Arc;

#[test]
fn passive_snapshot_has_zero_sql_and_explicit_analysis_is_exact() {
    let root = run_dir();
    let layerstack = Arc::new(LayerStackStore::create(root.join("layerstack.sqlite")).unwrap());
    let genesis = layerstack
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let branch = BranchStore::create(root.join("branch.sqlite"), layerstack.store_id()).unwrap();
    branch
        .pull_layer(
            layerstack.clone(),
            genesis,
            layerfs_storage::RemotePlacement::Reference,
        )
        .unwrap();
    branch
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    let workspaces = Arc::new(
        Workspaces::new(root.join("workspaces"), branch.clone(), layerstack.clone()).unwrap(),
    );
    let monitor = Monitor::new(
        root.join("monitor"),
        layerstack.clone(),
        branch.clone(),
        workspaces,
    )
    .unwrap();

    reset_sql_trace();
    let passive = monitor.snapshot().unwrap();
    assert!(passive.dedup.is_none());
    assert!(sql_trace().is_empty());
    let analysis = monitor.analyze_dedup().unwrap();
    assert_eq!(analysis.placements.len(), 2);
    assert_eq!(analysis.physical_cas_bytes, analysis.union_cas_bytes);
    assert_eq!(analysis.cross_store_placement_bytes, 0);
    assert!(matches!(
        analysis.local_cas,
        ExactOrUnavailable::Unavailable(_)
    ));
    assert!(matches!(
        analysis.transfer,
        ExactOrUnavailable::Unavailable(_)
    ));
    let unique_bytes = analysis.union_cas_bytes;
    assert!(!sql_trace().is_empty());
    reset_sql_trace();
    assert_eq!(monitor.snapshot().unwrap().dedup, Some(analysis));
    assert!(sql_trace().is_empty());

    monitor.begin_operation();
    branch
        .pull_layer(
            layerstack.clone(),
            genesis,
            layerfs_storage::RemotePlacement::Replica,
        )
        .unwrap();
    let mut storage = monitor.finish_operation();
    storage.extend((0..10).map(|index| {
        layerfs_storage::StorageReceipt::Local(layerfs_storage::LocalAdmissionReceipt {
            objects: layerfs_storage::LocalObjectReceipt {
                candidate_ids: 1,
                candidate_bytes: 1_261,
                inserted_ids: u64::from(index == 0),
                inserted_bytes: if index == 0 { 1_261 } else { 0 },
                reused_ids: u64::from(index != 0),
                reused_bytes: if index == 0 { 0 } else { 1_261 },
            },
            cdc_bytes_scanned: 0,
            encode_hash_invocations: 0,
            source_reused_ids: 0,
            source_reused_bytes: 0,
        })
    }));
    storage.extend([
        layerfs_storage::StorageReceipt::WorkspaceCommit(layerfs_storage::WorkspaceCommitReceipt {
            total_ns: 13,
            pause_fence_ns: 1,
            capture_ns: 2,
            capture_mode: Some(layerfs_storage::CaptureMode::Live),
            unattributed_ns: 10,
            ..layerfs_storage::WorkspaceCommitReceipt::default()
        }),
        layerfs_storage::StorageReceipt::Push(layerfs_storage::PushPhaseReceipt {
            total_ns: 4,
            history_ns: 1,
            durability_ns: 1,
            unattributed_ns: 2,
            endpoint_calls: 3,
            ..layerfs_storage::PushPhaseReceipt::default()
        }),
        layerfs_storage::StorageReceipt::Database(layerfs_storage::DatabaseReceipt {
            store_id: branch.store_id(),
            role: layerfs_storage::StoreRole::Branch,
            operation: layerfs_storage::DatabaseOperation::CommitCas,
            total_ns: 5,
            connection_wait_ns: 1,
            writer_acquire_ns: 1,
            statement_ns: 1,
            publication_ns: 0,
            commit_sync_ns: 1,
            unattributed_ns: 1,
            statement_count: 2,
            rows: 2,
            bytes: 64,
        }),
    ]);
    let transfer_sets = storage.iter().filter_map(|receipt| match receipt {
        layerfs_storage::StorageReceipt::Transfer(receipt) => Some(
            std::iter::once(&receipt.objects)
                .chain(receipt.facts.values())
                .collect::<Vec<_>>(),
        ),
        layerfs_storage::StorageReceipt::Local(_)
        | layerfs_storage::StorageReceipt::Durability(_)
        | layerfs_storage::StorageReceipt::WorkspaceCommit(_)
        | layerfs_storage::StorageReceipt::Push(_)
        | layerfs_storage::StorageReceipt::Database(_)
        | layerfs_storage::StorageReceipt::WorkspaceLifecycle(_) => None,
    });
    let (expected_announced, expected_sent) =
        transfer_sets
            .flatten()
            .fold((0_u64, 0_u64), |(announced, sent), set| {
                (
                    announced + set.announced_bytes.exact().unwrap(),
                    sent + set.sent_bytes,
                )
            });
    let expected_storage_receipts = storage.len();
    let receipt = OperationReceipt {
        id: OperationId::new(),
        operation: SemanticOperation::new(OperationFamily::BranchPush),
        outcome: OperationOutcome::Succeeded,
        queued_ns: 10,
        service_ns: 100,
        fragments: vec![TimingFragment {
            process_id: std::process::id(),
            started_ns: 1,
            elapsed_ns: 90,
        }],
        storage,
    };
    monitor.record(receipt.clone()).unwrap();
    let receipt_json = receipt.to_json();
    assert!(receipt_json.contains("\"family\":\"branch.push\""));
    assert!(receipt_json.contains("\"fragments\":["));
    assert!(receipt_json.contains("\"capture_mode\":\"live\""));
    assert!(receipt_json.contains("\"operation\":\"commit_cas\""));
    assert_eq!(monitor.snapshot().unwrap().operations, vec![receipt]);
    let analysis = monitor.analyze_dedup().unwrap();
    assert_eq!(analysis.physical_cas_bytes, unique_bytes * 2);
    assert_eq!(analysis.union_cas_bytes, unique_bytes);
    assert_eq!(analysis.cross_store_placement_bytes, unique_bytes);
    assert_eq!(analysis.placement_factor, 2.0);
    let ExactOrUnavailable::Exact(local) = analysis.local_cas else {
        panic!("local CAS analysis")
    };
    assert_eq!(local.candidate_bytes, 12_610);
    assert_eq!(local.inserted_bytes, 1_261);
    assert_eq!(local.reused_bytes, 11_349);
    assert_eq!(local.saved_fraction, 0.9);
    assert_eq!(local.logical_to_physical, 10.0);
    let ExactOrUnavailable::Exact(transfer) = analysis.transfer else {
        panic!("transfer analysis")
    };
    assert_eq!(transfer.announced_bytes, expected_announced);
    assert_eq!(transfer.sent_bytes, expected_sent);
    assert_eq!(transfer.avoided_bytes, expected_announced - expected_sent);

    drop(monitor);
    let reopened = Monitor::new(
        root.join("monitor"),
        layerstack.clone(),
        branch.clone(),
        Arc::new(
            Workspaces::new(root.join("reopened"), branch.clone(), layerstack.clone()).unwrap(),
        ),
    )
    .unwrap();
    let reopened_snapshot = reopened.snapshot().unwrap();
    assert_eq!(reopened_snapshot.operations.len(), 1);
    assert_eq!(
        reopened_snapshot.operations[0].storage.len(),
        expected_storage_receipts
    );

    drop(reopened);
    drop(branch);
    drop(layerstack);
    std::fs::remove_dir_all(root).unwrap();
}

fn run_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-v2-monitor-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
