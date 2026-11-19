//! A Sandbox reached directly (#535 G4) against a real daemon: a native
//! (macOS host process) Sandbox stands in for a machine's, its record made
//! a forwarded one on Lima machine `box` whose port nothing carries (the
//! daemon's own loopback port, as `forward` answers on `native`), and whose
//! worker is a stand-in that refuses every call. Mount, status, a Commit of
//! a file written into the Workspace, `sandbox.get`'s list and unmount then
//! run from this process against the home folder, and no call is sent to
//! the worker (the get dials it for overlay sizes, layerfs-project#164; the
//! stand-in's hello stops it before a call). The Sandbox is created and closed by helper processes (this
//! test binary run again), so this process opens it only through its
//! forwarded record. It does not prove an `ssh -L` carries the port, nor
//! a guest runtime (exec stays on the worker; `sandbox_direct.rs`).
//! Needs the macOS daemon at LAYERFS_DAEMON; without it, NOT_RUN.
#![cfg(all(target_os = "macos", feature = "machine"))]
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_api::worker::{read_frame, write_frame};
use layerfs_bridge::control::SdkValue;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The helper's home and action, when this binary runs as one.
const HELPER: &str = "LFS_DIRECT_HELPER";

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

/// Runs `helper` in a new process of this test binary, on `home`.
fn in_helper(home: &Path, action: &str) {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["helper", "--exact", "--nocapture"])
        .env(HELPER, format!("{action}:{}", home.display()))
        .status()
        .unwrap();
    assert!(status.success(), "helper {action}");
}

/// The helper: creates (`create`) or closes (`close`, forced, so a failed
/// run leaves no daemon) Sandbox `t1` on `native`. Run alone, it does
/// nothing.
#[test]
fn helper() {
    let Ok(value) = std::env::var(HELPER) else {
        return;
    };
    let (action, home) = value.split_once(':').unwrap();
    let home = Path::new(home);
    match action {
        "create" => {
            ok(
                home,
                "sandbox.create",
                &[
                    ("project", t("shop/api")),
                    ("name", t("t1")),
                    ("machine", t("native")),
                ],
            );
        }
        _ => {
            ok(
                home,
                "sandbox.remove",
                &[("sandbox", t("t1")), ("force", SdkValue::Bool(true))],
            );
        }
    }
}

/// A stand-in worker that refuses every call and counts them.
fn refusing_worker() -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            if read_frame(&mut stream).ok().flatten().is_none() {
                continue;
            }
            write_frame(
                &mut stream,
                &SdkValue::map([("ready", SdkValue::Bool(true))]),
            )
            .unwrap();
            let request = read_frame(&mut stream).ok().flatten().unwrap_or_default();
            seen.lock()
                .unwrap()
                .push(layerfs_api::json::print(&request));
            let refused = SdkValue::map([(
                "refused",
                SdkValue::map([
                    ("code", t("Failed")),
                    ("message", t("the stand-in worker refuses every call")),
                    ("detail", SdkValue::Null),
                ]),
            )]);
            let _ = write_frame(&mut stream, &refused);
        }
    });
    (address, log)
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-dlive-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

#[test]
fn a_forwarded_sandbox_s_workspace_calls_dial_its_daemon_from_here() {
    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset, no live Sandbox");
        return;
    };
    let home = tempdir("home");
    home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "direct\n").unwrap();
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
    // the native machine's daemon is a setting of its record
    layerfs_api::call(
        Some(&home),
        "machine.update",
        SdkValue::map([
            ("machine", SdkValue::text("native")),
            ("daemon", SdkValue::text(daemon.as_str())),
        ]),
        &mut |_| {},
    )
    .unwrap();
    in_helper(&home, "create");

    // the record of a Sandbox on machine `box` whose daemon's port this
    // host reaches as it is
    let (address, log) = refusing_worker();
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-vm")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address.to_string())), ("secret", t("5e"))]),
    )
    .unwrap();
    let path = paths::sandbox_record(&home, "t1");
    let native = read_record(&path).unwrap().unwrap();
    let keys = read_record(&paths::sandbox_dir(&home, "t1").join("control.json"))
        .unwrap()
        .unwrap();
    let endpoint = keys.get_text("endpoint").unwrap().to_owned();
    let daemon = native.get("daemon").unwrap();
    let copied = |name: &str| daemon.get(name).cloned().unwrap_or_default();
    let forwarded = home::with_field(native.clone(), "machine", t("box"));
    let forwarded = home::with_field(
        forwarded,
        "daemon",
        SdkValue::map([
            (
                "forward",
                SdkValue::map([("address", t(&endpoint)), ("machine", t("box"))]),
            ),
            ("name", copied("name")),
            ("runtime", copied("runtime")),
            ("platform", copied("platform")),
            ("image", copied("image")),
        ]),
    );
    write_record(&path, &forwarded).unwrap();

    let result = std::panic::catch_unwind(|| {
        let mounted = ok(
            &home,
            "workspace.mount",
            &[
                ("sandbox", t("t1")),
                ("branch", t("dev")),
                ("name", t("one")),
            ],
        );
        assert_eq!(mounted.get_text("workspace"), Some("t1:one"), "{mounted:?}");
        let directory = mounted.get_text("path").unwrap().to_owned();
        // its record is in the home's folder
        let records = std::fs::read_dir(paths::sandbox_dir(&home, "t1").join("workspace"))
            .unwrap()
            .count();
        assert!(records >= 1);
        // an unchanged fresh fork names its head: the Layer it was forked at
        let same = ok(&home, "workspace.commit", &[("workspace", t("t1:one"))]);
        assert!(same.get_text("commit").is_some(), "{same:?}");
        assert_eq!(
            same.get("unchanged"),
            Some(&SdkValue::Bool(true)),
            "{same:?}"
        );
        std::fs::write(Path::new(&directory).join("NEW.md"), "made\n").unwrap();
        // `branch` and `branch_head` are in the long form only
        let status = ok(
            &home,
            "workspace.status",
            &[("workspace", t("t1:one")), ("detail", SdkValue::Bool(true))],
        );
        assert_eq!(status.get_text("branch"), Some("dev"), "{status:?}");
        assert!(status.get("branch_head").is_some(), "{status:?}");
        let committed = ok(
            &home,
            "workspace.commit",
            &[("workspace", t("t1:one")), ("message", t("direct"))],
        );
        // a new Commit: its id, and no `unchanged`
        assert!(committed.get_text("commit").is_some(), "{committed:?}");
        assert!(committed.get("unchanged").is_none(), "{committed:?}");
        // no Workspace call so far went near the worker
        let dialled = || log.lock().unwrap().clone();
        assert!(
            dialled().is_empty(),
            "the worker was called: {:?}",
            dialled()
        );
        let got = ok(&home, "sandbox.get", &[("sandbox", t("t1"))]);
        assert!(layerfs_api::json::print(&got).contains("t1:one"), "{got:?}");
        // the get's one overlay-size inspect dials the machine's worker
        // (layerfs-project#164); the stand-in's hello names no protocol, so
        // no call is sent on it, and the list is answered anyway
        let by_get = dialled();
        assert!(by_get.len() <= 1, "{by_get:?}");
        assert!(
            by_get.iter().all(|request| request == "null"),
            "the worker was called: {by_get:?}"
        );
        ok(&home, "workspace.unmount", &[("workspace", t("t1:one"))]);
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
        by_get
    });
    // the Sandbox is closed as the native one it is, whatever happened
    write_record(&path, &native).unwrap();
    in_helper(&home, "close");
    let sent = log.lock().unwrap().clone();
    let by_get = match result {
        Ok(by_get) => by_get,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    // nothing after the get dialled the worker, the close included
    assert_eq!(sent, by_get, "the worker was called");
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
}
