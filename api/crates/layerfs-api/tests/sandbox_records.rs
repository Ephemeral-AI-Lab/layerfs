//! Sandbox records without a running Sandbox (#533 review P1-22, P1-25;
//! #536 §7.1): the built-in machine names are refused as Sandbox names, a
//! live create's intent refuses other operations `Busy`, a gone owner's
//! uncertain intent refuses them `Unresolved` until `sandbox.remove` clears
//! it, a gone owner's certain intent is ignored, `sandbox.get` answers the
//! state with its companion, `sandbox.list` pages by cursor, and a worker
//! replaces a project record
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
        ("machine", t(layerfs_home::names::HOST_MACHINE)),
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

fn names(listed: &SdkValue) -> Vec<String> {
    match listed.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|i| i.get_text("sandbox").unwrap_or("").to_owned())
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
    for name in [layerfs_home::names::HOST_MACHINE, "docker"] {
        let (code, message) = call(
            &home,
            "sandbox.create",
            &[
                ("project", t("shop/app")),
                ("name", t(name)),
                ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ],
        )
        .unwrap_err();
        assert_eq!(code, "InvalidInput", "{message}");
    }

    // a create in flight (its process lives) keeps its name: `Busy`
    unstarted(&home, "busy", Some((i64::from(std::process::id()), false)));
    let (code, message) = call(&home, "sandbox.remove", &[("sandbox", t("busy"))]).unwrap_err();
    assert_eq!(code, "Busy", "{message}");
    assert!(message.contains("sandbox.create"), "{message}");
    let (code, _) = call(
        &home,
        "workspace.exec",
        &[("workspace", t("busy:w")), ("command", t("true"))],
    )
    .unwrap_err();
    assert_eq!(code, "Busy");
    // a live op is `busy`, its companion the op's call word, phase and start
    let got = call(&home, "sandbox.get", &[("sandbox", t("busy"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("busy"), "{got:?}");
    let op = got.get("op").expect("a busy Sandbox names its op");
    assert_eq!(op.get_text("name"), Some("create"));
    assert_eq!(op.get_text("phase"), Some("start"));
    assert_eq!(op.get_text("started_at"), Some("1970-01-01T00:00:00Z"));
    assert!(
        op.get("owner").is_none() && op.get("id").is_none(),
        "{op:?}"
    );

    // a gone owner's certain intent holds nothing: the Sandbox, with no
    // runtime, is `broken` `missing`, and its remove says it was gone
    let mut ended = std::process::Command::new("true").spawn().unwrap();
    let dead = i64::from(ended.id());
    ended.wait().unwrap();
    unstarted(&home, "crashed", Some((dead, false)));
    let got = call(&home, "sandbox.get", &[("sandbox", t("crashed"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("broken"), "{got:?}");
    assert_eq!(got.get_text("reason"), Some("missing"), "{got:?}");
    assert_eq!(got.get_text("fix"), Some("layerfs sandbox remove crashed"));
    let removed = call(
        &home,
        "sandbox.remove",
        &[("sandbox", t("crashed")), ("detail", SdkValue::Bool(true))],
    )
    .unwrap();
    assert_eq!(
        removed.get_text("outcome"),
        Some("already_gone"),
        "{removed:?}"
    );
    assert_eq!(removed.get("left"), Some(&SdkValue::List(vec![])));

    // a gone owner's uncertain intent refuses every other operation
    // `Unresolved`, naming the get that shows it and the remove that clears it
    unstarted(&home, "lost", Some((dead, true)));
    let refusal = layerfs_api::call(
        Some(&home),
        "workspace.exec",
        SdkValue::map([("workspace", t("lost:w")), ("command", t("true"))]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refusal.code, "Unresolved", "{refusal:?}");
    assert!(
        refusal.message.contains("`layerfs sandbox get lost`"),
        "{refusal:?}"
    );
    assert!(
        refusal.message.contains("`layerfs sandbox remove lost`"),
        "{refusal:?}"
    );
    let detail = &refusal.detail;
    assert_eq!(detail.get_text("fix"), Some("layerfs sandbox remove lost"));
    let next_op = detail.get("next").and_then(|next| next.get("op"));
    assert_eq!(
        next_op.and_then(|op| op.get_text("name")),
        Some("create"),
        "{detail:?}"
    );
    let got = call(
        &home,
        "sandbox.get",
        &[("sandbox", t("lost")), ("detail", SdkValue::Bool(true))],
    )
    .unwrap();
    assert_eq!(got.get_text("state"), Some("broken"), "{got:?}");
    assert_eq!(got.get_text("reason"), Some("unresolved"), "{got:?}");
    assert_eq!(got.get("op"), Some(&SdkValue::Null), "{got:?}");
    let message = got.get_text("message").unwrap_or("");
    assert!(message.contains("create (phase start)"), "{message}");
    let removed = call(
        &home,
        "sandbox.remove",
        &[("sandbox", t("lost")), ("detail", SdkValue::Bool(true))],
    )
    .unwrap();
    assert_eq!(
        removed.get_text("outcome"),
        Some("already_gone"),
        "{removed:?}"
    );
    let left = removed.get("left").cloned().unwrap_or_default();
    assert!(
        matches!(&left, SdkValue::List(items) if items.len() == 1),
        "{removed:?}"
    );
    assert!(!paths::sandbox_dir(&home, "lost").exists());
    assert_eq!(
        call(&home, "sandbox.remove", &[("sandbox", t("lost"))])
            .unwrap_err()
            .0,
        "NotFound"
    );

    // sandbox.list pages by cursor, which carries its filters
    for name in ["a1", "a2", "a3"] {
        unstarted(&home, name, None);
    }
    let first = call(&home, "sandbox.list", &[("limit", SdkValue::Int(2))]).unwrap();
    assert_eq!(names(&first), ["a1", "a2"], "{first:?}");
    let cursor = first
        .get_text("next_cursor")
        .expect("a second page")
        .to_owned();
    let second = call(
        &home,
        "sandbox.list",
        &[("limit", SdkValue::Int(2)), ("cursor", t(&cursor))],
    )
    .unwrap();
    assert_eq!(names(&second), ["a3", "busy"], "{second:?}");
    assert_eq!(
        second.get("next_cursor"),
        None,
        "the last page has no cursor"
    );
    // a row says its state when not ready
    let rows = match second.get("items") {
        Some(SdkValue::List(rows)) => rows.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(rows[0].get_text("reason"), Some("missing"), "{rows:?}");
    assert_eq!(rows[1].get_text("state"), Some("busy"), "{rows:?}");
    // the cursor carries the filters: another filter is refused
    let (code, message) = call(
        &home,
        "sandbox.list",
        &[("cursor", t(&cursor)), ("machine", t("docker"))],
    )
    .unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    let (code, _) = call(&home, "sandbox.list", &[("cursor", t("bm90IGEgY3Vyc29y"))]).unwrap_err();
    assert_eq!(code, "InvalidInput");
    let (code, _) = call(&home, "sandbox.list", &[("limit", SdkValue::Int(0))]).unwrap_err();
    assert_eq!(code, "InvalidInput");
    // filters: one machine's, one project's
    let none = call(&home, "sandbox.list", &[("machine", t("docker"))]).unwrap();
    assert!(names(&none).is_empty(), "{none:?}");
    let all = call(&home, "sandbox.list", &[("project", t("shop/app"))]).unwrap();
    assert_eq!(names(&all), ["a1", "a2", "a3", "busy"]);
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
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "s".to_owned(), served)
    });
    let endpoint = worker::Endpoint {
        address,
        secret: "s".to_owned(),
    };
    let fresh = SdkValue::map([("name", t("app")), ("created", SdkValue::Int(2))]);
    // a Workspace status: `sandbox.get`'s own reads write no record
    // (layerfs-project#181)
    let request = SdkValue::map([
        ("op", t("backend.daemon.workspace.status")),
        ("store", t("shop")),
        ("project", t("app")),
        (
            "store_record",
            SdkValue::map([("name", t("shop")), ("kind", t("sqlite"))]),
        ),
        ("project_record", fresh.clone()),
        (layerfs_home::names::HOST_MACHINE, SdkValue::Map(Vec::new())),
        ("more", SdkValue::Map(Vec::new())),
    ]);
    // the call itself fails here (no daemon beside this test); the record
    // was written first
    let _ = worker::call(&endpoint, "sandbox.host", request, &mut |_| {});
    assert_eq!(read_record(&stale).unwrap(), Some(versioned(&fresh)));
    let _ = std::fs::remove_dir_all(root);
}
