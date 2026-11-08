//! Real native exec framing; no stand-in command output or cancellation.
mod common;
use common::{broken_pipe, finish, layerfs, run, tempdir, text};
use layerfs_api::{home, json};
use layerfs_bridge::control::SdkValue;
use std::{path::Path, process::Stdio};
fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn call(home: &Path, method: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    layerfs_api::call(
        Some(home),
        method,
        SdkValue::Map(
            args.iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        ),
        &mut |_| {},
    )
    .unwrap_or_else(|r| panic!("{method}: {r:?}"))
}
fn frames(bytes: &[u8]) -> Vec<SdkValue> {
    text(bytes)
        .lines()
        .map(|line| json::parse(line).unwrap())
        .collect()
}
#[test]
fn native_exec_keeps_large_binary_output_cancellation_and_effects_on_closed_stdout() {
    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset");
        return;
    };
    let home = tempdir("phase1-live-home");
    let source = tempdir("phase1-live-source");
    home::open(&home).unwrap();
    home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon)]).unwrap();
    call(&home, "store.create", &[("name", t("live"))]);
    call(
        &home,
        "project.init",
        &[
            ("project", t("live/p")),
            ("folder", t(source.to_str().unwrap())),
        ],
    );
    call(
        &home,
        "branch.fork",
        &[
            ("project", t("live/p")),
            ("name", t("dev")),
            ("from_branch", t("main")),
        ],
    );
    call(
        &home,
        "sandbox.create",
        &[
            ("project", t("live/p")),
            ("name", t("s1")),
            ("machine", t("native")),
        ],
    );
    let mounted = call(
        &home,
        "workspace.mount",
        &[
            ("sandbox", t("s1")),
            ("branch", t("dev")),
            ("name", t("one")),
        ],
    );
    let script = r"head -c 1100000 /dev/zero | tr '\0' x; printf '\377\376' >&2; exit 125";
    let out = run(&mut layerfs(
        &home,
        &["workspace", "exec", "s1:one", script, "--events"],
    ));
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(out.stderr.is_empty());
    let frames = frames(&out.stdout);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    for frame in &frames {
        let bytes = match frame.get("bytes") {
            Some(SdkValue::Text(s)) => s.as_bytes(),
            Some(SdkValue::Bytes(b)) => b,
            _ => continue,
        };
        match frame.get_text("stream") {
            Some("stdout") => stdout.extend_from_slice(bytes),
            Some("stderr") => stderr.extend_from_slice(bytes),
            _ => panic!("{frame:?}"),
        }
    }
    assert_eq!(stdout, vec![b'x'; 1100000]);
    assert_eq!(stderr, [0xff, 0xfe]);
    assert_eq!(
        frames.iter().filter(|f| f.get("result").is_some()).count(),
        1
    );
    assert_eq!(
        frames
            .last()
            .unwrap()
            .get("result")
            .unwrap()
            .get_int("exit_code"),
        Some(125)
    );
    let two = run(&mut layerfs(
        &home,
        &["workspace", "exec", "s1:one", "exit 2", "--events"],
    ));
    assert_eq!(two.status.code(), Some(0));
    assert_eq!(frames_of_result(&two.stdout).get_int("exit_code"), Some(2));
    call(
        &home,
        "workspace.exec_cancel",
        &[("ws", t("s1:one")), ("exec_id", t("early-framed"))],
    );
    let cancelled = run(&mut layerfs(
        &home,
        &[
            "workspace",
            "exec",
            "s1:one",
            "sleep 600",
            "--exec-id",
            "early-framed",
            "--events",
        ],
    ));
    assert_eq!(cancelled.status.code(), Some(0));
    let result = frames_of_result(&cancelled.stdout);
    assert_eq!(result.get_text("kind"), Some("cancelled"));
    assert_eq!(result.get_int("exit_code"), Some(137));
    let child = layerfs(
        &home,
        &[
            "workspace",
            "exec",
            "s1:one",
            "echo effect > kept",
            "--events",
        ],
    )
    .stdout(broken_pipe("phase1-live"))
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    let closed = finish(child);
    assert_eq!(closed.status.code(), Some(125));
    let kept = std::fs::read(Path::new(mounted.get_text("path").unwrap()).join("kept")).unwrap();
    assert_eq!(kept, b"effect\n");
    call(
        &home,
        "workspace.unmount",
        &[("ws", t("s1:one")), ("discard", SdkValue::Bool(true))],
    );
    call(&home, "sandbox.close", &[("name", t("s1"))]);
    layerfs_api::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}
fn frames_of_result(bytes: &[u8]) -> SdkValue {
    let f = frames(bytes);
    f.last().unwrap().get("result").unwrap().clone()
}

#[cfg(feature = "machine")]
#[path = "common/stream_worker.rs"]
mod stream_worker;

#[test]
#[cfg(feature = "machine")]
fn cli_json_keeps_i64_above_double_precision() {
    let home = stream_worker::home_with_workspace("phase1-integer");
    let out = run(&mut layerfs(
        &home,
        &["workspace", "exec", "d1:one", "exit 9007199254740993"],
    ));
    assert!(out.status.success(), "{}", text(&out.stderr));
    let value = json::parse(text(&out.stdout).trim()).unwrap();
    assert_eq!(value.get_int("exit_code"), Some(9007199254740993));
    assert!(text(&out.stdout).contains("9007199254740993"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn table_reads_initialize_missing_and_default_homes_but_doctor_does_not() {
    let scratch = tempdir("phase1-home");
    let missing = scratch.join("missing");
    let doctor = run(&mut layerfs(&missing, &["doctor"]));
    assert!(doctor.status.success());
    assert!(!missing.exists());
    let read = run(&mut layerfs(&missing, &["store", "list"]));
    assert!(read.status.success());
    assert!(missing.exists());
    let default = scratch.join("default");
    std::fs::create_dir(&default).unwrap();
    let read = run(layerfs(&missing, &["store", "list"])
        .env_remove("LAYERFS_HOME")
        .env("HOME", &default));
    assert!(read.status.success(), "{}", text(&read.stderr));
    assert!(default.join(".layerfs").exists());
    std::fs::remove_dir_all(scratch).unwrap();
}
