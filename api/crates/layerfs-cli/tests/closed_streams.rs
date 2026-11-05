//! A standard stream that is closed, full, or whose reader has gone
//! (`| head -1`, `>&-`) never panics the CLI. What each exits with is the
//! documented policy, pinned here so a change to it is a decision: a closed
//! stdout is not an error (the runtime reopens it on /dev/null, exit 0); a
//! stdout that fails while printing exits 125, though the call was made;
//! a stderr that fails loses its line and the exit code stays the outcome's.
mod common;
use common::{failing_sinks, finish, layerfs, run, tempdir, text, LAYERFS};
use std::process::{Command, Stdio};

fn no_panic(stderr: &[u8]) {
    let stderr = text(stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn a_closed_stdout_is_not_an_error() {
    let home = tempdir("closed-out");
    let script = format!("exec '{LAYERFS}' store list >&-");
    let out = run(Command::new("sh")
        .args(["-c", &script])
        .env("LAYERFS_HOME", &home));
    assert_eq!(out.status.code(), Some(0), "{:?}", out.stderr);
    assert!(out.stderr.is_empty(), "{:?}", out.stderr);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_stdout_that_cannot_be_written_exits_125_without_panicking() {
    let home = tempdir("broken-out");
    for (what, words) in [
        ("store list", &["store", "list"][..]),
        ("store list --text", &["store", "list", "--text"]),
        ("doctor", &["doctor"]),
        ("help", &["help"]),
        ("help family", &["help", "store"]),
        ("help call", &["help", "store", "get"]),
        ("help doctor", &["help", "doctor"]),
        ("--version", &["--version"]),
    ] {
        for (sink, stdout) in failing_sinks(what) {
            let child = layerfs(&home, words)
                .stdout(stdout)
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let out = finish(child);
            assert_eq!(out.status.code(), Some(125), "{what} on {sink}");
            no_panic(&out.stderr);
        }
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_stderr_that_cannot_be_written_keeps_the_outcomes_code() {
    let home = tempdir("broken-err");
    for (what, words, code) in [
        ("a refusal", &["store", "get", "nope"][..], 125),
        ("a wrong command line", &["store", "get"], 2),
        ("a wrong command", &["nope", "x"], 2),
        ("unknown help", &["help", "nope"], 2),
    ] {
        for (sink, stderr) in failing_sinks(what) {
            let child = layerfs(&home, words)
                .stdout(Stdio::piped())
                .stderr(stderr)
                .spawn()
                .unwrap();
            let out = finish(child);
            assert_eq!(out.status.code(), Some(code), "{what} on {sink}");
            assert!(out.stdout.is_empty(), "{what}: {:?}", out.stdout);
        }
    }
    std::fs::remove_dir_all(home).unwrap();
}
