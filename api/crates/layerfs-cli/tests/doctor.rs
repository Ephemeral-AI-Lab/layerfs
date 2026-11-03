//! `layerfs doctor` (#536 H7): a top-level command beside `import-env`
//! that prints the report (JSON, or `--text`), exits 0 whatever it finds,
//! and changes nothing.
use layerfs_api::home::{self, paths, private_dir};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const LAYERFS: &str = env!("CARGO_BIN_EXE_layerfs");

fn home_path(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-cli-doctor-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn layerfs(home: &Path, words: &[&str]) -> std::process::Output {
    Command::new(LAYERFS)
        .args(words)
        .env("LAYERFS_HOME", home)
        .output()
        .unwrap()
}

#[test]
fn doctor_reports_a_clean_home_and_makes_none() {
    let home = home_path("clean");
    let out = layerfs(&home, &["doctor"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.trim(), r#"{"ok":true,"findings":[]}"#);
    assert!(!home.exists(), "the doctor made the home");
    let text = layerfs(&home, &["doctor", "--text"]);
    assert_eq!(text.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&text.stdout).starts_with("ok"));
}

#[test]
fn doctor_text_names_each_finding_and_its_fix() {
    let home = home_path("bad");
    home::open(&home).unwrap();
    let bad = paths::store_record(&home, "bad");
    private_dir(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, "{not json").unwrap();
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o600)).unwrap();
    let out = layerfs(&home, &["doctor", "--text"]);
    // a report, not a refusal: exit 0, `ok` false
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("1 error(s), 0 warning(s)"), "{text}");
    assert!(text.contains("error    store bad:"), "{text}");
    assert!(text.contains("fix: no call reads it"), "{text}");
    let json = layerfs(&home, &["doctor"]);
    assert!(String::from_utf8_lossy(&json.stdout).starts_with(r#"{"ok":false,"#));
    assert_eq!(layerfs(&home, &["doctor", "--all"]).status.code(), Some(2));
    std::fs::remove_dir_all(home).unwrap();
}
