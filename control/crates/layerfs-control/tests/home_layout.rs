//! `layerfs.json` and the layout version (#533 step 3): an empty home, the
//! current layout, and a foreign folder (MIG), each in its own test.
use layerfs_home::home::{detect, ensure, open, Layout, LAYOUT_VERSION};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-home-{tag}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_missing_or_empty_home_is_empty_and_gets_a_layout_and_id() {
    let root = tempdir("empty");
    let home = root.join("home");
    assert_eq!(detect(&home).unwrap(), Layout::Empty);
    assert!(!home.exists(), "detect writes nothing");
    let file = ensure(&home).unwrap();
    assert_eq!(file.layout, LAYOUT_VERSION);
    assert_eq!(file.id.len(), 32);
    let meta = std::fs::metadata(home.join("layerfs.json")).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    assert_eq!(
        std::fs::metadata(&home).unwrap().permissions().mode() & 0o777,
        0o700
    );
    // a second ensure reads the same id
    assert_eq!(ensure(&home).unwrap(), file);
    assert_eq!(detect(&home).unwrap(), Layout::Current(file));
    // a home of this layout keeps the call log
    layerfs_home::logs::append(&home, &line());
    assert!(layerfs_home::home::paths::calls_file(&home).is_file());
    std::fs::remove_dir_all(root).unwrap();
}

/// One call-log line.
fn line() -> layerfs_bridge::control::SdkValue {
    use layerfs_bridge::control::SdkValue;
    SdkValue::map([("call", SdkValue::text("home.get"))])
}

/// The fix a refusal of a foreign folder carries.
fn fix(refusal: &layerfs_bridge::control::SdkRefusal) -> Option<&str> {
    refusal.detail.get_text("fix")
}

/// Every entry under `dir` with its bytes (folders as `None`), sorted.
fn snapshot(dir: &std::path::Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut all = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all.push((path.clone(), None));
            all.extend(snapshot(&path));
        } else {
            all.push((path.clone(), Some(std::fs::read(&path).unwrap())));
        }
    }
    all.sort();
    all
}

#[test]
fn what_making_a_home_writes_first_still_reads_as_empty() {
    // the home lock `open` takes before `layerfs.json`, and the temporary
    // `layerfs.json` is renamed from (another process mid-write, or a killed one)
    let home = tempdir("own");
    std::fs::write(home.join("layerfs.lock"), "").unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Empty);
    std::fs::write(home.join(".layerfs.json.7.0.tmp"), "{").unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Empty);
    let file = open(&home).unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Current(file));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_folder_with_a_stray_file_and_no_layerfs_json_is_foreign_and_left_as_it_was() {
    let home = tempdir("stray");
    std::fs::write(home.join("notes.txt"), "kept").unwrap();
    std::fs::write(home.join("layerfs.lock"), "").unwrap();
    std::fs::create_dir(home.join("log")).unwrap();
    let before = snapshot(&home);
    assert_eq!(detect(&home).unwrap(), Layout::Foreign);
    for refused in [ensure(&home).unwrap_err(), open(&home).unwrap_err()] {
        assert_eq!(refused.code, "Unsupported");
        assert!(
            refused.message.contains("not a LayerFS 2.0 home"),
            "{}",
            refused.message
        );
        let moved = format!("{}.old", home.display());
        assert_eq!(
            fix(&refused),
            Some(format!("mv {} {moved} && layerfs home init", home.display()).as_str())
        );
    }
    // a call refused there writes no call log into it
    layerfs_home::logs::append(&home, &line());
    assert_eq!(before, snapshot(&home), "the folder was written");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_layerfs_json_of_another_layout_is_foreign_and_a_broken_one_is_refused() {
    let home = tempdir("other");
    let id = "b".repeat(32);
    // any layout but this build's, older or newer: refused with the fix,
    // never converted or written
    for layout in [1, 2, LAYOUT_VERSION + 1] {
        let text = format!("{{\"layout\":{layout},\"id\":\"{id}\"}}");
        std::fs::write(home.join("layerfs.json"), &text).unwrap();
        let before = snapshot(&home);
        assert_eq!(detect(&home).unwrap(), Layout::Foreign, "layout {layout}");
        layerfs_home::logs::append(&home, &line());
        let refused = open(&home).unwrap_err();
        assert_eq!(refused.code, "Unsupported");
        assert!(fix(&refused).is_some_and(|fix| fix.ends_with("&& layerfs home init")));
        assert_eq!(before, snapshot(&home), "layout {layout} was written");
    }
    // a broken `layerfs.json` is refused `Failed`
    for broken in [
        "{\"layout\":3,\"id\":\"xyz\"}".to_owned(),
        format!("{{\"id\":\"{id}\"}}"),
        "not json".to_owned(),
    ] {
        std::fs::write(home.join("layerfs.json"), &broken).unwrap();
        assert_eq!(detect(&home).unwrap_err().code, "Failed", "{broken}");
    }
    std::fs::remove_dir_all(home).unwrap();
}
