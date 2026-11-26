#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
//! Idle exit under management polling. Expected runtime about 65-75 s.

#[path = "common/host_worker.rs"]
mod fixture;

use fixture::Fixture;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};
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
