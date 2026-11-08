//! `workspace.exec`'s exec id and `workspace.exec_cancel` on a machine's
//! worker (layerfs-project#113, compute-plane 2.0 §5, D-11), against a
//! stand-in worker (no VM, container or daemon): the exec id control makes
//! or the caller names is the call's first progress event and travels in
//! the `sandbox.host` frame; `layerfs workspace exec-cancel <ws> <exec-id>
//! [--grace-ms N]` rides the same frame and answers what the worker
//! answers; a worker that cannot be reached answers `unknown` and nothing
//! in the home changes.
#![cfg(feature = "machine")]
mod common;
use common::{layerfs, run, tempdir, text};
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::worker::{read_frame, write_frame};
use layerfs_bridge::control::SdkValue;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

type Seen = Arc<Mutex<Vec<SdkValue>>>;

/// A stand-in worker: it answers `sandbox.create`, `workspace.exec` and
/// `workspace.exec_cancel`, and keeps every Workspace frame's `more`.
fn worker(seen: Seen) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            if read_frame(&mut stream).ok().flatten().is_none() {
                continue;
            }
            let hello = SdkValue::map([
                ("ready", SdkValue::Bool(true)),
                ("protocol", SdkValue::Int(layerfs_api::worker::PROTOCOL)),
                ("bundle", SdkValue::Null),
            ]);
            write_frame(&mut stream, &hello).unwrap();
            let Some(request) = read_frame(&mut stream).ok().flatten() else {
                continue;
            };
            let args = request.get("args").cloned().unwrap_or_default();
            let more = args.get("more").cloned().unwrap_or_default();
            let value = match args.get_text("op") {
                Some("workspace.exec") => SdkValue::map([
                    ("kind", t("exit:0")),
                    ("exitCode", SdkValue::Int(0)),
                    ("stdout", t("")),
                    ("stderr", t("")),
                    ("execId", more.get("execId").cloned().unwrap_or_default()),
                    ("stop", t("cgroup")),
                ]),
                Some("workspace.exec_cancel") => {
                    SdkValue::map([("outcome", t("cancelled")), ("signal", t("SIGTERM"))])
                }
                _ => create(&more),
            };
            if args
                .get_text("op")
                .is_some_and(|op| op.starts_with("workspace."))
            {
                let mut seen = seen.lock().unwrap();
                seen.push(SdkValue::map([
                    ("op", args.get("op").cloned().unwrap()),
                    ("more", more),
                ]));
            }
            let _ = write_frame(&mut stream, &SdkValue::map([("value", value)]));
        }
    });
    address
}

/// `sandbox.create`'s answer; the daemon's port is one nothing listens on.
fn create(more: &SdkValue) -> SdkValue {
    let gone = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let key = |n: u8| SdkValue::Bytes(vec![n; 32]);
    SdkValue::map([
        ("sandbox", more.get("sandbox").cloned().unwrap_or_default()),
        ("name", t("lfs-x-t1")),
        ("image", t("local")),
        ("platform", t("docker")),
        ("runtime", t("namespaces")),
        ("route", t("fuse")),
        ("runtime_id", t("lfs-x-t1")),
        (
            "control",
            SdkValue::map([
                ("endpoint", t(&gone.to_string())),
                ("secret", key(1)),
                ("peer", key(2)),
                ("instance", key(3)),
            ]),
        ),
    ])
}

/// A home with docker machine `docker` and Sandbox `d1`, whose Workspace
/// calls go to the stand-in worker.
fn home_with_sandbox(tag: &str, seen: Seen) -> PathBuf {
    let home = tempdir(&format!("cancelw-{tag}"));
    home::open(&home).unwrap();
    let address = worker(seen);
    write_record(
        &paths::machine_record(&home, "docker"),
        &SdkValue::map([("name", t("docker")), ("platform", t("docker"))]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "docker"),
        &SdkValue::map([("address", t(&address.to_string())), ("secret", t("5e"))]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "dock"),
        &SdkValue::map([
            ("name", t("dock")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t("docker")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(&home, "dock", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    let args = SdkValue::map([
        ("project", t("dock/api")),
        ("name", t("d1")),
        ("machine", t("docker")),
        ("image", t(&format!("sha256:{}", "c".repeat(64)))),
    ]);
    layerfs_api::call(Some(&home), "sandbox.create", args, &mut |_| {})
        .unwrap_or_else(|r| panic!("sandbox.create: {} {}", r.code, r.message));
    home
}

/// `workspace.exec` through the library: its answer and its events.
fn exec(home: &Path, args: &[(&str, SdkValue)]) -> (SdkValue, Vec<SdkValue>) {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    let mut events = Vec::new();
    let answer = layerfs_api::call(Some(home), "workspace.exec", args, &mut |e| events.push(e))
        .unwrap_or_else(|r| panic!("workspace.exec: {} {}", r.code, r.message));
    (answer, events)
}

/// The exec id an exec's first event names.
fn first_id(events: &[SdkValue]) -> Option<String> {
    let exec = events.first()?.get("progress")?.get("exec")?;
    exec.get_text("id").map(str::to_owned)
}

#[test]
fn the_exec_id_is_the_first_event_and_travels_to_the_worker() {
    let seen = Seen::default();
    let home = home_with_sandbox("id", seen.clone());
    let (named, events) = exec(
        &home,
        &[
            ("ws", t("d1:one")),
            ("command", t("true")),
            ("exec_id", t("build-7")),
        ],
    );
    assert_eq!(first_id(&events).as_deref(), Some("build-7"));
    assert_eq!(named.get_text("exec_id"), Some("build-7"));
    let (made, events) = exec(&home, &[("ws", t("d1:one")), ("command", t("true"))]);
    let id = first_id(&events).unwrap();
    assert!(id.starts_with("exec-") && id.len() == 21, "{id}");
    assert_eq!(made.get_text("exec_id"), Some(id.as_str()));
    let frames = seen.lock().unwrap().clone();
    let sent: Vec<_> = frames
        .iter()
        .map(|f| f.get("more").unwrap().get_text("execId"))
        .collect();
    assert_eq!(sent, [Some("build-7"), Some(id.as_str())]);
    // a bad caller-named id is refused before anything is sent
    let args = SdkValue::map([
        ("ws", t("d1:one")),
        ("command", t("true")),
        ("exec_id", t("a/b")),
    ]);
    let refused = layerfs_api::call(Some(&home), "workspace.exec", args, &mut |_| {}).unwrap_err();
    assert_eq!(refused.code, "InvalidInput");
    assert_eq!(seen.lock().unwrap().len(), 2);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn exec_cancel_rides_the_sandbox_host_frame() {
    let seen = Seen::default();
    let home = home_with_sandbox("frame", seen.clone());
    let out = run(&mut layerfs(
        &home,
        &[
            "workspace",
            "exec-cancel",
            "d1:one",
            "e1",
            "--grace-ms",
            "250",
        ],
    ));
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout).trim(),
        r#"{"outcome":"cancelled","signal":"SIGTERM"}"#
    );
    let frames = seen.lock().unwrap().clone();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].get_text("op"), Some("workspace.exec_cancel"));
    let more = frames[0].get("more").unwrap();
    assert_eq!(more.get_text("path"), Some("one"));
    assert_eq!(more.get_text("execId"), Some("e1"));
    assert_eq!(more.get_int("graceMs"), Some(250));
    // the help names it as the spec does
    let help = run(&mut layerfs(&home, &["workspace", "exec-cancel", "--help"]));
    let help = text(&help.stdout) + &text(&help.stderr);
    assert!(
        help.contains("<exec-id>") && help.contains("--grace-ms"),
        "{help}"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_unreachable_worker_answers_unknown_and_changes_nothing() {
    let home = home_with_sandbox("unreached", Seen::default());
    // the worker's port now has nothing listening
    let gone = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    write_record(
        &paths::worker_record(&home, "docker"),
        &SdkValue::map([("address", t(&gone.to_string())), ("secret", t("5e"))]),
    )
    .unwrap();
    let before = listing(&home);
    let out = run(&mut layerfs(
        &home,
        &["workspace", "exec-cancel", "d1:one", "e1"],
    ));
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout).trim(), r#"{"outcome":"unknown"}"#);
    assert_eq!(listing(&home), before);
    std::fs::remove_dir_all(home).unwrap();
}

/// Every file under `home` but its logs and traces, with its bytes.
fn listing(home: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut folders = vec![home.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if matches!(name.as_str(), "log" | "trace" | "lock") || name.ends_with(".lock") {
                continue;
            }
            if path.is_dir() {
                folders.push(path);
            } else {
                found.push((path.clone(), std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    found.sort();
    found
}
