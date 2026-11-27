//! `workspace.exec`'s exec id and `workspace.cancel` on a machine's worker
//! (layerfs-project#113, compute-plane 2.0 §5, D-11, call surface 2.0),
//! against a stand-in worker (no VM, container or daemon): control makes
//! the exec id, which is the call's first progress event, the long
//! answer's `exec`, and travels in the `sandbox.host` frame; `layerfs
//! workspace cancel <workspace> [--exec <id>]` rides the same frame and
//! answers what the worker answers; a worker that cannot be reached
//! answers `unknown` and nothing in the home changes.
#![cfg(feature = "machine")]
mod common;
use common::{layerfs, run, tempdir, text};
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::worker::{read_frame, write_frame};
use layerfs_protocol::sdk::SdkValue;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

type Seen = Arc<Mutex<Vec<SdkValue>>>;

/// A stand-in worker: it answers `sandbox.create`, `workspace.exec` and
/// `workspace.cancel`, and keeps every Workspace frame's `more`.
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
            // the machine holds the named image already: nothing is copied
            let image = request.get_text("op") == Some("image.artifact");
            let value = match args.get_text("op") {
                _ if image => SdkValue::map([("present", SdkValue::Bool(true))]),
                Some("workspace.exec") => SdkValue::map([
                    ("exit_code", SdkValue::Int(0)),
                    ("cancelled", SdkValue::Bool(false)),
                    ("stdout", t("")),
                    ("stderr", t("")),
                    ("stdoutBytes", SdkValue::Int(0)),
                    ("stderrBytes", SdkValue::Int(0)),
                    ("exec", more.get("execId").cloned().unwrap_or_default()),
                    ("stop", t("cgroup")),
                    ("duration_ms", SdkValue::Int(3)),
                ]),
                // the command it names, cancelled
                Some("workspace.cancel") => SdkValue::map([(
                    "commands",
                    SdkValue::List(vec![SdkValue::map([
                        ("exec", more.get("execId").cloned().unwrap_or_default()),
                        ("outcome", t("cancelled")),
                        ("signal", t("SIGTERM")),
                    ])]),
                )]),
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
        &SdkValue::map([
            ("name", t("docker")),
            ("platform", t("docker")),
            ("vm_name", t("layerfs-00000000-docker")),
        ]),
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
            ("profile", t("normal")),
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

/// The exec id an exec's first event, `{ phase: "start", exec }`, names.
fn first_id(events: &[SdkValue]) -> Option<String> {
    let progress = events.first()?.get("progress")?;
    assert_eq!(progress.get_text("phase"), Some("start"), "{progress:?}");
    progress.get_text("exec").map(str::to_owned)
}

const ID: &str = "exec-00000000000000e1";

#[test]
fn the_exec_id_is_the_first_event_and_travels_to_the_worker() {
    let seen = Seen::default();
    let home = home_with_sandbox("id", seen.clone());
    let (made, events) = exec(
        &home,
        &[
            ("workspace", t("d1:one")),
            ("command", t("true")),
            ("log_tag", t("dsh:s9:t1")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    let id = first_id(&events).unwrap();
    assert!(id.starts_with("exec-") && id.len() == 21, "{id}");
    assert_eq!(made.get_text("exec"), Some(id.as_str()), "{made:?}");
    assert_eq!(made.get_int("exit_code"), Some(0), "{made:?}");
    assert_eq!(made.get_text("stop"), Some("cgroup"), "{made:?}");
    let (short, events) = exec(&home, &[("workspace", t("d1:one")), ("command", t("true"))]);
    let other = first_id(&events).unwrap();
    assert_ne!(other, id);
    // the short answer of a command that wrote nothing is its code alone
    assert_eq!(layerfs_home::json::print(&short), r#"{"exit_code":0}"#);
    let frames = seen.lock().unwrap().clone();
    let sent: Vec<_> = frames
        .iter()
        .map(|f| f.get("more").unwrap().get_text("execId"))
        .collect();
    assert_eq!(sent, [Some(id.as_str()), Some(other.as_str())]);
    // the call's log tag travels with it, for the record the worker keeps
    // while the command runs (layerfs-project#178); a call with none sends none
    let tags: Vec<_> = frames
        .iter()
        .map(|f| f.get("more").unwrap().get_text("tag"))
        .collect();
    assert_eq!(tags, [Some("dsh:s9:t1"), None]);
    // a caller cannot name the id: the option is gone, refused before
    // anything is sent
    let args = SdkValue::map([
        ("workspace", t("d1:one")),
        ("command", t("true")),
        ("exec_id", t("build-7")),
    ]);
    let refused = layerfs_api::call(Some(&home), "workspace.exec", args, &mut |_| {}).unwrap_err();
    assert_eq!(refused.code, "InvalidInput");
    assert_eq!(seen.lock().unwrap().len(), 2);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cancel_rides_the_sandbox_host_frame() {
    let seen = Seen::default();
    let home = home_with_sandbox("frame", seen.clone());
    let out = run(&mut layerfs(
        &home,
        &["workspace", "cancel", "d1:one", "--exec", ID],
    ));
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout).trim(), r#"{"signal":"SIGTERM"}"#);
    let out = run(&mut layerfs(
        &home,
        &["workspace", "cancel", "d1:one", "--exec", ID, "--detail"],
    ));
    let long = layerfs_home::json::parse(text(&out.stdout).trim()).unwrap();
    assert_eq!(long.get_text("outcome"), Some("cancelled"), "{long:?}");
    let commands = long.get("commands").unwrap();
    let SdkValue::List(commands) = commands else {
        panic!("{long:?}")
    };
    assert_eq!(commands[0].get_text("exec"), Some(ID), "{long:?}");
    let frames = seen.lock().unwrap().clone();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].get_text("op"), Some("workspace.cancel"));
    let more = frames[0].get("more").unwrap();
    assert_eq!(more.get_text("path"), Some("one"));
    assert_eq!(more.get_text("execId"), Some(ID));
    assert!(more.get("graceMs").is_none(), "{more:?}");
    // with no `--exec`, the frame names no command: every one running
    run(&mut layerfs(&home, &["workspace", "cancel", "d1:one"]));
    let frames = seen.lock().unwrap().clone();
    assert_eq!(frames[2].get("more").unwrap().get_text("execId"), None);
    // the help names it as the spec does; the old name is gone
    let help = run(&mut layerfs(&home, &["workspace", "cancel", "--help"]));
    let help = text(&help.stdout) + &text(&help.stderr);
    assert!(
        help.contains("<workspace>") && help.contains("--exec"),
        "{help}"
    );
    let old = run(&mut layerfs(
        &home,
        &["workspace", "exec-cancel", "d1:one", ID],
    ));
    assert_eq!(old.status.code(), Some(2), "{}", text(&old.stderr));
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
        &["workspace", "cancel", "d1:one", "--exec", ID],
    ));
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout).trim(), r#"{"unknown":true}"#);
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
