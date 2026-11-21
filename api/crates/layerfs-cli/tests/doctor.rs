//! The doctors on the command line (call surface 2.0): `layerfs home
//! doctor` prints the one report (JSON, or `--text`), exits 0 whatever it
//! finds and changes nothing; `layerfs home leftovers` and `layerfs home
//! prune` are gone (spec v20: LayerFS removes leftovers itself).
use layerfs_api::home::{self, paths, private_dir};
use layerfs_bridge::control::SdkValue;
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
fn home_doctor_reports_an_absent_home_and_makes_none() {
    let home = home_path("clean");
    let out = layerfs(&home, &["home", "doctor", "--detail"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let report = layerfs_api::json::parse(String::from_utf8(out.stdout).unwrap().trim()).unwrap();
    let tree = report.get("tree").unwrap();
    assert_eq!(tree.get_text("coverage"), Some("shallow"));
    assert!(!home.exists(), "the doctor made the home");
    let text = layerfs(&home, &["home", "doctor", "--text"]);
    assert_eq!(text.status.code(), Some(0));
    let text = String::from_utf8(text.stdout).unwrap();
    // the count line, or `ok` alone when nothing was found
    assert!(
        text.starts_with("ok") || text.contains(" error(s), "),
        "{text}"
    );
    assert!(!home.exists(), "the doctor made the home");
}

#[test]
fn home_doctor_text_names_each_finding_and_its_fix() {
    let home = home_path("bad");
    home::open(&home).unwrap();
    let bad = paths::store_record(&home, "bad");
    private_dir(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, "{not json").unwrap();
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o600)).unwrap();
    let out = layerfs(&home, &["home", "doctor", "--text"]);
    // a report, not a refusal: exit 0
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    // the home's records check names it, and so does the Store's own record
    // check on its broken node (conformance R4 F5)
    assert!(text.starts_with("2 error(s), "), "{text}");
    assert!(text.contains("error    store bad home.records: "), "{text}");
    assert!(text.contains("error    store bad store.record: "), "{text}");
    // no shell line repairs a record that does not parse: no fix line
    assert!(text.contains("no call reads it"), "{text}");
    assert!(!text.contains("fix: "), "{text}");
    let json = layerfs(&home, &["home", "doctor"]);
    let json = String::from_utf8_lossy(&json.stdout).into_owned();
    assert!(json.starts_with(r#"{"findings":["#), "{json}");
    assert!(
        json.contains(r#"{"store":"bad","check":"home.records","severity":"error","#),
        "{json}"
    );
    assert_eq!(
        layerfs(&home, &["home", "doctor", "--all"]).status.code(),
        Some(2)
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn doctor_fail_on_error_gates_errors_and_unchecks() {
    let home = home_path("fail-on");
    // an absent home has no disk of its own to measure: `home.capacity` is
    // not checked, which `--fail-on error` gates and `never` (the default)
    // does not
    for (words, want) in [
        (&["home", "doctor", "--text"][..], 0),
        (&["home", "doctor", "--text", "--fail-on", "never"], 0),
        (&["home", "doctor", "--text", "--fail-on", "error"], 1),
        (&["home", "doctor", "--json", "--fail-on", "error"], 1),
    ] {
        let out = layerfs(&home, words);
        assert_eq!(out.status.code(), Some(want), "{words:?}: {out:?}");
    }
    // an error finding is reported on stdout and gates the same way
    home::open(&home).unwrap();
    let bad = paths::store_record(&home, "bad");
    private_dir(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, "{not json").unwrap();
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o600)).unwrap();
    let out = layerfs(&home, &["home", "doctor", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let out = layerfs(&home, &["home", "doctor", "--json", "--fail-on", "error"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let report = layerfs_api::json::parse(String::from_utf8(out.stdout).unwrap().trim()).unwrap();
    let findings = match report.get("findings") {
        Some(SdkValue::List(rows)) => rows.as_slice(),
        _ => &[],
    };
    assert!(
        findings
            .iter()
            .any(|finding| finding.get_text("severity") == Some("error")),
        "{report:?}"
    );
    // an unknown word is a wrong command line
    let out = layerfs(&home, &["home", "doctor", "--fail-on", "sometimes"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("never or error"), "{err}");
    // a refusal stays 125, whatever the gate
    assert_eq!(
        layerfs(
            &home,
            &["project", "doctor", "shop/none", "--fail-on", "error"]
        )
        .status
        .code(),
        Some(125)
    );
    // the switch belongs to a doctor, not to every call
    assert_eq!(
        layerfs(&home, &["store", "list", "--fail-on", "error"])
            .status
            .code(),
        Some(2)
    );
    std::fs::remove_dir_all(home).unwrap();
}

/// `--fail-on error` reads the report's two lists: an error finding or a
/// check that could not be made fails it; a warning or a notice does not.
#[test]
fn fail_on_error_ignores_warnings_and_notices() {
    let report = |findings: Vec<SdkValue>, not_checked: Vec<SdkValue>| {
        SdkValue::map([
            ("findings", SdkValue::List(findings)),
            ("not_checked", SdkValue::List(not_checked)),
        ])
    };
    let finding = |severity: &str| SdkValue::map([("severity", SdkValue::text(severity))]);
    assert!(layerfs_cli::output::doctor_failed(&report(
        vec![finding("error")],
        vec![]
    )));
    assert!(!layerfs_cli::output::doctor_failed(&report(
        vec![finding("warning")],
        vec![]
    )));
    assert!(!layerfs_cli::output::doctor_failed(&report(
        vec![finding("notice")],
        vec![]
    )));
    let open = SdkValue::map([("reason", SdkValue::text("timeout"))]);
    assert!(layerfs_cli::output::doctor_failed(&report(
        vec![],
        vec![open]
    )));
}

#[test]
fn home_leftovers_and_home_prune_are_gone() {
    // spec v20: LayerFS removes leftovers itself; the two calls are gone
    // with no alias, and so is the name before them (R1)
    let home = home_path("gone");
    home::open(&home).unwrap();
    for words in [
        &["home", "leftovers"][..],
        &["home", "leftovers", "--machine", "native"],
        &["home", "prune", "native:image_tmp:x"],
        &["doctor", "fix", "native:image_tmp:x"],
    ] {
        let out = layerfs(&home, words);
        assert_eq!(out.status.code(), Some(2), "{words:?}: {out:?}");
    }
    let help = String::from_utf8(layerfs(&home, &["help", "home"]).stdout).unwrap();
    assert!(
        help.lines()
            .any(|l| l.split_whitespace().next() == Some("doctor")),
        "{help}"
    );
    assert!(
        !help.contains("leftovers") && !help.contains("home prune"),
        "{help}"
    );
    std::fs::remove_dir_all(home).unwrap();
}
