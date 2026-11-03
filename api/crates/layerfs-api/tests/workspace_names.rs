//! Workspace names (#536 C3): a mount may name its Workspace, unique within
//! its Sandbox (`Exists` otherwise), and every `workspace` call takes
//! `<sandbox>:<name>` as well as `<sandbox>:<path>`. Run on `native`
//! against a Sandbox folder whose records stand in for a running Sandbox:
//! no daemon runs, so a call that gets past the name reaches the Sandbox
//! and is refused there (`NoSuchSandbox`: no daemon keys).
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-wsn-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

/// A home with Sandbox `one` of `shop/app` recorded as started on
/// `native`, holding Workspace `dev` at `/w/1`.
fn home_with_a_named_workspace(tag: &str) -> PathBuf {
    let home = tempdir(tag);
    home::open(&home).unwrap();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    write_record(
        &paths::project_record(&home, "shop", "app"),
        &SdkValue::map([("name", t("app")), ("created", SdkValue::Int(1))]),
    )
    .unwrap();
    // the native daemon a call would launch: never started here
    let native = paths::machine_record(&home, "native");
    // (a home made without the machine calls has no native record yet)
    let record = read_record(&native)
        .unwrap()
        .unwrap_or_else(|| SdkValue::map([("name", t("native")), ("platform", t("native"))]));
    write_record(
        &native,
        &home::with_field(record, "daemon", t("/nonexistent")),
    )
    .unwrap();
    write_record(
        &paths::sandbox_record(&home, "one"),
        &SdkValue::map([
            ("name", t("one")),
            ("project", t("shop/app")),
            ("machine", t("native")),
            ("id", t("one")),
            ("daemon", SdkValue::map([("runtime", t("native"))])),
        ]),
    )
    .unwrap();
    write_record(
        &paths::workspace_dir(&home, "one").join(format!("{}.json", "ab".repeat(32))),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("id", t(&"ab".repeat(32))),
            ("name", t("dev")),
            ("branch", t("lane")),
            ("path", t("/w/1")),
            ("namespace", SdkValue::Int(1)),
            ("created", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
    home
}

#[test]
fn a_name_is_unique_in_its_sandbox() {
    let home = home_with_a_named_workspace("unique");
    let mount = |name: &str| {
        call(
            &home,
            "workspace.mount",
            &[
                ("sandbox", t("one")),
                ("branch", t("lane")),
                ("name", t(name)),
            ],
        )
    };
    let (code, message) = mount("dev").unwrap_err();
    assert_eq!(code, "Exists", "{message}");
    assert!(
        message.contains("already has a Workspace named dev"),
        "{message}"
    );
    // another name gets past the check, to the Sandbox itself
    let (code, message) = mount("other").unwrap_err();
    assert_eq!(code, "NoSuchSandbox", "{message}");
    assert!(message.contains("no daemon keys"), "{message}");
    // a name is a name: not a path, not upper case
    for bad in ["Dev", "a/b", "-x"] {
        let (code, message) = mount(bad).unwrap_err();
        assert_eq!(code, "InvalidInput", "{bad}: {message}");
    }
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn every_workspace_call_takes_the_name_or_the_path() {
    let home = home_with_a_named_workspace("forms");
    for (call_name, extra) in [
        ("workspace.status", vec![]),
        ("workspace.commit", vec![]),
        ("workspace.unmount", vec![]),
        ("workspace.exec", vec![("command", t("true"))]),
    ] {
        let run = |ws: &str| {
            let mut args = vec![("ws", t(ws))];
            args.extend(extra.iter().cloned());
            call(&home, call_name, &args)
        };
        // the name and the path reach the same Sandbox
        for ws in ["one:dev", "one:/w/1"] {
            let (code, message) = run(ws).unwrap_err();
            assert_eq!(code, "NoSuchSandbox", "{call_name} {ws}: {message}");
        }
        // a name no Workspace record carries is not mounted
        let (code, message) = run("one:nope").unwrap_err();
        assert_eq!(code, "NotMounted", "{call_name}: {message}");
        assert!(message.contains("one:nope is not mounted"), "{message}");
        // neither form, or a bad name, is refused before any call
        for bad in ["one", "one:", "one:Dev"] {
            let (code, message) = run(bad).unwrap_err();
            assert_eq!(code, "InvalidInput", "{call_name} {bad}: {message}");
        }
    }
    std::fs::remove_dir_all(&home).unwrap();
}
