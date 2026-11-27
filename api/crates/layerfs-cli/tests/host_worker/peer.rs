#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

use crate::fixture;

use fixture::Fixture;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::Duration;

const PROBE_SOCKET: &str = "LAYERFS_819_FOREIGN_PEER_PROBE";

#[test]
fn session_is_bound_to_its_home() {
    let first = Fixture::new();
    let second = Fixture::new();
    let mut first_worker = first.start();
    let mut second_worker = second.start();
    let first_ping = first.ping();
    let second_ping = second.ping();
    assert_ne!(
        first_ping.get_text("home_id"),
        second_ping.get_text("home_id")
    );
    assert_ne!(
        first_ping.get_text("generation"),
        second_ping.get_text("generation")
    );
    first.drain();
    second.drain();
    assert!(first_worker.wait(Duration::from_secs(5)).success());
    assert!(second_worker.wait(Duration::from_secs(5)).success());
}

#[test]
fn a_foreign_uid_receives_no_worker_hello() {
    let home = Fixture::new();
    if home.paths.home.metadata().unwrap().uid() != 0 {
        eprintln!("NOT_RUN: foreign peer probe needs root to spawn another uid");
        return;
    }
    let mut worker = home.start();
    home.ping();
    let machine_root = home.paths.home.join("machine");
    let machine = home.paths.control.parent().unwrap();
    let mut paths = vec![
        (&home.paths.home as &std::path::Path, 0o711),
        (&machine_root as &std::path::Path, 0o711),
        (machine, 0o711),
        (&home.paths.control as &std::path::Path, 0o666),
    ];
    let modes: Vec<_> = paths
        .iter()
        .map(|(path, _)| path.metadata().unwrap().permissions().mode())
        .collect();
    for (path, mode) in &paths {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(*mode)).unwrap();
    }
    // Widen only this test fixture's modes to reach the peer check even
    // though the filesystem would ordinarily reject the other uid first.
    let mut probe = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "peer::foreign_peer_probe"])
        .uid(65534)
        .gid(65534)
        .env(PROBE_SOCKET, &home.paths.control)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = probe.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            probe.kill().unwrap();
            break probe.wait().unwrap();
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    for ((path, _), mode) in paths.drain(..).zip(modes) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
    assert!(status.success(), "wrong-uid probe failed: {status}");
    home.drain();
    assert!(worker.wait(Duration::from_secs(5)).success());
}

#[test]
fn foreign_peer_probe() {
    let Some(socket) = std::env::var_os(PROBE_SOCKET) else {
        return;
    };
    let mut stream = UnixStream::connect(socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    assert_eq!(std::io::Read::read(&mut stream, &mut [0u8]).unwrap(), 0);
}
