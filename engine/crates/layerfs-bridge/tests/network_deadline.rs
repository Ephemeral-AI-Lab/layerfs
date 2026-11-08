//! Real system resolver behavior and expired budget without DNS threads.
use layerfs_bridge::network::{resolve_all_within, resolve_within};
use std::time::{Duration, Instant};
#[test]
fn numeric_addresses_and_expired_budget() {
    let until = Instant::now() + Duration::from_secs(1);
    assert_eq!(
        resolve_within("127.0.0.1", 443, until).unwrap().to_string(),
        "127.0.0.1:443"
    );
    assert_eq!(resolve_all_within("::1", 5432, until).unwrap().len(), 1);
    assert_eq!(
        resolve_within("localhost", 5432, Instant::now())
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn localhost_uses_bounded_system_helper_and_preserves_port() {
    let addresses =
        resolve_all_within("localhost", 5432, Instant::now() + Duration::from_secs(5)).unwrap();
    assert!(!addresses.is_empty());
    assert!(addresses
        .iter()
        .all(|address| address.ip().is_loopback() && address.port() == 5432));
}
