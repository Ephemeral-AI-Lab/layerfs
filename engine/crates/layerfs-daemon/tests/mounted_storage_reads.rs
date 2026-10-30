//! Real kernel mount: `backend.daemon.workspace.get_storage` reads only the
//! Workspace's overlay database, never the base tree from the Store (#508).
//! State reads call it for every mounted Workspace; walking a large base tree
//! there cost minutes of Store reads per call and saturated the daemon.
//! `list_changes` still names the changed paths on request. Disposable profile.
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

const FILES: usize = 10_000;

fn call(harness: &Harness, method: &str, directory: &str) -> SdkValue {
    let reply = harness
        .service
        .execute_control(&Request::Sdk(SdkCall {
            method: method.into(),
            args: SdkValue::map([("path", SdkValue::text(directory))]),
        }))
        .unwrap()
        .reply;
    match reply {
        Reply::Sdk(value) => value,
        other => panic!("{method}: {other:?}"),
    }
}

#[test]
fn storage_reads_no_base_tree_and_changes_are_still_listed() {
    let fixture = installed::Fixture::new(FILES, "storage-reads");
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
    let root = Path::new(&ready.directory);
    std::fs::write(root.join("added.txt"), b"new\n").unwrap();
    std::fs::write(root.join("file-000001"), b"changed\n").unwrap();

    let before = opened.store.work();
    let storage = call(
        &harness,
        "backend.daemon.workspace.get_storage",
        &ready.directory,
    );
    let after = opened.store.work();
    assert_eq!(storage.get("changes"), Some(&SdkValue::Null), "{storage:?}");
    assert!(
        matches!(storage.get("inodes"), Some(SdkValue::Int(n)) if *n >= 2),
        "{storage:?}"
    );
    assert!(
        matches!(storage.get("payload"), Some(SdkValue::Int(n)) if *n > 0),
        "{storage:?}"
    );
    // No Store object or length demand: the base tree of FILES entries is not read.
    assert_eq!(
        (after.object_ids, after.length_ids),
        (before.object_ids, before.length_ids),
        "get_storage read the Store"
    );

    let changes = call(
        &harness,
        "backend.daemon.workspace.list_changes",
        &ready.directory,
    );
    // The same counter sees a base walk: list_changes still pays one.
    assert!(
        opened.store.work().object_ids > after.object_ids,
        "list_changes demanded no Store object"
    );
    let Some(SdkValue::List(items)) = changes.get("items") else {
        panic!("{changes:?}");
    };
    let listed: Vec<(&str, &str)> = items
        .iter()
        .map(|item| {
            (
                item.get_text("path").unwrap(),
                item.get_text("kind").unwrap(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [("/added.txt", "Added"), ("/file-000001", "Modified")],
        "{changes:?}"
    );

    harness.unmount(&ready);
    harness.stop();
    drop(opened);
    fixture.cleanup();
}
