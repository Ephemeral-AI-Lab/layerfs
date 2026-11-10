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
use common::{broken_pipe, finish, layerfs, run, text};
use std::path::Path;
use std::process::Stdio;
#[path = "common/stream_worker.rs"]
mod stream_worker;
use stream_worker::{home_with_workspace, BIG};

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
