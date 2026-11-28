//! A Store call in this process traces its steps (#819): with
//! `LAYERFS_TRACE` and `LAYERFS_TRACE_STEP` set, a `file.read` on a local
//! sqlite Store appends `dispatch`, `lease`, `store_open` and `call`, and
//! `layerfs_api::close` appends op `close` with `stores` and `sessions`.
//! One test in its own binary: it sets the process's environment.
#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../support/root.rs"]
mod root;
use layerfs_api::home;
use layerfs_protocol::sdk::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-steps-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(crate::root::root_of(home)), name, args, &mut |_| {})
        .unwrap_or_else(|refusal| panic!("{name}: {refusal:?}"))
}

/// The step names `text` holds for op `op`, in order.
fn steps(text: &str, op: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| layerfs_home::json::parse(line).ok())
        .filter(|line| line.get_text("event") == Some("step") && line.get_text("op") == Some(op))
        .map(|line| line.get_text("step").unwrap().to_owned())
        .collect()
}

#[test]
fn a_local_store_call_and_the_close_trace_their_steps() {
    let home = tempdir("home");
    home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();
    let project = ("project", SdkValue::text("shop/api"));
    ok(&home, "store.create", &[("name", SdkValue::text("shop"))]);
    let path = SdkValue::text(folder.display().to_string());
    ok(&home, "project.init", &[project.clone(), ("folder", path)]);
    layerfs_api::close();

    let trace = home.join("trace.jsonl");
    std::env::set_var("LAYERFS_TRACE", &trace);
    std::env::set_var("LAYERFS_TRACE_STEP", "1");
    let read = [
        project,
        ("branch", SdkValue::text("main")),
        ("path", SdkValue::text("/README.md")),
    ];
    ok(&home, "file.read", &read);
    layerfs_api::close();
    std::env::remove_var("LAYERFS_TRACE_STEP");
    std::env::remove_var("LAYERFS_TRACE");

    let text = std::fs::read_to_string(&trace).unwrap();
    assert_eq!(
        steps(&text, "file.read"),
        ["dispatch", "lease", "store_open", "call"],
        "{text}"
    );
    assert_eq!(steps(&text, "close"), ["stores", "sessions"], "{text}");
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
}
