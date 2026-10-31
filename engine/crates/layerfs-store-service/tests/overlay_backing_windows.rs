//! The Windows overlay backing's last-owner deletion: the same identity
//! discipline as Linux, through the audited Windows identity. A dangling
//! `Weak` stands in for the workspace pool — `Drop` skips its callback when
//! the pool is gone, which is exactly the unit under test.
#![cfg(target_os = "windows")]
use std::sync::{Arc, Weak};

fn scratch(label: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-backing-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|span| span.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// The file goes away when the last owner releases: identity matches, the
/// removal is authorized, and closing the retained handle completes it.
#[test]
fn the_last_owner_deletes_a_matching_overlay_file() {
    let directory = scratch("delete");
    let path = directory.join("overlay.sqlite");
    std::fs::write(&path, b"overlay body").unwrap();
    let backing = Arc::new(
        layerfs_store_service::overlay::backing::Backing::new(
            path.clone(),
            1,
            [7; 32],
            Weak::<layerfs_store_service::overlay::workspaces::Workspaces>::new(),
        )
        .unwrap(),
    );
    assert!(path.exists(), "the file stays while its owner lives");
    backing.terminal();
    backing.closed();
    drop(backing);
    let mut gone = false;
    for _ in 0..40 {
        if !path.exists() {
            gone = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(gone, "the last owner's release deletes the file");
    std::fs::remove_dir_all(directory).unwrap();
}

/// A file swapped in behind the backing's back no longer matches the
/// retained identity: the removal is refused and the impostor stays.
#[test]
fn a_swapped_in_file_is_refused_the_deletion() {
    let directory = scratch("swap");
    let path = directory.join("overlay.sqlite");
    std::fs::write(&path, b"overlay body").unwrap();
    let backing = Arc::new(
        layerfs_store_service::overlay::backing::Backing::new(
            path.clone(),
            1,
            [7; 32],
            Weak::<layerfs_store_service::overlay::workspaces::Workspaces>::new(),
        )
        .unwrap(),
    );
    // Replace the entry at the path with a different file (different volume
    // index), so the path no longer names the acquisition's file.
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, b"impostor").unwrap();
    backing.terminal();
    backing.closed();
    drop(backing);
    assert!(path.exists(), "the impostor is not deleted");
    std::fs::remove_dir_all(directory).unwrap();
}
