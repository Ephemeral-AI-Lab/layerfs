//! The six doctors on the command line (call surface 2.0): their help, a
//! wrong command line and a thing that is not there never make a home, and
//! each doctor answers without making one.
use std::path::PathBuf;
use std::process::{Command, Output};
const CLI: &str = env!("CARGO_BIN_EXE_layerfs");
fn home(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-cli-target-{tag}-{}", std::process::id()))
}

fn layerfs(root: &PathBuf, args: &[&str]) -> Output {
    Command::new(CLI)
        .args(args)
        .env("LAYERFS_HOME", root)
        .output()
        .unwrap()
}

#[test]
fn each_doctor_s_help_is_home_free() {
    let root = home("help");
    assert!(!root.exists());
    for (args, first) in [
        (
            &["help", "home", "doctor"][..],
            "layerfs home doctor [--progress]",
        ),
        (
            &["home", "doctor", "--help"],
            "layerfs home doctor [--progress]",
        ),
        (
            &["machine", "doctor", "--help"],
            "layerfs machine doctor <machine>",
        ),
        (
            &["store", "doctor", "--help"],
            "layerfs store doctor <store>",
        ),
        (
            &["project", "doctor", "--help"],
            "layerfs project doctor <project>",
        ),
        (
            &["sandbox", "doctor", "--help"],
            "layerfs sandbox doctor <sandbox>",
        ),
        (
            &["backup", "doctor", "--help"],
            "layerfs backup doctor <bucket>",
        ),
    ] {
        let out = layerfs(&root, args);
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{args:?}: {out:?}");
        assert!(text.starts_with(first), "{args:?}: {text}");
        assert!(!root.exists());
    }
}

#[test]
fn wrong_command_lines_are_usage_errors_without_effects() {
    let root = home("invalid");
    assert!(!root.exists());
    for args in [
        &["doctor", "--all"][..],
        &["doctor", "host"],
        &["home", "doctor", "--timeout", "1s"],
        &["home", "doctor", "--full"],
        &["machine", "doctor"],
        &["store", "doctor", "shop", "--integrity"],
        &["sandbox", "doctor", "a", "b"],
        &["backup", "doctor", "r2", "--write-test"],
        &["backup", "doctor", "r2", "--progress"],
    ] {
        let out = layerfs(&root, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
        assert!(!root.exists(), "{args:?} made a home");
    }
}

#[test]
fn each_doctor_answers_or_refuses_without_making_a_home() {
    let root = home("families");
    assert!(!root.exists());
    // the home: a report, exit 0
    let out = layerfs(&root, &["home", "doctor"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let report = layerfs_api::json::parse(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert!(report.get("ok").is_some() || report.get("findings").is_some());
    // a thing that is not there: a refusal naming it
    for args in [
        &["machine", "doctor", "missing"][..],
        &["store", "doctor", "missing"],
        &["project", "doctor", "missing/p"],
        &["sandbox", "doctor", "missing"],
        &["backup", "doctor", "missing"],
    ] {
        let out = layerfs(&root, args);
        assert_eq!(out.status.code(), Some(125), "{args:?}: {out:?}");
        let refusal = String::from_utf8_lossy(&out.stderr);
        assert!(
            refusal.contains(r#""error":"NotFound""#),
            "{args:?}: {refusal}"
        );
        assert!(!root.exists(), "{args:?} made a home");
    }
}
