//! Which calls take `on_progress` (#37): the table declares it on exactly seven
//! calls (`workspace.exec` for its exec id, layerfs-project#113), and a call
//! that does not declare it refuses it before any effect.
//! `machine.create` is not one of the seven, so a caller cannot ask for the
//! `{ phase: "create", machine }` event the library emits for it.
//! The page that states this: docs/architecture/api/call-table/events.md.
use layerfs_api::Kind;
use layerfs_bridge::control::SdkValue;

fn declaring(option: &str) -> Vec<String> {
    layerfs_api::calls()
        .into_iter()
        .filter(|call| {
            call.options
                .iter()
                .any(|o| o.name == option && o.kind == Kind::Function)
        })
        .map(|call| call.name())
        .collect()
}

#[test]
fn seven_calls_declare_on_progress_and_one_declares_the_exec_sinks() {
    assert_eq!(
        declaring("on_progress"),
        [
            "project.init",
            "project.export",
            "workspace.exec",
            "store.push",
            "store.pull",
            "backup.create",
            "backup.restore"
        ]
    );
    assert_eq!(declaring("stdout"), ["workspace.exec"]);
    assert_eq!(declaring("stderr"), ["workspace.exec"]);
    // no other option is a function
    let functions: usize = layerfs_api::calls()
        .iter()
        .map(|call| {
            call.options
                .iter()
                .filter(|o| o.kind == Kind::Function)
                .count()
        })
        .sum();
    assert_eq!(functions, 7 + 2);
}

#[test]
fn machine_create_declares_no_on_progress_and_refuses_it() {
    let create = layerfs_api::find("machine.create").expect("machine.create is in the table");
    assert!(create.options.iter().all(|o| o.name != "on_progress"));
    let home =
        std::env::temp_dir().join(format!("layerfs-progress-options-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let args = SdkValue::map([
        ("name", SdkValue::text("m1")),
        ("platform", SdkValue::text("lima")),
        ("on_progress", SdkValue::text("f")),
    ]);
    let mut events = Vec::new();
    let refused = layerfs_api::call(Some(&home), "machine.create", args, &mut |e| events.push(e))
        .expect_err("an undeclared option is refused");
    assert_eq!(refused.code, "InvalidInput", "{}", refused.message);
    assert!(events.is_empty(), "refused before any effect: {events:?}");
    assert!(!home.join("machine").exists(), "nothing was created");
    let _ = std::fs::remove_dir_all(&home);
}
