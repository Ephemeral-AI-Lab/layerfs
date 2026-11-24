//! The bounded `PlaceView` the control layer builds: a retired generation in
//! the view settles a conflicting mount whose machine incarnation is still
//! current; a missing or restored place stays conservative live; the view is
//! scoped to one admission and never leaks.
mod support;

use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{
    BranchId, HistoryCatalog, HistoryCatalogConfig, LayerStackId, PlaceAuthority, PlaceView,
};
use layerfs_persistence::{with_place_view, HistoryProvider};
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"gc-place-view";
const CURSOR: [u8; 32] = [64; 32];

fn catalog(temp: &Temp) -> HistoryProvider {
    support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: CURSOR,
            binding_key: BINDING.to_vec(),
            incarnation: 6,
        },
    )
    .expect("catalog")
}

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

fn token(label: &str, milliseconds: u64, nonce: &str) -> String {
    format!("p1:{label}:{milliseconds:016}:{nonce}")
}

fn mount(branch: BranchId, body: u8, generation: &str) -> GcIntent {
    GcIntent::shared(
        op(body),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([body; 32]), Some("box-1"), Some("box"))
    .owned(Some("runtime-private"), Some("boot-private"))
    .authorized(PlaceAuthority {
        machine_incarnation: Some(token("worker", 20, "a")),
        sandbox_generation: Some(generation.to_owned()),
        ..PlaceAuthority::default()
    })
}

fn view(retired: Option<&str>, current: Option<&str>) -> PlaceView {
    let mut view = PlaceView::new();
    view.set_machine("box", token("worker", 20, "a"));
    if let Some(current) = current {
        view.set_sandbox_generation("box", "box-1", current);
    }
    if let Some(retired) = retired {
        view.set_sandbox_retired("box", "box-1", retired);
    }
    view
}

#[test]
fn a_supplied_tombstone_settles_a_retired_sandbox_mount() {
    let temp = Temp::new("gc-place-view-tombstone");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let old = token("sandbox", 10, "a");
    history.gc_register(&mount(branch, 1, &old)).unwrap();

    // Without the view the current machine incarnation keeps the mount.
    let held = history
        .gc_reconcile_machine("box", &token("worker", 20, "a"))
        .unwrap();
    assert!(held.settled.is_empty());
    assert_eq!((held.held, held.uncertain), (1, 0));

    // Control read the machine tombstone: the retired generation is dead.
    let settled = with_place_view(Some(view(Some(&old), None)), || {
        history.gc_reconcile_machine("box", &token("worker", 20, "a"))
    })
    .unwrap();
    assert_eq!(settled.settled, vec![op(1)]);
}

#[test]
fn a_recreated_sandbox_keeps_its_current_mount() {
    let temp = Temp::new("gc-place-view-reuse");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let current = token("sandbox", 20, "b");
    history.gc_register(&mount(branch, 2, &current)).unwrap();

    // The name was recreated: a newer current generation and the old
    // tombstone are both in the view. The current mount is live.
    let old = token("sandbox", 10, "a");
    let held = with_place_view(Some(view(Some(&old), Some(&current))), || {
        history.gc_reconcile_machine("box", &token("worker", 20, "a"))
    })
    .unwrap();
    assert!(held.settled.is_empty());
    assert_eq!((held.held, held.uncertain), (1, 0));
}

#[test]
fn a_missing_place_is_conservative_live() {
    let temp = Temp::new("gc-place-view-missing");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    history
        .gc_register(&mount(branch, 3, &token("sandbox", 10, "a")))
        .unwrap();

    let held = with_place_view(Some(PlaceView::new()), || {
        history.gc_reconcile_machine("box", &token("worker", 20, "a"))
    })
    .unwrap();
    assert!(held.settled.is_empty());
    assert_eq!((held.held, held.uncertain), (1, 0));
}

#[test]
fn a_restored_older_place_is_uncertain_never_dead() {
    let temp = Temp::new("gc-place-view-restore");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    history
        .gc_register(&mount(branch, 4, &token("sandbox", 10, "a")))
        .unwrap();

    // A restored home brings an older machine record back; the intent's
    // recorded incarnation is newer, so it is uncertain, never dead.
    let mut restored = PlaceView::new();
    restored.set_machine("box", token("worker", 10, "a"));
    let held = with_place_view(Some(restored), || {
        history.gc_reconcile_machine("box", &token("worker", 10, "a"))
    })
    .unwrap();
    assert!(held.settled.is_empty());
    assert_eq!((held.held, held.uncertain), (1, 1));
}

#[test]
fn the_view_ends_with_its_admission() {
    let temp = Temp::new("gc-place-view-scope");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let old = token("sandbox", 10, "a");
    history.gc_register(&mount(branch, 5, &old)).unwrap();

    let settled = with_place_view(Some(view(Some(&old), None)), || {
        history.gc_reconcile_machine("box", &token("worker", 20, "a"))
    })
    .unwrap();
    assert_eq!(settled.settled, vec![op(5)]);

    // After the scoped call the view is gone; a later admission without it
    // cannot settle anything it did not see.
    let again = history
        .gc_reconcile_machine("box", &token("worker", 20, "a"))
        .unwrap();
    assert!(again.settled.is_empty());
}
