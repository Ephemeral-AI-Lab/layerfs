use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_monitor::{
    Monitor, MonitorScope, MonitorSnapshot, MonitoredRoute, OperationId, OperationOutcome,
    OperationReceipt, TimingFragment,
};
use layerfs_storage::{reset_sql_trace, sql_trace, LayerInitialization};
use layerfs_workspace::Workspaces;
use std::sync::Arc;

#[test]
fn passive_dedup_snapshot_has_zero_sql_and_analysis_is_explicit() {
    let root = run_dir();
    let layer = Arc::new(LayerStore::create(root.join("layer.sqlite")).unwrap());
    let (_, genesis) = layer.initialize(LayerInitialization::Empty).unwrap();
    let branch = BranchStore::create(root.join("branch.sqlite"), layer.clone()).unwrap();
    let _record = branch
        .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
        .unwrap();
    let workspaces = Arc::new(Workspaces::new(root.join("workspaces"), [branch.clone()]).unwrap());
    let route = MonitoredRoute::new(branch, None, layer);
    let route_id = route.id;
    let monitor = Monitor::new(root.join("monitor"), [route], workspaces).unwrap();

    reset_sql_trace();
    let MonitorSnapshot::Dedup(passive) = monitor
        .snapshot(MonitorScope::Dedup {
            route: Some(route_id),
        })
        .unwrap()
    else {
        panic!("dedup snapshot")
    };
    assert!(passive.is_empty());
    assert!(sql_trace().is_empty());

    let analysis = monitor.analyze_dedup(route_id).unwrap();
    assert_eq!(analysis.placements.len(), 2);
    assert!(!sql_trace().is_empty());
    reset_sql_trace();
    let MonitorSnapshot::Dedup(cached) = monitor
        .snapshot(MonitorScope::Dedup {
            route: Some(route_id),
        })
        .unwrap()
    else {
        panic!("dedup snapshot")
    };
    assert_eq!(cached, vec![(route_id, analysis)]);
    assert!(sql_trace().is_empty());

    let receipt = OperationReceipt {
        id: OperationId::new(),
        name: "branch.push".to_owned(),
        outcome: OperationOutcome::Succeeded,
        queued_ns: 10,
        service_ns: 100,
        fragments: vec![TimingFragment {
            process_id: std::process::id(),
            started_ns: 1,
            elapsed_ns: 90,
        }],
        storage: Vec::new(),
    };
    monitor.record(receipt.clone()).unwrap();
    let MonitorSnapshot::Operations(receipts) = monitor
        .snapshot(MonitorScope::Operation(Some(receipt.id)))
        .unwrap()
    else {
        panic!("operation snapshot")
    };
    assert_eq!(receipts, vec![receipt]);

    drop(monitor);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn receipt_transaction_classes_match_the_sql_trace() {
    let root = run_dir();
    let db = layerfs_storage::StoreDb::create(
        root.join("branch.sqlite"),
        layerfs_storage::StoreRole::Branch,
    )
    .unwrap();
    let facts = (0..=layerfs_storage::FACT_BATCH_COUNT)
        .map(|index| {
            let root = layerfs_content::ObjectId::for_bytes(&(index as u64).to_be_bytes());
            layerfs_storage::Fact::Commit(layerfs_storage::CommitRecord {
                id: layerfs_storage::CommitId::derive(root, None, None),
                root_id: root,
                parent_id: None,
                merge_parent_id: None,
            })
        })
        .collect::<Vec<_>>();
    reset_sql_trace();
    let (exchange, _) = db
        .finish_transfer(&[], &facts, layerfs_storage::TransferIntent::None)
        .unwrap();
    let database = exchange.database_receipt();
    let trace = sql_trace();
    let begins = trace
        .iter()
        .filter(|statement| statement.trim_start().starts_with("BEGIN"))
        .count() as u64;
    let commits = trace
        .iter()
        .filter(|statement| statement.trim_start().starts_with("COMMIT"))
        .count() as u64;
    assert_eq!(database.write_transactions, 2);
    assert_eq!(database.fact_admission_transactions, 2);
    assert_eq!(database.object_admission_transactions, 0);
    assert_eq!(begins, database.write_transactions);
    assert_eq!(commits, database.write_transactions);
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

fn run_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-monitor-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
