//! Progress declarations include machine provisioning and retain function-value refusal.
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
fn nine_calls_declare_on_progress_and_one_declares_the_exec_sinks() {
    assert_eq!(
        declaring("on_progress"),
        [
            "project.init",
            "project.export",
            "workspace.exec",
            "machine.start",
            "machine.create",
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
    assert_eq!(functions, 9 + 2);
}

#[test]
fn function_options_are_refused_as_values_before_any_effect() {
    let create = layerfs_api::find("machine.create").expect("machine.create is in the table");
    assert!(create
        .options
        .iter()
        .any(|o| o.name == "on_progress" && o.kind == Kind::Function));
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
        .expect_err("a callback cannot cross the library boundary as a value");
    assert_eq!(refused.code, "InvalidInput", "{}", refused.message);
    assert!(events.is_empty(), "refused before any effect: {events:?}");
    assert!(!home.join("machine").exists(), "nothing was created");
    let _ = std::fs::remove_dir_all(&home);
}
