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
//! the call creates nothing and removes nothing. The claim step itself is
//! [`install::make_private_directory`], one function shared with
//! `store.create`'s `make_empty`; it has no installer-user check, so the test
//! below drives both of its branches and its failed `mkdir` unprivileged,
//! and `layerfs-store`'s `create_directory` test drives the same step and the
//! guard through `store.create`.
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

/// The claim step on its own, as any user: an absent directory is made
/// `0700` and belongs to this call, one that is already there keeps its mode
/// and belongs to no one, and a `mkdir` that fails on a taken name claims
/// nothing. `prepare` acts on exactly this answer, and `store.create`'s
/// `make_empty` uses the same step.
#[test]
fn the_directory_claim_creates_0700_and_marks_only_its_own_mkdir() {
    use std::os::unix::fs::PermissionsExt;
    let volume = scratch(&std::env::temp_dir(), "claim");
    let made = volume.join("made");
    assert_eq!(
        install::make_private_directory(&made).unwrap(),
        install::Made::ThisCall
    );
    let mode = std::fs::symlink_metadata(&made)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o700, "this call's own directory is private");
    std::fs::set_permissions(&made, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        install::make_private_directory(&made).unwrap(),
        install::Made::AlreadyThere
    );
    let mode = std::fs::symlink_metadata(&made)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(
        mode, 0o755,
        "a directory this call did not make keeps its mode"
    );
    // a taken name: `Path::exists` looks through the dangling link and sees
    // nothing, so the step reaches `mkdir`, which claims no directory
    let taken = volume.join("taken");
    std::os::unix::fs::symlink(volume.join("absent-target"), &taken).unwrap();
    let refused = install::make_private_directory(&taken).unwrap_err();
    assert_eq!(
        refused.kind(),
        std::io::ErrorKind::AlreadyExists,
        "{refused}"
    );
    assert!(
        std::fs::symlink_metadata(&taken)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the taken name stays"
    );
    std::fs::remove_dir_all(&volume).unwrap();
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

/// A `mkdir` that fails claims nothing: the destination's directory name is
/// already taken, by a dangling symbolic link here — `Path::exists` looks
/// through the link and sees nothing, so the call reaches `mkdir`, which
/// returns `AlreadyExists`. The call removes neither that name nor anything
/// behind it, and creates no target of its own.
#[test]
fn a_mkdir_failure_claims_nothing() {
    let volume = scratch(Path::new("/dev/shm"), "taken-name");
    let directory = volume.join("global");
    let absent_target = volume.join("link-target");
    std::os::unix::fs::symlink(&absent_target, &directory).unwrap();
    let destination = directory.join("store.sqlite");
    let refused = install::prepare(&destination).unwrap_err();
    if !daemons_user() {
        // the installer-user check comes first: this call never reached its
        // `mkdir`, and changes nothing
        assert!(refused.contains("must run as user 0"), "{refused}");
    } else {
        assert!(refused.contains("File exists"), "{refused}");
    }
    let link = std::fs::symlink_metadata(&directory).unwrap();
    assert!(link.file_type().is_symlink(), "the taken name stays");
    assert_eq!(std::fs::read_link(&directory).unwrap(), absent_target);
    assert!(
        std::fs::symlink_metadata(&absent_target).is_err(),
        "the call creates no directory of its own"
    );
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
