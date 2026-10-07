//! One combined history transition over real Disposable Stores.
#[path = "../../layerfs-daemon/tests/support/held_writer.rs"]
mod held_writer;
use layerfs_content::ObjectId;
use layerfs_history::{
    BranchId, CommitStagedOutcome, DiscardRequest, ForkRequest, ForkSource, HistoryCatalog,
    HistoryCatalogConfig, HistoryError, HistoryName, LayerStackId, StackInitialization,
    StageRequest, WorkspaceId,
};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use std::path::PathBuf;

struct Fixture {
    directory: PathBuf,
    handles: Handles,
    request: StageRequest,
}
fn id(label: &str) -> ObjectId {
    ObjectId::for_bytes(label.as_bytes())
}
impl Fixture {
    fn new(label: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-atomic-history-{}-{label}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let handles = Handles::create(
            PersistenceConfig::sqlite(directory.join("store.sqlite"))
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: b"atomic-history".to_vec(),
                incarnation: 1,
                cursor_key: [32; 32],
            },
        )
        .unwrap();
        let stack = handles
            .history
            .initialize_layerstack(&StackInitialization {
                stack: LayerStackId::from_authority([1; 16]),
                name: HistoryName::new("stack").unwrap(),
                scope: id("scope"),
                profile: id("profile"),
                genesis_root: id("genesis"),
            })
            .unwrap();
        let branch = handles
            .history
            .fork(&ForkRequest {
                stack: stack.id,
                branch: BranchId::from_authority([2; 16]),
                name: HistoryName::new("main").unwrap(),
                source: ForkSource::Layer(stack.head_layer),
            })
            .unwrap();
        let request = StageRequest {
            workspace: WorkspaceId::from_authority([3; 32]).unwrap(),
            branch: branch.branch.id,
            expected_head: None,
            expected_base: stack.head_layer,
            expected_root: branch.effective_root,
            construction_base_root: branch.effective_root,
            intended_commit_base: stack.head_layer,
            candidate_root: id("candidate"),
            profile: stack.profile,
            scope: stack.scope,
            generation: 1,
        };
        Self {
            directory,
            handles,
            request,
        }
    }
    fn cleanup(self) {
        drop(self.handles);
        std::fs::remove_dir_all(self.directory).unwrap();
    }
}

#[test]
fn overwrite_and_current_root_up_to_date_each_use_one_transaction() {
    let f = Fixture::new("overwrite-outcomes");
    let h = &f.handles.history;
    let publish = |request: &StageRequest| {
        let before = f.handles.diagnostics().unwrap();
        let outcome = h.stage_and_commit(request).unwrap();
        let after = f.handles.diagnostics().unwrap();
        assert_eq!(after.write_transactions - before.write_transactions, 1);
        assert_eq!(after.write_commits - before.write_commits, 1);
        assert_eq!(after.rollbacks, before.rollbacks);
        assert!(h.stage(request.workspace).unwrap().is_none());
        outcome
    };
    let record = match publish(&f.request) {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("{other:?}"),
    };
    assert_eq!(record.parent, None);
    let mut stale = f.request.clone();
    stale.workspace = WorkspaceId::from_authority([4; 32]).unwrap();
    stale.candidate_root = id("later-candidate");
    let later = match publish(&stale) {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("{other:?}"),
    };
    assert_eq!(later.parent, None, "keep captured provenance, never rebase");
    assert_ne!(later.id, record.id);
    assert_eq!(
        h.branch_snapshot(stale.branch)
            .unwrap()
            .unwrap()
            .effective_root,
        later.root
    );
    assert_eq!(
        publish(&stale),
        CommitStagedOutcome::UpToDate {
            head: Some(later.id),
            root: later.root,
        }
    );
    // An unchanged capture still overwrites a different current Branch root.
    stale.candidate_root = stale.expected_root;
    let restored = match publish(&stale) {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("{other:?}"),
    };
    assert_eq!(restored.parent, None);
    assert_eq!(restored.root, f.request.expected_root);
    let final_snapshot = h.branch_snapshot(stale.branch).unwrap().unwrap();
    assert_eq!(final_snapshot.branch.head_commit, Some(restored.id));
    assert_eq!(final_snapshot.effective_root, restored.root);
    assert_eq!(h.commit(record.id).unwrap(), Some(record));
    assert_eq!(h.commit(later.id).unwrap(), Some(later));
    let staged = h.stage_changes(&stale).unwrap();
    assert_eq!(staged.token.value(), 5);
    h.discard_stage(&DiscardRequest {
        workspace: staged.workspace,
        token: staged.token,
    })
    .unwrap();
    println!("ATOMIC_OVERWRITE publications=3 up_to_date=1 transactions_each=1 stages_after_each=0 captured_parent_preserved=true stale_unchanged_overwrites=true displaced_commits_retained=true");
    f.cleanup();
}

#[test]
fn an_existing_stage_is_preserved_on_refusal() {
    let f = Fixture::new("owned-stage");
    let original = f.handles.history.stage_changes(&f.request).unwrap();
    assert_eq!(
        f.handles.history.stage_and_commit(&f.request).unwrap_err(),
        HistoryError::InvalidInput("stage already present")
    );
    assert_eq!(
        f.handles.history.stage(f.request.workspace).unwrap(),
        Some(original.clone())
    );
    f.handles
        .history
        .discard_stage(&DiscardRequest {
            workspace: original.workspace,
            token: original.token,
        })
        .unwrap();
    f.cleanup();
}

#[test]
fn process_contention_is_busy_before_effect_and_later_explicit_call_succeeds() {
    let f = Fixture::new("busy");
    let held = held_writer::HeldWriter::acquire(&f.directory.join("store.sqlite"));
    let before = f.handles.diagnostics().unwrap();
    assert_eq!(
        f.handles.history.stage_and_commit(&f.request).unwrap_err(),
        HistoryError::Busy
    );
    let after = f.handles.diagnostics().unwrap();
    assert_eq!(after.write_transactions, before.write_transactions);
    assert_eq!(after.statements - before.statements, 1);
    assert_eq!(after.rollbacks, before.rollbacks);
    assert!(f
        .handles
        .history
        .stage(f.request.workspace)
        .unwrap()
        .is_none());
    assert_eq!(
        f.handles
            .history
            .branch(f.request.branch)
            .unwrap()
            .unwrap()
            .head_commit,
        None
    );
    held.release();
    assert!(matches!(
        f.handles.history.stage_and_commit(&f.request).unwrap(),
        CommitStagedOutcome::Committed(_)
    ));
    assert!(f
        .handles
        .history
        .stage(f.request.workspace)
        .unwrap()
        .is_none());
    println!("ATOMIC_HISTORY_BUSY statements=1 successful_write_transactions=0 rollbacks=0 new_stage_rows=0 later_explicit_call=Committed child=released-and-joined");
    f.cleanup();
}
