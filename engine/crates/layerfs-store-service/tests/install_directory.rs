//! The Store directory an install (`install_local`) or a backup restore
//! (`backup::fetch`) prepares before it writes anything (#570 P1). Both call
//! [`layerfs_store_service::install::prepare`]: the directory belongs to the
//! call only when that call's own `mkdir` returned success. A `mkdir` failure
//! claims nothing, and a guard refusal reclaims only that own, still-empty
//! directory, non-recursively.
use layerfs_store_service::install;
use std::path::{Path, PathBuf};

fn scratch(parent: &Path, label: &str) -> PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let directory = parent.join(format!("layerfs-install-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .unwrap();
    directory
}

/// The race of #570 P1, without a sleep: this call observes the directory
/// absent, another caller creates it and installs its Store there, and this
/// call's `mkdir` then fails. Nothing of the other caller's may be removed.
#[test]
fn a_lost_creation_race_never_removes_the_other_callers_directory() {
    use std::os::unix::fs::DirBuilderExt;
    let parent = scratch(&std::env::temp_dir(), "lost-race");
    let directory = parent.join("global");
    let destination = directory.join("store.sqlite");
    // this call sees no directory yet ...
    assert!(!directory.exists());
    // ... another caller makes the same directory `0700` and installs a Store
    let other = |directory: &Path| {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(directory)
            .unwrap();
        std::fs::write(directory.join("store.sqlite"), b"another caller's Store").unwrap();
    };
    // ... and this call's mkdir fails: it made nothing, so it removes nothing
    let refused = install::prepare_between(&destination, &|| other(&directory)).unwrap_err();
    assert!(refused.contains("File exists"), "{refused}");
    assert!(directory.is_dir(), "the other caller's directory stays");
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"another caller's Store",
        "the other caller's Store stays"
    );
    std::fs::remove_dir_all(parent).unwrap();
}

/// A guard refusal happens before any Store file exists: it reclaims the
/// empty directory this call's own `mkdir` made, and never one it did not
/// make. tmpfs refuses the guard by filesystem, after the `mkdir` succeeded.
#[cfg(target_os = "linux")]
#[test]
fn a_refused_guard_removes_only_the_empty_directory_this_call_made() {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let volume = scratch(Path::new("/dev/shm"), "tmpfs-volume");
    let directory = volume.join("global");
    let destination = directory.join("store.sqlite");
    let refused = install::prepare_between(&destination, &|| {}).unwrap_err();
    assert!(refused.contains("ext4, xfs or btrfs"), "{refused}");
    assert!(
        !directory.exists(),
        "the empty directory this call made is reclaimed"
    );
    assert!(volume.is_dir(), "the volume itself is untouched");

    // a directory this call did not make, and what it holds, stay
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    std::fs::write(&destination, b"another caller's Store").unwrap();
    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o600)).unwrap();
    let refused = install::prepare_between(&destination, &|| {}).unwrap_err();
    assert!(refused.contains("ext4, xfs or btrfs"), "{refused}");
    assert!(directory.is_dir(), "the other caller's directory stays");
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"another caller's Store"
    );
    std::fs::remove_dir_all(volume).unwrap();
}
