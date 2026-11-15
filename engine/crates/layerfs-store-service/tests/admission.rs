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
use layerfs_store_service::history::{deletion, lookup, removal};
use std::fs;
use support::Fixture;

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
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
