//! Real native exec output, exit codes and cancellation from another
//! process (call surface 2.0); no stand-in command output.
#[path = "../support/mod.rs"]
mod common;
use common::{broken_pipe, finish, layerfs, run, tempdir, text};
use layerfs_protocol::sdk::SdkValue;
use std::{path::Path, process::Stdio};
use {layerfs_api::home, layerfs_home::json};
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
#[test]
fn native_exec_keeps_large_binary_output_cancellation_and_effects_on_closed_stdout() {
    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset");
        return;
    };
    let home = tempdir("phase1-live-home");
    let source = tempdir("phase1-live-source");
    home::open(&home).unwrap();
    // the native machine's daemon is a setting of its record
    layerfs_api::call(
        Some(&home),
        "machine.update",
        SdkValue::map([
            ("machine", SdkValue::text(layerfs_home::names::HOST_MACHINE)),
            ("daemon", SdkValue::text(daemon.as_str())),
        ]),
        &mut |_| {},
    )
    .unwrap();
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
            ("from", t("main")),
        ],
    );
    call(
        &home,
        "sandbox.create",
        &[
            ("project", t("live/p")),
            ("name", t("s1")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
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
    // --stream: the output, whole and as bytes, on this process's streams,
    // and the command's code as the exit
    let script = r"head -c 1100000 /dev/zero | tr '\0' x; printf '\377\376' >&2; exit 125";
    let out = run(&mut layerfs(
        &home,
        &["workspace", "exec", "s1:one", script, "--stream"],
    ));
    assert_eq!(out.status.code(), Some(125), "{}", text(&out.stderr));
    assert_eq!(out.stdout, vec![b'x'; 1100000]);
    assert_eq!(out.stderr, [0xff, 0xfe]);
    // without it: the answer is printed, the exit is still the command's;
    // a stream past 1 MiB is its last 1 MiB, with the file that holds it
    let big = run(&mut layerfs(
        &home,
        &["workspace", "exec", "s1:one", script, "--detail"],
    ));
    assert_eq!(big.status.code(), Some(125), "{}", text(&big.stderr));
    let result = json::parse(text(&big.stdout).trim()).unwrap();
    assert_eq!(result.get_int("stdout_bytes"), Some(1100000), "{result:?}");
    assert_eq!(result.get_text("stdout").map(str::len), Some(1 << 20));
    let file = result
        .get_text("stdout_file")
        .expect("the spilled stream's file");
    assert_eq!(std::fs::metadata(file).unwrap().len(), 1100000);
    let two = run(&mut layerfs(
        &home,
        &["workspace", "exec", "s1:one", "exit 2"],
    ));
    assert_eq!(two.status.code(), Some(2));
    let result = json::parse(text(&two.stdout).trim()).unwrap();
    assert_eq!(result.get_int("exit_code"), Some(2));
    // a cancel from another process ends a running command: its exec
    // answers `cancelled`, exiting with the signal's code
    let sleeping = layerfs(&home, &["workspace", "exec", "s1:one", "sleep 600"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let cancel = call(&home, "workspace.cancel", &[("workspace", t("s1:one"))]);
        if cancel.get_text("signal").is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "never running: {cancel:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let cancelled = finish(sleeping);
    let result = json::parse(text(&cancelled.stdout).trim()).unwrap();
    assert_eq!(
        result.get("cancelled"),
        Some(&SdkValue::Bool(true)),
        "{result:?}"
    );
    let code = result.get_int("exit_code").unwrap();
    assert!(code == 143 || code == 137, "{result:?}");
    assert_eq!(cancelled.status.code(), Some(code as i32));
    // a closed stdout does not undo the command's effect or its exit
    let child = layerfs(
        &home,
        &["workspace", "exec", "s1:one", "echo effect > kept"],
    )
    .stdout(broken_pipe("phase1-live"))
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    let closed = finish(child);
    assert_eq!(closed.status.code(), Some(0));
    let kept = std::fs::read(Path::new(mounted.get_text("path").unwrap()).join("kept")).unwrap();
    assert_eq!(kept, b"effect\n");
    call(
        &home,
        "workspace.unmount",
        &[
            ("workspace", t("s1:one")),
            ("discard", SdkValue::Bool(true)),
        ],
    );
    call(&home, "sandbox.remove", &[("sandbox", t("s1"))]);
    layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}

#[cfg(feature = "machine")]
#[path = "../support/stream_worker.rs"]
mod stream_worker;

#[test]
#[cfg(feature = "machine")]
fn cli_json_keeps_i64_above_double_precision() {
    let home = stream_worker::home_with_workspace("phase1-integer");
    let out = run(&mut layerfs(
        &home,
        &["workspace", "exec", "d1:one", "exit 9007199254740993"],
    ));
    // a code past 0..=255 exits 1; the answer keeps it whole
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    let value = json::parse(text(&out.stdout).trim()).unwrap();
    assert_eq!(value.get_int("exit_code"), Some(9007199254740993));
    assert!(text(&out.stdout).contains("9007199254740993"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn table_reads_initialize_missing_and_default_homes_but_doctor_does_not() {
    let scratch = tempdir("phase1-home");
    let missing = scratch.join("missing");
    let doctor = run(&mut layerfs(&missing, &["home", "doctor"]));
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
