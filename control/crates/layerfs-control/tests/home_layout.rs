//! `layerfs.json` and the layout version (#533 step 3): one test per layout.
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
    std::fs::remove_dir_all(root).unwrap();
}

/// The fix a refusal of an older home carries.
fn fix(refusal: &layerfs_bridge::control::SdkRefusal) -> Option<&str> {
    refusal.detail.get_text("fix")
}

#[test]
fn a_1_1_0_home_is_older_and_refused_with_a_fix_never_written() {
    let home = tempdir("old");
    std::fs::write(home.join("stores.json"), "{}").unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Older);
    for refused in [ensure(&home).unwrap_err(), open(&home).unwrap_err()] {
        assert_eq!(refused.code, "Unsupported");
        let moved = format!("{}.old", home.display());
        assert_eq!(
            fix(&refused),
            Some(format!("mv {} {moved} && layerfs home init", home.display()).as_str())
        );
    }
    assert!(!home.join("layerfs.json").exists());
    // a folder holding something else and no 1.1.0 record is a new home
    let other = tempdir("other");
    std::fs::write(other.join("notes.txt"), "kept").unwrap();
    assert_eq!(detect(&other).unwrap(), Layout::Empty);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(other).unwrap();
}

#[test]
fn a_newer_or_broken_layout_is_refused() {
    let home = tempdir("newer");
    std::fs::write(
        home.join("layerfs.json"),
        format!(
            "{{\"layout\":{},\"id\":\"{}\"}}",
            LAYOUT_VERSION + 1,
            "a".repeat(32)
        ),
    )
    .unwrap();
    assert_eq!(detect(&home).unwrap_err().code, "Unsupported");
    std::fs::write(home.join("layerfs.json"), "{\"layout\":3,\"id\":\"xyz\"}").unwrap();
    assert_eq!(detect(&home).unwrap_err().code, "Failed");
    let id = "b".repeat(32);
    std::fs::write(
        home.join("layerfs.json"),
        format!("{{\"layout\":1,\"id\":\"{id}\"}}"),
    )
    .unwrap();
    assert_eq!(detect(&home).unwrap_err().code, "Failed");
    // layout 2 is an older home: refused with the fix, never converted
    std::fs::write(
        home.join("layerfs.json"),
        format!("{{\"layout\":2,\"id\":\"{id}\"}}"),
    )
    .unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Older);
    let refused = open(&home).unwrap_err();
    assert_eq!(refused.code, "Unsupported");
    assert!(fix(&refused).is_some_and(|fix| fix.ends_with("&& layerfs home init")));
    assert_eq!(
        std::fs::read_to_string(home.join("layerfs.json")).unwrap(),
        format!("{{\"layout\":2,\"id\":\"{id}\"}}")
    );
    std::fs::write(home.join("layerfs.json"), "not json").unwrap();
    assert_eq!(detect(&home).unwrap_err().code, "Failed");
    std::fs::remove_dir_all(home).unwrap();
}
