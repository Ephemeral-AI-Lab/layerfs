//! The doctors on the command line (call surface 2.0): `layerfs home
//! doctor` prints the one report (JSON, or `--text`), exits 0 whatever it
//! finds and changes nothing; `layerfs home leftovers` and `layerfs home
//! prune` are gone (spec v20: LayerFS removes leftovers itself).
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
    assert!(help.contains("layerfs home doctor"), "{help}");
    assert!(
        !help.contains("leftovers") && !help.contains("home prune"),
        "{help}"
    );
    std::fs::remove_dir_all(home).unwrap();
}
