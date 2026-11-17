//! #41 T6: the Store service's `branch.remove`, `project.delete` and
//! `project.remove_file` admit the Store's intents and report the holders.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::WorkspaceId;
use layerfs_store_service::history::{admission, deletion, lookup, removal};
use std::fs;
use support::Fixture;

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

/// Finding 5: a mount intent records who owns it (pid/start, boot, machine,
/// Sandbox and Workspace), so a gone owner can be marked uncertain and a
/// removal reports `Unresolved` instead of `MountsActive` forever. Releasing
/// the row is the owning call's settlement.
#[test]
fn a_mount_intent_records_owner_evidence_and_settles_unresolved() {
    let fixture = Fixture::new("admission-evidence", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let snapshot = history
        .branch_snapshot(branch.id)
        .expect("snapshot")
        .expect("the Branch");
    let workspace = WorkspaceId::from_authority([7; 32]).expect("workspace");
    let place = layerfs_bridge::control::MountPlace::new(Some("sandbox-7"), Some("machine-7"));
    let intent = admission::mount_intent(workspace, &snapshot, &place);
    let runtime = intent.owner_runtime.clone().expect("runtime evidence");
    let boot = intent.owner_boot.clone().expect("boot evidence");
    assert!(runtime.starts_with("pid:"), "{runtime}");
    assert!(boot.starts_with("boot:"), "{boot}");
    assert!(
        boot.contains(&lookup::hex(&[7; 32])),
        "the token names its Workspace: {boot}"
    );
    assert_eq!(intent.machine.as_deref(), Some("machine-7"));
    assert_eq!(intent.sandbox.as_deref(), Some("sandbox-7"));

    history.gc_register(&intent).expect("admit the mount");
    let stored = history
        .gc_intents(GcScopeKind::Branch, branch.id.as_slice())
        .expect("intents");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].owner_runtime.as_deref(), Some(runtime.as_str()));
    assert_eq!(stored[0].owner_boot.as_deref(), Some(boot.as_str()));

    // The production caller's effect when a release is refused: the row is
    // uncertain, so admission reports `Unresolved`.
    history
        .gc_mark_uncertain(&intent.op_id)
        .expect("mark uncertain");
    let refused = removal::remove(&fixture.service.store, &stack, "work")
        .expect_err("an uncertain owner holds");
    assert_eq!(refused.code, "Unresolved", "{refused:?}");
    assert_eq!(
        refused.detail.get_text("op"),
        Some(lookup::hex(&intent.op_id).as_str())
    );

    // The owning call settles the row by releasing it.
    history.gc_release(&intent.op_id).expect("settle");
    removal::remove(&fixture.service.store, &stack, "work").expect("removal after settlement");
    fixture.cleanup();
}

#[test]
fn a_mount_refuses_every_removal_through_the_service() {
    let fixture = Fixture::new("admission-service", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let mount = GcIntent::shared(
        [9; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.id.as_slice(),
    )
    .in_project(stack.id.as_slice())
    .named(Some("installed"), Some("work"))
    .placed(Some([9; 32]), Some("sandbox-9"), Some("machine-9"));
    fixture
        .service
        .store
        .history()
        .gc_register(&mount)
        .expect("admit the mount");

    let refused =
        removal::remove(&fixture.service.store, &stack, "work").expect_err("the mount holds");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");
    let SdkValue::List(holders) = refused.detail.get("holders").expect("holders") else {
        panic!("{refused:?}");
    };
    assert_eq!(holders[0].get_text("kind"), Some("mount"));
    assert_eq!(holders[0].get_text("sandbox"), Some("sandbox-9"));
    assert_eq!(holders[0].get_text("machine"), Some("machine-9"));
    let workspace = lookup::hex(&[9; 32]);
    assert_eq!(holders[0].get_text("workspace"), Some(workspace.as_str()));

    let refused = deletion::delete_stack(&fixture.service.store, stack.id, 1_000)
        .expect_err("the mount holds");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");

    let refused = fixture
        .try_call("project.remove_file", vec![("path", text("/a.txt"))])
        .expect_err("the mount holds");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");

    fixture
        .service
        .store
        .history()
        .gc_release(&[9; 16])
        .expect("release the mount");
    removal::remove(&fixture.service.store, &stack, "work").expect("remove after release");
}

/// F43R2-2 (§5.1): a mount checks every scope it names for a live operation,
/// so a live Init or merge row at the Branch's project refuses it `Busy` with
/// the operation's entity, phase and op. Two mounts of one Branch still share
/// the scope, so neither refuses the other.
#[test]
fn a_mount_sees_a_live_init_or_merge_but_shares_with_another_mount() {
    let fixture = Fixture::new("admission-init-mount", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let snapshot = history
        .branch_snapshot(branch.id)
        .expect("snapshot")
        .expect("the Branch");
    let place = layerfs_bridge::control::MountPlace::new(Some("sandbox-7"), Some("machine-7"));
    let mount = |tag: u8| {
        admission::mount_intent(
            WorkspaceId::from_authority([tag; 32]).expect("workspace"),
            &snapshot,
            &place,
        )
    };

    // A live Init or merge is an operation at project scope; a mount of the
    // project's Branch must see it and be refused with its phase and op.
    for (index, (kind, phase)) in [(GcIntentKind::Init, "init"), (GcIntentKind::Merge, "merge")]
        .into_iter()
        .enumerate()
    {
        let op = [0x40 + index as u8; 16];
        let operation = GcIntent::shared(op, kind, GcScopeKind::Project, stack.id.as_slice())
            .named(Some("installed"), None)
            .with_phase(Some(phase));
        history
            .gc_register(&operation)
            .expect("admit the Init/merge");

        let refused = history
            .gc_register(&mount(0x70 + index as u8))
            .expect_err("the live operation holds the project");
        let refused = lookup::history(refused);
        assert_eq!(refused.code, "Busy", "{refused:?}");
        let entity = refused.detail.get("entity").expect("entity detail");
        assert_eq!(entity.get_text("kind"), Some("project"), "{entity:?}");
        assert_eq!(entity.get_text("name"), Some("installed"), "{entity:?}");
        assert_eq!(refused.detail.get_text("phase"), Some(phase));
        assert_eq!(
            refused.detail.get_text("op"),
            Some(lookup::hex(&op).as_str())
        );

        history.gc_release(&op).expect("release the Init/merge");
        history.gc_release(&mount(0x70 + index as u8).op_id).ok();
    }

    // Mounts are shared: two live mounts of the Branch do not refuse each
    // other, so the OPS scan must keep `mount` out of its kind filter.
    let first = mount(0x81);
    let second = mount(0x82);
    history.gc_register(&first).expect("the first mount");
    history.gc_register(&second).expect("the second mount");
    history.gc_release(&first.op_id).expect("release the first");
    history
        .gc_release(&second.op_id)
        .expect("release the second");
    fixture.cleanup();
}
