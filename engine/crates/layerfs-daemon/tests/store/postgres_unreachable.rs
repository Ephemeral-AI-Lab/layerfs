//! A daemon whose PostgreSQL Store server cannot be reached exits with a
//! clear `cannot reach PostgreSQL Store at <host>` message instead of raw
//! `pack persistence: Refused { status: "Closed" }` debug text, and never
//! includes the URL password.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use layerfs_protocol::{
    duplex,
    provision::daemon_setup::{DaemonLimits, DaemonSetup},
    DaemonStore, KernelRoute, StoreAuthority,
};
use std::{
    fs::{self, File},
    net::TcpListener,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SECRET: &str = "s3cr3t-pg-pw";

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-daemon-pg-unreach-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

#[test]
fn daemon_reports_unreachable_postgres_store_without_pack_persistence_debug_text() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = listener.local_addr().unwrap().port();
    drop(listener);

    let dir = tempdir("closed");
    let mounts = dir.join("mounts");
    fs::create_dir_all(&mounts).unwrap();
    fs::set_permissions(&mounts, fs::Permissions::from_mode(0o700)).unwrap();
    let id = fs::metadata(&dir).unwrap();
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: [72; 32],
        control_peer: duplex::public_key(&[71; 32]).unwrap(),
        route: KernelRoute::Nfs,
        overlay: dir.join("overlay.sqlite").display().to_string(),
        mounts: mounts.display().to_string(),
        command_uid: id.uid(),
        command_gid: id.gid(),
        limits: DaemonLimits {
            connections: 4,
            handshake_ms: 2000,
            idle_ttl_ms: 60_000,
            request_deadline_ms: 60_000,
            cache_bytes: 1024 * 1024,
            namespaces: 4,
            pager_kib: 1024,
            serial_low_water: 256,
        },
        store: DaemonStore::Postgres {
            url: format!("postgres://app:{SECRET}@127.0.0.1:{closed_port}/shop?sslmode=disable"),
            schema: "layerfs".to_owned(),
            synchronous_commit: "local".to_owned(),
            authority: StoreAuthority {
                binding: vec![1; 16],
                cursor_key: [2; 32],
            },
        },
    };
    let config = dir.join("daemon.config");
    fs::write(&config, setup.encode().unwrap()).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let stderr_path = dir.join("daemon.stderr");
    let status = Command::new(env!("CARGO_BIN_EXE_layerfs-daemon"))
        .arg("--config")
        .arg(&config)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(File::create(&stderr_path).unwrap()))
        .status()
        .expect("run layerfs-daemon");
    let stderr = fs::read_to_string(&stderr_path).unwrap();
    let _ = fs::remove_dir_all(&dir);

    assert!(!status.success(), "expected failure: {stderr}");
    assert!(
        stderr.contains("cannot reach PostgreSQL Store at 127.0.0.1"),
        "{stderr}"
    );
    assert!(!stderr.contains("pack persistence"), "{stderr}");
    assert!(!stderr.contains("Refused"), "{stderr}");
    assert!(!stderr.contains(SECRET), "{stderr}");
}
