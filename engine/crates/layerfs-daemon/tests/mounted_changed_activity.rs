//! Real kernel mount: `backend.daemon.workspace.list_changed` answers the
//! Workspace's changes and its activity (word, phase, start), which ride
//! there rather than in the Status record so that record keeps the bytes
//! other builds read (RF re-check N-A). A path no Workspace is mounted at
//! is refused. Disposable profile.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
use layerfs_bridge::control::{Reply, Request, SdkCall, SdkValue};
use layerfs_store_service::bootstrap::open_store_observed;
use mounted::Harness;
use std::path::Path;

fn call(harness: &Harness, directory: &str) -> Reply {
    harness
        .service
        .execute_control(&Request::Sdk(SdkCall {
            method: "backend.daemon.workspace.list_changed".into(),
            args: SdkValue::map([("path", SdkValue::text(directory))]),
        }))
        .unwrap()
        .reply
}

#[test]
fn list_changed_names_the_changes_and_the_activity() {
    let fixture = installed::Fixture::new(8, "changed-activity");
    let opened = open_store_observed(
        fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
        64,
        8 * 1024 * 1024,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let harness = Harness::new(opened.store.clone(), &fixture.directory, fixture.branch);
    let ready = harness.mount(1);
    std::fs::write(Path::new(&ready.directory).join("added.txt"), b"new\n").unwrap();

    let Reply::Sdk(answer) = call(&harness, &ready.directory) else {
        panic!("list_changed was not answered");
    };
    let Some(SdkValue::List(entries)) = answer.get("entries") else {
        panic!("{answer:?}");
    };
    let listed: Vec<_> = (entries.iter())
        .map(|entry| (entry.get_text("path"), entry.get_text("change")))
        .collect();
    assert_eq!(listed, [(Some("/added.txt"), Some("added"))], "{answer:?}");
    assert_eq!(answer.get("partial"), Some(&SdkValue::Bool(false)));
    // an idle Workspace: no operation phase, and the time it became idle
    assert_eq!(answer.get_text("activity"), Some("Idle"), "{answer:?}");
    assert_eq!(answer.get("activityPhase"), Some(&SdkValue::Null));
    assert!(
        matches!(answer.get("activitySinceMs"), Some(SdkValue::Int(ms)) if *ms > 0),
        "{answer:?}"
    );
    // no Workspace there: refused, not answered empty
    assert!(matches!(call(&harness, "/nowhere"), Reply::SdkRefused(_)));

    harness.unmount(&ready);
    harness.stop();
    drop(opened);
    fixture.cleanup();
}
