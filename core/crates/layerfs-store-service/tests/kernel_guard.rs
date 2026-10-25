//! Direct Store access is refused `Unsupported` where the Store's locks do
//! not reach every opener (#352 F-020): on macOS always, on Linux for a
//! filesystem outside the allowlist (here tmpfs), and on Windows for a
//! spelling outside the NTFS-on-local-volume allowlist (here UNC). The
//! refusal comes before any file is opened: the Store file's bytes are
//! unchanged and no overlay template is created.
use layerfs_store_service::{OpenRefusal, StoreService};
use std::path::{Path, PathBuf};

fn refused(directory: &Path) -> (String, Vec<u8>, Vec<u8>) {
    let store = directory.join("store.sqlite");
    std::fs::write(&store, b"not opened").unwrap();
    let template = directory.join("overlay.sqlite");
    let before = std::fs::read(&store).unwrap();
    let outcome = StoreService::open_direct(&store, b"binding", [7; 32], &template);
    let reason = match outcome {
        Err(OpenRefusal::Unsupported(reason)) => reason.0,
        Err(other) => panic!("expected Unsupported, got {other:?}"),
        Ok(_) => panic!("expected Unsupported, the Store opened"),
    };
    assert!(
        !template.exists(),
        "no overlay template before the guard passes"
    );
    (reason, before, std::fs::read(&store).unwrap())
}

fn scratch(parent: &Path, label: &str) -> PathBuf {
    let directory = parent.join(format!("layerfs-guard-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[cfg(target_os = "macos")]
#[test]
fn direct_access_is_unsupported_off_linux() {
    let directory = scratch(&std::env::temp_dir(), "host");
    let (reason, before, after) = refused(&directory);
    assert!(reason.contains("Linux"), "{reason}");
    assert_eq!(before, after);
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(target_os = "windows")]
#[test]
fn direct_access_is_unsupported_on_a_non_local_spelling() {
    // A UNC spelling never reaches the NTFS-on-local-volume allowlist; the
    // refusal comes before any file is opened, so no template exists to
    // leave behind.
    let store = Path::new(r"\\localhost\c$\layerfs-guard-nonlocal\store.sqlite");
    let template = Path::new(r"\\localhost\c$\layerfs-guard-nonlocal\overlay.sqlite");
    match StoreService::open_direct(store, b"binding", [7; 32], template) {
        Err(OpenRefusal::Unsupported(reason)) => assert!(
            reason.0.contains("cannot be examined"),
            "{reason}"
        ),
        Err(other) => panic!("expected Unsupported, got {other:?}"),
        Ok(_) => panic!("expected Unsupported, the Store opened"),
    }
    assert!(
        !template.exists(),
        "no overlay template before the guard passes"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn direct_access_is_unsupported_on_a_filesystem_outside_the_allowlist() {
    let directory = scratch(Path::new("/dev/shm"), "tmpfs");
    let (reason, before, after) = refused(&directory);
    assert!(reason.contains("ext4, xfs or btrfs"), "{reason}");
    assert_eq!(before, after);
    std::fs::remove_dir_all(directory).unwrap();
}
