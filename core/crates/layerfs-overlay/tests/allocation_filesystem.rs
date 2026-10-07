//! Selected Linux compatibility refusal; no device-full qualification.
#![cfg(target_os = "linux")]

use layerfs_overlay::{AllocationWork, DatabaseWork, Overlay, OverlayError, ProfileConfig};
use std::{fs::File, os::unix::fs::MetadataExt, path::PathBuf};

#[test]
#[ignore = "requires an explicit empty owned directory on Linux magic 0x6a656a63"]
fn incompatible_host_share_refuses_before_allocation_and_retains_empty_artifact() {
    let root = PathBuf::from(
        std::env::var_os("LAYERFS_PROOF_UNSUPPORTED_ROOT").expect("explicit owned proof root"),
    );
    assert!(root.is_absolute(), "proof root must be absolute");
    let directory = File::open(&root).unwrap();
    assert!(directory.metadata().unwrap().is_dir());
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    let filesystem = nix::sys::statfs::fstatfs(&directory).unwrap();
    assert_eq!(filesystem.filesystem_type().0, 0x6a656a63);

    let path = root.join("overlay.sqlite");
    let created = Overlay::create_observed(&path, ProfileConfig::default());
    println!(
        "S7_FILESYSTEM_CREATION result={:?} work={:?}",
        created.result.as_ref().map(|_| ()),
        created.work
    );
    assert!(matches!(
        &created.result,
        Err(OverlayError::UnsupportedFilesystem {
            linux_magic: 0x6a656a63
        })
    ));
    let work = &created.work;
    assert_eq!(work.file_create_calls, 1);
    assert_eq!(work.filesystem_open_calls, 1);
    assert_eq!(work.filesystem_identity_calls, 2);
    assert_eq!(work.filesystem_probe_calls, 1);
    assert_eq!(work.linux_filesystem_type, Some(0x6a656a63));
    assert_eq!(work.sqlite_open_calls, 0);
    assert_eq!(work.connection_configuration_calls, 0);
    assert_eq!(work.cache_configuration_calls, 0);
    assert_eq!(work.sql, DatabaseWork::default());
    assert_eq!(work.allocation, AllocationWork::default());
    assert!(work.allocation_state.is_none());

    let retained = File::open(&path).unwrap();
    let metadata = retained.metadata().unwrap();
    let at_path = std::fs::symlink_metadata(&path).unwrap();
    assert!(metadata.is_file());
    assert!(at_path.is_file());
    assert_eq!(
        (metadata.dev(), metadata.ino()),
        (at_path.dev(), at_path.ino())
    );
    assert_eq!(metadata.nlink(), 1);
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert_eq!(metadata.len(), 0);
    assert_eq!(metadata.blocks(), 0);
    assert_eq!(
        nix::sys::statfs::fstatfs(&retained)
            .unwrap()
            .filesystem_type()
            .0,
        0x6a656a63
    );
    let mut entries = std::fs::read_dir(&root).unwrap();
    assert_eq!(entries.next().unwrap().unwrap().path(), path);
    assert!(entries.next().is_none());
    println!(
        "S7_UNSUPPORTED_FILESYSTEM error={:?} work={work:?} retained_path={path:?} device={} inode={} logical_bytes={} allocated_bytes={}",
        created.result.as_ref().err(),
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.blocks() * 512
    );
    // Keep the original failed startup artifact for the caller's custody review.
}
