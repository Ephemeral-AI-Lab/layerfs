//! The Store directory an install (`install_local`) or a backup restore
//! (`backup::fetch`) prepares before it writes anything (#570 P1). Both call
//! [`layerfs_store_service::install::prepare`]: the directory belongs to the
//! call only when that call's own `mkdir` returned success. A `mkdir` failure
//! claims nothing, and a guard refusal reclaims only that own, still-empty
//! directory, non-recursively.
//!
//! `prepare` starts with the installer-user check, so its own two guard
//! branches need the daemons' user (uid 0), as an install runs in production.
//! Run elsewhere, each case asserts what the same call does at this privilege
//! level: the check refuses before the directory or the guard is touched, so
//! the call creates nothing and removes nothing. `layerfs-store`'s
//! `create_directory` test drives the guard and the same ownership rule
//! through `store.create`, which has no installer-user check, so those run
//! unprivileged.
#![cfg(target_os = "linux")]
use layerfs_store_service::install;
use std::path::{Path, PathBuf};

/// Whether this process is the daemons' user, which installs a Store.
fn daemons_user() -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").is_ok_and(|meta| meta.uid() == 0)
}

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

/// A guard refusal happens before any Store file exists: it reclaims the
/// empty directory this call's own `mkdir` made, and leaves the volume.
/// tmpfs refuses the guard by filesystem, after the `mkdir` succeeded.
#[test]
fn a_refused_guard_removes_only_the_empty_directory_this_call_made() {
    let volume = scratch(Path::new("/dev/shm"), "tmpfs-volume");
    let directory = volume.join("global");
    let destination = directory.join("store.sqlite");
    let refused = install::prepare(&destination).unwrap_err();
    if !daemons_user() {
        // the installer-user check comes first: this call made no directory,
        // so it has none of its own to reclaim
        assert!(refused.contains("must run as user 0"), "{refused}");
        assert!(!directory.exists(), "a refused call made no directory");
        std::fs::remove_dir_all(&volume).unwrap();
        return;
    }
    assert!(refused.contains("ext4, xfs or btrfs"), "{refused}");
    assert!(
        !directory.exists(),
        "the empty directory this call made is reclaimed"
    );
    assert!(volume.is_dir(), "the volume itself is untouched");
    std::fs::remove_dir_all(&volume).unwrap();
}

/// A guard refusal never removes a directory this call did not make, nor the
/// Store another caller put in it.
#[test]
fn a_refused_guard_keeps_a_directory_this_call_did_not_make() {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let volume = scratch(Path::new("/dev/shm"), "tmpfs-foreign");
    let directory = volume.join("global");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let destination = directory.join("store.sqlite");
    std::fs::write(&destination, b"another caller's Store").unwrap();
    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o600)).unwrap();
    let refused = install::prepare(&destination).unwrap_err();
    let guard = daemons_user();
    assert!(
        refused.contains(if guard {
            "ext4, xfs or btrfs"
        } else {
            "must run as user 0"
        }),
        "{refused}"
    );
    assert!(directory.is_dir(), "the other caller's directory stays");
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"another caller's Store"
    );
    std::fs::remove_dir_all(&volume).unwrap();
}
