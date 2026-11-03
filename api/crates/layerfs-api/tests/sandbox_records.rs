//! Sandbox records without a running Sandbox (#533 review P1-22, P1-25):
//! the built-in machine names are refused as Sandbox names, a record that
//! never got an id can be closed once its creator is gone, `sandbox.list`
//! pages, and a worker replaces a project record left by a removed project
//! of the same name.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, read_record, versioned, write_record};
use layerfs_api::worker;
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-sbr-{tag}-{}", std::process::id()));
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
    calls::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

/// A Sandbox record as `sandbox.create` leaves it, without an id.
fn unstarted(home: &Path, name: &str, state: &str, pid: i64) {
    write_record(
        &paths::sandbox_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("project", t("shop/app")),
            ("machine", t("native")),
            ("state", t(state)),
            ("pid", SdkValue::Int(pid)),
        ]),
    )
    .unwrap();
}

fn names(listed: &SdkValue) -> Vec<String> {
    match listed.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|i| i.get_text("name").unwrap_or("").to_owned())
            .collect(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn sandbox_records_without_a_sandbox() {
    let home = tempdir("home");
    home::open(&home).unwrap();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    write_record(
        &paths::project_record(&home, "shop", "app"),
        &SdkValue::map([("name", t("app")), ("created", SdkValue::Int(1))]),
    )
    .unwrap();

    // H §7.6: a Sandbox is not named after a built-in machine
    for name in ["native", "docker"] {
        let (code, message) = call(
            &home,
            "sandbox.create",
            &[
                ("project", t("shop/app")),
                ("name", t(name)),
                ("machine", t("native")),
            ],
        )
        .unwrap_err();
        assert_eq!(code, "InvalidInput", "{message}");
    }

    // a create in flight (its process lives) keeps its name
    unstarted(&home, "busy", "creating", i64::from(std::process::id()));
    let (code, message) = call(&home, "sandbox.close", &[("name", t("busy"))]).unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    // one whose creator is gone, or whose outcome was unknown, can be closed
    let mut ended = std::process::Command::new("true").spawn().unwrap();
    let dead = i64::from(ended.id());
    ended.wait().unwrap();
    unstarted(&home, "crashed", "creating", dead);
    let closed = call(&home, "sandbox.close", &[("name", t("crashed"))]).unwrap();
    assert_eq!(closed.get_text("was"), Some("creating"), "{closed:?}");
    unstarted(&home, "lost", "unknown", 0);
    let closed = call(&home, "sandbox.close", &[("name", t("lost"))]).unwrap();
    assert_eq!(closed.get_text("was"), Some("unknown"), "{closed:?}");
    assert!(closed.get_text("left").is_some(), "{closed:?}");
    assert!(!paths::sandbox_dir(&home, "lost").exists());
    assert_eq!(
        call(&home, "sandbox.close", &[("name", t("lost"))])
            .unwrap_err()
            .0,
        "NoSuchSandbox"
    );

    // sandbox.list pages
    for name in ["a1", "a2", "a3"] {
        unstarted(&home, name, "unknown", 0);
    }
    let first = call(&home, "sandbox.list", &[("page_size", SdkValue::Int(2))]).unwrap();
    assert_eq!(names(&first).len(), 2, "{first:?}");
    let next = first.get_int("next_page_pos").expect("a second page");
    let second = call(
        &home,
        "sandbox.list",
        &[
            ("page_size", SdkValue::Int(2)),
            ("page_pos", SdkValue::Int(next)),
        ],
    )
    .unwrap();
    assert_eq!(second.get("next_page_pos"), Some(&SdkValue::Null));
    let mut all = names(&first);
    all.extend(names(&second));
    all.sort();
    assert_eq!(all, ["a1", "a2", "a3", "busy"]);
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn a_worker_replaces_a_removed_project_s_record() {
    let root = tempdir("root");
    let stale = paths::project_record(&root, "shop", "app");
    write_record(
        &stale,
        &SdkValue::map([("name", t("app")), ("created", SdkValue::Int(1))]),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let served = root.clone();
    std::thread::spawn(move || worker::serve(listener, "s".to_owned(), served));
    let endpoint = worker::Endpoint {
        address,
        secret: "s".to_owned(),
    };
    let fresh = SdkValue::map([("name", t("app")), ("created", SdkValue::Int(2))]);
    let request = SdkValue::map([
        ("op", t("sandbox.list_workspaces")),
        ("store", t("shop")),
        ("project", t("app")),
        (
            "store_record",
            SdkValue::map([("name", t("shop")), ("kind", t("sqlite"))]),
        ),
        ("project_record", fresh.clone()),
        ("native", SdkValue::Map(Vec::new())),
        ("more", SdkValue::Map(Vec::new())),
    ]);
    // the call itself fails here (no daemon beside this test); the record
    // was written first
    let _ = worker::call(&endpoint, "sandbox.host", request, &mut |_| {});
    assert_eq!(read_record(&stale).unwrap(), Some(versioned(&fresh)));
    let _ = std::fs::remove_dir_all(root);
}
