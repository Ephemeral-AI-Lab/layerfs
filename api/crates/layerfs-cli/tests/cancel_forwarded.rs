//! Command records of a Sandbox this host reaches directly (#535 G4,
//! layerfs-project#113 gaps 3 and 4, call surface 2.0), against a stand-in
//! worker: no VM, container or daemon runs. The home keeps those records
//! with route `machine:<name>`; the command runs in the machine's worker.
//!
//! - A stale one (its owner gone) is ended in the worker, through the same
//!   `workspace.cancel` op in `sandbox.host` with zero grace, before it is
//!   removed and logged: by `workspace.cancel`, `workspace.unmount` and
//!   `sandbox.remove`. A worker that answers the Sandbox does not run there
//!   confirms it ended.
//! - A worker that cannot be reached keeps it as it was: the cancel answers
//!   `unknown`, unmount and remove are refused `Unknown`.
//! - A worker that takes a cancel and never answers leaves the call waiting
//!   until the connection closes (no operation timeout, by design); then it
//!   answers `unknown`, and the record is kept.
//! - A cancel before the command runs (its record has no stop handle yet)
//!   is answered `cancelled` (SIGKILL) and only marked; when the worker
//!   tells that it started, the owner ends it there with zero grace.
//! - A cancel of an id with no record yet leaves a marker and answers
//!   `already_ended`.
#![cfg(all(unix, feature = "machine"))]
mod common;
use common::{layerfs, run, tempdir, text};
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_api::worker::{read_frame, write_frame};
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::process::LocalProcess;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// What the stand-in does with a `workspace.cancel`.
#[derive(Clone)]
enum Cancel {
    /// Answers `{ outcome: "cancelled", signal: "SIGKILL" }`.
    Cancelled,
    /// Refuses it `NoSuchSandbox`.
    NoSandbox,
    /// Reads it, answers nothing, and closes the connection after this.
    Silent(Duration),
}

type Seen = Arc<Mutex<Vec<SdkValue>>>;

/// A stand-in worker for machine `box`: it keeps each `workspace.*`
/// frame's `more`, answers `workspace.exec` with exit 0 and a cancel as
/// `cancel` says.
fn worker(seen: Seen, cancel: Cancel) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
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
            let op = args.get_text("op").unwrap_or("").to_owned();
            seen.lock()
                .unwrap()
                .push(SdkValue::map([("op", t(&op)), ("more", more.clone())]));
            let answer = match (op.as_str(), &cancel) {
                ("workspace.exec", _) => SdkValue::map([(
                    "value",
                    exec_result(0, more.get_text("execId").unwrap_or("")),
                )]),
                ("workspace.cancel", Cancel::Cancelled) => SdkValue::map([(
                    "value",
                    SdkValue::map([("outcome", t("cancelled")), ("signal", t("SIGKILL"))]),
                )]),
                ("workspace.cancel", Cancel::NoSandbox) => refusal("NoSuchSandbox"),
                ("workspace.cancel", Cancel::Silent(hold)) => {
                    // the request is in; nothing is answered
                    std::thread::sleep(*hold);
                    continue;
                }
                _ => refusal("Failed"),
            };
            let _ = write_frame(&mut stream, &answer);
        }
    });
    address
}

/// The side's result of a command that wrote nothing and exited `code`.
fn exec_result(code: i64, id: &str) -> SdkValue {
    SdkValue::map([
        ("exit_code", SdkValue::Int(code)),
        ("cancelled", SdkValue::Bool(false)),
        ("stdout", t("")),
        ("stderr", t("")),
        ("stdoutBytes", SdkValue::Int(0)),
        ("stderrBytes", SdkValue::Int(0)),
        ("exec", t(id)),
        ("stop", t("cgroup")),
    ])
}

fn refusal(code: &str) -> SdkValue {
    SdkValue::map([(
        "refused",
        SdkValue::map([("code", t(code)), ("message", t("stand-in"))]),
    )])
}

/// A stand-in worker that holds `workspace.exec` until `go`: it tells
/// `arrived` that the exec came, waits for `go`, sends the
/// `{ exec_started: { stop } }` event, then waits for a cancel of that id
/// (each connection on a thread of its own) and answers exit 137, as a
/// command ended by SIGKILL does; without one it answers exit 0. A cancel
/// is answered `cancelled`. Every wait is bounded.
fn holding(seen: Seen, arrived: Sender<()>, go: Receiver<()>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let go = Arc::new(Mutex::new(go));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let (seen, arrived, go) = (seen.clone(), arrived.clone(), go.clone());
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                if read_frame(&mut stream).ok().flatten().is_none() {
                    return;
                }
                let hello = SdkValue::map([
                    ("ready", SdkValue::Bool(true)),
                    ("protocol", SdkValue::Int(layerfs_api::worker::PROTOCOL)),
                    ("bundle", SdkValue::Null),
                ]);
                write_frame(&mut stream, &hello).unwrap();
                let Some(request) = read_frame(&mut stream).ok().flatten() else {
                    return;
                };
                let args = request.get("args").cloned().unwrap_or_default();
                let more = args.get("more").cloned().unwrap_or_default();
                let op = args.get_text("op").unwrap_or("").to_owned();
                seen.lock()
                    .unwrap()
                    .push(SdkValue::map([("op", t(&op)), ("more", more.clone())]));
                let id = more.get_text("execId").unwrap_or("").to_owned();
                let answer = match op.as_str() {
                    "workspace.exec" => {
                        arrived.send(()).unwrap();
                        let wait = Duration::from_secs(30);
                        go.lock().unwrap().recv_timeout(wait).unwrap();
                        let path = format!("/sys/fs/cgroup/layerfs/b1/ws-{WS}/exec-{id}");
                        let stop = SdkValue::map([("cgroup", SdkValue::map([("path", t(&path))]))]);
                        let started = SdkValue::map([("stop", stop)]);
                        let event = SdkValue::map([("exec_started", started)]);
                        write_frame(&mut stream, &SdkValue::map([("event", event)])).unwrap();
                        let until = Instant::now() + wait;
                        let killed = loop {
                            if cancels(&seen).iter().any(|(of, ..)| *of == id) {
                                break true;
                            }
                            if Instant::now() > until {
                                break false;
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        };
                        let code = if killed { 137 } else { 0 };
                        SdkValue::map([("value", exec_result(code, &id))])
                    }
                    "workspace.cancel" => SdkValue::map([(
                        "value",
                        SdkValue::map([("outcome", t("cancelled")), ("signal", t("SIGKILL"))]),
                    )]),
                    _ => refusal("Failed"),
                };
                let _ = write_frame(&mut stream, &answer);
            });
        }
    });
    address
}

/// An address nothing listens on.
fn gone() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

const WS: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A home with Lima machine `box` (worker at `worker`) and Sandbox `b1`,
/// whose daemon this host reaches directly (at a port nothing carries) and
/// whose Workspace `one` (id [`WS`]) is recorded here.
fn home_with(tag: &str, worker: SocketAddr) -> PathBuf {
    let home = tempdir(&format!("fwd-{tag}"));
    home::open(&home).unwrap();
    let record = |path: PathBuf, value: SdkValue| write_record(&path, &value).unwrap();
    record(
        paths::machine_record(&home, "box"),
        // a 2.0 record names its VM: one without is an earlier build's,
        // whose Sandboxes a remove forgets without reaching the worker
        SdkValue::map([
            ("name", t("box")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-00000000-box")),
        ]),
    );
    record(
        paths::worker_record(&home, "box"),
        SdkValue::map([("address", t(&worker.to_string())), ("secret", t("5e"))]),
    );
    record(
        paths::store_record(&home, "dock"),
        SdkValue::map([
            ("name", t("dock")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t("box")),
        ]),
    );
    record(
        paths::project_record(&home, "dock", "api"),
        SdkValue::map([("name", t("api"))]),
    );
    record(
        paths::sandbox_record(&home, "b1"),
        SdkValue::map([
            ("name", t("b1")),
            ("id", t("b1")),
            ("project", t("dock/api")),
            ("machine", t("box")),
            (
                "daemon",
                SdkValue::map([
                    (
                        "forward",
                        SdkValue::map([("address", t(&gone().to_string())), ("machine", t("box"))]),
                    ),
                    ("runtime", t("namespaces")),
                    ("platform", t("lima")),
                ]),
            ),
        ]),
    );
    record(
        paths::sandbox_dir(&home, "b1").join(format!("workspace/{WS}.json")),
        SdkValue::map([("id", t(WS)), ("path", t("/w/1")), ("name", t("one"))]),
    );
    home
}

/// The record file of command `id` in Workspace `one`.
fn exec_file(home: &Path, id: &str) -> PathBuf {
    paths::sandbox_dir(home, "b1").join(format!("workspace/{WS}/exec/{id}.json"))
}

/// Writes the record of command `id` on machine `box`, owned by `owner`.
fn exec_record(home: &Path, id: &str, owner: SdkValue) -> String {
    let file = exec_file(home, id);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let path = format!("/sys/fs/cgroup/layerfs/b1/ws-{WS}/exec-{id}");
    write_record(
        &file,
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("id", t(id)),
            ("workspace", t(WS)),
            ("path", t("/w/1")),
            ("name", t("one")),
            ("owner", owner),
            ("route", t("machine:box")),
            (
                "stop",
                SdkValue::map([("cgroup", SdkValue::map([("path", t(&path))]))]),
            ),
            ("started", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
    std::fs::read_to_string(&file).unwrap()
}

/// This process, as a record names its owner; with `alive` false, a start
/// time it never had, so the record is stale.
fn owner(alive: bool) -> SdkValue {
    let me = LocalProcess::observe(std::process::id()).unwrap();
    let start = match alive {
        true => me.start.to_string(),
        false => "1".to_owned(),
    };
    SdkValue::map([
        ("pid", SdkValue::Int(i64::from(me.pid))),
        ("pid_start", t(&start)),
        ("pid_boot", t(&format!("{:032x}", me.boot.unwrap_or(0)))),
    ])
}

/// The cancels the stand-in was sent: (execId, graceMs, workspace).
fn cancels(seen: &Seen) -> Vec<(String, Option<i64>, String)> {
    let frames = seen.lock().unwrap().clone();
    frames
        .iter()
        .filter(|f| f.get_text("op") == Some("workspace.cancel"))
        .map(|f| {
            let more = f.get("more").unwrap();
            (
                more.get_text("execId").unwrap_or("").to_owned(),
                more.get_int("graceMs"),
                more.get_text("workspace").unwrap_or("").to_owned(),
            )
        })
        .collect()
}

/// `layerfs <words>`: exit code, stdout and stderr.
fn cli(home: &Path, words: &[&str]) -> (Option<i32>, String, String) {
    let out = run(&mut layerfs(home, words));
    (out.status.code(), text(&out.stdout), text(&out.stderr))
}

fn log(home: &Path) -> String {
    std::fs::read_to_string(home.join("log/exec.jsonl")).unwrap_or_default()
}

/// `layerfs workspace cancel b1:one --exec <id>`: exit code, stdout, stderr.
fn cancel(home: &Path, id: &str) -> (Option<i32>, String, String) {
    cli(home, &["workspace", "cancel", "b1:one", "--exec", id])
}

/// The one command record of Workspace `one`, waited for (bounded).
fn only_exec(home: &Path) -> String {
    let folder = exec_file(home, "x").parent().unwrap().to_owned();
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let names: Vec<String> = std::fs::read_dir(&folder)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".json").map(str::to_owned)
            })
            .collect();
        if let [one] = names.as_slice() {
            return one.clone();
        }
        assert!(Instant::now() < until, "no one command record: {names:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_stale_forwarded_record_is_ended_in_its_worker_then_removed() {
    let seen = Seen::default();
    let home = home_with("ended", worker(seen.clone(), Cancel::Cancelled));
    let zero = |id: &str| (id.to_owned(), Some(0), WS.to_owned());

    // workspace.cancel
    exec_record(&home, "e1", owner(false));
    let (code, out, err) = cancel(&home, "e1");
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out.trim(), r#"{"signal":"SIGKILL"}"#);
    assert_eq!(cancels(&seen), [zero("e1")]);
    assert!(!exec_file(&home, "e1").exists());
    let logged = log(&home);
    assert!(
        logged.contains("\"exec\":\"e1\"") && logged.contains("\"kept\":false"),
        "{logged}"
    );

    // with no --exec, every command of the Workspace: here the stale one
    exec_record(&home, "e4", owner(false));
    let (code, out, err) = cli(&home, &["workspace", "cancel", "b1:one", "--detail"]);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.contains(r#""exec":"e4""#), "{out}");
    assert_eq!(cancels(&seen), [zero("e1"), zero("e4")]);
    assert!(!exec_file(&home, "e4").exists());

    // workspace.unmount ends it before it goes on to the daemon (which
    // nothing carries here, so the unmount itself then fails)
    exec_record(&home, "e2", owner(false));
    let _ = cli(&home, &["workspace", "unmount", "b1:one"]);
    assert_eq!(cancels(&seen), [zero("e1"), zero("e4"), zero("e2")]);
    assert!(!exec_file(&home, "e2").exists());

    // sandbox.remove too
    exec_record(&home, "e3", owner(false));
    let _ = cli(&home, &["sandbox", "remove", "b1"]);
    assert_eq!(cancels(&seen).last(), Some(&zero("e3")));
    assert!(!exec_file(&home, "e3").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_worker_without_the_sandbox_confirms_its_stale_command_ended() {
    let seen = Seen::default();
    let home = home_with("nosandbox", worker(seen.clone(), Cancel::NoSandbox));
    exec_record(&home, "e7", owner(false));
    let (code, out, err) = cancel(&home, "e7");
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out.trim(), r#"{"already_ended":true}"#);
    assert_eq!(cancels(&seen).len(), 1);
    assert!(!exec_file(&home, "e7").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_worker_not_reached_keeps_the_stale_record_as_it_was() {
    let home = home_with("unreached", gone());
    let bytes = exec_record(&home, "e1", owner(false));
    let kept = || std::fs::read_to_string(exec_file(&home, "e1")).unwrap();
    let (code, out, err) = cancel(&home, "e1");
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out.trim(), r#"{"unknown":true}"#);
    assert_eq!(kept(), bytes);
    assert!(log(&home).contains("\"kept\":true"), "{}", log(&home));
    // unmount and remove are refused Unknown, naming it; nothing changed
    for words in [
        ["workspace", "unmount", "b1:one"].as_slice(),
        ["workspace", "unmount", "b1:one", "--force"].as_slice(),
        ["sandbox", "remove", "b1"].as_slice(),
        ["sandbox", "remove", "b1", "--force"].as_slice(),
    ] {
        // the commands it names are in the refusal's long form
        let words = [words, &["--detail"]].concat();
        let (code, _, err) = cli(&home, &words);
        assert_eq!(code, Some(125), "{words:?}: {err}");
        assert!(
            err.contains("\"error\":\"Unknown\"") && err.contains("\"execs\":[\"e1\"]"),
            "{words:?}: {err}"
        );
        assert_eq!(kept(), bytes, "{words:?}");
    }
    let sandbox = read_record(&paths::sandbox_record(&home, "b1"))
        .unwrap()
        .unwrap();
    assert!(sandbox.get("op").is_none(), "{sandbox:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_worker_that_never_answers_leaves_the_cancel_waiting_until_it_closes() {
    let hold = Duration::from_secs(3);
    let seen = Seen::default();
    let home = home_with("silent", worker(seen.clone(), Cancel::Silent(hold)));
    // a live command (this process owns its record) and a stale one
    exec_record(&home, "live", owner(true));
    let stale = exec_record(&home, "stale", owner(false));
    for id in ["live", "stale"] {
        let started = Instant::now();
        let mut child = layerfs(&home, &["workspace", "cancel", "b1:one", "--exec", id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // the request is in and unanswered: the call still waits
        std::thread::sleep(hold / 2);
        assert!(child.try_wait().unwrap().is_none(), "{id}: it did not wait");
        // once the worker closes the connection it answers `unknown`
        let out = common::finish(child);
        assert!(started.elapsed() >= hold, "{id}");
        assert_eq!(out.status.code(), Some(0), "{id}: {}", text(&out.stderr));
        assert_eq!(text(&out.stdout).trim(), r#"{"unknown":true}"#, "{id}");
    }
    assert_eq!(cancels(&seen).len(), 2);
    // the live record is kept, marked by the cancel it was sent
    let live = read_record(&exec_file(&home, "live")).unwrap().unwrap();
    assert!(live.get_int("cancel").is_some(), "{live:?}");
    // the stale one is kept as it was
    assert_eq!(
        std::fs::read_to_string(exec_file(&home, "stale")).unwrap(),
        stale
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_cancel_before_the_command_starts_is_answered_cancelled_and_its_owner_ends_it() {
    let seen = Seen::default();
    let (arrived, came) = channel();
    let (go, held) = channel();
    let home = home_with("early", holding(seen.clone(), arrived, held));
    let child = layerfs(&home, &["workspace", "exec", "b1:one", "sleep 600"])
        .env("LAYERFS_LOGS_TAG", "dsh:s9:t1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // the exec is in the worker and has not started: no stop handle yet
    came.recv_timeout(Duration::from_secs(30)).unwrap();
    let id = only_exec(&home);
    assert!(id.starts_with("exec-") && id.len() == 21, "{id}");
    let recorded = read_record(&exec_file(&home, &id)).unwrap().unwrap();
    assert_eq!(recorded.get("stop"), Some(&SdkValue::Null), "{recorded:?}");
    // the home's record names the call's log tag and the command's
    // program, nothing more of the command (layerfs-project#178)
    assert_eq!(recorded.get_text("tag"), Some("dsh:s9:t1"), "{recorded:?}");
    assert_eq!(recorded.get_text("program"), Some("sleep"), "{recorded:?}");
    // the cancel, of every command, is answered at once, and only marks it
    let (code, out, err) = cli(&home, &["workspace", "cancel", "b1:one"]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out.trim(), r#"{"signal":"SIGKILL"}"#);
    assert!(cancels(&seen).is_empty(), "nothing was sent to the worker");
    let marked = read_record(&exec_file(&home, &id)).unwrap().unwrap();
    assert!(marked.get_int("cancel").is_some(), "{marked:?}");
    // the command starts: its owner ends it there, with zero grace
    go.send(()).unwrap();
    let ran = common::finish(child);
    let (out, err) = (text(&ran.stdout), text(&ran.stderr));
    assert_eq!(cancels(&seen), [(id.clone(), Some(0), WS.to_owned())]);
    assert!(
        out.contains("\"cancelled\":true") && out.contains("\"exit_code\":137"),
        "{:?}: {out} {err}",
        ran.status.code()
    );
    // the shell exits with the command's code
    assert_eq!(ran.status.code(), Some(137));
    assert!(!exec_file(&home, &id).exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_cancel_of_an_id_with_no_record_leaves_a_marker() {
    let seen = Seen::default();
    let home = home_with("unregistered", worker(seen.clone(), Cancel::Cancelled));
    let id = "exec-00000000000000e9";
    let (code, out, err) = cancel(&home, id);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out.trim(), r#"{"already_ended":true}"#);
    assert!(exec_file(&home, id).with_extension("cancel").exists());
    assert!(cancels(&seen).is_empty());
    std::fs::remove_dir_all(home).unwrap();
}
