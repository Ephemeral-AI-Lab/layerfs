//! `project.delete` is reachable at the control layer behind the call
//! dispatch T11 will add (the API/MCP call table itself is unchanged here).
use layerfs_home::events::Events;
use layerfs_home::names::ProjectRef;

#[test]
fn project_delete_is_reachable_from_control_and_refuses_an_unknown_store() {
    let home = std::env::temp_dir().join(format!("layerfs-control-delete-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    layerfs_home::home::layout::create(&home).unwrap();
    let project = ProjectRef::parse("shop/app").unwrap();
    let mut sink = |_| {};
    let refused = layerfs_store::data::project::with_users::delete(
        &home,
        &mut Events::new(&mut sink),
        &project,
        &layerfs_sandbox::users::STORE,
    )
    .unwrap_err();
    assert_eq!(refused.code, "NoSuchStore");
    let _ = std::fs::remove_dir_all(&home);
}
