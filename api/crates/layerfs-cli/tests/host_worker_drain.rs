#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;

use fixture::Fixture;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
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

#[test]
fn management_polling_does_not_keep_an_idle_worker_alive() {
    // §8.3/§12: a doctor or `machine get --live` ping observes the worker;
    // only calls keep it warm.
    let home = Fixture::new();
    let mut worker = home.start();
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(80) {
        if catch_unwind(AssertUnwindSafe(|| home.ping())).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    assert!(worker.wait(Duration::from_secs(10)).success());
    assert!(began.elapsed() >= Duration::from_secs(59));
    assert!(!home.paths.socket.exists());
}

#[test]
fn connections_waiting_at_the_idle_mark_keep_the_worker_accepting() {
    // Ordinary connections that are accepted but not yet admitted span the
    // 60 s mark, so the worker must not drain under them.
    let home = Fixture::new();
    let mut worker = home.start();
    let began = Instant::now();
    std::thread::sleep(Duration::from_secs(54));
    let mut waiting = VecDeque::new();
    while began.elapsed() < Duration::from_secs(68) {
        waiting.push_back(home.dial(false));
        if waiting.len() > 3 {
            waiting.pop_front();
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    let ping = home.ping();
    assert_eq!(ping.get_text("state"), Some("ready"));
    drop(waiting);
    home.drain();
    assert!(worker.wait(Duration::from_secs(10)).success());
}
