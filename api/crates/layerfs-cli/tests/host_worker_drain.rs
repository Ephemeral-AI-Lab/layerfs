#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;

use fixture::Fixture;
use std::time::{Duration, Instant};

#[test]
fn management_drains_independently_of_full_ordinary_pool() {
    let home = Fixture::new();
    let mut worker = home.start();
    // Each stream has received its hello. All eight ordinary threads are
    // waiting for these connections, with no operation admitted yet.
    let connections: Vec<_> = (0..8).map(|_| home.dial(false)).collect();
    let ping = home.ping();
    assert_eq!(ping.get_int("ordinary"), Some(8));

    home.drain();
    assert!(home.paths.try_hold().unwrap().is_none());
    for connection in connections {
        let refusal = connection.request("store.call");
        assert_eq!(
            refusal.get("error").unwrap().get_text("code"),
            Some("Unavailable")
        );
        assert!(refusal
            .get("error")
            .unwrap()
            .get_text("message")
            .unwrap()
            .contains("draining"));
    }
    assert!(worker.wait(Duration::from_secs(5)).success());
    assert!(home.paths.try_hold().unwrap().is_some());
    assert!(!home.paths.socket.exists());
    assert!(!home.paths.control.exists());
}

#[test]
fn idle_worker_releases_endpoints_even_after_abandoned_hellos() {
    let home = Fixture::new();
    let mut worker = home.start();
    home.ping();
    let last_call = Instant::now();
    std::thread::sleep(Duration::from_secs(50));
    // Both have consumed a hello, but neither sent a complete operation.
    drop(home.dial(false));
    let pending = home.dial(false);
    assert!(worker.wait(Duration::from_secs(22)).success());
    drop(pending);
    assert!(last_call.elapsed() >= Duration::from_secs(59));
    assert!(home.paths.try_hold().unwrap().is_some());
    assert!(!home.paths.socket.exists());
    assert!(!home.paths.control.exists());
}
