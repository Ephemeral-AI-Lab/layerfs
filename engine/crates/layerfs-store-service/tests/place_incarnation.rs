//! The worker process records its incarnation once at start; every mount
//! intent it places on a machine carries it, so the host's next start can
//! prove the mount old. A private, place-null mount records none.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_history::{PlaceAuthority, WorkspaceId};
use layerfs_store_service::history::{admission, lookup};
use std::fs;
use support::Fixture;

#[test]
fn a_placed_mount_carries_the_serving_workers_incarnation() {
    let fixture = Fixture::new("place-incarnation", |source| {
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
    let workspace = WorkspaceId::from_authority([11; 32]).expect("workspace");

    // the worker records its incarnation before it serves a mount
    admission::set_machine_incarnation("worker:2".to_owned());
    assert_eq!(admission::machine_incarnation(), Some("worker:2"));

    let place = layerfs_bridge::control::MountPlace::new(Some("sandbox-11"), Some("machine-11"));
    let placed = admission::mount_intent(workspace, &snapshot, &place, PlaceAuthority::default());
    assert_eq!(placed.machine_incarnation.as_deref(), Some("worker:2"));

    // an authority the caller states is kept, never replaced
    let stated = admission::mount_intent(
        WorkspaceId::from_authority([12; 32]).expect("workspace"),
        &snapshot,
        &place,
        PlaceAuthority {
            machine_incarnation: Some("worker:stale".to_owned()),
            ..PlaceAuthority::default()
        },
    );
    assert_eq!(stated.machine_incarnation.as_deref(), Some("worker:stale"));

    // a private Store-call Workspace knows no place: it records no machine
    let private = admission::mount_intent(
        WorkspaceId::from_authority([13; 32]).expect("workspace"),
        &snapshot,
        &layerfs_bridge::control::MountPlace::default(),
        PlaceAuthority::default(),
    );
    assert_eq!(private.machine_incarnation, None);

    fixture.cleanup();
}
