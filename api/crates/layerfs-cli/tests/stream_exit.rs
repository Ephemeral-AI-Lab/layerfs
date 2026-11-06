//! `workspace exec --stream` and the codes it can exit with, against a
//! stand-in worker (no VM, container or daemon): a docker-machine Sandbox
//! whose exec goes to the worker, which answers with the exit code and the
//! output the command text asks for (`exit N`, `big`). Without `--stream`
//! the command's code is in the JSON and the exit is 0.
//!
//! With `--stream` the command's own code is the exit code, so a command
//! that exits 2 or 125 is the same exit as a wrong command line or a
//! refusal; the stderr line (usage) or the absence of one tells them apart.
//! That is the policy as built, pinned here until the owner decides.
#![cfg(feature = "machine")]
mod common;
use common::{broken_pipe, finish, layerfs, run, tempdir, text};
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::worker::{read_frame, write_frame};
use layerfs_bridge::control::SdkValue;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

/// The size `big` writes: under the 1 MiB window that is spooled instead.
const BIG: usize = 900_000;

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A stand-in worker: it answers `sandbox.create` and `workspace.exec`.
fn worker() -> SocketAddr {
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
            // a Sandbox operation arrives as `sandbox.host` naming its own op
            let op = match request.get_text("op") {
                Some("sandbox.host") => args.get_text("op"),
                other => other,
            };
            let answer = match op {
                Some("workspace.exec") => exec(more.get_text("command").unwrap_or("")),
                _ => create(&more),
            };
            let _ = write_frame(&mut stream, &answer);
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
    SdkValue::map([(
        "value",
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
        ]),
    )])
}

/// `exit N` exits N with a line on each stream; `big` writes `BIG` bytes.
fn exec(command: &str) -> SdkValue {
    let code = command
        .strip_prefix("exit ")
        .and_then(|n| n.parse::<i64>().ok())
        .unwrap_or(0);
    let (stdout, stderr) = match command {
        "big" => ("x".repeat(BIG), String::new()),
        _ => ("out\n".to_owned(), "err\n".to_owned()),
    };
    SdkValue::map([(
        "value",
        SdkValue::map([
            ("kind", t(&format!("exit:{code}"))),
            ("exitCode", SdkValue::Int(code)),
            ("stdout", SdkValue::Text(stdout)),
            ("stderr", SdkValue::Text(stderr)),
        ]),
    )])
}

/// A home with docker machine `docker` and Sandbox `d1`, whose Workspace
/// calls go to the stand-in worker (a docker Sandbox runs in namespaces in
/// the worker container and is reached through it, D-8).
fn home_with_workspace(tag: &str) -> PathBuf {
    let home = tempdir(&format!("stream-{tag}"));
    home::open(&home).unwrap();
    let address = worker();
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

/// `workspace exec <ws> <command> <rest>` in `home`: code, stdout, stderr.
fn exec_in(home: &Path, ws: &str, command: &str, rest: &[&str]) -> (Option<i32>, String, String) {
    let words = [&["workspace", "exec", ws, command][..], rest].concat();
    let out = run(&mut layerfs(home, &words));
    (out.status.code(), text(&out.stdout), text(&out.stderr))
}

#[test]
fn stream_exits_with_the_commands_code_and_passes_its_output() {
    let home = home_with_workspace("stream_exits_with_the_commands_code_and_passes_its_output");
    for code in [0, 1, 2, 3, 124, 125, 126, 255] {
        let (c, out, err) = exec_in(&home, "d1:one", &format!("exit {code}"), &["--stream"]);
        assert_eq!(c, Some(code), "exit {code}: {err}");
        assert_eq!((out.as_str(), err.as_str()), ("out\n", "err\n"));
    }
    // a code that does not fit 0..=255 is 1; the policy is documented
    for code in [256, 300, -1] {
        let (c, ..) = exec_in(&home, "d1:one", &format!("exit {code}"), &["--stream"]);
        assert_eq!(c, Some(1), "exit {code}");
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn without_stream_the_commands_code_is_data_and_the_exit_is_0() {
    let home = home_with_workspace("without_stream_the_commands_code_is_data_and_the_exit_is_0");
    for code in [0, 2, 125] {
        let (c, out, err) = exec_in(&home, "d1:one", &format!("exit {code}"), &[]);
        assert_eq!(c, Some(0), "{err}");
        assert!(out.contains(&format!("\"exit_code\":{code}")), "{out}");
        assert!(out.contains(r#""stdout":"out\n""#), "{out}");
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn with_stream_a_command_exit_of_2_or_125_looks_like_usage_or_refusal() {
    let home =
        home_with_workspace("with_stream_a_command_exit_of_2_or_125_looks_like_usage_or_refusal");
    // the same exit codes as a wrong command line and a refusal ...
    let (two, ..) = exec_in(&home, "d1:one", "exit 2", &["--stream"]);
    let (usage, _, usage_err) = exec_in(&home, "d1:one", "exit 2", &["--stream", "--bogus"]);
    assert_eq!((two, usage), (Some(2), Some(2)));
    // a Sandbox the home does not have is refused
    let (refused, _, refusal) = exec_in(&home, "nosuch:two", "true", &["--stream"]);
    let (command, _, command_err) = exec_in(&home, "d1:one", "exit 125", &["--stream"]);
    assert_eq!((refused, command), (Some(125), Some(125)));
    // ... told apart only by what is on stderr
    assert!(usage_err.starts_with("layerfs: "), "{usage_err}");
    assert!(refusal.starts_with(r#"{"code":"#), "{refusal}");
    assert_eq!(command_err, "err\n");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_large_streamed_output_arrives_whole() {
    let home = home_with_workspace("a_large_streamed_output_arrives_whole");
    let (c, out, err) = exec_in(&home, "d1:one", "big", &["--stream"]);
    assert_eq!(c, Some(0), "{err}");
    assert_eq!(out.len(), BIG);
    assert!(out.bytes().all(|b| b == b'x'));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_stream_whose_reader_has_gone_still_exits_with_the_commands_code() {
    let home =
        home_with_workspace("a_stream_whose_reader_has_gone_still_exits_with_the_commands_code");
    for (command, code) in [("exit 7", 7), ("big", 0)] {
        let words = ["workspace", "exec", "d1:one", command, "--stream"];
        let child = layerfs(&home, &words)
            .stdout(broken_pipe(command))
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let out = finish(child);
        assert_eq!(out.status.code(), Some(code), "{command}");
        assert!(!text(&out.stderr).contains("panicked"), "{:?}", out.stderr);
    }
    std::fs::remove_dir_all(home).unwrap();
}
