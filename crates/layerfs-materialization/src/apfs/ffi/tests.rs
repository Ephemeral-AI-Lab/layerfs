use super::*;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn compact_xattr_name_index_sorts_without_per_name_allocations() {
    let bytes = b"z\0alpha\0middle\0".to_vec();
    let mut names = XattrNames {
        bytes,
        offsets: vec![vec![0, 2, 8]],
        count: 3,
    };
    names.sort();
    assert_eq!(
        names.iter().collect::<Vec<_>>(),
        [b"alpha".as_slice(), b"middle".as_slice(), b"z".as_slice()]
    );
}

#[test]
fn entry_token_changes_for_an_in_place_live_writer() {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-token-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("file"), b"before").unwrap();
    let parent = File::open(&directory).unwrap();
    let before = token_at(&parent, b"file").unwrap();
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(directory.join("file"))
        .unwrap()
        .write_all(b"after-content")
        .unwrap();
    let after = token_at(&parent, b"file").unwrap();
    assert_ne!(before, after);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn verified_tombstone_cleanup_clears_immutable_and_append_flags() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-flags-cleanup-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    fs::create_dir(base.join("owned")).unwrap();
    fs::write(base.join("owned/file"), b"content").unwrap();
    let parent = File::open(&base).unwrap();
    let root = open_directory_at(&parent, b"owned").unwrap();
    let child = open_entry_at(&root, b"file").unwrap();
    set_flags_file(&child, 0x0000_0006).unwrap();
    set_flags_file(&root, 0x0000_0006).unwrap();
    let identity = file_stable_token(&root).unwrap();
    detach_and_remove_owned_tree(&root, &parent, b"owned", b"private-tombstone", &identity)
        .unwrap();
    assert!(!base.join("owned").exists());
    assert!(!base.join("private-tombstone").exists());
    fs::remove_dir(base).unwrap();
}

#[test]
fn apfs_clone_temp_supports_independent_same_offset_patch() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-clone-patch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    fs::create_dir(base.join("staging")).unwrap();
    fs::write(base.join("source"), vec![0x5a; 1024 * 1024]).unwrap();
    let source = File::open(base.join("source")).unwrap();
    let staging = File::open(base.join("staging")).unwrap();
    let mut cloned = clone_file_at(&source, &staging, b"clone").unwrap();
    use std::io::{Seek, SeekFrom};
    cloned.seek(SeekFrom::Start(4096)).unwrap();
    cloned.write_all(b"PATCH").unwrap();
    cloned.sync_all().unwrap();
    assert_eq!(
        &fs::read(base.join("source")).unwrap()[4096..4101],
        &[0x5a; 5]
    );
    assert_eq!(
        &fs::read(base.join("staging/clone")).unwrap()[4096..4101],
        b"PATCH"
    );
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn private_cleanup_neutralizes_mode_zero_and_deny_delete_acl() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-restrictive-cleanup-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    fs::create_dir_all(base.join("owned/child")).unwrap();
    fs::write(base.join("owned/child/file"), b"content").unwrap();
    assert!(Command::new("chmod")
        .args(["+a", "everyone deny delete"])
        .arg(base.join("owned"))
        .status()
        .unwrap()
        .success());
    assert!(Command::new("chmod")
        .args(["+a", "everyone deny delete_child"])
        .arg(base.join("owned/child"))
        .status()
        .unwrap()
        .success());
    let parent = File::open(&base).unwrap();
    let root = open_directory_at(&parent, b"owned").unwrap();
    fs::set_permissions(
        base.join("owned/child/file"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    fs::set_permissions(base.join("owned/child"), fs::Permissions::from_mode(0o000)).unwrap();
    fs::set_permissions(base.join("owned"), fs::Permissions::from_mode(0o000)).unwrap();
    let identity = file_stable_token(&root).unwrap();
    detach_and_remove_owned_tree(&root, &parent, b"owned", b"private-tombstone", &identity)
        .unwrap();
    assert!(!base.join("owned").exists());
    assert!(!base.join("private-tombstone").exists());
    fs::remove_dir(base).unwrap();
}
