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
    ForkRequest, ForkSource, GcDeletion, GcIntent, GcIntentKind, GcScopeKind, PathRemovalKind,
    PathRemovalRequest,
};
use layerfs_history::{CatalogId, HistoryCatalog, LayerStackId};
use support::pg::{
    commit, commit_request, counts, epoch, layer_request, merge_request, project, publish, url,
    Schema,
};
use support::root;
use support::{branch_id, name};

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
    .with_phase(Some("init"))
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
    .with_phase(Some("merge"))
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
    let HistoryError::BusyOp(state) = &refusal else {
        panic!("an Init is a Busy operation, not a mount: {refusal:?}");
    };
    assert_eq!(state.kind, GcIntentKind::Init);
    assert_eq!(state.phase.as_deref(), Some("init"));
    handles.history.gc_release(&op(5)).unwrap();

    handles.history.gc_register(&merge(project_id, 7)).unwrap();
    let refusal = handles
        .history
        .gc_remove_path(&path_request(project_id), &removal(project_id, 8))
        .unwrap_err();
    let HistoryError::BusyOp(state) = &refusal else {
        panic!("a merge is a Busy operation, not a mount: {refusal:?}");
    };
    assert_eq!(state.kind, GcIntentKind::Merge);
    assert_eq!(state.phase.as_deref(), Some("merge"));
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

/// Runs `run` while this test holds `project_epoch` against readers, waits
/// until the writer blocks on that lock (its fence read), then retires `stack`
/// in the holding transaction before releasing it. The writer's resource
/// pre-read and its fence read therefore straddle a committed removal — the
/// exact window the fix closes.
fn retire_under_fence<T, F>(schema: &Schema, stack: LayerStackId, run: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let mut admin = schema.admin();
    admin
        .batch_execute("BEGIN; LOCK TABLE project_epoch IN ACCESS EXCLUSIVE MODE")
        .expect("hold the epoch against readers");
    let writer = std::thread::spawn(run);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let waiting: i64 = admin
            .query_one(
                "SELECT count(*) FROM pg_locks l \
                  WHERE l.relation = 'project_epoch'::regclass AND NOT l.granted",
                &[],
            )
            .expect("lock waiters")
            .get(0);
        if waiting > 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the writer never reached its fence read"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    admin
        .execute(
            "INSERT INTO layer_stack_retired(layer_stack_id, deleted_ms) VALUES ($1, 1)",
            &[&stack.as_slice()],
        )
        .expect("retire the stack");
    admin
        .execute(
            "INSERT INTO branch_retired(branch_id, removed_ms) \
             SELECT branch_id, 1 FROM branch WHERE layer_stack_id = $1 \
             ON CONFLICT DO NOTHING",
            &[&stack.as_slice()],
        )
        .expect("retire the branches");
    admin
        .execute(
            "UPDATE project_epoch SET epoch = epoch + 1 WHERE layer_stack_id = $1",
            &[&stack.as_slice()],
        )
        .expect("bump the epoch");
    admin.batch_execute("COMMIT").expect("release the epoch");
    writer.join().expect("the writer thread")
}

fn branch_rows(schema: &Schema) -> i64 {
    schema
        .admin()
        .query_one("SELECT count(*) FROM branch", &[])
        .unwrap()
        .get(0)
}

/// Branch rows a caller can still see: `branch.remove` is a soft marker, so a
/// retired Branch keeps its row until compaction.
fn live_branch_rows(schema: &Schema) -> i64 {
    schema
        .admin()
        .query_one(
            "SELECT count(*) FROM branch b \
             WHERE NOT EXISTS (SELECT 1 FROM branch_retired r WHERE r.branch_id = b.branch_id)",
            &[],
        )
        .unwrap()
        .get(0)
}

/// Finding 4: the fence opens before the Branch read, so a Branch retired
/// between the write's resource read and its fence is visible on the re-read.
/// Reading the Branch first let the Commit land on a retired Branch.
#[test]
fn a_commit_retired_in_the_fence_window_never_lands() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_window_commit");
    let binding = binding("fence_window_commit");
    let handles = std::sync::Arc::new(schema.create(&binding, CURSOR));
    let (shop, work, _genesis) = project(&handles.history, 1);
    let _first = commit(&handles.history, work, 5);
    let request = commit_request(&handles.history, work, root(6));
    let before = counts(&mut schema.admin());

    let writer = handles.clone();
    let refused = retire_under_fence(&schema, shop, move || {
        writer.history.commit_branch(&request)
    })
    .expect_err("a Commit cannot land on a Branch retired in the window");
    assert!(
        matches!(refused, HistoryError::Missing(Missing::Branch)),
        "{refused:?}"
    );
    assert_eq!(
        counts(&mut schema.admin()),
        before,
        "no Commit row was written"
    );
}

/// The same window on the Layer publication: the stack is re-read after the
/// fence, so a retired stack is `Missing` instead of being extended.
#[test]
fn a_publish_retired_in_the_fence_window_never_lands() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_window_publish");
    let binding = binding("fence_window_publish");
    let handles = std::sync::Arc::new(schema.create(&binding, CURSOR));
    let (shop, work, _genesis) = project(&handles.history, 1);
    let _first = commit(&handles.history, work, 5);
    let request = layer_request(&handles.history, shop, work);
    let before = counts(&mut schema.admin());

    let writer = handles.clone();
    let refused = retire_under_fence(&schema, shop, move || writer.history.add_layer(&request))
        .expect_err("a Layer cannot extend a stack retired in the window");
    assert!(matches!(refused, HistoryError::Missing(_)), "{refused:?}");
    assert_eq!(
        counts(&mut schema.admin()),
        before,
        "no Layer row was written"
    );
}

#[test]
fn a_merge_retired_in_the_fence_window_never_starts() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_window_merge");
    let binding = binding("fence_window_merge");
    let handles = std::sync::Arc::new(schema.create(&binding, CURSOR));
    let (shop, work, genesis) = project(&handles.history, 1);
    let first = commit(&handles.history, work, 5);
    let request = merge_request(&handles.history, shop, work, first.id, genesis, 60);
    let before = counts(&mut schema.admin());

    let writer = handles.clone();
    let refused = retire_under_fence(&schema, shop, move || writer.history.merge_branch(&request))
        .expect_err("a merge cannot start on a stack retired in the window");
    assert!(matches!(refused, HistoryError::Missing(_)), "{refused:?}");
    assert_eq!(
        counts(&mut schema.admin()),
        before,
        "no merge Commit or Branch was written"
    );
}

#[test]
fn a_fork_retired_in_the_fence_window_never_starts() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_window_fork");
    let binding = binding("fence_window_fork");
    let handles = std::sync::Arc::new(schema.create(&binding, CURSOR));
    let (shop, _work, genesis) = project(&handles.history, 1);
    let request = ForkRequest {
        stack: shop,
        branch: branch_id(7),
        name: name("late"),
        source: ForkSource::Layer(genesis),
    };
    let before = branch_rows(&schema);

    let writer = handles.clone();
    let refused = retire_under_fence(&schema, shop, move || writer.history.fork(&request))
        .expect_err("a fork cannot start on a stack retired in the window");
    assert!(matches!(refused, HistoryError::Missing(_)), "{refused:?}");
    assert_eq!(branch_rows(&schema), before, "no Branch row was written");
}

/// Finding 4: fork now takes the project fence at all; a project whose epoch
/// row is gone is `Conflict`, like every other writer.
#[test]
fn a_fork_without_a_project_epoch_is_conflict() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_fork_missing");
    let binding = binding("fence_fork_missing");
    let handles = schema.create(&binding, CURSOR);
    let (shop, _work, genesis) = project(&handles.history, 1);
    let request = ForkRequest {
        stack: shop,
        branch: branch_id(7),
        name: name("late"),
        source: ForkSource::Layer(genesis),
    };
    schema
        .admin()
        .execute(
            "DELETE FROM project_epoch WHERE layer_stack_id=$1",
            &[&shop.as_slice()],
        )
        .unwrap();
    let refused = handles.history.fork(&request).unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
}

/// Finding 11: `project.delete` takes the Store key shared and only its own
/// project key exclusively. A mount of another project must not refuse it, and
/// the removal must still refuse a mount of its own project.
#[test]
fn a_project_removal_does_not_take_the_store_key_exclusively() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "keys_project");
    let binding = binding("keys_project");
    let handles = schema.create(&binding, CURSOR);
    let (shop_a, work_a, _) = project(&handles.history, 1);
    let (shop_b, _work_b, _) = project(&handles.history, 2);

    // A live mount in project A takes the Store key shared. Project names are
    // unique in a Store, so the two projects carry different names.
    let mount_a = mount(shop_a, work_a, 1).named(Some("project-a"), Some("work-a"));
    handles.history.gc_register(&mount_a).unwrap();
    // Deleting project B registers Store shared + project B exclusive.
    let removal_b = removal(shop_b, 2).named(Some("project-b"), None);
    handles.history.gc_register(&removal_b).unwrap();
    // The removal's own project is excluded...
    let mount_b = mount(shop_b, branch_id(2), 3).named(Some("project-b"), Some("work-b"));
    assert!(matches!(
        handles.history.gc_register(&mount_b).unwrap_err(),
        HistoryError::BusyOp(_) | HistoryError::Busy
    ));
    // ...but another project's mount is not: the Store key stayed shared.
    let mount_a2 = mount(shop_a, work_a, 4).named(Some("project-a"), Some("work-a"));
    handles.history.gc_register(&mount_a2).unwrap();
    // The delete itself still admits with the other project mounted.
    let marked = handles
        .history
        .gc_delete_stack(shop_b, 1_000, &removal_b)
        .unwrap();
    assert!(matches!(marked, GcDeletion::Marked { deleted_ms: 1_000 }));
    handles.history.gc_release(&op(1)).unwrap();
    handles.history.gc_release(&op(2)).unwrap();
    handles.history.gc_release(&op(4)).unwrap();
}

/// Finding 8: PostgreSQL checks `catalog_incarnation` in every write
/// transaction, so a session opened at `G` cannot publish at `G` after a swap
/// has installed `G + 1`. The refused transaction writes nothing, and a fresh
/// open at the installed generation works.
#[test]
fn a_write_from_a_session_opened_before_the_swap_is_stale() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_generation");
    let binding = binding("fence_generation");
    let handles = schema.create(&binding, CURSOR);
    let (shop, work, _) = project(&handles.history, 1);
    let first = commit(&handles.history, work, 5);
    // The client reads at `G` build the request before the swap, as a real
    // caller does; the write that runs after it is the one under test.
    let request = commit_request(&handles.history, work, root(6));
    let before = counts(&mut schema.admin());

    // Stand in for compaction's Phase B swap: install `G + 1`.
    schema
        .admin()
        .execute(
            "UPDATE history_meta SET catalog_incarnation=catalog_incarnation+1 WHERE id=1",
            &[],
        )
        .unwrap();

    let refused = handles.history.commit_branch(&request).unwrap_err();
    assert!(
        matches!(refused, HistoryError::StaleStore),
        "a Commit at the old generation is StaleStore: {refused:?}"
    );
    let refused = handles
        .history
        .fork(&ForkRequest {
            stack: shop,
            branch: branch_id(9),
            name: name("stale"),
            source: ForkSource::Commit {
                branch: work,
                commit: first.id,
            },
        })
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::StaleStore),
        "a fork at the old generation is StaleStore: {refused:?}"
    );
    assert_eq!(counts(&mut schema.admin()), before, "nothing was written");

    // Reopening at the installed generation is the documented recovery.
    let fresh = schema.open(&binding, CURSOR).unwrap();
    let landed = fresh
        .history
        .commit_branch(&commit_request(&fresh.history, work, root(6)))
        .unwrap();
    assert!(matches!(
        landed,
        layerfs_history::records::CommitOutcome::Committed { .. }
    ));
}

/// F44R2-1: the same generation fence guards reads. A session opened at `G`
/// cannot render a project or a Branch at `G` after a swap installed `G + 1`:
/// the `Mode::Read` transaction and the single-statement reads join
/// `history_meta.catalog_incarnation` and answer `StaleStore`, and a fresh
/// open at the installed generation reads them.
#[test]
fn a_read_from_a_session_opened_before_the_swap_is_stale() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "fence_read_generation");
    let binding = binding("fence_read_generation");
    let handles = schema.create(&binding, CURSOR);
    let (shop, work, _) = project(&handles.history, 1);
    commit(&handles.history, work, 5);

    // Stand in for compaction's Phase B swap: install `G + 1`.
    schema
        .admin()
        .execute(
            "UPDATE history_meta SET catalog_incarnation=catalog_incarnation+1 WHERE id=1",
            &[],
        )
        .unwrap();

    // A read transaction: the check rides in its BEGIN batch.
    let refused = handles.history.layer_stack(shop).unwrap_err();
    assert!(
        matches!(refused, HistoryError::StaleStore),
        "a project read at the old generation is StaleStore: {refused:?}"
    );
    // A single-statement read that resolves a Branch and its roots.
    let refused = handles.history.branch_snapshot(work).unwrap_err();
    assert!(
        matches!(refused, HistoryError::StaleStore),
        "a Branch snapshot at the old generation is StaleStore: {refused:?}"
    );
    // The by-name one-statement read joins the same generation column.
    let refused = handles
        .history
        .branch_snapshot_named(&name("project-1"), &name("work"))
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::StaleStore),
        "a named Branch snapshot at the old generation is StaleStore: {refused:?}"
    );

    // Reopening at the installed generation is the documented recovery.
    let fresh = schema.open(&binding, CURSOR).unwrap();
    assert!(fresh.history.layer_stack(shop).unwrap().is_some());
    assert!(fresh.history.branch_snapshot(work).unwrap().is_some());
}

/// Two concurrent `branch.remove` calls in one project take the same epoch
/// row. The marker transaction locks it `FOR UPDATE` first, so the second
/// waits for the first and then bumps the value it read; both removals land
/// with one epoch bump each instead of a `40P01` deadlock on a shared-lock
/// upgrade.
#[test]
fn two_concurrent_branch_removes_in_one_project_are_ordered() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "concurrent_removal");
    let binding = binding("concurrent_removal");
    let first = schema.create(&binding, CURSOR);
    let (project_id, work, genesis) = project(&first.history, 1);
    let other = branch_id(2);
    first
        .history
        .fork(&ForkRequest {
            stack: project_id,
            branch: other,
            name: name("other"),
            source: ForkSource::Layer(genesis),
        })
        .unwrap();
    let second = schema.open(&binding, CURSOR).expect("a second session");
    let (left, right) = std::thread::scope(|scope| {
        let left = scope.spawn(|| {
            first
                .history
                .gc_remove_branch(work, 1_000, &removal(project_id, 40))
        });
        let right = scope.spawn(|| {
            second
                .history
                .gc_remove_branch(other, 2_000, &removal(project_id, 41))
        });
        (left.join().unwrap(), right.join().unwrap())
    });
    for result in [&left, &right] {
        match result {
            Ok(()) => {}
            // A bounded lane wait is a definite typed refusal, never a panic.
            Err(HistoryError::Busy) => {}
            Err(other) => panic!("a removal refusal must be typed: {other:?}"),
        }
    }
    assert_eq!(epoch(&mut schema.admin(), project_id), 2);
    assert_eq!(live_branch_rows(&schema), 0, "both Branches are retired");
}

/// A shared Store may be admitted from another machine whose boot and PID
/// namespace this one cannot observe, so a recorded owner on another boot
/// stays conservatively live: it is reported as a holder and never adopted,
/// until its owner releases it. (A local SQLite Store adopts the same row;
/// see `admission.rs#an_earlier_boot_is_settled_on_a_local_store`.)
#[test]
fn another_boot_stays_conservatively_live_on_a_shared_store() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "other_boot");
    let binding = binding("other_boot");
    let handles = schema.create(&binding, CURSOR);
    let (project_id, branch, _genesis) = project(&handles.history, 1);
    let mount = mount(project_id, branch, 50).owned(Some("pid:1@1@ns:1"), Some("boot:0:test:50"));
    handles.history.gc_register(&mount).unwrap();
    let refusal = handles
        .history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 51))
        .unwrap_err();
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("a shared Store keeps an unobservable owner live: {refusal:?}");
    };
    assert_eq!(state.holders.len(), 1);
    assert_eq!(state.holders[0].op, hex16(&op(50)));

    handles.history.gc_release(&op(50)).unwrap();
    handles
        .history
        .gc_remove_branch(branch, 2_000, &removal(project_id, 52))
        .unwrap();
    assert_eq!(epoch(&mut schema.admin(), project_id), 1);
}
