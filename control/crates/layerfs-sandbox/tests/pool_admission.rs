//! A daemon's advisory capacity notice (#540) as a client reads it: a
//! daemon whose pool has no vacant slot answers the next connection with
//! the notice before any handshake, so the refusal is `Capacity` naming the
//! resource, the vacant slots and the retained failure count, not a bare
//! EOF. Nothing is sent by the client and nothing is retried, so the caller
//! may try again. A daemon that is still starting says so the same way.
use layerfs_bridge::duplex;
use layerfs_bridge::duplex::admission::{self, AdmissionNotice, AdmissionReason};
use layerfs_sandbox::daemon::pool::Dial;
use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;

const CLIENT_PRIVATE: [u8; 32] = [2; 32];
const DAEMON_PRIVATE: [u8; 32] = [1; 32];
const DAEMON_INSTANCE: [u8; 32] = [3; 32];

/// A daemon that refuses every connection with `notice` before any
/// handshake, exactly as an accept loop does when no slot is vacant: it
/// writes the notice, shuts its write end and drains what the client sent.
fn refusing(notice: AdmissionNotice) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
            let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
            if admission::write(&mut stream, &notice).is_err() {
                continue;
            }
            let mut sink = [0u8; 512];
            let _ = stream.read(&mut sink);
        }
    });
    endpoint
}

fn dial(endpoint: SocketAddr) -> Dial {
    Dial {
        endpoint,
        secret: CLIENT_PRIVATE,
        peer: duplex::public_key(&DAEMON_PRIVATE).unwrap(),
        instance: DAEMON_INSTANCE,
    }
}

#[test]
fn a_full_pool_refuses_the_next_client_with_its_counts() {
    let endpoint = refusing(AdmissionNotice::new(AdmissionReason::NoPoolSlot, 3, 0, 1));
    let refusal = dial(endpoint)
        .open_within(Duration::from_secs(2))
        .err()
        .expect("a full pool admits nothing");
    assert_eq!(refusal.code, "Capacity", "{refusal:?}");
    let detail = &refusal.detail;
    assert_eq!(detail.get_text("resource"), Some("daemon_control_slot"));
    assert_eq!(detail.get_int("pool_vacant"), Some(0));
    assert_eq!(detail.get_int("reserved_vacant"), Some(1));
    assert_eq!(detail.get_int("retained"), Some(3));
    assert_eq!(detail.get_text("admission"), Some("no-pool-slot"));
    // nothing was attempted, so a later open is safe
    assert!(detail.get_bool("retryable"));
    assert!(
        refusal.message.contains("no vacant control slot"),
        "{refusal:?}"
    );
    assert!(refusal.message.contains("3 retained"), "{refusal:?}");
}

#[test]
fn a_still_starting_daemon_is_not_a_full_pool() {
    let endpoint = refusing(AdmissionNotice::new(AdmissionReason::Starting, 0, 8, 1));
    let refusal = dial(endpoint)
        .open_within(Duration::from_secs(2))
        .err()
        .expect("a starting daemon admits nothing");
    // its own code and admission, not Capacity: a full pool it is not
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    assert_eq!(refusal.detail.get_text("admission"), Some("starting"));
    assert!(refusal.detail.get_bool("retryable"));
}
