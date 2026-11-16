//! PostgreSQL runtime evidence for the GC admission and epoch paths that
//! shipped compile-only: T6's `gc_intent` admission and T6.5's writer-side
//! `project_epoch` fence, including the `SELECT ... FOR SHARE` re-check.
//!
//! The tests drive `PostgresHandles` through the public `HistoryCatalog`
//! methods. A trigger stands in for a removal marker that bumps the epoch
//! inside the writer's own transaction, which is exactly where the fence's
//! re-read runs; a deleted epoch row stands in for a compacted-away project.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#![cfg(feature = "postgresql")]
mod support;

use layerfs_history::error::{HistoryError, Missing};
use layerfs_history::records::{
    GcDeletion, GcIntent, GcIntentKind, GcScopeKind, PathRemovalKind, PathRemovalRequest,
};
use layerfs_history::{CatalogId, HistoryCatalog};
use support::pg::{
    commit, commit_request, counts, epoch, layer_request, merge_request, project, publish, url,
    Schema,
};
use support::root;

const CURSOR: [u8; 32] = [78; 32];

/// Every test opens its own catalog identity, as a real Store does: the
/// PostgreSQL advisory keys are server-global, so two tests that shared a
/// binding could see each other's intents while running in parallel.
fn binding(tag: &str) -> Vec<u8> {
    format!("postgresql-gc-fence-{tag}").into_bytes()
}

fn op(tag: u8) -> [u8; 16] {
    [tag; 16]
}

fn mount(
    stack: layerfs_history::LayerStackId,
    branch: layerfs_history::BranchId,
    tag: u8,
) -> GcIntent {
    GcIntent::shared(
        op(tag),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("proj"), Some("work"))
    .placed(
        Some([tag; 32]),
        Some(&format!("sandbox-{tag}")),
        Some(&format!("machine-{tag}")),
    )
    .owned(Some("runtime-a"), Some(&format!("boot-{tag}")))
}

fn init(stack: layerfs_history::LayerStackId, tag: u8) -> GcIntent {
    GcIntent::shared(
        op(tag),
        GcIntentKind::Init,
        GcScopeKind::Project,
        stack.as_slice(),
    )
    .named(Some("proj"), None)
    .owned(Some("runtime-a"), Some(&format!("boot-{tag}")))
}

fn merge(stack: layerfs_history::LayerStackId, tag: u8) -> GcIntent {
    GcIntent::shared(
        op(tag),
        GcIntentKind::Merge,
        GcScopeKind::Project,
        stack.as_slice(),
    )
    .named(Some("proj"), Some("merged"))
    .owned(Some("runtime-a"), Some(&format!("boot-{tag}")))
}

fn removal(stack: layerfs_history::LayerStackId, body: u8) -> GcIntent {
    GcIntent::operation(
        op(body),
        GcIntentKind::Removal,
        GcScopeKind::Project,
        stack.as_slice(),
    )
    .named(Some("proj"), None)
}

fn path_request(stack: layerfs_history::LayerStackId) -> PathRemovalRequest {
    PathRemovalRequest {
        stack,
        path: b"a.txt".to_vec(),
        removed_ms: 1_000,
    }
}

fn hex16(bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// A trigger that bumps the project epoch before each inserted row, standing
/// in for a removal marker that wins the race inside the writer's transaction.
fn bump_trigger(admin: &mut postgres::Client, table: &str) {
    admin
        .batch_execute(&format!(
            "CREATE OR REPLACE FUNCTION g6a_epoch_race() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN UPDATE project_epoch SET epoch=epoch+1 WHERE layer_stack_id=NEW.layer_stack_id; RETURN NEW; END $$; \
             CREATE TRIGGER g6a_epoch_race BEFORE INSERT ON \"{table}\" FOR EACH ROW \
             EXECUTE FUNCTION g6a_epoch_race();"
        ))
        .unwrap();
}

fn drop_trigger(admin: &mut postgres::Client) {
    admin
        .batch_execute(
            "DROP TRIGGER IF EXISTS g6a_epoch_race ON \"commit\"; \
             DROP TRIGGER IF EXISTS g6a_epoch_race ON layer; \
             DROP FUNCTION IF EXISTS g6a_epoch_race();",
        )
        .unwrap();
}

#[test]
fn a_normal_commit_publish_and_merge_leave_the_epoch_alone() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_normal");
    let binding = binding("fence_normal");
    let handles = schema.create(&binding, CURSOR);
    let (shop, work, genesis) = project(&handles.history, 1);

    let first = commit(&handles.history, work, 5);
    assert_eq!(
        epoch(&mut schema.admin(), shop),
        0,
        "a Commit does not bump the epoch"
    );
    let layer = publish(&handles.history, shop, work);
    assert_eq!(layer.parent, Some(genesis));
    assert_eq!(
        epoch(&mut schema.admin(), shop),
        0,
        "a published Layer does not bump it"
    );
    let merged = handles
        .history
        .merge_branch(&merge_request(
            &handles.history,
            shop,
            work,
            first.id,
            genesis,
            60,
        ))
        .unwrap();
    assert_eq!(merged.branch.name.as_str(), "merged");
    assert_eq!(
        epoch(&mut schema.admin(), shop),
        0,
        "a merge does not bump it"
    );
    assert_eq!(counts(&mut schema.admin()), (2, 2));
}

#[test]
fn a_missing_epoch_row_refuses_every_writer_with_conflict() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_missing");
    let binding = binding("fence_missing");
    let handles = schema.create(&binding, CURSOR);
    let (shop, work, genesis) = project(&handles.history, 1);
    let first = commit(&handles.history, work, 5);
    let side_request = merge_request(&handles.history, shop, work, first.id, genesis, 61);
    let before = counts(&mut schema.admin());

    schema
        .admin()
        .execute(
            "DELETE FROM project_epoch WHERE layer_stack_id=$1",
            &[&shop.as_slice()],
        )
        .unwrap();

    let refused = handles
        .history
        .commit_branch(&commit_request(&handles.history, work, root(6)))
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    let refused = handles
        .history
        .add_layer(&layer_request(&handles.history, shop, work))
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    let refused = handles.history.merge_branch(&side_request).unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );

    assert_eq!(counts(&mut schema.admin()), before, "no partial write");
    let snapshot = handles.history.branch_snapshot(work).unwrap().unwrap();
    assert_eq!(snapshot.branch.head_commit, Some(first.id));
    assert_eq!(
        handles
            .history
            .layer_stack(shop)
            .unwrap()
            .unwrap()
            .head_layer,
        genesis
    );
    assert!(handles.history.branch(merged_branch()).unwrap().is_none());
}

/// The Branch id `merge_request` creates for the merge.
fn merged_branch() -> layerfs_history::BranchId {
    layerfs_history::BranchId::from_authority([9; 16])
}

#[test]
fn an_epoch_change_during_a_commit_is_detected_and_rolled_back() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_trigger_commit");
    let binding = binding("fence_trigger_commit");
    let handles = schema.create(&binding, CURSOR);
    let (shop, work, _genesis) = project(&handles.history, 1);

    bump_trigger(&mut schema.admin(), "commit");
    let refused = handles
        .history
        .commit_branch(&commit_request(&handles.history, work, root(5)))
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    assert_eq!(counts(&mut schema.admin()).0, 0, "the Commit rolled back");
    assert_eq!(
        epoch(&mut schema.admin(), shop),
        0,
        "the fake bump rolled back too"
    );
    assert!(handles
        .history
        .branch(work)
        .unwrap()
        .unwrap()
        .head_commit
        .is_none());

    drop_trigger(&mut schema.admin());
    let record = commit(&handles.history, work, 5);
    assert_eq!(record.stack, shop);
    assert_eq!(epoch(&mut schema.admin(), shop), 0);
    assert_eq!(
        handles.history.branch(work).unwrap().unwrap().head_commit,
        Some(record.id)
    );
}

#[test]
fn an_epoch_change_during_a_publish_is_detected_and_rolled_back() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_trigger_layer");
    let binding = binding("fence_trigger_layer");
    let handles = schema.create(&binding, CURSOR);
    let (shop, work, genesis) = project(&handles.history, 1);
    let _first = commit(&handles.history, work, 5);
    let before = counts(&mut schema.admin());

    bump_trigger(&mut schema.admin(), "layer");
    let refused = handles
        .history
        .add_layer(&layer_request(&handles.history, shop, work))
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    assert_eq!(counts(&mut schema.admin()), before, "the Layer rolled back");
    assert_eq!(epoch(&mut schema.admin(), shop), 0);
    assert_eq!(
        handles
            .history
            .layer_stack(shop)
            .unwrap()
            .unwrap()
            .head_layer,
        genesis
    );

    drop_trigger(&mut schema.admin());
    let layer = publish(&handles.history, shop, work);
    assert_eq!(layer.parent, Some(genesis));
    assert_eq!(counts(&mut schema.admin()).1, before.1 + 1);
}

#[test]
fn an_epoch_change_during_a_merge_is_detected_and_rolled_back() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_trigger_merge");
    let binding = binding("fence_trigger_merge");
    let handles = schema.create(&binding, CURSOR);
    let (shop, work, genesis) = project(&handles.history, 1);
    let first = commit(&handles.history, work, 5);
    let request = merge_request(&handles.history, shop, work, first.id, genesis, 62);
    let before = counts(&mut schema.admin());

    bump_trigger(&mut schema.admin(), "commit");
    let refused = handles.history.merge_branch(&request).unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    assert_eq!(counts(&mut schema.admin()), before, "the merge rolled back");
    assert_eq!(epoch(&mut schema.admin(), shop), 0);
    assert!(handles.history.branch(merged_branch()).unwrap().is_none());

    drop_trigger(&mut schema.admin());
    let merged = handles.history.merge_branch(&request).unwrap();
    assert_eq!(merged.branch.name.as_str(), "merged");
    assert_eq!(counts(&mut schema.admin()).0, before.0 + 1);
}

#[test]
fn a_mount_blocks_all_three_removals_until_released() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "admission_mount");
    let binding = binding("admission_mount");
    let handles = schema.create(&binding, CURSOR);
    let (project_id, branch, _genesis) = project(&handles.history, 1);
    let mount = mount(project_id, branch, 1);
    handles.history.gc_register(&mount).unwrap();

    let refusal = handles
        .history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 2))
        .unwrap_err();
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holders.len(), 1);
    assert_eq!(state.holders[0].kind, GcIntentKind::Mount);
    assert_eq!(state.holders[0].sandbox.as_deref(), Some("sandbox-1"));
    assert_eq!(state.holders[0].machine.as_deref(), Some("machine-1"));
    assert_eq!(
        state.holders[0].workspace.as_deref(),
        Some("01".repeat(32).as_str())
    );
    assert_eq!(state.holders[0].op, hex16(&op(1)));

    assert!(matches!(
        handles
            .history
            .gc_delete_stack(project_id, 1_000, &removal(project_id, 3))
            .unwrap_err(),
        HistoryError::MountsActive(_)
    ));
    assert!(matches!(
        handles
            .history
            .gc_remove_path(&path_request(project_id), &removal(project_id, 4))
            .unwrap_err(),
        HistoryError::MountsActive(_)
    ));

    let listed = handles
        .history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].kind, GcIntentKind::Mount);
    handles.history.gc_release(&op(1)).unwrap();
    assert!(handles
        .history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
    handles.history.gc_release(&op(1)).unwrap();

    let outcome = handles
        .history
        .gc_remove_path(&path_request(project_id), &removal(project_id, 4))
        .unwrap();
    assert_eq!(outcome.kind, PathRemovalKind::Removed);
    assert_eq!(epoch(&mut schema.admin(), project_id), 1);
    handles
        .history
        .gc_remove_branch(branch, 2_000, &removal(project_id, 2))
        .unwrap();
    assert_eq!(epoch(&mut schema.admin(), project_id), 2);
    assert!(matches!(
        handles
            .history
            .gc_delete_stack(project_id, 3_000, &removal(project_id, 3))
            .unwrap(),
        GcDeletion::Marked { deleted_ms: 3_000 }
    ));
    assert_eq!(epoch(&mut schema.admin(), project_id), 3);
}

#[test]
fn init_and_merge_are_project_holders() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "admission_shared");
    let binding = binding("admission_shared");
    let handles = schema.create(&binding, CURSOR);
    let (project_id, _branch, _genesis) = project(&handles.history, 1);

    handles.history.gc_register(&init(project_id, 5)).unwrap();
    let refusal = handles
        .history
        .gc_delete_stack(project_id, 1_000, &removal(project_id, 6))
        .unwrap_err();
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holders[0].kind, GcIntentKind::Init);
    handles.history.gc_release(&op(5)).unwrap();

    handles.history.gc_register(&merge(project_id, 7)).unwrap();
    let refusal = handles
        .history
        .gc_remove_path(&path_request(project_id), &removal(project_id, 8))
        .unwrap_err();
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holders[0].kind, GcIntentKind::Merge);
    handles.history.gc_release(&op(7)).unwrap();

    handles
        .history
        .gc_remove_path(&path_request(project_id), &removal(project_id, 8))
        .unwrap();
}

#[test]
fn a_live_operation_is_busy_and_an_uncertain_owner_is_unresolved() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "admission_op");
    let binding = binding("admission_op");
    let handles = schema.create(&binding, CURSOR);
    let (project_id, branch, _genesis) = project(&handles.history, 1);
    let catalog = CatalogId::derive(&binding).unwrap();
    let compaction = GcIntent::operation(
        op(9),
        GcIntentKind::Compaction,
        GcScopeKind::Store,
        catalog.as_slice(),
    )
    .owned(Some("runtime-a"), Some("boot-9"))
    .with_phase(Some("mark"));
    handles.history.gc_register(&compaction).unwrap();

    let refusal = handles
        .history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 10))
        .unwrap_err();
    let HistoryError::BusyOp(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.kind, GcIntentKind::Compaction);
    assert_eq!(state.entity, "store");
    assert_eq!(state.phase.as_deref(), Some("mark"));
    assert_eq!(state.op, hex16(&op(9)));

    assert!(matches!(
        handles
            .history
            .gc_register(&mount(project_id, branch, 11))
            .unwrap_err(),
        HistoryError::BusyOp(_)
    ));

    handles.history.gc_mark_uncertain(&op(9)).unwrap();
    let refusal = handles
        .history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 10))
        .unwrap_err();
    assert!(matches!(refusal, HistoryError::Unresolved(_)));

    handles.history.gc_release(&op(9)).unwrap();
    handles
        .history
        .gc_remove_branch(branch, 2_000, &removal(project_id, 10))
        .unwrap();
}

#[test]
fn a_removal_first_refuses_the_later_commit_and_writes_nothing() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_removal_first");
    let binding = binding("fence_removal_first");
    let handles = schema.create(&binding, CURSOR);
    let (shop, work, _genesis) = project(&handles.history, 1);
    let _first = commit(&handles.history, work, 5);
    let before = counts(&mut schema.admin());

    // Capture the request while the Branch is still visible; the removal then
    // wins, and the filtered Branch resolution hides the retired Branch before
    // the fence opens, so the write is refused and leaves no Commit behind.
    let request = commit_request(&handles.history, work, root(6));
    handles.history.mark_branch_removed(work, 1_000).unwrap();
    assert_eq!(
        epoch(&mut schema.admin(), shop),
        1,
        "the marker bumped the epoch once"
    );

    let refused = handles.history.commit_branch(&request).unwrap_err();
    assert!(
        matches!(
            refused,
            HistoryError::Missing(Missing::Branch) | HistoryError::Conflict("project epoch")
        ),
        "{refused:?}"
    );
    assert_eq!(counts(&mut schema.admin()), before);
}
