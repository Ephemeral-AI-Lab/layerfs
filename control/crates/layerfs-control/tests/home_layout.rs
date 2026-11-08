//! `layerfs.json` and the layout version (#533 step 3): one test per layout.
use layerfs_home::home::{detect, ensure, Layout, LAYOUT_VERSION};
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

#[test]
fn a_1_1_0_home_is_old_and_is_not_written_by_ensure() {
    let home = tempdir("old");
    std::fs::write(home.join("stores.json"), "{}").unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Old);
    let refused = ensure(&home).unwrap_err();
    assert_eq!(refused.code, "InvalidInput");
    assert!(!home.join("layerfs.json").exists());
    std::fs::remove_dir_all(home).unwrap();
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
    // layout 2 is read, and refused by `ensure` until it converts
    std::fs::write(
        home.join("layerfs.json"),
        format!("{{\"layout\":2,\"id\":\"{id}\"}}"),
    )
    .unwrap();
    assert!(matches!(detect(&home).unwrap(), Layout::Previous(file) if file.layout == 2));
    assert_eq!(ensure(&home).unwrap_err().code, "InvalidInput");
    std::fs::write(home.join("layerfs.json"), "not json").unwrap();
    assert_eq!(detect(&home).unwrap_err().code, "Failed");
    std::fs::remove_dir_all(home).unwrap();
}
