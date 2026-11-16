//! #41 T6: admission (`gc_intent`) on SQLite. A mount/Init/merge holds its
//! scopes, a removal sees `MountsActive`/`Busy`/`Unresolved`, the epoch is
//! bumped once by the marker and a missing epoch row is `Conflict`.
mod support;

use layerfs_history::error::{HistoryError, Missing};
use layerfs_history::records::{
    GcDeletion, GcIntent, GcIntentKind, GcScopeKind, PathRemovalKind, PathRemovalRequest,
    StackInitialization,
};
use layerfs_history::{BranchId, CatalogId, HistoryCatalog, HistoryCatalogConfig, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"gc-admission";
const CURSOR: [u8; 32] = [47; 32];

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}

fn catalog(temp: &Temp) -> layerfs_persistence::HistoryProvider {
    support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: CURSOR,
            binding_key: BINDING.to_vec(),
            incarnation: 3,
        },
    )
    .expect("catalog")
}

fn writable(path: &std::path::Path) -> layerfs_persistence::HistoryProvider {
    SqliteHandles::open_writable(config(path), BINDING, CURSOR)
        .expect("writable catalog")
        .history
}

/// Project `proj` with one Branch `work`.
fn project(history: &dyn HistoryCatalog) -> (LayerStackId, BranchId) {
    history
        .initialize_layerstack(&StackInitialization {
            logical: 1,
            stack: stack(1),
            name: name("proj"),
            scope: root(200),
            profile: root(201),
            genesis_root: root(1),
        })
        .unwrap();
    let branch = branch_id(1);
    history
        .fork(&layerfs_history::records::ForkRequest {
            stack: stack(1),
            branch,
            name: name("work"),
            source: layerfs_history::records::ForkSource::Layer(
                history.layer_stack(stack(1)).unwrap().unwrap().head_layer,
            ),
        })
        .unwrap();
    (stack(1), branch)
}

fn op(body: u8) -> [u8; 16] {
    [body; 16]
}

fn mount(stack: LayerStackId, branch: BranchId, tag: u8) -> GcIntent {
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

fn init(stack: LayerStackId, tag: u8) -> GcIntent {
    GcIntent::shared(
        op(tag),
        GcIntentKind::Init,
        GcScopeKind::Project,
        stack.as_slice(),
    )
    .named(Some("proj"), None)
    .owned(Some("runtime-a"), Some(&format!("boot-{tag}")))
}

fn merge(stack: LayerStackId, tag: u8) -> GcIntent {
    GcIntent::shared(
        op(tag),
        GcIntentKind::Merge,
        GcScopeKind::Project,
        stack.as_slice(),
    )
    .named(Some("proj"), Some("merged"))
    .owned(Some("runtime-a"), Some(&format!("boot-{tag}")))
}

fn removal(stack: LayerStackId, body: u8) -> GcIntent {
    GcIntent::operation(
        op(body),
        GcIntentKind::Removal,
        GcScopeKind::Project,
        stack.as_slice(),
    )
    .named(Some("proj"), None)
}

fn epoch(path: &std::path::Path, stack: LayerStackId) -> i64 {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .query_row(
            "SELECT epoch FROM project_epoch WHERE layer_stack_id = ?1",
            [stack.as_slice()],
            |row| row.get(0),
        )
        .unwrap()
}

fn path_request(stack: LayerStackId) -> PathRemovalRequest {
    PathRemovalRequest {
        stack,
        path: b"a.txt".to_vec(),
        removed_ms: 1_000,
    }
}

#[test]
fn a_mount_blocks_all_three_removals_until_released() {
    let temp = Temp::new("admission-mount");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    let mount = mount(project_id, branch, 1);
    history.gc_register(&mount).unwrap();

    let refusal = history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 2))
        .unwrap_err();
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holders.len(), 1);
    assert_eq!(state.holders[0].kind, GcIntentKind::Mount);
    assert_eq!(state.holders[0].sandbox.as_deref(), Some("sandbox-1"));
    assert_eq!(state.holders[0].machine.as_deref(), Some("machine-1"));
    let expected_workspace = "01".repeat(32);
    assert_eq!(
        state.holders[0].workspace.as_deref(),
        Some(expected_workspace.as_str())
    );
    assert_eq!(state.holders[0].op, hex16(&op(1)));

    assert!(matches!(
        history
            .gc_delete_stack(project_id, 1_000, &removal(project_id, 3))
            .unwrap_err(),
        HistoryError::MountsActive(_)
    ));
    assert!(matches!(
        history
            .gc_remove_path(&path_request(project_id), &removal(project_id, 4))
            .unwrap_err(),
        HistoryError::MountsActive(_)
    ));

    history.gc_release(&op(1)).unwrap();
    let outcome = history
        .gc_remove_path(&path_request(project_id), &removal(project_id, 4))
        .unwrap();
    assert_eq!(outcome.kind, PathRemovalKind::Removed);
    assert_eq!(epoch(&path, project_id), 1);
    history
        .gc_remove_branch(branch, 2_000, &removal(project_id, 2))
        .unwrap();
    assert_eq!(epoch(&path, project_id), 2);
    assert!(matches!(
        history
            .gc_delete_stack(project_id, 3_000, &removal(project_id, 3))
            .unwrap(),
        GcDeletion::Marked { deleted_ms: 3_000 }
    ));
    assert_eq!(epoch(&path, project_id), 3);
}

#[test]
fn init_and_merge_are_project_holders() {
    let temp = Temp::new("admission-shared");
    let history = catalog(&temp);
    let (project_id, _branch) = project(&history);

    history.gc_register(&init(project_id, 5)).unwrap();
    let refusal = history
        .gc_delete_stack(project_id, 1_000, &removal(project_id, 6))
        .unwrap_err();
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holders[0].kind, GcIntentKind::Init);
    history.gc_release(&op(5)).unwrap();

    history.gc_register(&merge(project_id, 7)).unwrap();
    let refusal = history
        .gc_remove_path(&path_request(project_id), &removal(project_id, 8))
        .unwrap_err();
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holders[0].kind, GcIntentKind::Merge);
    history.gc_release(&op(7)).unwrap();

    history
        .gc_remove_path(&path_request(project_id), &removal(project_id, 8))
        .unwrap();
}

#[test]
fn a_live_operation_is_busy_and_an_uncertain_owner_is_unresolved() {
    let temp = Temp::new("admission-op");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    let catalog = CatalogId::derive(BINDING).unwrap();
    let compaction = GcIntent::operation(
        op(9),
        GcIntentKind::Compaction,
        GcScopeKind::Store,
        catalog.as_slice(),
    )
    .owned(Some("runtime-a"), Some("boot-9"))
    .with_phase(Some("mark"));
    history.gc_register(&compaction).unwrap();

    let refusal = history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 10))
        .unwrap_err();
    let HistoryError::BusyOp(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.kind, GcIntentKind::Compaction);
    assert_eq!(state.entity, "store");
    assert_eq!(state.phase.as_deref(), Some("mark"));
    assert_eq!(state.op, hex16(&op(9)));

    // A mount is refused while the operation is live.
    assert!(matches!(
        history
            .gc_register(&mount(project_id, branch, 11))
            .unwrap_err(),
        HistoryError::BusyOp(_)
    ));

    history.gc_mark_uncertain(&op(9)).unwrap();
    let refusal = history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 10))
        .unwrap_err();
    assert!(matches!(refusal, HistoryError::Unresolved(_)));

    history.gc_release(&op(9)).unwrap();
    history
        .gc_remove_branch(branch, 2_000, &removal(project_id, 10))
        .unwrap();
}

#[test]
fn a_concurrent_mount_and_branch_remove_have_one_winner() {
    let temp = Temp::new("admission-race");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    drop(history);

    let left = writable(&path);
    let right = writable(&path);
    let (mounted, removed) = std::thread::scope(|scope| {
        let mounted = scope.spawn(move || left.gc_register(&mount(project_id, branch, 12)));
        let removed =
            scope.spawn(move || right.gc_remove_branch(branch, 2_000, &removal(project_id, 13)));
        (mounted.join().unwrap(), removed.join().unwrap())
    });
    let wins = [&mounted, &removed]
        .iter()
        .filter(|result| result.is_ok())
        .count();
    assert_eq!(wins, 1, "{mounted:?} / {removed:?}");
    if let Err(error) = &removed {
        assert!(matches!(error, HistoryError::MountsActive(_)), "{error:?}");
    }
    if let Err(error) = &mounted {
        assert!(
            matches!(error, HistoryError::Missing(Missing::Branch)),
            "{error:?}"
        );
    }
}

#[test]
fn a_missing_epoch_row_is_conflict_and_release_is_idempotent() {
    let temp = Temp::new("admission-epoch");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);

    let mount = mount(project_id, branch, 14);
    history.gc_register(&mount).unwrap();
    let listed = history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].kind, GcIntentKind::Mount);
    history.gc_release(&op(14)).unwrap();
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
    history.gc_release(&op(14)).unwrap();

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "DELETE FROM project_epoch WHERE layer_stack_id = ?1",
            [project_id.as_slice()],
        )
        .unwrap();
    drop(connection);
    assert!(matches!(
        history
            .gc_remove_branch(branch, 3_000, &removal(project_id, 15))
            .unwrap_err(),
        HistoryError::Conflict("project epoch")
    ));
    assert!(matches!(
        history
            .gc_delete_stack(project_id, 3_000, &removal(project_id, 16))
            .unwrap_err(),
        HistoryError::Conflict("project epoch")
    ));
}

fn hex16(bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
