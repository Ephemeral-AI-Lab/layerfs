#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
//! Idle exit with callers waiting at the 60 s mark. Expected runtime about
//! 75-85 s.

use crate::proof;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[test]
fn connections_waiting_at_the_idle_mark_keep_the_worker_accepting() {
    // Ordinary connections that are accepted but not yet admitted span the
    // 60 s mark, so the worker must not drain under them.
    let home = proof::create();
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

    // Once they close, the idle worker first removes its socket names, then
    // exits. A call made then finds no endpoint and starts the next worker
    // within its start deadline; it is never reset in a dying listen queue.
    drop(waiting);
    let removed = Instant::now() + Duration::from_secs(10);
    while home.paths.socket.exists() {
        assert!(Instant::now() < removed, "the idle worker kept its socket");
        std::thread::sleep(Duration::from_millis(20));
    }
    proof::assert_reads(proof::spawn_reads(&home, 1, false));
    assert!(worker.wait(Duration::from_secs(10)).success());
    let next = home.ping();
    assert_ne!(next.get_int("pid"), Some(worker.pid()));
    home.drain();
    proof::wait_for("the next worker to release its lock", || {
        home.paths.try_hold().unwrap()
    });
}
