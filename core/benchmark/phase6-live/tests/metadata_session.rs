//! External authenticated-peer fixtures exercise transport identity/custody only.
use layerfs_bridge::adapters::native::{
    connection::{self, Peer, VerifiedPeer},
    protocol::{Frame, Kind},
};
use phase6_live_probe::{metadata_session::Session, wire};
use std::{
    net::TcpListener,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
fn peer() -> Peer {
    Peer {
        selector: 1,
        public: *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key(),
        expires_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 60,
    }
}
fn response(c: &mut connection::Connection, expected: u64, wrong_id: bool) {
    let frame = c.receive.read().unwrap();
    assert_eq!(frame.kind, Kind::Begin);
    assert_eq!(frame.id, expected);
    assert!(frame.bytes.starts_with(wire::PREFIX));
    let mut bytes = wire::PREFIX.to_vec();
    bytes.extend_from_slice(b"opaque response");
    c.send
        .write(&Frame {
            kind: Kind::Success,
            id: frame.id + u64::from(wrong_id),
            bytes,
        })
        .unwrap();
}
#[test]
fn two_requests_share_real_authenticated_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        response(&mut c, 1, false);
        response(&mut c, 2, false);
    });
    let key = *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key();
    let mut session = Session::default();
    assert_eq!(
        session.call(address, 1, &[7; 32], &key, 1, &[]).unwrap(),
        b"opaque response"
    );
    assert_eq!(
        session.call(address, 1, &[7; 32], &key, 2, &[]).unwrap(),
        b"opaque response"
    );
    assert_eq!(session.statistics.connect_attempts, 1);
    assert_eq!(session.statistics.calls, [0, 1, 1, 0, 0, 0, 0, 0]);
    server.join().unwrap();
}
#[test]
fn wrong_reply_identity_quarantines_without_resend() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        response(&mut c, 1, true);
    });
    let key = *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key();
    let mut session = Session::default();
    assert!(session.call(address, 1, &[7; 32], &key, 4, &[]).is_err());
    assert!(session
        .call(address, 1, &[7; 32], &key, 4, &[])
        .unwrap_err()
        .contains("quarantined"));
    assert_eq!(session.statistics.connect_attempts, 1);
    assert_eq!(session.statistics.calls, [0, 0, 0, 0, 1, 0, 0, 0]);
    server.join().unwrap();
}
#[test]
fn oversized_input_refuses_before_connection_effect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut session = Session::default();
    assert!(session
        .call(
            listener.local_addr().unwrap(),
            1,
            &[7; 32],
            &[8; 32],
            1,
            &vec![0; 16384]
        )
        .is_err());
    assert_eq!(session.statistics.connect_attempts, 0);
    assert_eq!(session.statistics.calls, [0; 8]);
}
#[test]
fn known_idle_rotation_opens_new_session_without_replaying_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut first = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        response(&mut first, 1, false);
        assert!(first.receive.read().is_err());
        let (stream, _) = listener.accept().unwrap();
        let mut second = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        response(&mut second, 2, false);
    });
    let key = *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key();
    let mut session = Session::default();
    session.call(address, 1, &[7; 32], &key, 1, &[]).unwrap();
    std::thread::sleep(Duration::from_millis(2050));
    session.call(address, 1, &[7; 32], &key, 1, &[]).unwrap();
    assert_eq!(session.statistics.connect_attempts, 2);
    assert_eq!(session.statistics.calls, [0, 2, 0, 0, 0, 0, 0, 0]);
    server.join().unwrap();
}
