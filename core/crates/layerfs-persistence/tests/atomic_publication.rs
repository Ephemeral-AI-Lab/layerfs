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
fn committed_up_to_date_and_exact_conflict_each_use_one_transaction() {
    let f = Fixture::new("outcomes");
    let h = &f.handles.history;
    let before = f.handles.diagnostics().unwrap();
    let committed = h.stage_and_commit(&f.request).unwrap();
    let after = f.handles.diagnostics().unwrap();
    assert_eq!(after.write_transactions - before.write_transactions, 1);
    assert_eq!(after.write_commits - before.write_commits, 1);
    let record = match committed {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("{other:?}"),
    };
    assert!(h.stage(f.request.workspace).unwrap().is_none());
    assert_eq!(
        h.branch_snapshot(f.request.branch)
            .unwrap()
            .unwrap()
            .effective_root,
        record.root
    );

    let mut stale = f.request.clone();
    stale.workspace = WorkspaceId::from_authority([4; 32]).unwrap();
    let before = f.handles.diagnostics().unwrap();
    let conflict = h.stage_and_commit(&stale).unwrap_err();
    let after = f.handles.diagnostics().unwrap();
    assert_eq!(after.write_transactions - before.write_transactions, 1);
    assert_eq!(after.write_commits - before.write_commits, 0);
    assert_eq!(after.rollbacks - before.rollbacks, 1);
    assert_eq!(
        conflict,
        HistoryError::HeadMoved(Box::new(layerfs_history::error::MovedState {
            expected_head: None,
            actual_head: Some(record.id),
            expected_base: stale.expected_base,
            actual_base: stale.expected_base,
        }))
    );
    assert!(h.stage(stale.workspace).unwrap().is_none());

    let mut same = f.request.clone();
    same.expected_head = Some(record.id);
    same.expected_root = record.root;
    same.construction_base_root = record.root;
    same.candidate_root = record.root;
    same.generation = 2;
    let before = f.handles.diagnostics().unwrap();
    assert_eq!(
        h.stage_and_commit(&same).unwrap(),
        CommitStagedOutcome::UpToDate {
            head: Some(record.id),
            root: record.root
        }
    );
    let after = f.handles.diagnostics().unwrap();
    assert_eq!(after.write_transactions - before.write_transactions, 1);
    assert_eq!(after.write_commits - before.write_commits, 1);
    assert!(h.stage(same.workspace).unwrap().is_none());
    // Successful transitions used two tokens; the conflict's token rolled back.
    let staged = h.stage_changes(&same).unwrap();
    assert_eq!(staged.token.value(), 3);
    h.discard_stage(&DiscardRequest {
        workspace: staged.workspace,
        token: staged.token,
    })
    .unwrap();
    println!("ATOMIC_HISTORY committed=1tx up_to_date=1tx conflict=1tx/1rollback exact_head_moved=true new_stage_rows=0 failed_token_consumption=0");
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
