//! Named Sandboxes and Workspaces (#533 step 12) on `native`: create, the
//! name unique across the home, the wrong-machine refusal, mount, exec,
//! status, commit, unmount and remove. The live half needs the macOS (or
//! Linux, as root) daemon at LAYERFS_DAEMON; without it, it says NOT_RUN.
use layerfs_api::home::{self, paths, write_record};
use layerfs_protocol::sdk::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-sbx-{tag}-{}", std::process::id()));
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

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn text(value: &SdkValue, field: &str) -> String {
    match value.get(field) {
        Some(SdkValue::Text(t)) => t.clone(),
        Some(SdkValue::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
        other => panic!("{field}: {other:?} in {value:?}"),
    }
}

#[test]
fn named_sandboxes_on_native() {
    let home = tempdir("home");
    home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "in a sandbox\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    ok(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/api")),
            ("name", t("dev")),
            ("from", t("main")),
        ],
    );
    write_record(
        &paths::machine_record(&home, "box"),
        // docker runs on macOS and Linux hosts, so the support table passes
        // this machine on either
        &SdkValue::map([("name", t("box")), ("platform", t("docker"))]),
    )
    .unwrap();

    // a sqlite project's Sandbox runs on its Store's machine
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("t1")),
            ("machine", t("box")),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Unsupported");
    assert!(
        message.contains(layerfs_home::names::HOST_MACHINE),
        "{message}"
    );
    assert_eq!(
        call(
            &home,
            "workspace.mount",
            &[("sandbox", t("t1")), ("branch", t("dev"))]
        )
        .unwrap_err()
        .0,
        "NotFound"
    );
    assert_eq!(
        call(&home, "workspace.status", &[("workspace", t("t1"))])
            .unwrap_err()
            .0,
        "InvalidInput"
    );

    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset, no live Sandbox");
        return;
    };
    assert!(Path::new(&daemon).is_file());
    // the native machine's daemon is a setting of its record
    layerfs_api::call(
        Some(&home),
        "machine.update",
        SdkValue::map([
            ("machine", SdkValue::text(layerfs_home::names::HOST_MACHINE)),
            ("daemon", SdkValue::text(daemon.as_str())),
        ]),
        &mut |_| {},
    )
    .unwrap();
    let made = ok(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("t1")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(made.get_text("state"), Some("ready"));
    // its success dropped the create's intent in the same write (#536 §7.1)
    assert!(made.get("op").is_none(), "{made:?}");
    let record = home::read_record(&paths::sandbox_record(&home, "t1"))
        .unwrap()
        .unwrap();
    assert!(record.get("op").is_none(), "{record:?}");
    // Host-process Sandboxes store `local`, not an image digest. Reads
    // must still reach the daemon; public image overrides stay strict.
    let inspected = ok(
        &home,
        "sandbox.get",
        &[("sandbox", t("t1")), ("detail", SdkValue::Bool(true))],
    );
    assert_eq!(inspected.get_text("state"), Some("ready"));
    assert_eq!(inspected.get("image"), made.get("image"));
    let invalid = call(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("invalid-image")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("image", t("local")),
        ],
    );
    // macOS native runs no image: refused, never dropped for the default (R0)
    let expected = if cfg!(target_os = "macos") {
        "Unsupported"
    } else {
        "InvalidInput"
    };
    assert_eq!(invalid.unwrap_err().0, expected);
    let taken = call(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("t1")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ],
    );
    assert_eq!(taken.unwrap_err().0, "Exists");
    // `--mount` of a Branch that is not there: the Sandbox stays, and the
    // fix forks the Branch before it mounts it (LF2 review: only for a
    // missing Branch)
    let missing = layerfs_api::call(
        Some(&home),
        "sandbox.create",
        SdkValue::map([
            ("project", t("shop/api")),
            ("name", t("t2")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("mount", t("ghost")),
        ]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(missing.code, "NotFound", "{missing:?}");
    assert_eq!(
        missing.detail.get_text("fix"),
        Some("layerfs branch fork shop/api ghost && layerfs workspace mount t2 ghost"),
        "{missing:?}"
    );
    ok(&home, "sandbox.remove", &[("sandbox", t("t2"))]);
    let listed = ok(&home, "sandbox.list", &[]);
    assert!(layerfs_home::json::print(&listed).contains("\"t1\""));

    let mounted = ok(
        &home,
        "workspace.mount",
        &[("sandbox", t("t1")), ("branch", t("dev"))],
    );
    let ws = text(&mounted, "workspace");
    let ran = ok(
        &home,
        "workspace.exec",
        &[
            ("workspace", t(&ws)),
            ("command", t("cat README.md && echo made > NEW.md")),
        ],
    );
    assert_eq!(ran.get("exit_code"), Some(&SdkValue::Int(0)), "{ran:?}");
    assert_eq!(text(&ran, "stdout"), "in a sandbox\n");
    let status = ok(&home, "workspace.status", &[("workspace", t(&ws))]);
    let SdkValue::List(changes) = status.get("changes").unwrap() else {
        panic!("{status:?}")
    };
    assert_eq!(changes.len(), 1, "one changed path: {status:?}");
    assert_eq!(changes[0].get_text("path"), Some("/NEW.md"), "{status:?}");
    assert_eq!(changes[0].get_text("change"), Some("added"), "{status:?}");
    ok(
        &home,
        "workspace.commit",
        &[("workspace", t(&ws)), ("message", t("from a sandbox"))],
    );
    let got = ok(&home, "sandbox.get", &[("sandbox", t("t1"))]);
    assert!(
        layerfs_home::json::print(&got).contains(&ws[3..]),
        "{got:?}"
    );
    ok(&home, "workspace.unmount", &[("workspace", t(&ws))]);
    let read = ok(
        &home,
        "file.read",
        &[
            ("project", t("shop/api")),
            ("branch", t("dev")),
            ("path", t("/NEW.md")),
        ],
    );
    assert_eq!(read.get_text("text"), Some("made\n"));

    // RC-A F2: `--expected` on a fork with no Commit of its own compares
    // with the head `branch get` answers (the Commit it was forked at)
    ok(
        &home,
        "branch.fork",
        &[("project", t("shop/api")), ("name", t("fresh"))],
    );
    let head = ok(
        &home,
        "branch.get",
        &[("project", t("shop/api")), ("branch", t("fresh"))],
    );
    let head = text(&head, "commit");
    let mounted = ok(
        &home,
        "workspace.mount",
        &[("sandbox", t("t1")), ("branch", t("fresh"))],
    );
    let fresh = text(&mounted, "workspace");
    ok(
        &home,
        "workspace.exec",
        &[("workspace", t(&fresh)), ("command", t("echo f > F.md"))],
    );
    // dev's head, a Commit that is not fresh's
    let dev = ok(
        &home,
        "branch.get",
        &[("project", t("shop/api")), ("branch", t("dev"))],
    );
    let dev = text(&dev, "commit");
    assert_ne!(dev, head);
    let wrong = layerfs_api::call(
        Some(&home),
        "workspace.commit",
        SdkValue::map([("workspace", t(&fresh)), ("expected", t(&dev))]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(wrong.code, "Conflict", "{wrong:?}");
    let next = wrong.detail.get("next").expect("flat fields");
    assert_eq!(next.get_text("commit"), Some(head.as_str()), "{wrong:?}");
    let committed = ok(
        &home,
        "workspace.commit",
        &[("workspace", t(&fresh)), ("expected", t(&head))],
    );
    assert!(committed.get_text("commit").is_some(), "{committed:?}");
    ok(&home, "workspace.unmount", &[("workspace", t(&fresh))]);

    // #533 review P1-20: two Workspaces write one file; the second commit
    // conflicts, and with `force` publishes over the moved head
    let mut both = Vec::new();
    for content in ["first", "second"] {
        let mounted = ok(
            &home,
            "workspace.mount",
            &[
                ("sandbox", t("t1")),
                ("branch", t("dev")),
                ("name", t(content)),
            ],
        );
        let ws = text(&mounted, "workspace");
        let command = format!("echo {content} > SAME.md");
        ok(
            &home,
            "workspace.exec",
            &[("workspace", t(&ws)), ("command", t(&command))],
        );
        both.push(ws);
    }
    ok(&home, "workspace.commit", &[("workspace", t(&both[0]))]);
    let forced = ok(
        &home,
        "workspace.commit",
        &[("workspace", t(&both[1])), ("force", SdkValue::Bool(true))],
    );
    assert!(forced.get_text("commit").is_some(), "{forced:?}");
    assert_eq!(forced.get("merged"), None, "{forced:?}");
    let same = ok(
        &home,
        "file.read",
        &[
            ("project", t("shop/api")),
            ("branch", t("dev")),
            ("path", t("/SAME.md")),
        ],
    );
    assert_eq!(same.get_text("text"), Some("second\n"));

    // #533 review P1-18: output over the 1 MiB inline window arrives as
    // output events, in full; output that is not UTF-8 stays bytes
    let mut events = Vec::new();
    let big = layerfs_api::call(
        Some(&home),
        "workspace.exec",
        SdkValue::map([
            ("workspace", t(&both[0])),
            (
                "command",
                t("head -c 3000000 /dev/zero | tr '\\0' a; printf '\\377' >&2"),
            ),
            ("detail", SdkValue::Bool(true)),
        ]),
        &mut |event| events.push(event),
    )
    .unwrap();
    // the answer holds the last 1 MiB and names the file that holds all
    let tail = big.get_text("stdout").unwrap_or("");
    assert_eq!(tail.len(), 1 << 20, "{big:?}");
    let file = big
        .get_text("stdout_file")
        .expect("a spilled stream's file");
    assert_eq!(
        std::fs::metadata(file).map(|m| m.len()).ok(),
        Some(3_000_000)
    );
    assert_eq!(big.get_int("stdout_bytes"), Some(3_000_000), "{big:?}");
    assert_eq!(
        big.get("stderr"),
        Some(&SdkValue::Bytes(vec![0xff])),
        "{big:?}"
    );
    let mut streamed = Vec::new();
    for event in &events {
        if event.get_text("stream") == Some("stdout") {
            if let Some(SdkValue::Bytes(bytes)) = event.get("bytes") {
                streamed.extend_from_slice(bytes);
            }
        }
    }
    assert_eq!(streamed.len(), 3_000_000);
    assert!(streamed.iter().all(|b| *b == b'a'));

    // #533 review P1-21 (O31): a Sandbox holding Workspaces is removed
    // only with `force`, which names each one it ended under `unmounted`
    let (code, _) = call(&home, "sandbox.remove", &[("sandbox", t("t1"))]).unwrap_err();
    assert_eq!(code, "InUse");
    let closed = ok(
        &home,
        "sandbox.remove",
        &[("sandbox", t("t1")), ("force", SdkValue::Bool(true))],
    );
    let unmounted = match closed.get("unmounted") {
        Some(SdkValue::List(handles)) => handles.len(),
        other => panic!("{other:?} in {closed:?}"),
    };
    assert_eq!(unmounted, 2, "{closed:?}");
    assert!(!paths::sandbox_dir(&home, "t1").exists());
    // RV5 FAIL-2: its stop ended both Workspaces' mount intents, so
    // nothing it says is left, and their Branch and the Store are free
    assert!(
        matches!(closed.get("left"), None | Some(SdkValue::Null))
            || closed.get("left") == Some(&SdkValue::List(Vec::new())),
        "{closed:?}"
    );
    ok(
        &home,
        "branch.remove",
        &[("project", t("shop/api")), ("branch", t("dev"))],
    );
    ok(&home, "store.compact", &[("store", t("shop"))]);
    ok(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("t1")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ],
    );

    // a Sandbox record keeps its Store from being pushed
    assert_eq!(
        call(
            &home,
            "store.move",
            &[("store", t("shop")), ("machine", t("box"))]
        )
        .unwrap_err()
        .0,
        "InUse"
    );
    ok(&home, "sandbox.remove", &[("sandbox", t("t1"))]);
    assert!(!paths::sandbox_dir(&home, "t1").exists());
    // RV5 FAIL-2: its stop ended both Workspaces' mount intents, so
    // nothing it says is left, and their Branch and the Store are free
    assert!(
        matches!(closed.get("left"), None | Some(SdkValue::Null))
            || closed.get("left") == Some(&SdkValue::List(Vec::new())),
        "{closed:?}"
    );
    ok(
        &home,
        "branch.remove",
        &[("project", t("shop/api")), ("branch", t("dev"))],
    );
    ok(&home, "store.compact", &[("store", t("shop"))]);
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(folder);
}
