//! A mount intent records only the place its caller states: there is no
//! process-wide incarnation a worker fills in, so a row naming one machine
//! never carries another machine's value. A private, place-null mount
//! records none. Every mount row names its project for the operator audit.
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
fn a_mount_records_only_the_place_its_caller_states() {
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

    // a placed mount whose caller states no incarnation records none
    let place = layerfs_bridge::control::MountPlace::new(Some("sandbox-11"), Some("machine-11"));
    let placed = admission::mount_intent(
        workspace,
        &snapshot,
        Some("installed"),
        &place,
        PlaceAuthority::default(),
    );
    assert_eq!(placed.machine_incarnation, None);
    assert_eq!(placed.project.as_deref(), Some("installed"));
    assert_eq!(placed.branch.as_deref(), Some("work"));

    // an authority the caller states is kept, never replaced
    let stated = admission::mount_intent(
        WorkspaceId::from_authority([12; 32]).expect("workspace"),
        &snapshot,
        Some("installed"),
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
        None,
        &layerfs_bridge::control::MountPlace::default(),
        PlaceAuthority::default(),
    );
    assert_eq!(private.machine_incarnation, None);

    fixture.cleanup();
}
