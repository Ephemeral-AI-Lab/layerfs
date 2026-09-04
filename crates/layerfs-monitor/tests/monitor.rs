use layerfs_layerstack_store::{
    reset_sql_trace, sql_trace, EntityName, LayerStackInitialization, LayerStackStore,
};
use layerfs_monitor::{
    CandidateStats, Monitor, OperationFamily, OperationId, OperationOutcome, OperationReceipt,
    SemanticOperation,
};
use layerfs_workspace::Workspaces;
use std::sync::Arc;

#[test]
fn candidate_stats_accept_store_admission_bounds() {
    let stats = |objects, bytes| CandidateStats {
        candidate_objects: objects,
        candidate_bytes: bytes,
        inserted_objects: objects,
        inserted_bytes: bytes,
        batch_inserted_objects: objects,
        batch_inserted_bytes: bytes,
        admission_transactions: 1,
        max_transaction_objects: objects,
        max_transaction_bytes: bytes,
        ..CandidateStats::default()
    };
    for family in [
        OperationFamily::WorkspaceCommit,
        OperationFamily::WorkspaceResolve,
        OperationFamily::LayerStackInitialize,
        OperationFamily::LayerStackAdd,
    ] {
        for objects in [127, 128, 512, 8191] {
            assert!(stats(objects, objects).validate_for(family));
        }
        assert!(stats(8191, 4 * 1024 * 1024 - 1).validate_for(family));
        assert!(!stats(8192, 8192).validate_for(family));
        assert!(!stats(1, 4 * 1024 * 1024).validate_for(family));
        let mut invalid = stats(128, 128);
        invalid.candidate_objects += 1;
        assert!(!invalid.validate_for(family));
    }
    assert!(stats(128, 128).validate());
}

#[test]
fn passive_snapshot_has_zero_sql_and_explicit_analysis_is_exact() {
    let root = temp();
    std::fs::create_dir_all(&root).unwrap();
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite")).unwrap());
    store
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let workspaces =
        Arc::new(Workspaces::new(root.join("runtime"), store.as_ref().clone()).unwrap());
    let monitor = Monitor::new(store.clone(), workspaces);
    let candidate = CandidateStats {
        candidate_objects: 3,
        candidate_bytes: 30,
        inserted_objects: 2,
        inserted_bytes: 20,
        reused_objects: 1,
        reused_bytes: 10,
        batch_inserted_objects: 1,
        batch_inserted_bytes: 10,
        final_inserted_objects: 1,
        final_inserted_bytes: 10,
        preexisting_reused_objects: 1,
        preexisting_reused_bytes: 10,
        admission_transactions: 1,
        max_transaction_objects: 1,
        max_transaction_bytes: 10,
    };
    monitor
        .record(OperationReceipt {
            id: OperationId::new(),
            operation: SemanticOperation::new(OperationFamily::WorkspaceCommit),
            outcome: OperationOutcome::Success,
            queue_ns: 0,
            service_ns: 1,
            candidate: Some(candidate),
            storage: Vec::new(),
        })
        .unwrap();

    reset_sql_trace();
    let snapshot = monitor.snapshot().unwrap();
    assert!(sql_trace().is_empty());
    assert_eq!(snapshot.operations.len(), 1);
    assert_eq!(snapshot.database.location, store.path());

    let analysis = monitor.analyze_dedup().unwrap();
    assert!(!sql_trace().is_empty());
    assert_eq!(analysis.physical_objects, analysis.reachable_objects);
    assert_eq!(analysis.physical_bytes, analysis.reachable_bytes);
    assert_eq!(analysis.candidates.candidate_objects, 3);
    assert_eq!(analysis.candidates.inserted_objects, 2);
    assert_eq!(analysis.candidates.reused_objects, 1);
    assert_eq!(analysis.saved_fraction, Some(1.0 / 3.0));
    assert_eq!(analysis.unreachable_objects, 0);

    drop(monitor);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn temp() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-v4-monitor-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
