//! Owning native TCP/Noise proofs, through the public product API.
#![cfg(feature = "native")]
use layerfs_bridge::native::{
    accept, initiate, public_key, ChannelError, Connection, MAX_PLAINTEXT_BYTES, NOISE,
};
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

// Fixed independent proof stop fence only; no product socket/Exec timeout exists.
fn socket(stream: TcpStream) -> TcpStream {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
}
fn pair() -> (Connection, Connection, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client_private = [1; 32];
    let server_private = [2; 32];
    let client_public = public_key(&client_private).unwrap();
    let server_public = public_key(&server_private).unwrap();
    let worker = thread::spawn(move || {
        accept(
            socket(listener.accept().unwrap().0),
            &server_private,
            client_public,
        )
        .unwrap()
    });
    let stream = socket(TcpStream::connect(address).unwrap());
    let raw = stream.try_clone().unwrap();
    let client = initiate(stream, &client_private, server_public).unwrap();
    let server = worker.join().unwrap();
    assert_eq!(client.peer.public_key(), server_public);
    assert_eq!(server.peer.public_key(), client_public);
    (client, server, raw)
}

#[test]
fn authenticated_duplex_records_stream_beyond_one_window_with_fixed_buffers() {
    let (mut client, mut server, _raw) = pair();
    let worker = thread::spawn(move || {
        for _ in 0..5 {
            let received = server.receive.receive().unwrap();
            server.send.send(received).unwrap();
        }
        let receive = server.receive.work();
        let send = server.send.work();
        assert_eq!(receive.records, 5);
        assert_eq!(send.records, 5);
        assert_eq!(receive.sealed_capacity, u16::MAX as usize);
        assert_eq!(receive.plain_capacity, MAX_PLAINTEXT_BYTES);
        println!(
            "NATIVE suite={NOISE} server_receive={receive:?} server_send={send:?} handshake={:?}",
            server.handshake_work
        );
    });
    let bytes = vec![0xa7; MAX_PLAINTEXT_BYTES];
    for payload in [
        &bytes[..],
        &bytes[..],
        &b"control"[..],
        &b""[..],
        &bytes[..],
    ] {
        client.send.send(payload).unwrap();
        assert_eq!(client.receive.receive().unwrap(), payload);
    }
    worker.join().unwrap();
    let send = client.send.work();
    assert_eq!(send.records, 5);
    assert_eq!(send.plaintext_bytes, (3 * MAX_PLAINTEXT_BYTES + 7) as u64);
    assert_eq!(send.sealed_capacity, u16::MAX as usize);
    assert_eq!(send.wire_bytes, send.plaintext_bytes + 5 * 18);
    println!(
        "NATIVE client_send={send:?} client_receive={:?}",
        client.receive.work()
    );
}

#[test]
fn oversized_unattempted_record_does_not_consume_a_nonce() {
    let (mut client, mut server, _raw) = pair();
    assert!(matches!(
        client.send.send(&vec![0; MAX_PLAINTEXT_BYTES + 1]),
        Err(ChannelError::RecordLimit)
    ));
    assert_eq!(client.send.work().records, 0);
    client.send.send(b"next explicit record").unwrap();
    assert_eq!(server.receive.receive().unwrap(), b"next explicit record");
    client.send.close().unwrap();
    assert!(matches!(
        client.send.send(b"again"),
        Err(ChannelError::Quarantined)
    ));
    assert!(matches!(
        client.send.close(),
        Err(ChannelError::Quarantined)
    ));
}

#[test]
fn wrong_static_identity_fails_the_actual_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let expected_wrong = public_key(&[3; 32]).unwrap();
    let server_public = public_key(&[2; 32]).unwrap();
    let worker = thread::spawn(move || {
        accept(
            socket(listener.accept().unwrap().0),
            &[2; 32],
            expected_wrong,
        )
        .err()
        .unwrap()
    });
    let client = initiate(
        socket(TcpStream::connect(address).unwrap()),
        &[1; 32],
        server_public,
    );
    assert!(client.is_err());
    assert!(matches!(worker.join().unwrap(), ChannelError::Noise(_)));
}

#[test]
fn malformed_record_quarantines_both_directions_before_body_allocation() {
    let (_client, mut server, mut raw) = pair();
    raw.write_all(&[0, 0]).unwrap();
    assert!(matches!(
        server.receive.receive(),
        Err(ChannelError::Invalid("encrypted record length"))
    ));
    assert_eq!(server.receive.work().records, 0);
    assert_eq!(server.receive.work().wire_bytes, 2);
    assert!(matches!(
        server.receive.receive(),
        Err(ChannelError::Quarantined)
    ));
    assert!(matches!(
        server.send.send(b"no resend"),
        Err(ChannelError::Quarantined)
    ));
}

#[test]
fn corrupted_authenticated_record_retains_the_crypto_failure_without_replay() {
    let (_client, mut server, mut raw) = pair();
    raw.write_all(&16_u16.to_be_bytes()).unwrap();
    raw.write_all(&[0; 16]).unwrap();
    assert!(matches!(
        server.receive.receive(),
        Err(ChannelError::Noise(_))
    ));
    assert!(matches!(
        server.receive.receive(),
        Err(ChannelError::Quarantined)
    ));
    assert!(matches!(
        server.send.send(b"no fallback"),
        Err(ChannelError::Quarantined)
    ));
}
