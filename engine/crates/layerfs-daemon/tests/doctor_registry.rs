//! The diagnostic endpoint reads registry facts without Store SQL or overlay jobs.
#[path = "../../layerfs-store-service/tests/support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../../layerfs-store-service/tests/support/sdk.rs"]
#[allow(dead_code)]
mod support;
use layerfs_bridge::control::{Reply, Request, SdkCall, SdkValue, UnmountControl};
use layerfs_daemon::control::Service;
use layerfs_history::WorkspaceId;

fn page(service: &Service, offset: i64, generation: Option<String>) -> Reply {
    service
        .execute_control(&Request::Sdk(SdkCall {
            method: "sandbox.doctor_workspaces".into(),
            args: SdkValue::map([
                ("offset", SdkValue::Int(offset)),
                (
                    "generation",
                    generation.map_or(SdkValue::Null, SdkValue::text),
                ),
            ]),
        }))
        .unwrap()
        .reply
}
#[test]
fn registry_observation_is_complete_read_only_and_detects_membership_changes() {
    let fixture = support::Fixture::new("doctor-registry", |_| 0);
    let service = Service::new(fixture.service.store.clone(), &fixture.owner);
    let before = fixture.work();
    let Reply::Sdk(empty) = page(&service, 0, None) else {
        panic!("empty registry")
    };
    assert_eq!(empty.get_int("total"), Some(0));
    assert_eq!(empty.get("next_offset"), Some(&SdkValue::Null));
    let after = fixture.work();
    assert_eq!(
        (before.statements, before.object_batches),
        (after.statements, after.object_batches)
    );
    let bound = service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([7; 32]).unwrap(),
            branch: fixture.host.branch,
        })
        .unwrap();
    let Reply::Bound { token, .. } = bound.reply else {
        panic!("bind")
    };
    let generation = empty.get_text("generation").unwrap().to_owned();
    let Reply::SdkRefused(changed) = page(&service, 0, Some(generation)) else {
        panic!("changed membership")
    };
    assert_eq!(changed.code, "Changed");
    let before = fixture.work();
    let Reply::Sdk(one) = page(&service, 0, None) else {
        panic!("registry")
    };
    assert_eq!(one.get_int("total"), Some(1));
    let Some(SdkValue::List(rows)) = one.get("workspaces") else {
        panic!("rows")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get_text("activity"), Some("idle"));
    assert!(!rows[0].get_bool("mounted"));
    let after = fixture.work();
    assert_eq!(
        (before.statements, before.object_batches),
        (after.statements, after.object_batches)
    );
    service
        .execute_control(&Request::Unmount(UnmountControl {
            token,
            discard: true,
        }))
        .unwrap();
    drop(service);
    fixture.cleanup();
}
