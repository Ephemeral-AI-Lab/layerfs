//! Explicit doctor parsing/help must never initialize a home on invalid input.
use std::path::PathBuf;
use std::process::Command;
const CLI: &str = env!("CARGO_BIN_EXE_layerfs");
fn home(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-cli-target-{tag}-{}", std::process::id()))
}

#[test]
fn every_help_level_and_full_target_is_home_free() {
    let root = home("help");
    assert!(!root.exists());
    for args in [
        vec!["help", "doctor"],
        vec!["help", "doctor", "store"],
        vec!["help", "doctor", "store", "integrity"],
        vec!["help", "doctor", "store", "connection"],
        vec!["help", "doctor", "backup", "connection"],
        vec!["doctor", "host", "--help"],
        vec!["doctor", "machine", "example", "--help"],
        vec!["doctor", "sandbox", "example", "--help"],
        vec!["doctor", "store", "integrity", "example", "--help"],
    ] {
        let out = Command::new(CLI)
            .args(&args)
            .env("LAYERFS_HOME", &root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("layerfs doctor"));
        assert!(!root.exists());
    }
}

#[test]
fn invalid_flags_and_targets_are_usage_errors_without_effects() {
    let root = home("invalid");
    assert!(!root.exists());
    for args in [
        vec!["doctor", "store", "example"],
        vec!["doctor", "sandbox"],
        vec!["doctor", "host", "--quick"],
        vec!["doctor", "store", "integrity", "example", "--write-test"],
        vec!["doctor", "machine", "example", "--timeout", "0s"],
        vec!["doctor", "host", "--timeout", "1s", "--timeout", "2s"],
        vec!["doctor", "backup", "connection", "example", "--unexpected"],
    ] {
        let out = Command::new(CLI)
            .args(&args)
            .env("LAYERFS_HOME", &root)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(!root.exists());
    }
}

#[test]
fn every_family_produces_a_real_report_without_creating_an_absent_home() {
    let root = home("families");
    assert!(!root.exists());
    for (args, kind, operation) in [
        (vec!["doctor", "host"], "host", "diagnose"),
        (vec!["doctor", "machine", "missing"], "machine", "diagnose"),
        (vec!["doctor", "sandbox", "missing"], "sandbox", "diagnose"),
        (
            vec!["doctor", "store", "integrity", "missing"],
            "store",
            "integrity",
        ),
        (
            vec!["doctor", "store", "connection", "missing"],
            "store",
            "connection",
        ),
        (
            vec!["doctor", "backup", "connection", "missing"],
            "backup",
            "connection",
        ),
        (
            vec!["doctor", "backup", "connection", "missing", "--write-test"],
            "backup",
            "connection",
        ),
        (
            vec!["doctor", "store", "integrity", "missing", "--quick"],
            "store",
            "integrity",
        ),
    ] {
        let out = Command::new(CLI)
            .args(&args)
            .env("LAYERFS_HOME", &root)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        let raw = String::from_utf8(out.stdout).unwrap();
        let report = layerfs_api::json::parse(&raw).unwrap();
        assert_eq!(report.get("target").unwrap().get_text("kind"), Some(kind));
        assert_eq!(report.get_text("operation"), Some(operation));
        let mode = if args.contains(&"--write-test") {
            "write-test"
        } else if args.contains(&"--quick") {
            "quick"
        } else if operation == "integrity" {
            "full"
        } else {
            "observation"
        };
        assert_eq!(report.get_text("mode"), Some(mode));
        assert!(report.get("complete").is_some());
        assert!(!raw.contains("integration.pending"), "{args:?}");
        assert!(!root.exists(), "{args:?} created a home");
    }
}
