//! NDJSON framing through the real CLI and a stand-in worker transport.
#![cfg(feature = "machine")]
mod common;
#[path = "common/stream_worker.rs"]
mod stream_worker;
use common::{broken_pipe, finish, layerfs, run, tempdir, text};
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;
use std::process::Stdio;
use stream_worker::home_with_workspace;

fn frames(bytes: &[u8]) -> Vec<SdkValue> {
    text(bytes)
        .lines()
        .map(|line| json::parse(line).unwrap())
        .collect()
}

#[test]
fn result_is_final_even_for_command_exit_2_125_or_cancellation() {
    let home = home_with_workspace("framed-results");
    for command in ["exit 2", "exit 125", "cancelled"] {
        let out = run(&mut layerfs(
            &home,
            &["workspace", "exec", "d1:one", command, "--events"],
        ));
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        assert!(out.stderr.is_empty());
        let frames = frames(&out.stdout);
        assert!(frames[0].get("progress").is_some());
        assert_eq!(
            frames.iter().filter(|f| f.get("result").is_some()).count(),
            1
        );
        let result = frames.last().unwrap().get("result").unwrap();
        if command == "cancelled" {
            assert_eq!(result.get_text("kind"), Some("cancelled"));
        } else {
            assert_eq!(
                result.get_int("exit_code"),
                Some(command[5..].parse().unwrap())
            );
        }
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn output_over_one_mib_and_binary_bytes_are_preserved_in_order() {
    let home = home_with_workspace("framed-bytes");
    let out = run(&mut layerfs(
        &home,
        &["workspace", "exec", "d1:one", "framed-big", "--events"],
    ));
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let frames = frames(&out.stdout);
    let bytes: Vec<_> = frames.iter().filter_map(|f| f.get("bytes")).collect();
    assert_eq!(bytes.len(), 3);
    assert_eq!(bytes[0], &SdkValue::text("x".repeat(1_048_576)));
    assert_eq!(bytes[1], &SdkValue::Bytes(vec![0xff, 0xfe, 0]));
    assert_eq!(bytes[2], &SdkValue::text("z".repeat(8192)));
    assert!(text(&out.stdout).contains("base64"));
    assert!(frames.last().unwrap().get("result").is_some());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn refusal_and_usage_are_distinct_and_output_failure_is_125() {
    let home = tempdir("framed-refusal");
    let args = ["workspace", "exec", "missing:one", "true", "--events"];
    let out = run(&mut layerfs(&home, &args));
    assert_eq!(out.status.code(), Some(125));
    assert!(out.stderr.is_empty());
    let frames = frames(&out.stdout);
    assert_eq!(frames.len(), 1);
    assert!(frames[0].get("refusal").unwrap().get_text("code").is_some());
    for flag in ["--text", "--stream", "--bogus"] {
        let args = [&args[..], &[flag]].concat();
        let out = run(&mut layerfs(&home, &args));
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(text(&out.stderr).starts_with("layerfs: "));
    }
    std::fs::remove_dir_all(home).unwrap();
    let home = home_with_workspace("framed-broken");
    let child = layerfs(
        &home,
        &["workspace", "exec", "d1:one", "exit 2", "--events"],
    )
    .stdout(broken_pipe("framed"))
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    let out = finish(child);
    assert_eq!(out.status.code(), Some(125));
    assert!(out.stderr.is_empty());
    std::fs::remove_dir_all(home).unwrap();
}
