//! The home's generation (call surface 2.0): every entity record write in
//! a home moves it once, through the one shared writer (`write_entity`),
//! and nothing else does: a command's records, an observation file, a
//! plain record file and any record under a folder that is not a home.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{generation, observed, write_entity, write_record};
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A fresh home: a folder with its `layerfs.json`.
fn home() -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "layerfs-generation-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("layerfs.json"), "{}").unwrap();
    home
}

fn record() -> SdkValue {
    SdkValue::map([("name", SdkValue::text("x"))])
}

/// Writes entity record `at` (relative to `home`) and answers how far the
/// generation moved.
fn moved_by(home: &Path, at: &str) -> i64 {
    let before = generation::read(home);
    write_entity(&home.join(at), &record()).unwrap();
    generation::read(home) - before
}

#[test]
fn each_record_kind_moves_it_once() {
    let home = home();
    for at in [
        "machine/dev/record.json",
        "store/shop/record.json",
        "store/shop/project/api/record.json",
        "sandbox/tests/record.json",
        "sandbox/tests/workspace/0a1b.json",
        "backup/r2-main/record.json",
        // a record kind kept the same way gets it with no line of its own
        "auth/main/record.json",
    ] {
        assert_eq!(moved_by(&home, at), 1, "{at}");
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn what_is_not_a_home_record_moves_nothing() {
    let home = home();
    for at in [
        // a command's records under a Workspace, and its cancel mark
        "sandbox/tests/workspace/0a1b/exec/77.json",
        "sandbox/tests/workspace/0a1b/exec/77.cancel",
        // a record deeper than its kind keeps them
        "machine/dev/run/record.json",
        // a file at the top of the home
        "record.json",
    ] {
        assert_eq!(moved_by(&home, at), 0, "{at}");
    }
    // a plain record file and an observation file move nothing either
    let before = generation::read(&home);
    write_record(&home.join("machine/dev/worker.json"), &record()).unwrap();
    observed::write(&home.join("machine/dev/record.json"), 1).unwrap();
    assert_eq!(generation::read(&home), before);
    // a machine's worker root keeps records too, and is no home
    let root = home.join("machine/dev/root");
    write_entity(&root.join("sandbox/tests/record.json"), &record()).unwrap();
    assert_eq!(generation::read(&home), before);
    assert!(!root.join("generation").exists());
    let _ = std::fs::remove_dir_all(&home);
}
