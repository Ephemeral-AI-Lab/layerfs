//! Direct Store access is refused `Unsupported` where the Store's locks do
//! not reach every opener (#352 F-020): on macOS, and on Linux for a
//! filesystem outside the allowlist (here tmpfs). The refusal comes before any
//! file is opened: the Store file's bytes are unchanged and no overlay
//! template is created.
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

#[cfg(not(target_os = "linux"))]
#[test]
fn direct_access_is_unsupported_off_linux() {
    let directory = scratch(&std::env::temp_dir(), "host");
    let (reason, before, after) = refused(&directory);
    assert!(reason.contains("Linux"), "{reason}");
    assert_eq!(before, after);
    std::fs::remove_dir_all(directory).unwrap();
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

#[cfg(target_os = "linux")]
#[test]
fn a_read_only_open_is_unsupported_on_a_filesystem_outside_the_allowlist() {
    let directory = scratch(Path::new("/dev/shm"), "tmpfs-ro");
    let store = directory.join("store.sqlite");
    std::fs::write(&store, b"not opened").unwrap();
    let before = std::fs::read(&store).unwrap();
    let outcome = StoreService::open_read_only(&store, b"binding", [7; 32]);
    let reason = match outcome {
        Err(OpenRefusal::Unsupported(reason)) => reason.0,
        Err(other) => panic!("expected Unsupported, got {other:?}"),
        Ok(_) => panic!("expected Unsupported, the Store opened"),
    };
    assert!(reason.contains("ext4, xfs or btrfs"), "{reason}");
    assert_eq!(before, std::fs::read(&store).unwrap());
    let extras: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(extras, [std::ffi::OsString::from("store.sqlite")]);
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_read_only_open_names_a_missing_store() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("layerfs-missing-ro-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let store = directory.join("store.sqlite");
    match StoreService::open_read_only(&store, b"binding", [7; 32]) {
        Err(OpenRefusal::Missing(reason)) => {
            assert!(reason.contains("no installed Store"), "{reason}");
        }
        Err(OpenRefusal::Unsupported(_)) => {
            // The target directory is not on ext4/xfs/btrfs; the guard still
            // ran before any file was created.
        }
        Err(other) => panic!("expected Missing or Unsupported, got {other:?}"),
        Ok(_) => panic!("expected Missing or Unsupported, the Store opened"),
    }
    assert!(!store.exists());
    std::fs::remove_dir_all(directory).unwrap();
}
