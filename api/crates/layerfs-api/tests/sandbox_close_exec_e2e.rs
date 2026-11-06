//! #548 B5 live E2E: `sandbox.close` against a real runtime.
//!
//! Two live tests, each saying NOT_RUN and passing when its settings are
//! missing (the repository's convention for a live test):
//!
//! - `native`: a real host-process Sandbox through the daemon at
//!   `LAYERFS_DAEMON`. It mounts a Branch, execs, commits, then proves a
//!   live exec lease refuses the close (`LeaseActive`), a stale generation
//!   is refused before any side effect (`StaleGeneration`), and the close
//!   that follows records `Closed` (tombstone `reason: operator`) with the
//!   Sandbox folder gone.
//! - `docker`: the same through a real docker worker. It creates the
//!   Sandbox on `machine.update docker --image LAYERFS_TEST_IMAGE`, mounts,
//!   execs, commits, and proves the same two refusals. The close must stop
//!   and delete the runtime: `docker inspect <record id>` succeeds before
//!   the close and fails after it. `LAYERFS_BUNDLE` names the sealed Linux
//!   bundle the worker runs.
//!
//! A live exec lease is real: `workspace.exec` on another thread takes
//! `ExecLease` for the length of the command, and the close reads it from
//! the record. Lease expiry to `Lost`/`Unresolved` is not driven here: it
//! needs the exec owner to die (or its record to be written), and its
//! protocol coverage is `layerfs-api --test sandbox_lifecycle`.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-b5e2e-{tag}-{}", std::process::id()));
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

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// A folder with one file, for Init.
fn source(tag: &str) -> PathBuf {
    let folder = tempdir(tag);
    std::fs::write(folder.join("readme.txt"), "hello\n").unwrap();
    folder
}

/// Sandbox `name`'s record, as written.
fn record(home: &Path, name: &str) -> SdkValue {
    home::read_record(&paths::sandbox_record(home, name))
        .unwrap()
        .unwrap_or_else(|| panic!("no record for {name}"))
}

/// The id of the one live exec lease on `name`, if any.
fn live_lease(home: &Path, name: &str) -> Option<String> {
    let record = home::read_record(&paths::sandbox_record(home, name))
        .unwrap()
        .unwrap();
    match record.get("exec_leases") {
        Some(SdkValue::List(items)) => items
            .first()
            .and_then(|i| i.get_text("id"))
            .map(str::to_owned),
        _ => None,
    }
}

fn wait_for_lease(home: &Path, name: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(id) = live_lease(home, name) {
            return id;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no exec lease appeared on {name}");
}

fn wait_for_no_lease(home: &Path, name: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if live_lease(home, name).is_none() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the exec lease on {name} was not released");
}

/// One command in a mounted Workspace, on this thread.
fn run(home: &Path, ws: &str, command: &str) -> SdkValue {
    ok(
        home,
        "workspace.exec",
        &[("ws", t(ws)), ("command", t(command))],
    )
}

/// Fork `branch`, mount it, run `command`, Commit and unmount.
fn work(home: &Path, project: &str, sandbox: &str, branch: &str, command: &str) {
    ok(
        home,
        "branch.fork",
        &[
            ("project", t(project)),
            ("name", t(branch)),
            ("from_branch", t("main")),
        ],
    );
    let mounted = ok(
        home,
        "workspace.mount",
        &[("sandbox", t(sandbox)), ("branch", t(branch))],
    );
    let ws = mounted.get_text("ws").unwrap().to_owned();
    let ran = run(home, &ws, command);
    assert_eq!(ran.get_int("exit_code"), Some(0), "{ran:?}");
    let committed = ok(
        home,
        "workspace.commit",
        &[("ws", t(&ws)), ("message", t("from a live Sandbox"))],
    );
    assert_eq!(
        committed.get_text("kind"),
        Some("Committed"),
        "{committed:?}"
    );
    ok(home, "workspace.unmount", &[("ws", t(&ws))]);
}

/// The generation CAS and the exec lease, against a live Sandbox: a stale
/// generation is refused with no write, a live exec lease refuses the close
/// `LeaseActive` and changes nothing, and after the exec ends the close wins
/// and records `Closed`.
fn refusals_then_closed(home: &Path, sandbox: &str, branch: &str) {
    let mounted = ok(
        home,
        "workspace.mount",
        &[("sandbox", t(sandbox)), ("branch", t(branch))],
    );
    let ws = mounted.get_text("ws").unwrap().to_owned();
    let generation = record(home, sandbox).get_int("generation").unwrap();
    let exec_home = home.to_owned();
    let exec_ws = ws.clone();
    let exec = std::thread::spawn(move || {
        call(
            &exec_home,
            "workspace.exec",
            &[("ws", t(&exec_ws)), ("command", t("sleep 4; echo live"))],
        )
    });
    let lease = wait_for_lease(home, sandbox);

    // A live exec lease: the record is Busy, and the close may not kill it.
    let got = ok(home, "sandbox.get", &[("name", t(sandbox))]);
    assert_eq!(got.get_text("state"), Some("busy"), "{got:?}");

    // A stale generation aborts before any side effect.
    let (code, message) = call(
        home,
        "sandbox.close",
        &[
            ("name", t(sandbox)),
            ("generation", SdkValue::Int(generation + 7)),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "StaleGeneration", "{message}");
    assert_eq!(
        record(home, sandbox).get_int("generation"),
        Some(generation)
    );
    assert!(record(home, sandbox).get("op").is_none(), "no close intent");

    // The live lease holds the close; nothing is killed or written.
    let (code, message) = call(home, "sandbox.close", &[("name", t(sandbox))]).unwrap_err();
    assert_eq!(code, "LeaseActive", "{message}");
    assert!(message.contains(lease.as_str()), "{message}");
    assert!(paths::sandbox_dir(home, sandbox).exists());
    assert_eq!(
        record(home, sandbox).get_int("generation"),
        Some(generation)
    );
    assert!(record(home, sandbox).get("op").is_none(), "no close intent");

    let ran = exec
        .join()
        .unwrap()
        .unwrap_or_else(|e| panic!("exec: {e:?}"));
    assert_eq!(ran.get_int("exit_code"), Some(0), "{ran:?}");
    wait_for_no_lease(home, sandbox);
    ok(home, "workspace.unmount", &[("ws", t(&ws))]);

    let closed = ok(home, "sandbox.close", &[("name", t(sandbox))]);
    assert_eq!(closed.get_text("closed"), Some(sandbox), "{closed:?}");
    assert!(
        !paths::sandbox_dir(home, sandbox).exists(),
        "the Sandbox folder is gone"
    );
    let tomb = home::read_record(&home.join("sandbox").join(format!("{sandbox}.closed.json")))
        .unwrap()
        .unwrap_or_else(|| panic!("no tombstone"));
    assert_eq!(tomb.get_text("reason"), Some("operator"), "{tomb:?}");
    assert_eq!(tomb.get_int("generation"), Some(generation), "{tomb:?}");
    assert_eq!(
        tomb.get_int("draining_generation"),
        Some(generation + 1),
        "{tomb:?}"
    );
    // A replay is a quiet no-op naming the same Closed outcome.
    let replay = ok(home, "sandbox.close", &[("name", t(sandbox))]);
    assert!(replay.get_bool("quiet"), "{replay:?}");
    assert_eq!(replay.get_text("reason"), Some("operator"), "{replay:?}");
}

#[test]
fn a_live_sandbox_on_native_closes_only_after_its_exec_lease_ends() {
    let Some(daemon) = env("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset, no live Sandbox");
        return;
    };
    assert!(Path::new(&daemon).is_file(), "{daemon}");
    let home = tempdir("native");
    let folder = source("native-src");
    home::open(&home).unwrap();
    layerfs_api::home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon)]).unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    ok(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/app")),
            ("name", t("n")),
            ("machine", t("native")),
        ],
    );
    work(
        &home,
        "shop/app",
        "n",
        "lane",
        "cat readme.txt && echo native.txt > native.txt",
    );
    refusals_then_closed(&home, "n", "lane");
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_live_sandbox_on_docker_closes_only_after_its_exec_lease_ends() {
    let (Some(image), Some(bundle)) = (env("LAYERFS_TEST_IMAGE"), env("LAYERFS_BUNDLE")) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_IMAGE / LAYERFS_BUNDLE unset");
        return;
    };
    assert!(Path::new(&bundle).is_dir(), "{bundle}");
    let home = tempdir("docker");
    let folder = source("docker-src");
    let home_id = home::open(&home).unwrap().id;
    ok(
        &home,
        "machine.update",
        &[("name", t("docker")), ("image", t(&image))],
    );
    ok(&home, "machine.start", &[("name", t("docker"))]);
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    let moved = ok(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("docker"))],
    );
    assert_eq!(moved.get_text("to"), Some("docker"), "{moved:?}");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let created = ok(
            &home,
            "sandbox.create",
            &[
                ("project", t("shop/app")),
                ("name", t("d")),
                ("machine", t("docker")),
            ],
        );
        assert_eq!(created.get_text("runtime"), Some("docker"), "{created:?}");
        work(
            &home,
            "shop/app",
            "d",
            "lane",
            "cat readme.txt && echo docker.txt > docker.txt",
        );
        // the record `id` is the Sandbox name: `d`; `runtime_id` is the container
        let container = record(&home, "d")
            .get_text("runtime_id")
            .unwrap()
            .to_owned();
        assert!(
            docker(&["inspect", &container]),
            "a live container {container}"
        );
        refusals_then_closed(&home, "d", "lane");
        assert!(
            !docker(&["inspect", &container]),
            "the container {container} is deleted"
        );
    }));
    // this test's own worker container and its data volume, whatever happened
    if paths::sandbox_dir(&home, "d").exists() {
        let _ = call(
            &home,
            "sandbox.close",
            &[("name", t("d")), ("force", SdkValue::Bool(true))],
        );
    }
    let worker = format!("lfs-{}-docker", &home_id[..8]);
    let volume = format!("{worker}-data");
    for args in [
        ["rm", "-f", worker.as_str()],
        ["volume", "rm", volume.as_str()],
    ] {
        let _ = std::process::Command::new("docker").args(args).output();
    }
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// Whether `docker <args>` exits 0.
fn docker(args: &[&str]) -> bool {
    std::process::Command::new("docker")
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
