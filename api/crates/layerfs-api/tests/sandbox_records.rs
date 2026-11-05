//! Sandbox records without a running Sandbox (#533 review P1-22, P1-25;
//! #536 §7.1): the built-in machine names are refused as Sandbox names, a
//! live create's intent refuses other operations `Busy`, a gone owner's
//! uncertain intent refuses them `Unresolved` until `sandbox.close` clears
//! it, a gone owner's certain intent is ignored, `sandbox.get` shows the
//! intent, `sandbox.list` pages, and a worker replaces a project record
//! left by a removed project of the same name.
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
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

/// A Sandbox record as `sandbox.create` leaves it, without an id: with
/// its create's intent owned by `pid`, uncertain or not, or with none.
fn unstarted(home: &Path, name: &str, owner: Option<(i64, bool)>) {
    let mut record = SdkValue::map([
        ("name", t(name)),
        ("project", t("shop/app")),
        ("machine", t("native")),
    ]);
    if let Some((pid, uncertain)) = owner {
        let op = SdkValue::map([
            ("id", t("op-00000000000000bb")),
            ("name", t("sandbox.create")),
            ("phase", t("start")),
            ("args", SdkValue::Null),
            ("owner", SdkValue::map([("pid", SdkValue::Int(pid))])),
            ("started", SdkValue::Int(1)),
            ("updated", SdkValue::Int(1)),
            ("uncertain", SdkValue::Bool(uncertain)),
        ]);
        record = home::with_field(record, "op", op);
    }
    write_record(&paths::sandbox_record(home, name), &record).unwrap();
}

/// A process that has ended (and been reaped).
fn dead_pid() -> i64 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    child.wait().unwrap();
    i64::from(child.id())
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

    // a create in flight (its process lives) keeps its name: `Busy`
    unstarted(&home, "busy", Some((i64::from(std::process::id()), false)));
    let (code, message) = call(&home, "sandbox.close", &[("name", t("busy"))]).unwrap_err();
    assert_eq!(code, "Busy", "{message}");
    assert!(message.contains("sandbox.create"), "{message}");
    let (code, _) = call(
        &home,
        "workspace.exec",
        &[("ws", t("busy:/w")), ("command", t("true"))],
    )
    .unwrap_err();
    assert_eq!(code, "Busy");
    let got = call(&home, "sandbox.get", &[("name", t("busy"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("starting"), "{got:?}");

    // a gone owner's certain intent holds nothing: the Sandbox reads
    // `gone` and closes
    let mut ended = std::process::Command::new("true").spawn().unwrap();
    let dead = i64::from(ended.id());
    ended.wait().unwrap();
    unstarted(&home, "crashed", Some((dead, false)));
    let got = call(&home, "sandbox.get", &[("name", t("crashed"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("closed"), "{got:?}");
    let closed = call(&home, "sandbox.close", &[("name", t("crashed"))]).unwrap();
    assert_eq!(closed.get_text("was"), Some("closed"), "{closed:?}");

    // a gone owner's uncertain intent refuses every other operation
    // `Unresolved`, naming the get that shows it and the close that clears it
    unstarted(&home, "lost", Some((dead, true)));
    let (code, message) = call(
        &home,
        "workspace.exec",
        &[("ws", t("lost:/w")), ("command", t("true"))],
    )
    .unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    assert!(message.contains("`layerfs sandbox get lost`"), "{message}");
    assert!(
        message.contains("`layerfs sandbox close lost`"),
        "{message}"
    );
    let got = call(&home, "sandbox.get", &[("name", t("lost"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("unresolved"), "{got:?}");
    let op = got.get("op").expect("sandbox.get shows the intent");
    assert_eq!(op.get_text("id"), Some("op-00000000000000bb"));
    assert_eq!(op.get_text("name"), Some("sandbox.create"));
    assert_eq!(op.get_text("phase"), Some("start"));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    assert!(op.get("owner").is_none(), "{op:?}");
    let closed = call(&home, "sandbox.close", &[("name", t("lost"))]).unwrap();
    assert_eq!(closed.get_text("was"), Some("unresolved"), "{closed:?}");
    assert!(closed.get_text("left").is_some(), "{closed:?}");
    assert!(!paths::sandbox_dir(&home, "lost").exists());
    // B5 (#548): a repeated close is a quiet no-op, not a second delete
    let replay = call(&home, "sandbox.close", &[("name", t("lost"))]).unwrap();
    assert!(replay.get_bool("quiet"), "{replay:?}");

    // sandbox.list pages
    for name in ["a1", "a2", "a3"] {
        unstarted(&home, name, None);
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

#[test]
fn sandbox_get_and_list_serialize_the_explicit_states() {
    let home = tempdir("states");
    home::open(&home).unwrap();
    let me = i64::from(std::process::id());
    let dead = dead_pid();
    let op = |name: &str, phase: &str, pid: i64, uncertain: bool| {
        format!(
            r#","op":{{"id":"op-00000000000000aa","name":"{name}","phase":"{phase}","args":null,"owner":{{"pid":{pid}}},"started":1,"updated":2,"uncertain":{uncertain}}}"#
        )
    };
    let records = [
        (
            "creating",
            String::new(),
            op("sandbox.create", "claim", me, false),
        ),
        (
            "provisioning",
            String::new(),
            op("sandbox.create", "provisioning", me, false),
        ),
        (
            "starting",
            String::new(),
            op("sandbox.create", "start", me, false),
        ),
        ("ready", String::from(r#","id":"ready""#), String::new()),
        (
            "busy",
            String::from(
                r#","id":"busy","exec_leases":[{"id":"lease-1","deadline":9999999999999}]"#,
            ),
            String::new(),
        ),
        (
            "draining",
            String::from(r#","id":"draining""#),
            op("sandbox.close", "draining", me, false),
        ),
        ("closed", String::new(), String::new()),
        (
            "lost",
            String::from(r#","id":"lost","lost":true,"close_reason":"an exec lease expired""#),
            String::new(),
        ),
        (
            "failed",
            String::from(r#","state":"failed""#),
            String::new(),
        ),
        (
            "unresolved",
            String::new(),
            op("sandbox.create", "start", dead, true),
        ),
    ];
    for (name, facts, intent) in records {
        let text = format!(
            r#"{{"name":"{name}","project":"shop/app","machine":"native","generation":1{facts}{intent}}}"#
        );
        write_record(
            &paths::sandbox_record(&home, name),
            &layerfs_api::json::parse(&text).unwrap(),
        )
        .unwrap();
    }

    // Every one of the ten states is serialized under the existing `state`
    // key, with the record's other keys unchanged.
    let states = [
        ("creating", "creating"),
        ("provisioning", "provisioning"),
        ("starting", "starting"),
        ("ready", "ready"),
        ("busy", "busy"),
        ("draining", "draining"),
        ("closed", "closed"),
        ("lost", "lost"),
        ("failed", "failed"),
        ("unresolved", "unresolved"),
    ];
    let list = call(&home, "sandbox.list", &[]).unwrap();
    let item = |listed: &SdkValue, name: &str| -> SdkValue {
        match listed.get("items") {
            Some(SdkValue::List(items)) => items
                .iter()
                .find(|item| item.get_text("name") == Some(name))
                .unwrap_or_else(|| panic!("{name} not listed: {listed:?}"))
                .clone(),
            other => panic!("{other:?}"),
        }
    };
    for (name, state) in states {
        let item = item(&list, name);
        assert_eq!(item.get_text("state"), Some(state), "{name}: {item:?}");
        for key in ["name", "project", "machine", "generation"] {
            assert!(
                item.get(key).is_some(),
                "{name} lost existing key {key}: {item:?}"
            );
        }
        assert!(
            item.get("v").is_none()
                && item.get("runtime_id").is_none()
                && item.get("daemon").is_none(),
            "{name} shows a hidden field: {item:?}"
        );
    }

    // `sandbox.get` shows the same explicit state for a record with no
    // runtime id to ask; its keys are still the record's plus its Workspaces.
    for (name, state) in states {
        if matches!(name, "ready" | "busy" | "draining" | "lost") {
            continue;
        }
        let got = call(&home, "sandbox.get", &[("name", t(name))]).unwrap();
        assert_eq!(got.get_text("state"), Some(state), "{name}: {got:?}");
        assert!(got.get("workspaces").is_some(), "{name}: {got:?}");
        assert!(got.get("next_page_pos").is_some(), "{name}: {got:?}");
        for key in ["name", "project", "machine", "generation"] {
            assert!(
                got.get(key).is_some(),
                "{name} lost existing key {key}: {got:?}"
            );
        }
        assert!(
            got.get("v").is_none()
                && got.get("runtime_id").is_none()
                && got.get("daemon").is_none(),
            "{name} shows a hidden field: {got:?}"
        );
        if matches!(
            name,
            "creating" | "provisioning" | "starting" | "unresolved"
        ) {
            assert!(got.get("op").is_some(), "{name} lost its intent: {got:?}");
        }
    }
    std::fs::remove_dir_all(home).unwrap();
}
