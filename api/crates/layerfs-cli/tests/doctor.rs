//! The doctors on the command line (call surface 2.0): `layerfs home
//! doctor` prints the one report (JSON, or `--text`), exits 0 whatever it
//! finds and changes nothing; and `layerfs home leftovers` and `layerfs
//! home prune <leftover>`, which list what earlier runs left and remove one
//! of them (layerfs-project#180).
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
fn home_leftovers_lists_and_home_prune_removes_one() {
    let home = home_path("prune");
    home::open(&home).unwrap();
    // an unpack's temporary folder, its image's lock, and a partly
    // received image (no leftover since spec v19: `machine prune` owns it)
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
    let tmp = images.join(format!(".tmp-{digest}.2-ab12"));
    for folder in [tmp.clone(), tmp.join("usr"), tmp.join("usr/bin")] {
        private_dir(&folder).unwrap();
    }
    std::fs::write(tmp.join("usr/bin/tool"), vec![b'x'; 50]).unwrap();
    let part = images.join(format!(".recv-{digest}.part"));
    std::fs::write(&part, vec![b'x'; 50]).unwrap();
    let then = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
    (std::fs::File::open(&part).unwrap())
        .set_modified(then)
        .unwrap();
    let lock = images.join(format!("{digest}.2.lock"));
    std::fs::write(&lock, "").unwrap();
    let id = format!("native:image_tmp:{digest}.2-ab12");

    // the list names the temporary by its id, with its size; the lock and
    // the part are no leftover
    let out = layerfs(&home, &["home", "leftovers", "--machine", "native"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let listed = layerfs_api::json::parse(String::from_utf8(out.stdout).unwrap().trim()).unwrap();
    let Some(layerfs_bridge::control::SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}");
    };
    assert_eq!(items.len(), 1, "{listed:?}");
    assert_eq!(items[0].get_text("leftover"), Some(id.as_str()));
    assert_eq!(items[0].get_int("size"), Some(50));

    // wrong command lines exit 2, refusals 125 with their code
    assert_eq!(layerfs(&home, &["home", "prune"]).status.code(), Some(2));
    assert_eq!(
        layerfs(&home, &["home", "prune", &id, "more"])
            .status
            .code(),
        Some(2)
    );
    for (wrong, code) in [
        ("native:image_tmp:../../x".to_owned(), "NotFound"),
        (format!("native:image-tmp:{digest}.2-ab12"), "NotFound"),
        (format!("gone:image_tmp:{digest}.2-ab12"), "NotFound"),
        (format!("native:image_part:{digest}"), "NotFound"),
    ] {
        let out = layerfs(&home, &["home", "prune", &wrong]);
        assert_eq!(out.status.code(), Some(125), "{out:?}");
        let refusal = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            refusal.contains(&format!(r#""error":"{code}""#)),
            "{refusal}"
        );
    }
    assert!(tmp.is_dir() && part.is_file() && lock.is_file());

    let out = layerfs(&home, &["home", "prune", &id, "--detail"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let removed = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        removed.trim(),
        format!(
            r#"{{"ok":true,"freed_bytes":null,"outcome":"removed","leftover":"{id}","kind":"image_tmp","machine":"native","path":"{}"}}"#,
            tmp.display()
        )
    );
    assert!(!tmp.exists(), "the temporary is still there");
    assert!(lock.is_file(), "the lock file was removed");
    assert!(part.is_file(), "the part was removed");
    assert_eq!(
        layerfs(&home, &["home", "prune", &id]).status.code(),
        Some(125)
    );
    // the old name is gone (R1)
    assert_eq!(
        layerfs(&home, &["doctor", "fix", &id]).status.code(),
        Some(2)
    );

    // help names both under the home
    let help = String::from_utf8(layerfs(&home, &["help", "home"]).stdout).unwrap();
    assert!(help.contains("layerfs home prune <leftover>\n"), "{help}");
    assert!(
        help.contains("layerfs home leftovers [--machine M]\n"),
        "{help}"
    );
    for words in [&["help", "home", "prune"][..], &["home", "prune", "--help"]] {
        let help = String::from_utf8(layerfs(&home, words).stdout).unwrap();
        assert!(
            help.starts_with("layerfs home prune <leftover>\n"),
            "{help}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}
