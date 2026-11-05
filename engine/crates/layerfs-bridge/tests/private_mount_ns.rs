//! The opt-in private mount namespace in the daemon's private config record.

use layerfs_bridge::daemon_setup::{DaemonLimits, DaemonSetup};
use std::path::PathBuf;

fn setup(overlay: &str, private_mount_ns: bool) -> DaemonSetup {
    DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: [1; 32],
        control_peer: [2; 32],
        store: "/layerfs-store/global/store.sqlite".into(),
        overlay: overlay.into(),
        mounts: "/workspaces".into(),
        command_uid: 501,
        command_gid: 20,
        limits: DaemonLimits {
            connections: 4,
            handshake_ms: 3000,
            cache_bytes: 64 * 1024,
            namespaces: 8,
            pager_kib: 1024,
            serial_low_water: 0,
            idle_exit_s: 0,
        },
        existing_store: None,
        private_mount_ns,
    }
}

#[test]
fn the_private_namespace_setting_survives_the_config_record() {
    assert!(setup("/local/overlay/overlay.sqlite", false)
        .mount_ns_pid_file()
        .is_none());
    assert!(setup("/local/overlay/overlay.sqlite", false)
        .mount_ns_socket()
        .is_none());
    let encoded = setup("/local/overlay/overlay.sqlite", true)
        .encode()
        .unwrap();
    let decoded = DaemonSetup::decode(&encoded).unwrap();
    assert!(decoded.private_mount_ns);
    assert_eq!(
        decoded.mount_ns_pid_file(),
        Some(PathBuf::from("/local/daemon.pid"))
    );
    assert_eq!(
        decoded.mount_ns_socket(),
        Some(PathBuf::from("/local/ns-exec.sock"))
    );
}

#[test]
fn the_pid_file_is_beside_the_local_directory_above_the_overlay() {
    assert_eq!(
        setup("/layerfs-local/overlay/overlay.sqlite", true).mount_ns_pid_file(),
        Some(PathBuf::from("/layerfs-local/daemon.pid"))
    );
    assert_eq!(
        setup("/layerfs-local/overlay/overlay.sqlite", true).mount_ns_socket(),
        Some(PathBuf::from("/layerfs-local/ns-exec.sock"))
    );
    // An Overlay path with no local directory cannot name a pid file.
    assert!(setup("/overlay.sqlite", true).encode().is_err());
}
