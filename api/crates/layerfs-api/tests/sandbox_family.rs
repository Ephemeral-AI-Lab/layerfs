//! Named Sandboxes and Workspaces (#533 step 12) on `native`: create, the
//! name unique across the home, the wrong-machine refusal, mount, exec,
//! status, commit, unmount and close. The live half needs the macOS (or
//! Linux, as root) daemon at LAYERFS_DAEMON; without it, it says NOT_RUN.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
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
    calls::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
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
            ("from_branch", t("main")),
        ],
    );
    write_record(
        &paths::machine_record(&home, "box"),
        // microsandbox runs on macOS and Linux hosts, so the support table
        // passes this machine on either
        &SdkValue::map([("name", t("box")), ("platform", t("microsandbox"))]),
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
    assert!(message.contains("native"), "{message}");
    assert_eq!(
        call(
            &home,
            "workspace.mount",
            &[("sandbox", t("t1")), ("branch", t("dev"))]
        )
        .unwrap_err()
        .0,
        "NoSuchSandbox"
    );
    assert_eq!(
        call(&home, "workspace.status", &[("ws", t("t1"))])
            .unwrap_err()
            .0,
        "InvalidInput"
    );

    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset, no live Sandbox");
        return;
    };
    assert!(Path::new(&daemon).is_file());
    // the daemon is a record, written once from a 1.1.0 environment (step 17)
    layerfs_api::home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon.clone())]).unwrap();
    let made = ok(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("t1")),
            ("machine", t("native")),
        ],
    );
    assert_eq!(made.get_text("state"), Some("running"));
    // its success dropped the create's intent in the same write (#536 §7.1)
    assert!(made.get("op").is_none(), "{made:?}");
    let record = home::read_record(&paths::sandbox_record(&home, "t1"))
        .unwrap()
        .unwrap();
    assert!(record.get("op").is_none(), "{record:?}");
    let taken = call(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("t1")),
            ("machine", t("native")),
        ],
    );
    assert_eq!(taken.unwrap_err().0, "Exists");
    let listed = ok(&home, "sandbox.list", &[]);
    assert!(calls::json::print(&listed).contains("\"t1\""));

    let mounted = ok(
        &home,
        "workspace.mount",
        &[("sandbox", t("t1")), ("branch", t("dev"))],
    );
    let ws = text(&mounted, "ws");
    let ran = ok(
        &home,
        "workspace.exec",
        &[
            ("ws", t(&ws)),
            ("command", t("cat README.md && echo made > NEW.md")),
        ],
    );
    assert_eq!(ran.get("exit_code"), Some(&SdkValue::Int(0)), "{ran:?}");
    assert_eq!(text(&ran, "stdout"), "in a sandbox\n");
    let status = ok(&home, "workspace.status", &[("ws", t(&ws))]);
    let entries = status.get("overlay").and_then(|o| o.get_int("entries"));
    assert_eq!(entries, Some(1), "one changed path: {status:?}");
    ok(
        &home,
        "workspace.commit",
        &[("ws", t(&ws)), ("message", t("from a sandbox"))],
    );
    let got = ok(&home, "sandbox.get", &[("name", t("t1"))]);
    assert!(calls::json::print(&got).contains(&ws[3..]), "{got:?}");
    ok(&home, "workspace.unmount", &[("ws", t(&ws))]);
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

    // #533 review P1-20: two Workspaces write one file; the second commit
    // conflicts, and with `force` publishes over the moved head
    let mut both = Vec::new();
    for content in ["first", "second"] {
        let mounted = ok(
            &home,
            "workspace.mount",
            &[("sandbox", t("t1")), ("branch", t("dev"))],
        );
        let ws = text(&mounted, "ws");
        let command = format!("echo {content} > SAME.md");
        ok(
            &home,
            "workspace.exec",
            &[("ws", t(&ws)), ("command", t(&command))],
        );
        both.push(ws);
    }
    ok(&home, "workspace.commit", &[("ws", t(&both[0]))]);
    let forced = ok(
        &home,
        "workspace.commit",
        &[("ws", t(&both[1])), ("force", SdkValue::Bool(true))],
    );
    assert_eq!(forced.get_text("kind"), Some("Committed"), "{forced:?}");
    assert_eq!(forced.get("merged"), Some(&SdkValue::Null), "{forced:?}");
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
    let big = calls::call(
        Some(&home),
        "workspace.exec",
        SdkValue::map([
            ("ws", t(&both[0])),
            (
                "command",
                t("head -c 3000000 /dev/zero | tr '\\0' a; printf '\\377' >&2"),
            ),
        ]),
        &mut |event| events.push(event),
    )
    .unwrap();
    assert_eq!(big.get("stdout"), Some(&SdkValue::Null), "{big:?}");
    assert_eq!(big.get_int("stdout_bytes"), Some(3_000_000), "{big:?}");
    assert_eq!(big.get("streamed"), Some(&SdkValue::Bool(true)), "{big:?}");
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

    // #533 review P1-21 (O31): a Sandbox holding Workspaces closes only
    // with `force`, which reports each one it could not drain as Unknown
    let (code, _) = call(&home, "sandbox.close", &[("name", t("t1"))]).unwrap_err();
    assert_eq!(code, "MountsActive");
    let closed = ok(
        &home,
        "sandbox.close",
        &[("name", t("t1")), ("force", SdkValue::Bool(true))],
    );
    let unknown = match closed.get("unknown") {
        Some(SdkValue::List(paths)) => paths.len(),
        other => panic!("{other:?} in {closed:?}"),
    };
    assert_eq!(unknown, 2, "{closed:?}");
    assert!(!paths::sandbox_dir(&home, "t1").exists());
    ok(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("t1")),
            ("machine", t("native")),
        ],
    );

    // a Sandbox record keeps its Store from being pushed
    assert_eq!(
        call(
            &home,
            "store.push",
            &[("name", t("shop")), ("machine", t("box"))]
        )
        .unwrap_err()
        .0,
        "InUse"
    );
    ok(&home, "sandbox.close", &[("name", t("t1"))]);
    assert!(!paths::sandbox_dir(&home, "t1").exists());
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(folder);
}
