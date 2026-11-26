#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;

use fixture::Fixture;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

#[test]
fn aliased_homes_have_one_owner_and_keep_their_mode() {
    let mut home = Fixture::new();
    let alias = home.alias();
    let mut worker = home.start();
    let ready = home.ping();
    assert_eq!(ready.get_int("pid"), Some(worker.pid()));
    assert!(ready.get_text("pid_start").is_some());
    assert!(ready.get_text("pid_boot").is_some());
    let compiled = concat!(
        env!("CARGO_PKG_VERSION"),
        "/",
        env!("LAYERFS_BUILD_COMMIT"),
        "/",
        env!("LAYERFS_BUILD_DIRTY"),
        "@"
    );
    assert!(ready.get_text("build").unwrap().starts_with(compiled));
    let generation = ready.get_text("generation");

    let mut second = Fixture::spawn(&alias);
    assert_eq!(second.wait(Duration::from_secs(5)).code(), Some(125));
    let next = home.ping();
    assert_eq!(next.get_text("generation"), generation);
    assert_eq!(next.get_int("pid"), Some(worker.pid()));
    assert_eq!(
        home.paths.home.metadata().unwrap().permissions().mode() & 0o777,
        0o700
    );
    for endpoint in [&home.paths.socket, &home.paths.control] {
        assert_eq!(
            endpoint.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(!home.paths.home.join("worker-incarnation.json").exists());
    assert!(!home.paths.home.join("log/sweep.jsonl").exists());
    home.drain();
    assert!(worker.wait(Duration::from_secs(5)).success());
    assert!(!home.paths.socket.exists());
    assert!(!home.paths.control.exists());
}

#[test]
fn a_new_owner_recovers_socket_paths_after_a_worker_dies() {
    let home = Fixture::new();
    let mut first = home.start();
    let first_ping = home.ping();
    first.kill();
    assert!(home.paths.socket.exists());
    assert!(home.paths.control.exists());

    let mut second = home.start();
    let second_ping = home.ping();
    assert_ne!(
        first_ping.get_text("generation"),
        second_ping.get_text("generation")
    );
    assert_eq!(second_ping.get_int("pid"), Some(second.pid()));
    home.drain();
    assert!(second.wait(Duration::from_secs(5)).success());
}
