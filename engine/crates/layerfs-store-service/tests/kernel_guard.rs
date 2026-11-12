//! Direct Store access is refused `Unsupported` where the Store's locks do
//! not reach every opener (#352 F-020): off Linux and macOS, and on Linux for
//! a filesystem outside the allowlist (here tmpfs). The refusal comes before
//! any file is opened: the Store file's bytes are unchanged and no overlay
//! template is created. On macOS a local apfs or hfs directory is allowed.
use layerfs_store_service::{OpenRefusal, StoreService};
use std::path::{Path, PathBuf};

#[cfg(not(target_os = "macos"))]
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
    use std::os::unix::fs::DirBuilderExt;
    let directory = parent.join(format!("layerfs-guard-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .unwrap();
    directory
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn missing_store_directory_is_refused_without_creating_it_and_non_private_directory_is_refused() {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let parent = scratch(&std::env::temp_dir(), "private-mode");
    let missing = parent.join("global");
    assert!(!missing.exists());
    let refused_missing = layerfs_store_service::kernel_guard::check(&missing.join("store.sqlite"))
        .expect_err("missing directory must be refused without creating it");
    assert!(
        refused_missing.0.contains(&missing.display().to_string()),
        "{refused_missing}"
    );
    assert!(
        !missing.exists(),
        "kernel_guard::check must not create a missing directory"
    );
    let template = parent.join("overlay.sqlite");
    let open_missing = StoreService::open_direct(
        &missing.join("store.sqlite"),
        b"binding",
        [7; 32],
        &template,
    );
    match open_missing {
        Err(OpenRefusal::Unsupported(_)) => {}
        Err(other) => panic!("expected OpenRefusal::Unsupported, got {other:?}"),
        Ok(_) => panic!("expected OpenRefusal::Unsupported, Store opened"),
    }
    assert!(
        !missing.exists(),
        "open_direct on a missing Store directory must not leave an empty directory"
    );

    // An existing 0700 directory passes, even for a non-.sqlite filename.
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&missing)
        .unwrap();
    let checked = layerfs_store_service::kernel_guard::check(&missing.join("db"));
    assert!(checked.is_ok(), "{checked:?}");

    // An existing directory with group/other access (e.g. 0755 from default
    // umask) is refused up front with its path, current mode, expected 0700
    // and chmod command, without modifying the user's directory permissions.
    std::fs::set_permissions(&missing, std::fs::Permissions::from_mode(0o755)).unwrap();
    let refused = layerfs_store_service::kernel_guard::check(&missing.join("store.sqlite"))
        .expect_err("0755 directory must be refused");
    assert!(
        refused.0.contains(&missing.display().to_string()),
        "{refused}"
    );
    assert!(refused.0.contains("0755"), "{refused}");
    assert!(refused.0.contains("0700"), "{refused}");
    assert!(
        refused
            .0
            .contains(&format!("chmod 700 {}", missing.display())),
        "{refused}"
    );
    let unchanged = std::fs::metadata(&missing).unwrap().permissions().mode() & 0o7777;
    assert_eq!(unchanged, 0o755);
    std::fs::remove_dir_all(parent).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn direct_access_is_allowed_on_a_local_macos_filesystem() {
    let directory = scratch(&std::env::temp_dir(), "apfs");
    let checked = layerfs_store_service::kernel_guard::check(&directory.join("store.sqlite"));
    assert!(matches!(checked, Ok("apfs") | Ok("hfs")), "{checked:?}");
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
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
