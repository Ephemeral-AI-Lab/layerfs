//! Exec ids, exec records and `workspace.exec_cancel` on `native`
//! (layerfs-project#113, compute-plane 2.0 §5, §8): a caller-named exec id
//! is used and an omitted one is the first progress event; a duplicate is
//! refused; a `sleep 600` started by one process (a `layerfs` command) is
//! cancelled by another (this test) and its exec answers `cancelled` with
//! its background children gone; cancelling an ended command answers
//! `ended`; unmount and close are refused `Busy` while a command runs and
//! `force` cancels it first; a stale record (its owner gone) is ended,
//! removed and logged. While that command runs, `sandbox.get` from this
//! other process lists it under its Workspace's `running` with the log tag
//! of the call that started it and its program, beside the Workspace's
//! `overlay_bytes` and the Sandbox's `workspace_capacity`
//! (layerfs-project#178); once it ended, it is listed no more. Two commands
//! run at once in one Workspace: a third runs and ends beside them, both
//! are listed, and cancelling one leaves the other running. The live
//! halves need the native daemon at LAYERFS_DAEMON; without it, they say
//! NOT_RUN.
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

/// The log tag of every command [`started`] starts.
const TAG: &str = "dsh:s9:t1";

/// `layerfs workspace exec <ws> <script> --exec-id <id>`, started, its call
/// tagged [`TAG`].
fn started(home: &Path, ws: &str, script: &str, id: &str) -> Child {
    Command::new(LAYERFS)
        .args(["workspace", "exec", ws, script, "--exec-id", id])
        .env("LAYERFS_HOME", home)
        .env("LAYERFS_LOGS_TAG", TAG)
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

    // Cancel before even starting the CLI: the pending mark must survive
    // process startup and be consumed at registration, without a resend.
    let early = ok(
        &home,
        "workspace.exec_cancel",
        &[ws(), ("exec_id", t("before-cli"))],
    );
    assert_eq!(early.get_text("outcome"), Some("ended"));
    let early = started(&home, "s1:w", "sleep 600", "before-cli");
    let answer = answered(early);
    assert!(answer.contains("\"kind\":\"cancelled\""), "{answer}");
    assert!(answer.contains("\"exit_code\":137"), "{answer}");

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
    // ...its record names the call's log tag and the command's program
    assert_eq!(record.get_text("tag"), Some(TAG), "{record:?}");
    assert_eq!(record.get_text("program"), Some("sleep"), "{record:?}");
    // ...`sandbox.get`, from this other process, lists it with them,
    // beside the Workspace's overlay size and the Sandbox's Workspace slots
    let listed = |got: &SdkValue| match got.get("workspaces") {
        Some(SdkValue::List(items)) if items.len() == 1 => items[0].clone(),
        _ => panic!("{got:?}"),
    };
    let got = ok(&home, "sandbox.get", &get_s1());
    assert_eq!(got.get_int("workspace_capacity"), Some(16), "{got:?}");
    let workspace = listed(&got);
    // the command, by its exec id, program and tag, with when it started
    // (`started_at`, the record's `started` as an ISO time)
    let command = match workspace.get("commands") {
        Some(SdkValue::List(commands)) if commands.len() == 1 => commands[0].clone(),
        _ => panic!("{got:?}"),
    };
    let fields = ["exec", "program", "tag"].map(|key| command.get_text(key));
    assert_eq!(
        fields,
        [Some("long-1"), Some("sleep"), Some(TAG)],
        "{got:?}"
    );
    let at = command.get_text("started_at").unwrap_or("");
    assert!(at.len() == 20 && at.ends_with('Z'), "{got:?}");
    assert!(record.get_int("started").is_some(), "{record:?}");
    let running = SdkValue::List(vec![t("long-1")]);
    assert_eq!(workspace.get("running"), Some(&running), "{got:?}");
    let run = std::fs::read_dir(home.join("machine/native/run"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.join("overlay").is_dir())
        .expect("the Sandbox's run folder");
    // the Workspace's own overlay file: `<namespace>.sqlite`
    let overlay = std::fs::read_dir(run.join("overlay"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("");
            path.extension().is_some_and(|ext| ext == "sqlite")
                && stem.chars().all(|c| c.is_ascii_digit())
        })
        .expect("the Workspace's overlay file");
    let bytes = i64::try_from(std::fs::metadata(&overlay).unwrap().len()).unwrap();
    assert!(bytes > 0, "{}", overlay.display());
    let stats = match got.get("stats").and_then(|stats| stats.get("workspaces")) {
        Some(SdkValue::List(items)) if items.len() == 1 => items[0].clone(),
        _ => panic!("{got:?}"),
    };
    assert_eq!(stats.get_int("overlay_bytes"), Some(bytes), "{got:?}");
    // ...the same id is refused while it runs
    let duplicate = [ws(), ("command", t("true")), ("exec_id", t("long-1"))];
    assert_eq!(refused(&home, "workspace.exec", &duplicate), "Exists");
    // ...unmount and close are refused while it runs
    assert_eq!(refused(&home, "workspace.unmount", &[ws()]), "Busy");
    assert_eq!(
        refused(&home, "sandbox.remove", &[("sandbox", t("s1"))]),
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
    // ...and a get lists it no more
    let got = ok(&home, "sandbox.get", &get_s1());
    let none = SdkValue::List(Vec::new());
    assert_eq!(listed(&got).get("running"), Some(&none), "{got:?}");

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
        "sandbox.remove",
        &[("sandbox", t("s1")), ("force", SdkValue::Bool(true))],
    );
    let answer = answered(third);
    assert!(answer.contains("\"kind\":\"cancelled\""), "{answer}");
    assert!(!home.join("sandbox/s1").exists());

    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}

/// `sandbox get s1 --detail --with stats`.
fn get_s1() -> [(&'static str, SdkValue); 3] {
    [
        ("sandbox", t("s1")),
        ("detail", SdkValue::Bool(true)),
        ("with", SdkValue::List(vec![t("stats")])),
    ]
}

/// The exec ids `sandbox.get` lists under Sandbox `s1`'s one Workspace's
/// `running`, oldest first.
fn running_ids(home: &Path) -> Vec<String> {
    let got = ok(home, "sandbox.get", &get_s1());
    let Some(SdkValue::List(items)) = got.get("workspaces") else {
        panic!("{got:?}")
    };
    assert_eq!(items.len(), 1, "{got:?}");
    let Some(SdkValue::List(running)) = items[0].get("running") else {
        panic!("{got:?}")
    };
    running
        .iter()
        .map(|item| match item {
            SdkValue::Text(id) => id.clone(),
            other => panic!("{other:?}"),
        })
        .collect()
}

#[test]
fn two_commands_run_at_once_in_one_workspace_on_native() {
    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset, no live Sandbox");
        return;
    };
    let home = tempdir("both");
    home::open(&home).unwrap();
    home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon)]).unwrap();
    let folder = tempdir("both-folder");
    std::fs::write(folder.join("README.md"), "both\n").unwrap();
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

    // two long commands, started one after the other, both run
    let first = started(&home, "s1:w", "sleep 600", "both-a");
    let a = recorded(&execs.join("both-a.json"));
    let second = started(&home, "s1:w", "sleep 600", "both-b");
    let b = recorded(&execs.join("both-b.json"));
    let group = |record: &SdkValue| {
        let stop = record.get("stop").unwrap().get("group").unwrap();
        stop.get_int("pgid").unwrap()
    };
    let (a_group, b_group) = (group(&a), group(&b));
    assert_ne!(a_group, b_group, "each command has its own stop handle");
    assert!(group_runs(a_group) && group_runs(b_group));
    // a third runs and ends while both run
    let third = ok(
        &home,
        "workspace.exec",
        &[
            ws(),
            ("command", t("echo beside")),
            ("exec_id", t("both-c")),
        ],
    );
    assert_eq!(third.get_text("kind"), Some("exit:0"), "{third:?}");
    assert!(!execs.join("both-c.json").exists());
    // a get lists both, oldest first
    assert_eq!(running_ids(&home), ["both-a", "both-b"]);

    // cancelling one leaves the other running
    let cancel = |id: &str| {
        let args = [ws(), ("exec_id", t(id)), ("grace_ms", SdkValue::Int(5000))];
        ok(&home, "workspace.exec_cancel", &args)
    };
    let cancelled = cancel("both-a");
    assert_eq!(
        cancelled.get_text("outcome"),
        Some("cancelled"),
        "{cancelled:?}"
    );
    let answer = answered(first);
    assert!(answer.contains("\"kind\":\"cancelled\""), "{answer}");
    assert!(
        !group_runs(a_group),
        "the cancelled command's group still runs"
    );
    assert!(group_runs(b_group), "the other command was ended too");
    assert!(execs.join("both-b.json").exists());
    assert_eq!(running_ids(&home), ["both-b"]);

    let cancelled = cancel("both-b");
    assert_eq!(
        cancelled.get_text("outcome"),
        Some("cancelled"),
        "{cancelled:?}"
    );
    let answer = answered(second);
    assert!(answer.contains("\"kind\":\"cancelled\""), "{answer}");
    assert!(running_ids(&home).is_empty());

    ok(&home, "workspace.unmount", &[ws()]);
    ok(&home, "sandbox.remove", &[("sandbox", t("s1"))]);
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}

/// A forced `machine.stop` of `native` cancels each Sandbox's running
/// command before its runtime stops, as a forced close does: the exec
/// answers `cancelled`, not the `exit:137` of the runtime's kill. Live only.
#[test]
fn forced_native_machine_stop_cancels_running_commands() {
    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset, no live Sandbox");
        return;
    };
    let home = tempdir("stop-home");
    home::open(&home).unwrap();
    home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon)]).unwrap();
    let folder = tempdir("stop-folder");
    std::fs::write(folder.join("README.md"), "stop\n").unwrap();
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
    let sandbox = [
        ("project", t("shop/api")),
        ("name", t("s1")),
        ("machine", t("native")),
    ];
    ok(&home, "sandbox.create", &sandbox);
    let mount = [("sandbox", t("s1")), ("branch", t("dev")), ("name", t("w"))];
    let wsid = ok(&home, "workspace.mount", &mount)
        .get_text("workspace")
        .unwrap()
        .to_owned();
    let child = started(&home, "s1:w", "sleep 600", "stop-1");
    let record = recorded(
        &home
            .join("sandbox/s1/workspace")
            .join(&wsid)
            .join("exec/stop-1.json"),
    );
    let pgid = record
        .get("stop")
        .unwrap()
        .get("group")
        .unwrap()
        .get_int("pgid")
        .unwrap();
    let force = [("name", t("native")), ("force", SdkValue::Bool(true))];
    let stopped = ok(&home, "machine.stop", &force);
    assert_eq!(
        stopped.get("stopped_sandboxes"),
        Some(&SdkValue::List(vec![t("s1")])),
        "{stopped:?}"
    );
    let answer = answered(child);
    assert!(answer.contains("\"kind\":\"cancelled\""), "{answer}");
    assert!(!group_runs(pgid), "the command's group still runs");
    // its record was settled, not left for a later close
    let execs = home.join("sandbox/s1/workspace").join(&wsid).join("exec");
    assert!(!execs.join("stop-1.json").exists());
    let _ = call(
        &home,
        "sandbox.remove",
        &[("sandbox", t("s1")), ("force", SdkValue::Bool(true))],
        &mut |_| {},
    );
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}
