//! #41 T6: admission (`gc_intent`) on SQLite. A mount/Init/merge holds its
//! scopes, a removal sees `MountsActive`/`Busy`/`Unresolved`, the epoch is
//! bumped once by the marker and a missing epoch row is `Conflict`.
mod support;

use layerfs_history::error::{HistoryError, Missing};
use layerfs_history::records::{
    GcDeletion, GcIntent, GcIntentKind, GcScopeKind, PathRemovalKind, PathRemovalRequest,
    StackInitialization,
};
use layerfs_history::{
    BranchId, CatalogId, HistoryCatalog, HistoryCatalogConfig, LayerStackId, PlaceAuthority,
    PlaceView,
};
use layerfs_persistence::{
    with_place_view, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"gc-admission";
const CURSOR: [u8; 32] = [47; 32];

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
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
    .with_phase(Some("init"))
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
    .with_phase(Some("merge"))
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
    let HistoryError::BusyOp(state) = &refusal else {
        panic!("an Init is a Busy operation, not a mount: {refusal:?}");
    };
    assert_eq!(state.kind, GcIntentKind::Init);
    assert_eq!(state.phase.as_deref(), Some("init"));
    history.gc_release(&op(5)).unwrap();

    history.gc_register(&merge(project_id, 7)).unwrap();
    let refusal = history
        .gc_remove_path(&path_request(project_id), &removal(project_id, 8))
        .unwrap_err();
    let HistoryError::BusyOp(state) = &refusal else {
        panic!("a merge is a Busy operation, not a mount: {refusal:?}");
    };
    assert_eq!(state.kind, GcIntentKind::Merge);
    assert_eq!(state.phase.as_deref(), Some("merge"));
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

#[test]
fn an_intent_round_trips_its_place_authority() {
    let temp = Temp::new("admission-intent-authority");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);

    let authority = PlaceAuthority {
        machine_incarnation: Some("worker:7".to_owned()),
        sandbox_generation: Some("sandbox:3".to_owned()),
    };
    let intent = mount(project_id, branch, 30).authorized(authority.clone());
    assert_eq!(intent.machine_incarnation.as_deref(), Some("worker:7"));
    assert_eq!(intent.sandbox_generation.as_deref(), Some("sandbox:3"));

    history.gc_register(&intent).unwrap();
    let listed = history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].machine_incarnation, authority.machine_incarnation);
    assert_eq!(listed[0].sandbox_generation, authority.sandbox_generation);

    // The registration writes the two place columns, not just the memory value.
    let connection = rusqlite::Connection::open(&path).unwrap();
    let row: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT machine_incarnation, sandbox_generation FROM gc_intent WHERE op_id = ?1",
            [op(30).as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(row.0.as_deref(), Some("worker:7"));
    assert_eq!(row.1.as_deref(), Some("sandbox:3"));
    history.gc_release(&op(30)).unwrap();
}

/// A mount whose owner is provably dead (the same boot and PID namespace, a
/// pid that does not run) no longer blocks a removal: the removal adopts the
/// row, while a live owner still reports a holder.
/// A `pid`/start/namespace token no process runs under: pid `u32::MAX` is not
/// assigned on this boot, and the namespace is this process's, so
/// `OwnerView::dead` can prove the owner gone.
#[cfg(target_os = "linux")]
fn dead_runtime() -> String {
    format!("pid:4294967295@0@ns:{}", this_namespace())
}

/// This boot's id with a unique suffix for one operation.
#[cfg(target_os = "linux")]
fn dead_boot(tag: u8) -> String {
    format!("boot:{:032x}:ws:{tag}", this_boot())
}

#[cfg(target_os = "linux")]
fn dead_mount(stack: LayerStackId, branch: BranchId, tag: u8) -> GcIntent {
    GcIntent::shared(
        op(tag),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([tag; 32]), Some("sandbox"), Some("machine"))
    .owned(Some(&dead_runtime()), Some(&dead_boot(tag)))
}

#[cfg(target_os = "linux")]
#[test]
fn a_dead_mount_owner_is_settled_and_does_not_hold_the_scope() {
    let temp = Temp::new("admission-dead-owner");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    // The recorded owner does not run: the removal adopts its row instead of
    // reporting `MountsActive` forever after a `machine.stop`/SIGKILL.
    history
        .gc_register(&dead_mount(project_id, branch, 23))
        .unwrap();
    let live = mount(project_id, branch, 24);
    history.gc_register(&live).unwrap();
    let refusal = history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 25))
        .expect_err("a live owner holds");
    assert!(
        matches!(refusal, HistoryError::MountsActive(_)),
        "{refusal:?}"
    );
    history.gc_release(&op(24)).unwrap();
    history
        .gc_remove_branch(branch, 2_000, &removal(project_id, 26))
        .unwrap();
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
}

/// Place evidence only adds proofs of death: an owner the namespace/boot
/// evidence proves gone is dead even when its recorded place is still
/// current, exactly as admission decided before place evidence existed.
#[cfg(target_os = "linux")]
#[test]
fn a_dead_owner_with_a_current_place_is_still_settled() {
    let temp = Temp::new("admission-dead-owner-place");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    let incarnation = token("worker", 10, "a");
    history
        .gc_register(
            &dead_mount(project_id, branch, 90).authorized(PlaceAuthority {
                machine_incarnation: Some(incarnation.clone()),
                sandbox_generation: None,
            }),
        )
        .unwrap();
    // The machine's current incarnation equals the recorded one: the place
    // alone would keep the row, but its owner is provably gone.
    let mut view = PlaceView::new();
    view.set_machine("machine", incarnation);
    with_place_view(Some(view), || {
        history.gc_remove_branch(branch, 1_000, &removal(project_id, 91))
    })
    .expect("the dead owner does not hold the Branch");
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
}

/// This boot's id, as `current_boot` reads it.
#[cfg(target_os = "linux")]
fn this_boot() -> u128 {
    let text = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
    u128::from_str_radix(text.trim().replace('-', "").as_str(), 16).unwrap()
}

/// This process's PID namespace inode.
#[cfg(target_os = "linux")]
fn this_namespace() -> u64 {
    let target = std::fs::read_link("/proc/self/ns/pid").unwrap();
    target
        .to_str()
        .unwrap()
        .strip_prefix("pid:[")
        .unwrap()
        .strip_suffix(']')
        .unwrap()
        .parse()
        .unwrap()
}

/// A mount whose recorded owner ran in `namespace` on `boot`.
#[cfg(target_os = "linux")]
fn placed_mount(
    stack: LayerStackId,
    branch: BranchId,
    tag: u8,
    namespace: u64,
    boot: u128,
) -> GcIntent {
    GcIntent::shared(
        op(tag),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([tag; 32]), Some("sandbox"), Some("machine"))
    .owned(
        Some(&format!("pid:1@1@ns:{namespace}")),
        Some(&format!("boot:{boot:032x}:test:{tag}")),
    )
}

/// A same-boot owner in another PID namespace cannot be probed soundly from
/// this one: `/proc` may be a view of one namespace, and another user's
/// processes can be unreadable. It therefore stays explicitly and boundedly
/// live — reported as a holder naming its Sandbox and machine, never adopted
/// — and the settlement path is the owner's release, or an operator's
/// `gc_mark_uncertain` followed by `gc_release`.
#[cfg(target_os = "linux")]
#[test]
fn a_cross_namespace_owner_stays_conservatively_live() {
    let temp = Temp::new("admission-cross-namespace");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    // On this boot, in a namespace this process cannot observe.
    history
        .gc_register(&placed_mount(
            project_id,
            branch,
            31,
            4_000_000_000,
            this_boot(),
        ))
        .unwrap();
    let refusal = history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 32))
        .expect_err("an unobservable owner holds");
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holders.len(), 1);
    assert_eq!(state.holders[0].sandbox.as_deref(), Some("sandbox"));
    assert_eq!(state.holders[0].machine.as_deref(), Some("machine"));

    // The owning call is gone; the operator settles the row, and only then
    // does the Branch go.
    history.gc_mark_uncertain(&op(31)).unwrap();
    assert!(matches!(
        history.gc_remove_branch(branch, 2_000, &removal(project_id, 33)),
        Err(HistoryError::Unresolved(_))
    ));
    history.gc_release(&op(31)).unwrap();
    history
        .gc_remove_branch(branch, 3_000, &removal(project_id, 34))
        .unwrap();
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
}

/// A SQLite Store is admitted only by processes on the machine that holds the
/// file, so an owner recorded on an earlier boot cannot still be running: its
/// intent is adopted rather than reported as a live holder forever.
#[cfg(target_os = "linux")]
#[test]
fn an_earlier_boot_is_settled_on_a_local_store() {
    let temp = Temp::new("admission-earlier-boot");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    let earlier = this_boot().wrapping_add(1);
    history
        .gc_register(&placed_mount(
            project_id,
            branch,
            33,
            this_namespace(),
            earlier,
        ))
        .unwrap();
    history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 34))
        .unwrap();
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
}

/// A provably dead owner does not let an observer delete an uncertain row: the
/// row stays `Unresolved` until the owning call (or an operator) releases it.
#[cfg(target_os = "linux")]
#[test]
fn a_dead_uncertain_owner_stays_unresolved_until_released() {
    let temp = Temp::new("admission-dead-uncertain");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    history
        .gc_register(&dead_mount(project_id, branch, 27))
        .expect("admit the dead mount");
    history.gc_mark_uncertain(&op(27)).expect("mark uncertain");

    // The owner is provably dead, but the row is uncertain: settling it would
    // erase the evidence that its operation may not have finished.
    let refusal = history
        .gc_remove_branch(branch, 1_000, &removal(project_id, 28))
        .expect_err("an uncertain dead owner is Unresolved, not adopted");
    let HistoryError::Unresolved(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.op, hex16(&op(27)));
    let listed = history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .expect("intents");
    assert_eq!(listed.len(), 1);
    assert!(
        listed[0].uncertain,
        "the removed call leaves the row uncertain"
    );

    // The owning call settles it; only then does the Branch go.
    history.gc_release(&op(27)).expect("settle");
    history
        .gc_remove_branch(branch, 2_000, &removal(project_id, 29))
        .expect("removal after settlement");
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .expect("intents")
        .is_empty());
}

/// A retry reuses the operation id with fresh owner evidence. The stored row
/// must carry the retry's evidence: otherwise a concurrent GC call would
/// settle the retried, live operation as dead instead of reporting `Busy`.
#[cfg(target_os = "linux")]
#[test]
fn a_retried_live_op_replaces_its_dead_predecessors_evidence() {
    let temp = Temp::new("admission-retry");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (project_id, _branch) = project(&history);

    // The predecessor's process died while holding this operation identity.
    let dead = init(project_id, 35).owned(Some(&dead_runtime()), Some(&dead_boot(35)));
    history.gc_register(&dead).expect("admit the dead op");

    // The same operation identity retries from this live process.
    let live = init(project_id, 35);
    history.gc_register(&live).expect("retry the op");

    // A concurrent removal sees the retried owner, not the dead predecessor:
    // the row holds the project scope and is `Busy`, never adopted.
    let refusal = history
        .gc_delete_stack(project_id, 1_000, &removal(project_id, 36))
        .expect_err("a live retry holds the scope");
    let HistoryError::BusyOp(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.kind, GcIntentKind::Init);
    assert_eq!(state.entity, "proj");
    assert_eq!(state.op, hex16(&op(35)));

    // The stored row itself carries the retry's ownership evidence.
    let connection = rusqlite::Connection::open(&path).unwrap();
    let (runtime, boot): (String, String) = connection
        .query_row(
            "SELECT owner_runtime, owner_boot FROM gc_intent WHERE op_id = ?1",
            [op(35).as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(Some(runtime.as_str()), live.owner_runtime.as_deref());
    assert_eq!(Some(boot.as_str()), live.owner_boot.as_deref());
    history.gc_release(&op(35)).expect("release");
}

/// An ordered place value in the format the records mint, minted by home
/// `aa11`.
fn token(label: &str, milliseconds: u64, nonce: &str) -> String {
    format!("p1:aa11.{label}:{milliseconds:016}:{nonce}")
}

/// A mount placed at `machine`/`sandbox` carrying `authority`.
fn authority_mount(
    stack: LayerStackId,
    branch: BranchId,
    tag: u8,
    machine: &str,
    sandbox: &str,
    authority: PlaceAuthority,
) -> GcIntent {
    GcIntent::shared(
        op(tag),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([tag; 32]), Some(sandbox), Some(machine))
    .owned(Some("runtime-a"), Some(&format!("boot-{tag}")))
    .authorized(authority)
}

/// A newer machine incarnation proves the older placement dead while its
/// owner evidence is inconclusive: a removal's admission given the machine's
/// current incarnation settles the older row, and the newer place still holds
/// the Branch. Admitting the newer mount itself settles nothing, as before
/// place evidence.
#[test]
fn a_newer_machine_incarnation_settles_the_older_mount_at_removal() {
    let temp = Temp::new("admission-machine-incarnation");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    history
        .gc_register(&authority_mount(
            project_id,
            branch,
            80,
            "machine-80",
            "sandbox-80",
            PlaceAuthority {
                machine_incarnation: Some(token("worker", 10, "a")),
                sandbox_generation: None,
            },
        ))
        .expect("admit the stale mount");
    history
        .gc_register(&authority_mount(
            project_id,
            branch,
            81,
            "machine-80",
            "sandbox-81",
            PlaceAuthority {
                machine_incarnation: Some(token("worker", 20, "b")),
                sandbox_generation: None,
            },
        ))
        .expect("admit the current mount");

    // The registration scans only conflicting operations: both rows stay.
    let listed = history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .expect("intents");
    assert_eq!(listed.len(), 2);

    let view = || {
        let mut view = PlaceView::new();
        view.set_machine("machine-80", token("worker", 20, "b"));
        Some(view)
    };
    let refusal = with_place_view(view(), || {
        history.gc_remove_branch(branch, 1_000, &removal(project_id, 82))
    })
    .expect_err("the current incarnation holds the Branch");
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holders.len(), 1);
    assert_eq!(state.holders[0].op, hex16(&op(81)));
    history
        .gc_release(&op(81))
        .expect("release the current place");
    with_place_view(view(), || {
        history.gc_remove_branch(branch, 2_000, &removal(project_id, 83))
    })
    .expect("the stale place no longer holds");
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .expect("intents")
        .is_empty());
}

/// A newer Sandbox generation proves the older generation dead: a recreated
/// Sandbox cannot revive the old intent. A removal's admission given the
/// Sandbox's current generation settles it; the newer generation still holds
/// the Branch.
#[test]
fn a_newer_sandbox_generation_settles_the_older_mount_at_removal() {
    let temp = Temp::new("admission-sandbox-generation");
    let history = catalog(&temp);
    let (project_id, branch) = project(&history);
    history
        .gc_register(&authority_mount(
            project_id,
            branch,
            84,
            "machine-84",
            "sandbox-84",
            PlaceAuthority {
                machine_incarnation: None,
                sandbox_generation: Some(token("sandbox", 10, "a")),
            },
        ))
        .expect("admit the older generation");
    history
        .gc_register(&authority_mount(
            project_id,
            branch,
            85,
            "machine-84",
            "sandbox-84",
            PlaceAuthority {
                machine_incarnation: None,
                sandbox_generation: Some(token("sandbox", 20, "b")),
            },
        ))
        .expect("admit the newer generation");

    let listed = history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .expect("intents");
    assert_eq!(listed.len(), 2);

    let view = || {
        let mut view = PlaceView::new();
        view.set_sandbox_generation("machine-84", "sandbox-84", token("sandbox", 20, "b"));
        Some(view)
    };
    let refusal = with_place_view(view(), || {
        history.gc_remove_branch(branch, 1_000, &removal(project_id, 86))
    })
    .expect_err("the live generation holds the Branch");
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holders.len(), 1);
    assert_eq!(state.holders[0].op, hex16(&op(85)));
    history
        .gc_release(&op(85))
        .expect("release the live Sandbox");
    with_place_view(view(), || {
        history.gc_remove_branch(branch, 2_000, &removal(project_id, 87))
    })
    .expect("the retired generation no longer holds");
}

/// An operation id as the holder report prints it.
fn hex16(bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
