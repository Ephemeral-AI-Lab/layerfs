//! `layerfs.json` and the layout version (#533 step 3): an empty home, the
//! current layout, a foreign or unparsable `layerfs.json` (MIG, RC-8) and one
//! the system refuses to read, each in its own test.
use layerfs_home::disk::{detect, ensure, open, Layout, LAYOUT_VERSION};
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
    layerfs_home::log::calls::append(&home, &line());
    assert!(layerfs_home::disk::paths::calls_file(&home).is_file());
    std::fs::remove_dir_all(root).unwrap();
}

/// One call-log line.
fn line() -> layerfs_protocol::sdk::SdkValue {
    use layerfs_protocol::sdk::SdkValue;
    SdkValue::map([("call", SdkValue::text("home.get"))])
}

/// The fix a refusal of a foreign folder carries.
fn fix(refusal: &layerfs_protocol::sdk::SdkRefusal) -> Option<&str> {
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
fn integration_folders_and_ds_store_read_as_empty_and_init_leaves_them() {
    // an integration's reserved folders, made before the first call (as
    // `@layerfs/runtime` makes `runtime/`), and macOS's `.DS_Store`
    let home = tempdir("integration");
    std::fs::write(home.join("layerfs.lock"), "").unwrap();
    for folder in ["runtime", "job"] {
        std::fs::create_dir(home.join(folder)).unwrap();
        std::fs::write(home.join(folder).join("kept"), folder).unwrap();
    }
    std::fs::write(home.join(".DS_Store"), "finder").unwrap();
    let before = snapshot(&home);
    assert_eq!(detect(&home).unwrap(), Layout::Empty);
    let created = layerfs_inspect::home::init::init(&home).unwrap();
    assert_eq!(created.get_text("outcome"), Some("created"));
    assert!(matches!(detect(&home).unwrap(), Layout::Current(_)));
    let after = snapshot(&home);
    for entry in &before {
        assert!(after.contains(entry), "{:?} changed", entry.0);
    }
    // a reserved name that is a file, not a folder, is not the integration's
    let other = tempdir("integration-file");
    std::fs::write(other.join("runtime"), "a file").unwrap();
    assert_eq!(detect(&other).unwrap(), Layout::Foreign);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(other).unwrap();
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
    layerfs_home::log::calls::append(&home, &line());
    assert_eq!(before, snapshot(&home), "the folder was written");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_layerfs_json_of_another_layout_or_unparsable_is_foreign() {
    let home = tempdir("other");
    let id = "b".repeat(32);
    // any layout but this build's, older or newer: refused with the fix,
    // never converted or written
    for layout in [1, 2, LAYOUT_VERSION + 1] {
        let text = format!("{{\"layout\":{layout},\"id\":\"{id}\"}}");
        std::fs::write(home.join("layerfs.json"), &text).unwrap();
        let before = snapshot(&home);
        assert_eq!(detect(&home).unwrap(), Layout::Foreign, "layout {layout}");
        layerfs_home::log::calls::append(&home, &line());
        let refused = open(&home).unwrap_err();
        assert_eq!(refused.code, "Unsupported");
        assert!(fix(&refused).is_some_and(|fix| fix.ends_with("&& layerfs home init")));
        assert_eq!(before, snapshot(&home), "layout {layout} was written");
    }
    // one that cannot be parsed (not JSON, no layout number, this layout
    // with a bad id) is not a 2.0 home's either: the same refusal (MIG, RC-8)
    for broken in [
        "{not json".to_owned(),
        "{}".to_owned(),
        "not json".to_owned(),
        format!("{{\"id\":\"{id}\"}}"),
        "{\"layout\":3,\"id\":\"xyz\"}".to_owned(),
        "{\"layout\":3}".to_owned(),
    ] {
        std::fs::write(home.join("layerfs.json"), &broken).unwrap();
        let before = snapshot(&home);
        assert_eq!(detect(&home).unwrap(), Layout::Foreign, "{broken}");
        layerfs_home::log::calls::append(&home, &line());
        let refused = open(&home).unwrap_err();
        assert_eq!(refused.code, "Unsupported", "{broken}");
        assert_eq!(
            fix(&refused),
            Some(layerfs_home::disk::layout::foreign_fix(&home).as_str())
        );
        assert_eq!(before, snapshot(&home), "{broken} was written");
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_layerfs_json_the_system_refuses_to_read_is_refused_unreadable() {
    let home = tempdir("unreadable");
    // a folder in its place: the read is refused with EISDIR, for root too
    std::fs::create_dir(home.join("layerfs.json")).unwrap();
    let refused = detect(&home).unwrap_err();
    assert_eq!(refused.code, "Failed");
    assert!(refused.detail.get_bool("unreadable"), "{refused:?}");
    assert!(
        refused.message.ends_with("Is a directory (os error 21)"),
        "{}",
        refused.message
    );
    assert_eq!(fix(&refused), None);
    std::fs::remove_dir_all(home).unwrap();
}
