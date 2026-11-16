//! One real mount on an empty interface: the drive appears and lists.
//! Skips when the WinFsp driver service is absent (fsptool install).
#![cfg(windows)]
use layerfs_winfsp::mount;

#[test]
fn mounts_an_empty_file_system() {
    let installed = std::process::Command::new("sc")
        .args(["query", "WinFsp"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !installed {
        eprintln!("skipped: the WinFsp driver service is not installed (run fsptool install as admin)");
        return;
    }
    let mounted = mount("Y:", Default::default()).expect("mount");
    let listing = std::fs::read_dir("Y:").expect("list the mounted drive");
    assert_eq!(listing.count(), 0, "an empty interface lists nothing");
    drop(mounted);
}
