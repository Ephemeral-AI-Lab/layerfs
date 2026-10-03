//! Independent PostgreSQL history authorities and measured exchange counts.
#[path = "support/history.rs"]
mod support;
use layerfs_history::*;
use layerfs_metadata::PgHistory;
use support::*;
fn record<T>(catalog: &PgHistory, label: &str, operation: impl FnOnce() -> T) -> T {
    let before = catalog.diagnostics().unwrap();
    let value = operation();
    let after = catalog.diagnostics().unwrap();
    println!("DIAGNOSTIC history-{label} operations={} sync={} ready={} protocol_sent={} protocol_received={}",after.operations-before.operations,after.sync_messages-before.sync_messages,after.ready_for_query-before.ready_for_query,after.protocol_sent-before.protocol_sent,after.protocol_received-before.protocol_received);
    assert_eq!(
        after.operations - before.operations,
        after.sync_messages - before.sync_messages
    );
    assert_eq!(
        after.operations - before.operations,
        after.ready_for_query - before.ready_for_query
    );
    value
}
#[test]
fn two_connections_preserve_the_loser_and_record_all_contract_units() {
    let temp = Temp::new("two-authorities");
    let path = temp.join("db");
    let a = create(&path);
    let stack_id = stack(1);
    let s = record(&a, "initialize", || {
        a.initialize_layerstack(&StackInitialization {
            stack: stack_id,
            name: name("main"),
            scope: root(200),
            profile: root(201),
            genesis_root: root(0),
        })
    })
    .unwrap();
    let branch_id = branch_id(1);
    let fork = record(&a, "fork", || {
        a.fork(&ForkRequest {
            stack: s.id,
            branch: branch_id,
            name: name("work"),
            source: ForkSource::Layer(s.head_layer),
        })
    })
    .unwrap();
    let b = PgHistory::open_writable(config(&path), BINDING, [71; 32]).unwrap();
    assert_eq!(a.catalog_id(), b.catalog_id());
    assert_eq!(a.incarnation(), b.incarnation());
    let counts = a.diagnostics().unwrap();
    record(&a, "catalog-id", || a.catalog_id());
    record(&a, "incarnation", || a.incarnation());
    assert_eq!(a.diagnostics().unwrap(), counts);
    let make = |w, candidate| StageRequest {
        workspace: workspace(w),
        branch: branch_id,
        expected_head: None,
        expected_base: s.head_layer,
        expected_root: fork.effective_root,
        construction_base_root: fork.effective_root,
        intended_commit_base: s.head_layer,
        candidate_root: root(candidate),
        profile: s.profile,
        scope: s.scope,
        generation: 1,
    };
    let winner = record(&a, "stage-changes", || a.stage_changes(&make(1, 1))).unwrap();
    let loser = b.stage_changes(&make(2, 2)).unwrap();
    let head = match record(&a, "commit-staged", || {
        a.commit_staged(&CommitStagedRequest {
            workspace: winner.workspace,
            token: winner.token,
        })
    })
    .unwrap()
    {
        CommitStagedOutcome::Committed(c) => c,
        _ => panic!(),
    };
    let error = b
        .commit_staged(&CommitStagedRequest {
            workspace: loser.workspace,
            token: loser.token,
        })
        .unwrap_err();
    assert!(
        matches!(error.cause(),HistoryError::HeadMoved(moved) if moved.actual_head == Some(head.id))
    );
    assert_eq!(b.stage(loser.workspace).unwrap(), Some(loser.clone()));
    assert_eq!(
        b.branch(branch_id).unwrap().unwrap().head_commit,
        Some(head.id)
    );
    record(&a, "layer-stack", || a.layer_stack(s.id)).unwrap();
    record(&a, "layer-stacks", || {
        a.layer_stacks(&Page {
            cursor: None,
            limit: 128,
        })
    })
    .unwrap();
    record(&a, "branch", || a.branch(branch_id)).unwrap();
    record(&a, "branch-snapshot", || a.branch_snapshot(branch_id)).unwrap();
    record(&a, "branches", || {
        a.branches(
            s.id,
            &Page {
                cursor: None,
                limit: 128,
            },
        )
    })
    .unwrap();
    record(&a, "commit", || a.commit(head.id)).unwrap();
    record(&a, "layer", || a.layer(s.head_layer)).unwrap();
    record(&a, "stage", || a.stage(loser.workspace)).unwrap();
    record(&a, "stages", || {
        a.stages(
            branch_id,
            &Page {
                cursor: None,
                limit: 128,
            },
        )
    })
    .unwrap();
    record(&a, "commit-history", || {
        a.commit_history(&CommitHistoryRequest {
            branch: branch_id,
            start: None,
            cursor: None,
            limit: 128,
        })
    })
    .unwrap();
    record(&a, "layer-history", || {
        a.layer_history(&LayerHistoryRequest {
            stack: s.id,
            start: None,
            cursor: None,
            limit: 128,
        })
    })
    .unwrap();
    record(&a, "add-layer", || {
        a.add_layer(&AddLayerRequest {
            stack: s.id,
            branch: branch_id,
            commit: head.id,
            expected_stack_head: s.head_layer,
            expected_branch_base: s.head_layer,
        })
    })
    .unwrap();
    record(&a, "discard-stage", || {
        a.discard_stage(&DiscardRequest {
            workspace: loser.workspace,
            token: loser.token,
        })
    })
    .unwrap();
    record(&a, "reserve-inodes", || {
        a.reserve_inodes(&ReserveRequest {
            scope: s.scope,
            count: 4,
        })
    })
    .unwrap();
    let mut control = control(&path);
    control
        .batch_execute("BEGIN; LOCK TABLE history_meta IN EXCLUSIVE MODE NOWAIT")
        .unwrap();
    let refusal = record(&a, "write-busy", || {
        a.reserve_inodes(&ReserveRequest {
            scope: s.scope,
            count: 1,
        })
    });
    assert_eq!(refusal.unwrap_err(), HistoryError::Busy);
    assert!(b.branch(branch_id).unwrap().is_some());
    control.batch_execute("ROLLBACK").unwrap();
    println!(
        "DIAGNOSTIC history-authority-a {:?}",
        a.diagnostics().unwrap()
    );
    println!(
        "DIAGNOSTIC history-authority-b {:?}",
        b.diagnostics().unwrap()
    );
}
