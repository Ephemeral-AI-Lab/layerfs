//! The bounded retained-`done` audit read: oldest first, page/limit, and
//! invisible to admission. The live queries keep filtering `state <> 'done'`.
mod support;

use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{
    BranchId, HistoryCatalog, HistoryCatalogConfig, LayerStackId, PlaceAuthority,
};
use layerfs_persistence::HistoryProvider;
use rusqlite::params;
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"gc-done-audit";
const CURSOR: [u8; 32] = [65; 32];

fn catalog(temp: &Temp) -> HistoryProvider {
    support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: CURSOR,
            binding_key: BINDING.to_vec(),
            incarnation: 7,
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
            scope: root(30),
            profile: root(31),
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

/// Inserts one retained done row, as an operator release leaves it: owner
/// columns cleared so the unique owner key can never block a remount.
fn done_row(path: &std::path::Path, branch: BranchId, body: u8, evidence: &str, updated_ms: i64) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute(
            "INSERT INTO gc_intent (
                op_id, kind, scope_kind, scope_id, workspace, sandbox, machine,
                project, branch, owner_runtime, owner_boot, state, uncertain,
                phase, machine_incarnation, sandbox_generation, lease_expires_ms,
                fence_token, evidence, updated_ms
             ) VALUES (?1, 'mount', 'branch', ?2, NULL, 'box', 'box',
                'proj', 'work', NULL, NULL, 'done', 0, 'release', NULL, NULL,
                NULL, NULL, ?3, ?4)",
            params![op(body).as_slice(), branch.as_slice(), evidence, updated_ms],
        )
        .unwrap();
}

#[test]
fn the_done_audit_reads_oldest_first_in_bounded_pages() {
    let temp = Temp::new("gc-done-audit-page");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let path = temp.join("store.sqlite");
    done_row(&path, branch, 3, "release three", 30);
    done_row(&path, branch, 1, "release one", 10);
    done_row(&path, branch, 2, "release two", 20);

    let first = history.gc_done_intents(0, 2).unwrap();
    assert_eq!(first.page, 0);
    assert_eq!(first.limit, 2);
    assert!(first.has_more);
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.items[0].op_id, op(1));
    assert_eq!(first.items[0].evidence.as_deref(), Some("release one"));
    assert_eq!(first.items[1].op_id, op(2));

    let second = history.gc_done_intents(1, 2).unwrap();
    assert!(!second.has_more);
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].op_id, op(3));
    assert_eq!(second.items[0].updated_ms, 30);

    // The limit is bounded, not the caller's number.
    let clamped = history.gc_done_intents(0, u32::MAX).unwrap();
    assert_eq!(clamped.limit, layerfs_history::MAXIMUM_AUDIT_PAGE);
}

/// The operator release is the writer of the retained row: the owning mount
/// keeps its op, its place and its phase, loses its owner columns, and gains
/// the action/operator/op evidence. A second release finds no live row and
/// invents no second audit row.
#[test]
fn release_retains_the_operator_audit_row_and_clears_the_owner() {
    let temp = Temp::new("gc-done-audit-release");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let mounted = GcIntent::shared(
        op(20),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([20; 32]), Some("box"), Some("box"))
    .owned(Some("pid:1@0"), Some("boot:1"))
    .with_phase(Some("bind"))
    .authorized(PlaceAuthority {
        machine_incarnation: Some("worker:1".to_owned()),
        sandbox_generation: Some("generation:1".to_owned()),
        lease_expires_ms: Some(1_000),
        fence_token: Some("fence:1".to_owned()),
    });
    history.gc_register(&mounted).unwrap();
    history.gc_mark_uncertain(&op(20)).unwrap();

    let evidence = "action=release;operator=alice;op=14141414141414141414141414141414";
    assert!(history.gc_release_done(&op(20), evidence).unwrap());
    // The row is gone from the live read and from a second release.
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
    assert!(!history
        .gc_release_done(&op(20), evidence)
        .expect("an absent row is not an error"));

    let page = history.gc_done_intents(0, 10).unwrap();
    assert_eq!(page.items.len(), 1, "{page:?}");
    let row = &page.items[0];
    assert_eq!(row.op_id, op(20));
    assert_eq!(row.kind, GcIntentKind::Mount);
    assert_eq!(row.scope_kind, GcScopeKind::Branch);
    assert_eq!(row.scope_id.as_slice(), branch.as_slice());
    assert_eq!(row.project.as_deref(), Some("proj"));
    assert_eq!(row.branch.as_deref(), Some("work"));
    assert_eq!(row.machine.as_deref(), Some("box"));
    assert_eq!(row.sandbox.as_deref(), Some("box"));
    assert_eq!(row.phase.as_deref(), Some("bind"));
    assert_eq!(row.evidence.as_deref(), Some(evidence));
    assert!(row.updated_ms > 0);

    // The cleared owner key no longer blocks the same owner and scope.
    let remounted = GcIntent::shared(
        op(21),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([21; 32]), Some("box"), Some("box"))
    .owned(Some("pid:1@0"), Some("boot:1"));
    history
        .gc_register(&remounted)
        .expect("the retained row's cleared owner key does not hold");
}

#[test]
fn a_done_row_is_never_a_live_holder() {
    let temp = Temp::new("gc-done-audit-hidden");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let path = temp.join("store.sqlite");
    done_row(&path, branch, 9, "release nine", 10);

    // The live read on the scope does not report it.
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());

    // A later mount on the Branch is admitted: the done row's cleared owner
    // key cannot block it, and admission filters done.
    let mount = GcIntent::shared(
        op(10),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([10; 32]), Some("box"), Some("box"))
    .owned(Some("runtime-private"), Some("boot-private"))
    .authorized(PlaceAuthority::default());
    history
        .gc_register(&mount)
        .expect("the done row does not hold");
}

/// The durable preview: the same retained-row columns as a release, with
/// `action=preview`, the operator label and no operation. It is invisible to
/// the live admission read and does not block a later mount.
#[test]
fn preview_retains_a_done_row_without_an_operation() {
    let temp = Temp::new("gc-done-audit-preview");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let preview = GcIntent::shared(
        op(30),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .named(Some("proj"), Some("work"));
    assert!(history
        .gc_record_done(&preview, "action=preview;operator=alice")
        .unwrap());
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());

    let page = history.gc_done_intents(0, 10).unwrap();
    assert_eq!(page.items.len(), 1, "{page:?}");
    let row = &page.items[0];
    assert_eq!(row.op_id, op(30));
    assert_eq!(row.kind, GcIntentKind::Mount);
    assert_eq!(row.scope_id.as_slice(), branch.as_slice());
    assert_eq!(row.project.as_deref(), Some("proj"));
    assert_eq!(row.branch.as_deref(), Some("work"));
    assert_eq!(row.phase, None);
    assert_eq!(
        row.evidence.as_deref(),
        Some("action=preview;operator=alice")
    );
    assert_eq!(row.machine, None);
    assert_eq!(row.sandbox, None);

    let mount = GcIntent::shared(
        op(31),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([31; 32]), Some("box"), Some("box"))
    .owned(Some("runtime-private"), Some("boot-private"));
    history
        .gc_register(&mount)
        .expect("the preview row's cleared owner key does not hold");
}

/// A prune deletes exactly the exported done rows it names; a live intent
/// named by the same call is not a done row and stays.
#[test]
fn a_prune_deletes_only_the_named_done_rows() {
    let temp = Temp::new("gc-done-audit-prune");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let path = temp.join("store.sqlite");
    done_row(&path, branch, 1, "release one", 10);
    done_row(&path, branch, 2, "release two", 20);
    done_row(&path, branch, 3, "release three", 30);
    let live = GcIntent::shared(
        op(4),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([4; 32]), Some("box"), Some("box"))
    .owned(Some("runtime-private"), Some("boot-private"));
    history.gc_register(&live).unwrap();

    let removed = history
        .gc_prune_done(&[op(1), op(3), op(4), op(9)])
        .unwrap();
    assert_eq!(removed, 2, "only the named done rows go");
    let page = history.gc_done_intents(0, 10).unwrap();
    assert_eq!(page.items.len(), 1, "{page:?}");
    assert_eq!(page.items[0].op_id, op(2));
    assert_eq!(
        history
            .gc_intents(GcScopeKind::Branch, branch.as_slice())
            .unwrap()
            .len(),
        1,
        "the live intent stays"
    );
}
