//! Direct Workspace calls (#535 G4, phase 6) with stand-ins only: no VM,
//! container or daemon runs. A stand-in worker answers `sandbox.create`
//! with its daemon's endpoint and keys, `workspace.exec` and
//! `sandbox.close`, and refuses everything else; a stand-in `ssh` runs as
//! the forward (`layerfs_machine::forward`) until it is ended.
//!
//! What it proves: on `ssh` the create records the forward (address, pid,
//! start and boot) in the home's record and the keys in `control.json`
//! (0600), never in the answer; mount, commit, unmount, status and
//! `sandbox.get`'s list never reach the worker (with no daemon behind the
//! forward they are refused `Unavailable`); exec goes to the worker with
//! the directory its home record names; close tells the worker the
//! Workspaces the home holds and ends the forward. On `docker` the daemon's
//! published port is recorded as it is, with no forward process; on
//! `microsandbox`, whose adapter forwards nothing, every call goes through
//! the worker as before. What it does not prove: a real daemon answering
//! through a real forward (`sandbox_direct_live.rs` dials a host daemon).
//! One test, because it sets the process's PATH.
#![cfg(all(unix, feature = "machine"))]
use layerfs_api::calls;
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_api::worker::{read_frame, write_frame};
use layerfs_bridge::control::SdkValue;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-direct-{tag}-{}", std::process::id()));
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

/// What the stand-in worker was sent: each Sandbox operation's name and
/// its `more` arguments.
type Log = Arc<Mutex<Vec<(String, SdkValue)>>>;

/// A stand-in worker whose Sandbox daemon is at `endpoint` (nothing
/// listens there); it answers `sandbox.create`, `workspace.exec` and
/// `sandbox.close`, and refuses every other operation.
fn worker(endpoint: SocketAddr) -> (SocketAddr, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let log: Log = Arc::default();
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
                // this build's hello: the protocol, and no sealed bundle
                &SdkValue::map([
                    ("ready", SdkValue::Bool(true)),
                    ("protocol", SdkValue::Int(layerfs_api::worker::PROTOCOL)),
                    ("bundle", SdkValue::Null),
                ]),
            )
            .unwrap();
            let Some(request) = read_frame(&mut stream).ok().flatten() else {
                continue;
            };
            let args = request.get("args").cloned().unwrap_or_default();
            let op = match request.get_text("op") {
                Some("sandbox.host") => args.get_text("op").unwrap_or("").to_owned(),
                other => other.unwrap_or("").to_owned(),
            };
            let more = args.get("more").cloned().unwrap_or_default();
            seen.lock().unwrap().push((op.clone(), more.clone()));
            let answer = match op.as_str() {
                "sandbox.create" => SdkValue::map([(
                    "value",
                    SdkValue::map([
                        ("sandbox", more.get("sandbox").cloned().unwrap_or_default()),
                        ("name", t("lfs-x-t1")),
                        ("image", t("local")),
                        ("platform", t("ssh")),
                        ("runtime", t("namespaces")),
                        ("route", t("fuse")),
                        ("runtime_id", t("lfs-x-t1")),
                        (
                            "control",
                            SdkValue::map([
                                ("endpoint", t(&endpoint.to_string())),
                                ("secret", SdkValue::Bytes(vec![1; 32])),
                                ("peer", SdkValue::Bytes(vec![2; 32])),
                                ("instance", SdkValue::Bytes(vec![3; 32])),
                            ]),
                        ),
                    ]),
                )]),
                "workspace.exec" => SdkValue::map([(
                    "value",
                    SdkValue::map([
                        ("kind", t("exit:0")),
                        ("exitCode", SdkValue::Int(0)),
                        ("stdout", t("")),
                        ("stderr", t("")),
                    ]),
                )]),
                "sandbox.close" => SdkValue::map([(
                    "value",
                    SdkValue::map([("kind", t("Closed")), ("unknown", SdkValue::List(vec![]))]),
                )]),
                _ => SdkValue::map([(
                    "refused",
                    SdkValue::map([
                        ("code", t("Failed")),
                        ("message", t(&format!("the stand-in worker refuses {op}"))),
                        ("detail", SdkValue::Null),
                    ]),
                )]),
            };
            let _ = write_frame(&mut stream, &answer);
        }
    });
    (address, log)
}

/// Machine `machine` (`platform`, with `extra` fields) whose worker is at
/// `worker`, holding sqlite Store `store` with project `api`.
fn machine(
    home: &Path,
    machine: &str,
    platform: &str,
    extra: &[(&str, SdkValue)],
    worker: SocketAddr,
    store: &str,
) {
    let mut fields = vec![
        ("name".to_owned(), t(machine)),
        ("platform".to_owned(), t(platform)),
    ];
    fields.extend(extra.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    write_record(
        &paths::machine_record(home, machine),
        &SdkValue::Map(fields),
    )
    .unwrap();
    write_record(
        &paths::worker_record(home, machine),
        &SdkValue::map([("address", t(&worker.to_string())), ("secret", t("5e"))]),
    )
    .unwrap();
    write_record(
        &paths::store_record(home, store),
        &SdkValue::map([
            ("name", t(store)),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t(machine)),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(home, store, "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
}

fn ops(log: &Log) -> Vec<String> {
    log.lock()
        .unwrap()
        .iter()
        .map(|(op, _)| op.clone())
        .collect()
}

fn runs(pid: i64) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .unwrap()
        .success()
}

/// A Workspace record `{ id, name, branch, path, namespace, created }` in
/// Sandbox `sandbox`'s home folder.
fn workspace_record(home: &Path, sandbox: &str, name: &str, path: &str) {
    write_record(
        &paths::sandbox_dir(home, sandbox)
            .join("workspace")
            .join(format!("{}.json", "ab".repeat(32))),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("id", t(&"ab".repeat(32))),
            ("name", t(name)),
            ("branch", t("dev")),
            ("path", t(path)),
            ("namespace", SdkValue::Int(1)),
            ("created", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
}

#[test]
fn workspace_calls_reach_a_forwarded_sandbox_s_daemon_directly() {
    let cache = tempdir("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    // the stand-in `ssh`: `-V` ends at once; a forward runs until ended
    let bin = tempdir("bin");
    let ssh = bin.join("ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\n[ \"$1\" = -V ] && exit 0\ntrap 'kill $! 2>/dev/null; exit 0' TERM\nsleep 60 &\nwait\n",
    )
    .unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::process::Command::new(&ssh).arg("-V").output().unwrap();
    std::env::set_var("PATH", format!("{}:/bin:/usr/bin", bin.display()));

    let home = tempdir("home");
    home::open(&home).unwrap();
    // nothing listens at the daemon's endpoint in the "machine"
    let gone = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let (address, log) = worker(gone);
    machine(
        &home,
        "box",
        "ssh",
        &[(
            "ssh",
            SdkValue::map([
                ("host", t("example.test")),
                ("user", t("u")),
                ("port", SdkValue::Int(22)),
            ]),
        )],
        address,
        "shop",
    );

    // create: the forward and the keys are recorded here, never answered
    let made = call(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("t1")),
            ("machine", t("box")),
        ],
    )
    .unwrap();
    let shown = calls::json::print(&made);
    assert!(
        !shown.contains("secret") && !shown.contains("forward"),
        "{shown}"
    );
    assert_eq!(made.get_text("state"), Some("running"), "{made:?}");
    let dir = paths::sandbox_dir(&home, "t1");
    let record = read_record(&dir.join("record.json")).unwrap().unwrap();
    let forward = record
        .get("daemon")
        .and_then(|d| d.get("forward"))
        .cloned()
        .unwrap();
    let pid = forward.get_int("pid").expect("the forward's pid");
    assert!(runs(pid), "the forward runs");
    assert!(forward.get_text("pid_start").is_some(), "{forward:?}");
    let forwarded: SocketAddr = forward.get_text("address").unwrap().parse().unwrap();
    assert!(forwarded.ip().is_loopback());
    assert_ne!(
        forwarded.port(),
        gone.port(),
        "ssh carries it to another port"
    );
    let keys = dir.join("control.json");
    let mode = std::fs::metadata(&keys).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let keys = read_record(&keys).unwrap().unwrap();
    assert_eq!(keys.get("secret"), Some(&SdkValue::Bytes(vec![1; 32])));
    assert_eq!(
        keys.get_text("endpoint"),
        Some(forwarded.to_string().as_str())
    );
    assert_eq!(ops(&log), ["sandbox.create"]);

    // mount, status, commit, unmount and get's list dial the daemon from
    // here: none reaches the worker; nothing answers behind the forward
    let mounted = call(
        &home,
        "workspace.mount",
        &[("sandbox", t("t1")), ("branch", t("dev"))],
    );
    assert_eq!(mounted.unwrap_err().0, "Unavailable");
    workspace_record(&home, "t1", "one", "/w/one");
    for name in ["workspace.status", "workspace.commit", "workspace.unmount"] {
        let (code, message) = call(&home, name, &[("ws", t("t1:one"))]).unwrap_err();
        assert_eq!(code, "Unavailable", "{name}: {message}");
        assert!(message.contains("sandbox close t1"), "{message}");
    }
    let got = call(&home, "sandbox.get", &[("name", t("t1"))]).unwrap();
    assert!(got.get_text("down_reason").is_some(), "{got:?}");
    assert_eq!(
        ops(&log),
        ["sandbox.create"],
        "no Workspace call reached the worker"
    );

    // exec runs through the Sandbox's runtime, in its worker, in the
    // directory its home record names; one not recorded is refused here
    let ran = call(
        &home,
        "workspace.exec",
        &[("ws", t("t1:one")), ("command", t("true"))],
    )
    .unwrap();
    assert_eq!(ran.get_text("kind"), Some("exit:0"), "{ran:?}");
    let (code, _) = call(
        &home,
        "workspace.exec",
        &[("ws", t("t1:two")), ("command", t("true"))],
    )
    .unwrap_err();
    assert_eq!(code, "NotMounted");
    {
        let log = log.lock().unwrap();
        assert_eq!(log.len(), 2, "{log:?}");
        assert_eq!(log[1].0, "workspace.exec");
        assert_eq!(log[1].1.get_text("directory"), Some("/w/one"));
    }

    // a stopped machine (no worker recorded) is refused before any dial,
    // naming the call that starts it
    let recorded = read_record(&paths::worker_record(&home, "box"))
        .unwrap()
        .unwrap();
    std::fs::remove_file(paths::worker_record(&home, "box")).unwrap();
    let (code, message) = call(&home, "workspace.status", &[("ws", t("t1:one"))]).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("layerfs machine start box"), "{message}");
    write_record(&paths::worker_record(&home, "box"), &recorded).unwrap();

    // close: the worker is told what the home holds (the daemon does not
    // answer, so every recorded Workspace); the forward and the folder go
    call(
        &home,
        "sandbox.close",
        &[("name", t("t1")), ("force", SdkValue::Bool(true))],
    )
    .unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(log[2].0, "sandbox.close");
        assert_eq!(
            log[2].1.get("held"),
            Some(&SdkValue::List(vec![t("/w/one")]))
        );
    }
    assert!(!dir.exists(), "the folder went");
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while runs(pid) && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!runs(pid), "the forward was ended");

    // docker: its Sandbox publishes the daemon's port on this host's
    // loopback, which is recorded as it is, with no forward process
    let published = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let (address, log) = worker(published);
    machine(&home, "docker", "docker", &[], address, "dock");
    call(
        &home,
        "sandbox.create",
        &[
            ("project", t("dock/api")),
            ("name", t("d1")),
            ("machine", t("docker")),
            ("image", t(&format!("sha256:{}", "c".repeat(64)))),
        ],
    )
    .unwrap();
    let record = read_record(&paths::sandbox_record(&home, "d1"))
        .unwrap()
        .unwrap();
    let forward = record
        .get("daemon")
        .and_then(|d| d.get("forward"))
        .cloned()
        .unwrap();
    assert_eq!(
        forward.get_text("address"),
        Some(published.to_string().as_str())
    );
    assert_eq!(forward.get_int("pid"), None);
    let (code, _) = call(&home, "workspace.status", &[("ws", t("d1:/w/x"))]).unwrap_err();
    assert_eq!(code, "Unavailable");
    assert_eq!(ops(&log), ["sandbox.create"]);
    call(&home, "sandbox.close", &[("name", t("d1"))]).unwrap();

    // microsandbox forwards nothing: its Workspace calls go through the
    // worker as before, and it records no keys here
    let (address, log) = worker(gone);
    machine(&home, "micro", "microsandbox", &[], address, "small");
    call(
        &home,
        "sandbox.create",
        &[
            ("project", t("small/api")),
            ("name", t("m1")),
            ("machine", t("micro")),
        ],
    )
    .unwrap();
    let dir = paths::sandbox_dir(&home, "m1");
    assert!(!dir.join("control.json").exists());
    let record = read_record(&dir.join("record.json")).unwrap().unwrap();
    assert!(record.get("daemon").is_none(), "{record:?}");
    let (code, message) = call(&home, "workspace.status", &[("ws", t("m1:/w/x"))]).unwrap_err();
    assert_eq!(code, "Failed", "{message}");
    assert!(message.contains("stand-in worker refuses"), "{message}");
    assert_eq!(
        ops(&log),
        ["sandbox.create", "backend.daemon.workspace.status"]
    );
    call(&home, "sandbox.close", &[("name", t("m1"))]).unwrap();

    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&bin).unwrap();
    std::fs::remove_dir_all(&cache).unwrap();
}
