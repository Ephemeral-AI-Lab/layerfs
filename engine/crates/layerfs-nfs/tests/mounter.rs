//! The native macOS client must not send atime updates the export ignores.
#![cfg(target_os = "macos")]
use layerfs_daemon::control::Mounter;

#[test]
fn host_mount_disables_access_time_updates() {
    let mounter = Mounter::host();
    let options = mounter
        .mount
        .0
        .windows(2)
        .find(|pair| pair[0] == "-o")
        .expect("mount options");
    assert!(options[1].split(',').any(|option| option == "noatime"));
    // Keep the existing attribute-cache contract; no global noac tradeoff.
    assert!(options[1].split(',').any(|option| option == "actimeo=60"));
}
