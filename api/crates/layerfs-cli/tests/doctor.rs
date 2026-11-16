//! `layerfs doctor` (#536 H7): a top-level command beside `import-env`
//! that prints the report (JSON, or `--text`), exits 0 whatever it finds,
//! and changes nothing; and `layerfs doctor fix <id>`, the table call that
//! removes one leftover the report listed (layerfs-project#180).
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

#[test]
fn doctor_fix_removes_the_leftover_the_report_listed() {
    let home = home_path("fix");
    home::open(&home).unwrap();
    // a partly received image, last written two hours ago, and its lock
    let images = paths::run_root(&home).join("images");
    for folder in [
        home.join("machine"),
        home.join("machine/native"),
        paths::run_root(&home),
        images.clone(),
    ] {
        private_dir(&folder).unwrap();
    }
    let digest = "ab".repeat(32);
    let part = images.join(format!(".recv-{digest}.part"));
    std::fs::write(&part, vec![b'x'; 50]).unwrap();
    let then = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
    (std::fs::File::open(&part).unwrap())
        .set_modified(then)
        .unwrap();
    let lock = images.join(format!("{digest}.2.lock"));
    std::fs::write(&lock, "").unwrap();
    let id = format!("native:image-part:{digest}");

    // the report: a warning with the command that removes it, exit 0
    let text = layerfs(&home, &["doctor", "--text"]);
    assert_eq!(text.status.code(), Some(0), "{text:?}");
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.starts_with("0 error(s), 1 warning(s)"), "{text}");
    assert!(text.contains("warning  machine native:"), "{text}");
    assert!(
        text.contains(&format!("`layerfs doctor fix {id}`")),
        "{text}"
    );
    let json = String::from_utf8(layerfs(&home, &["doctor"]).stdout).unwrap();
    assert!(json.contains(&format!(r#""id":"{id}""#)), "{json}");
    assert!(json.contains(r#""bytes":50"#), "{json}");
    assert!(json.contains(r#""image_locks":1"#), "{json}");

    // wrong command lines exit 2, refusals 125 with their code
    assert_eq!(layerfs(&home, &["doctor", "fix"]).status.code(), Some(2));
    assert_eq!(
        layerfs(&home, &["doctor", "fix", &id, "more"])
            .status
            .code(),
        Some(2)
    );
    for (wrong, code) in [
        ("native:image-part:../x".to_owned(), "NotFound"),
        (format!("native:image-lock:{digest}.2"), "InvalidInput"),
        (format!("gone:image-part:{digest}"), "NotFound"),
    ] {
        let out = layerfs(&home, &["doctor", "fix", &wrong]);
        assert_eq!(out.status.code(), Some(125), "{out:?}");
        let refusal = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            refusal.contains(&format!(r#""code":"{code}""#)),
            "{refusal}"
        );
    }
    assert!(part.is_file() && lock.is_file());

    let out = layerfs(&home, &["doctor", "fix", &id]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let removed = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        removed.trim(),
        format!(
            r#"{{"kind":"image-part","path":"{}","removed":true,"bytes":50,"machine":"native","id":"{id}"}}"#,
            part.display()
        )
    );
    assert!(!part.exists(), "the part is still there");
    assert!(lock.is_file(), "the lock file was removed");
    let after = String::from_utf8(layerfs(&home, &["doctor"]).stdout).unwrap();
    assert!(after.starts_with(r#"{"ok":true,"findings":[],"#), "{after}");
    assert_eq!(
        layerfs(&home, &["doctor", "fix", &id]).status.code(),
        Some(125)
    );

    // help names it under the doctor and as its own topic
    let help = String::from_utf8(layerfs(&home, &["help", "doctor"]).stdout).unwrap();
    assert!(help.contains("layerfs doctor fix <id>\n"), "{help}");
    for words in [&["help", "doctor", "fix"][..], &["doctor", "fix", "--help"]] {
        let help = String::from_utf8(layerfs(&home, words).stdout).unwrap();
        assert!(help.starts_with("layerfs doctor fix <id>\n"), "{help}");
    }
    std::fs::remove_dir_all(home).unwrap();
}
