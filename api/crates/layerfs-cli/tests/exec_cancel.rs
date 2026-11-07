//! Exec ids, exec records and `workspace.exec_cancel` on `native`
//! (layerfs-project#113, compute-plane 2.0 §5, §8): a caller-named exec id
//! is used and an omitted one is the first progress event; a duplicate is
//! refused; a `sleep 600` started by one process (a `layerfs` command) is
//! cancelled by another (this test) and its exec answers `cancelled` with
//! its background children gone; cancelling an ended command answers
//! `ended`; unmount and close are refused `Busy` while a command runs and
//! `force` cancels it first; a stale record (its owner gone) is ended,
//! removed and logged. The live half needs the native daemon at
//! LAYERFS_DAEMON; without it, it says NOT_RUN.
use layerfs_api::home;
use layerfs_bridge::control::SdkValue;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const LAYERFS: &str = env!("CARGO_BIN_EXE_layerfs");

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-cli-cancel-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(
    home: &Path,
    name: &str,
    args: &[(&str, SdkValue)],
    events: &mut dyn FnMut(SdkValue),
) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, events).map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args, &mut |_| {}).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn refused(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> String {
    call(home, name, args, &mut |_| {}).unwrap_err().0
}

/// `layerfs workspace exec <ws> <script> --exec-id <id>`, started.
fn started(home: &Path, ws: &str, script: &str, id: &str) -> Child {
    Command::new(LAYERFS)
        .args(["workspace", "exec", ws, script, "--exec-id", id])
        .env("LAYERFS_HOME", home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap()
}

/// Waits (at most 30 s) for `file` to exist with a stop handle.
fn recorded(file: &Path) -> SdkValue {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(Some(value)) = home::read_record(file) {
            if !matches!(value.get("stop"), None | Some(SdkValue::Null)) {
                return value;
            }
        }
        assert!(Instant::now() < deadline, "no record at {}", file.display());
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The exec's JSON answer once its command ended (at most 30 s).
fn answered(mut child: Child) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("the exec did not end");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Whether process group `pgid` still has a member.
fn group_runs(pgid: i64) -> bool {
    Command::new("pgrep")
        .args(["-g", &pgid.to_string()])
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn exec_ids_records_and_cancel_on_native() {
    let home = tempdir("home");
    // a refusal before any Sandbox: no such Sandbox, and a bad exec id
    assert_eq!(
        refused(
            &home,
            "workspace.exec_cancel",
            &[("ws", t("s1:w")), ("exec_id", t("../x"))]
        ),
        "InvalidInput"
    );
    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset, no live Sandbox");
        let _ = std::fs::remove_dir_all(home);
        return;
    };
    home::open(&home).unwrap();
    home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon)]).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "cancel\n").unwrap();
    let folder_text = folder.display().to_string();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[("project", t("shop/api")), ("folder", t(&folder_text))],
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
    let sandbox = [
        ("project", t("shop/api")),
        ("name", t("s1")),
        ("machine", t("native")),
    ];
    ok(&home, "sandbox.create", &sandbox);
    let mount = [("sandbox", t("s1")), ("branch", t("dev")), ("name", t("w"))];
    let mounted = ok(&home, "workspace.mount", &mount);
    let wsid = mounted.get_text("workspace").unwrap().to_owned();
    let execs = home.join("sandbox/s1/workspace").join(&wsid).join("exec");
    let ws = || ("ws", t("s1:w"));

    // a caller-named exec id is used, and is the first progress event
    let mut events = Vec::new();
    let named = call(
        &home,
        "workspace.exec",
        &[ws(), ("command", t("echo hi")), ("exec_id", t("named-1"))],
        &mut |event| events.push(event),
    )
    .unwrap();
    assert_eq!(named.get_text("exec_id"), Some("named-1"));
    assert_eq!(named.get_text("stop"), Some("group"));
    assert_eq!(named.get_text("kind"), Some("exit:0"));
    let first = events[0]
        .get("progress")
        .and_then(|p| p.get("exec"))
        .cloned();
    assert_eq!(
        first
            .and_then(|e| e.get_text("id").map(str::to_owned))
            .as_deref(),
        Some("named-1")
    );
    // an omitted one is made by control and sent first too
    let mut events = Vec::new();
    let made = call(
        &home,
        "workspace.exec",
        &[ws(), ("command", t("true"))],
        &mut |e| events.push(e),
    )
    .unwrap();
    let id = made.get_text("exec_id").unwrap().to_owned();
    assert!(id.starts_with("exec-"), "{id}");
    let first = events[0]
        .get("progress")
        .and_then(|p| p.get("exec"))
        .cloned();
    assert_eq!(
        first.and_then(|e| e.get_text("id").map(str::to_owned)),
        Some(id)
    );
    // an ended command leaves no record
    assert!(!execs.join("named-1.json").exists());

    // one process starts a command with a background child...
    let child = started(&home, "s1:w", "sleep 600 & sleep 600", "long-1");
    let record = recorded(&execs.join("long-1.json"));
    let pgid = record
        .get("stop")
        .unwrap()
        .get("group")
        .unwrap()
        .get_int("pgid")
        .unwrap();
    assert_eq!(record.get_text("route"), Some("local"));
    // ...the same id is refused while it runs
    let duplicate = [ws(), ("command", t("true")), ("exec_id", t("long-1"))];
    assert_eq!(refused(&home, "workspace.exec", &duplicate), "Exists");
    // ...unmount and close are refused while it runs
    assert_eq!(refused(&home, "workspace.unmount", &[ws()]), "Busy");
    assert_eq!(
        refused(&home, "sandbox.close", &[("name", t("s1"))]),
        "Busy"
    );
    // ...and another process cancels it
    let cancel = [
        ws(),
        ("exec_id", t("long-1")),
        ("grace_ms", SdkValue::Int(5000)),
    ];
    let cancelled = ok(&home, "workspace.exec_cancel", &cancel);
    assert_eq!(
        cancelled.get_text("outcome"),
        Some("cancelled"),
        "{cancelled:?}"
    );
    assert_eq!(cancelled.get_text("signal"), Some("SIGTERM"));
    let answer = answered(child);
    assert!(answer.contains("\"kind\":\"cancelled\""), "{answer}");
    assert!(answer.contains("\"exit_code\":143"), "{answer}");
    assert!(!group_runs(pgid), "the command's group still runs");
    // cancelling it again: it ended
    let again = ok(&home, "workspace.exec_cancel", &cancel);
    assert_eq!(again.get_text("outcome"), Some("ended"));

    // a command started with no exec id: `--progress` prints the id control
    // made as its first line on stderr, and another process cancels by it
    let mut child = Command::new(LAYERFS)
        .args(["workspace", "exec", "s1:w", "sleep 600", "--progress"])
        .env("LAYERFS_HOME", &home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let mut first = String::new();
    BufReader::new(child.stderr.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    let event = layerfs_api::json::parse(first.trim()).unwrap_or_else(|e| panic!("{first}: {e:?}"));
    let made = event
        .get("progress")
        .and_then(|p| p.get("exec"))
        .and_then(|e| e.get_text("id"))
        .unwrap_or_else(|| panic!("{first}"))
        .to_owned();
    assert!(made.starts_with("exec-"), "{made}");
    recorded(&execs.join(format!("{made}.json")));
    let cancel_made = [ws(), ("exec_id", t(&made))];
    let cancelled = ok(&home, "workspace.exec_cancel", &cancel_made);
    assert_eq!(cancelled.get_text("outcome"), Some("cancelled"));
    let answer = answered(child);
    assert!(answer.contains("\"kind\":\"cancelled\""), "{answer}");
    assert!(
        answer.contains(&format!("\"exec_id\":\"{made}\"")),
        "{answer}"
    );

    // a stale record (its owner gone) whose command still runs: ended,
    // removed and logged by the unmount that finds it
    let mut orphan = Command::new("/bin/sleep")
        .arg("600")
        .process_group(0)
        .spawn()
        .unwrap();
    let leader = layerfs_bridge::process::LocalProcess::observe(orphan.id()).unwrap();
    let mut gone = Command::new("/usr/bin/true").spawn().unwrap();
    let gone_pid = gone.id();
    gone.wait().unwrap();
    std::fs::create_dir_all(&execs).unwrap();
    let boot = format!("{:032x}", leader.boot.unwrap());
    home::write_record(
        &execs.join("stale-1.json"),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("id", t("stale-1")),
            ("workspace", t(&wsid)),
            ("path", t(mounted.get_text("path").unwrap())),
            ("name", t("w")),
            (
                "owner",
                SdkValue::map([
                    ("pid", SdkValue::Int(i64::from(gone_pid))),
                    ("pid_start", t("1")),
                    ("pid_boot", t(&boot)),
                ]),
            ),
            ("route", t("local")),
            (
                "stop",
                SdkValue::map([(
                    "group",
                    SdkValue::map([
                        ("pgid", SdkValue::Int(i64::from(orphan.id()))),
                        ("pid", SdkValue::Int(i64::from(leader.pid))),
                        ("pid_start", t(&leader.start.to_string())),
                        ("pid_boot", t(&boot)),
                    ]),
                )]),
            ),
            ("started", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
    // force: a second command is cancelled first; the stale one is ended
    let second = started(&home, "s1:w", "sleep 600", "long-2");
    recorded(&execs.join("long-2.json"));
    ok(
        &home,
        "workspace.unmount",
        &[ws(), ("force", SdkValue::Bool(true))],
    );
    let answer = answered(second);
    assert!(answer.contains("\"kind\":\"cancelled\""), "{answer}");
    let status = orphan.wait().unwrap();
    assert!(
        status.code().is_none(),
        "the stale command was ended: {status:?}"
    );
    let log = std::fs::read_to_string(home.join("log/exec.jsonl")).unwrap();
    assert!(
        log.contains("\"stale-1\"") && log.contains("\"stale\":true"),
        "{log}"
    );
    // terminal unmount removed the Workspace's folder
    assert!(!home.join("sandbox/s1/workspace").join(&wsid).exists());

    // close with force cancels the command first
    let mounted = ok(&home, "workspace.mount", &mount);
    let wsid = mounted.get_text("workspace").unwrap().to_owned();
    let third = started(&home, "s1:w", "sleep 600", "long-3");
    recorded(
        &home
            .join("sandbox/s1/workspace")
            .join(&wsid)
            .join("exec/long-3.json"),
    );
    ok(
        &home,
        "sandbox.close",
        &[("name", t("s1")), ("force", SdkValue::Bool(true))],
    );
    let answer = answered(third);
    assert!(answer.contains("\"kind\":\"cancelled\""), "{answer}");
    assert!(!home.join("sandbox/s1").exists());

    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}
