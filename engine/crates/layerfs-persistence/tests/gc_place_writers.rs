//! Lean phase 3 place classification on SQLite: a worker start records an
//! ordered incarnation and a Sandbox create an ordered generation; the
//! reconcile compares a recorded intent's place with the start's value.
//! Only an older recorded value is dead. A newer recorded value (a restore)
//! and an unordered pre-release value that differs are uncertain, never dead;
//! an uncertain owner, an expired lease and another machine's rows stay.
mod support;

use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{
    BranchId, HistoryCatalog, HistoryCatalogConfig, LayerStackId, PlaceAuthority,
};
use layerfs_persistence::HistoryProvider;
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"gc-place-writers";
const CURSOR: [u8; 32] = [63; 32];

fn catalog(temp: &Temp) -> HistoryProvider {
    support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: CURSOR,
            binding_key: BINDING.to_vec(),
            incarnation: 5,
        },
    )
    .expect("catalog")
}

/// Project `proj` with one Branch `work`, as `admission.rs` sets one up.
fn project(history: &dyn HistoryCatalog) -> (LayerStackId, BranchId) {
    history
        .initialize_layerstack(&layerfs_history::records::StackInitialization {
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

/// An ordered place value in the format the records mint.
fn token(label: &str, milliseconds: u64, nonce: &str) -> String {
    format!("p1:{label}:{milliseconds:016}:{nonce}")
}

fn incarnation(milliseconds: u64, nonce: &str) -> String {
    token("worker", milliseconds, nonce)
}

fn generation(milliseconds: u64, nonce: &str) -> String {
    token("sandbox", milliseconds, nonce)
}

/// A mount of `sandbox` on `machine`, carrying `authority`. Its owner tokens
/// parse as no live process, so the place alone decides it.
fn mount(
    branch: BranchId,
    body: u8,
    sandbox: &str,
    machine: &str,
    authority: PlaceAuthority,
) -> GcIntent {
    GcIntent::shared(
        op(body),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([body; 32]), Some(sandbox), Some(machine))
    .owned(Some("runtime-private"), Some("boot-private"))
    .authorized(authority)
}

fn authority(incarnation: &str) -> PlaceAuthority {
    PlaceAuthority {
        machine_incarnation: Some(incarnation.to_owned()),
        ..PlaceAuthority::default()
    }
}

#[test]
fn an_older_recorded_incarnation_is_settled_once() {
    let temp = Temp::new("gc-place-writers-settle");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    history
        .gc_register(&mount(
            branch,
            1,
            "sandbox-1",
            "machine-1",
            authority(&incarnation(10, "a")),
        ))
        .unwrap();

    let reconcile = history
        .gc_reconcile_machine("machine-1", &incarnation(20, "b"))
        .unwrap();
    assert_eq!(reconcile.machine, "machine-1");
    assert_eq!(reconcile.incarnation, incarnation(20, "b"));
    assert_eq!(reconcile.settled, vec![op(1)]);
    assert_eq!((reconcile.held, reconcile.uncertain), (0, 0));
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());

    // the row is gone: a second reconcile settles nothing
    let again = history
        .gc_reconcile_machine("machine-1", &incarnation(20, "b"))
        .unwrap();
    assert!(again.settled.is_empty());
}

#[test]
fn the_current_incarnation_keeps_a_live_mount() {
    let temp = Temp::new("gc-place-writers-live");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    history
        .gc_register(&mount(
            branch,
            2,
            "sandbox-2",
            "machine-2",
            authority(&incarnation(30, "a")),
        ))
        .unwrap();

    let reconcile = history
        .gc_reconcile_machine("machine-2", &incarnation(30, "a"))
        .unwrap();
    assert!(reconcile.settled.is_empty());
    assert_eq!((reconcile.held, reconcile.uncertain), (1, 0));
    assert_eq!(
        history
            .gc_intents(GcScopeKind::Branch, branch.as_slice())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn an_uncertain_owner_is_never_settled() {
    let temp = Temp::new("gc-place-writers-uncertain");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let intent = mount(
        branch,
        3,
        "sandbox-3",
        "machine-3",
        authority(&incarnation(10, "a")),
    );
    history.gc_register(&intent).unwrap();
    history.gc_mark_uncertain(&intent.op_id).unwrap();

    let reconcile = history
        .gc_reconcile_machine("machine-3", &incarnation(20, "a"))
        .unwrap();
    assert!(
        reconcile.settled.is_empty(),
        "an uncertain owner is not auto-released"
    );
    assert_eq!((reconcile.held, reconcile.uncertain), (1, 1));
}

#[test]
fn an_expired_lease_is_kept_as_uncertain() {
    let temp = Temp::new("gc-place-writers-lease");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let mut lease = authority(&incarnation(40, "a"));
    lease.lease_expires_ms = Some(1);
    history
        .gc_register(&mount(branch, 4, "sandbox-4", "machine-4", lease))
        .unwrap();

    // the place is current, so only the lease speaks: it is uncertain
    let reconcile = history
        .gc_reconcile_machine("machine-4", &incarnation(40, "a"))
        .unwrap();
    assert!(reconcile.settled.is_empty());
    assert_eq!((reconcile.held, reconcile.uncertain), (1, 1));
}

#[test]
fn an_older_current_incarnation_is_never_dead() {
    let temp = Temp::new("gc-place-writers-restore");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    history
        .gc_register(&mount(
            branch,
            5,
            "sandbox-5",
            "machine-5",
            authority(&incarnation(20, "a")),
        ))
        .unwrap();

    // a restored home brings an older current record back
    let reconcile = history
        .gc_reconcile_machine("machine-5", &incarnation(10, "a"))
        .unwrap();
    assert!(reconcile.settled.is_empty());
    assert_eq!((reconcile.held, reconcile.uncertain), (1, 1));
}

#[test]
fn an_unordered_value_that_differs_is_never_dead() {
    let temp = Temp::new("gc-place-writers-unordered");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    history
        .gc_register(&mount(
            branch,
            6,
            "sandbox-6",
            "machine-6",
            authority("worker:1"),
        ))
        .unwrap();

    let reconcile = history
        .gc_reconcile_machine("machine-6", "worker:2")
        .unwrap();
    assert!(reconcile.settled.is_empty());
    assert_eq!((reconcile.held, reconcile.uncertain), (1, 1));
}

#[test]
fn a_newer_sandbox_generation_settles_the_old_mount() {
    let temp = Temp::new("gc-place-writers-tombstone");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let mut older = authority(&incarnation(10, "a"));
    older.sandbox_generation = Some(generation(10, "a"));
    history
        .gc_register(&mount(branch, 7, "sandbox-7", "machine-7", older))
        .unwrap();

    // a recreated Sandbox mints a newer generation; admitting its mount
    // settles the old generation's holder
    let mut newer = authority(&incarnation(10, "a"));
    newer.sandbox_generation = Some(generation(20, "b"));
    history
        .gc_register(&mount(branch, 71, "sandbox-7", "machine-7", newer))
        .unwrap();

    let left = history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].op_id, op(71));
}

#[test]
fn another_machines_rows_are_not_read() {
    let temp = Temp::new("gc-place-writers-other");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    history
        .gc_register(&mount(
            branch,
            8,
            "sandbox-8",
            "machine-8",
            authority(&incarnation(10, "a")),
        ))
        .unwrap();

    let reconcile = history
        .gc_reconcile_machine("machine-9", &incarnation(20, "a"))
        .unwrap();
    assert!(reconcile.settled.is_empty());
    assert_eq!(reconcile.held, 0);
    assert_eq!(
        history
            .gc_intents(GcScopeKind::Branch, branch.as_slice())
            .unwrap()
            .len(),
        1
    );
}
