//! Actual authenticated fragmentation, correlation and failure fences.
#![cfg(feature = "native")]
use layerfs_bridge::{
    codec::{Reassembly, ReassemblyConfig},
    contract::*,
    native::{self, ChannelError, Connection, FramingError, RecordReceiver, RecordSender},
};
use std::{
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};
fn socket(s: TcpStream) -> TcpStream {
    s.set_nonblocking(false).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
    s
}
fn pair() -> (Connection, Connection) {
    let listen = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listen.local_addr().unwrap();
    let client = native::public_key(&[1; 32]).unwrap();
    let server = native::public_key(&[2; 32]).unwrap();
    // Accept is bounded even if the other side fails before connection.
    listen.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            match listen.accept() {
                Ok((s, _)) => return native::accept(socket(s), &[2; 32], client).unwrap(),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => panic!("accept: {e}"),
            }
        }
    });
    let s = socket(TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap());
    let a = native::initiate(s, &[1; 32], server).unwrap();
    (a, worker.join().unwrap())
}
fn cfg(kind: MessageKind) -> ReassemblyConfig {
    ReassemblyConfig {
        kind,
        messages: 4,
        demand_messages: 1,
        control_messages: 1,
        message_bytes: 2 << 20,
        bytes: 8 << 20,
        demand_reserve: 2 << 20,
        control_reserve: 4096,
    }
}
#[test]
fn failed_native_send_keeps_original_framing_copy_and_initialization_work() {
    let (client, _server) = pair();
    let mut sender = RecordSender::new(client.send).map_err(|(e, _)| e).unwrap();
    sender.close().unwrap();
    assert!(matches!(
        sender.begin(MessageKind::Request, MessageClass::Save, 1, 7, b"copied!"),
        Err(FramingError::Send {
            completed_bytes: 0,
            error: ChannelError::Quarantined,
            ..
        })
    ));
    let work = sender.work();
    assert_eq!(work.encoding_attempts, 1);
    assert_eq!(work.encoded_fragments, 1);
    assert_eq!(work.fragments, 0);
    assert_eq!(work.copied_bytes, 7);
    assert_eq!(work.header_bytes, 40);
    assert_eq!(work.zeroed_bytes, MAX_RECORD_BYTES as u64);
    assert_eq!(work.requested_buffer_bytes, MAX_RECORD_BYTES as u64);
    assert_eq!(sender.native_work().channel_calls, 1);
    assert_eq!(sender.native_work().record_io_attempts, 0);
}
#[test]
fn native_fragments_interleave_and_reply_correlations_keep_original_requests() {
    let (client, server) = pair();
    let mut sender = RecordSender::new(client.send).map_err(|(e, _)| e).unwrap();
    let mut receiver = RecordReceiver::new(client.receive);
    let worker = thread::spawn(move || {
        let mut rx = RecordReceiver::new(server.receive);
        let mut tx = RecordSender::new(server.send).map_err(|(e, _)| e).unwrap();
        let mut bodies = Reassembly::new(cfg(MessageKind::Request)).unwrap();
        let mut done = Vec::new();
        while done.len() < 2 {
            let frame = rx.receive().unwrap();
            if let Some(body) = bodies.push(frame).unwrap() {
                done.push(body);
            }
        }
        assert_eq!(done[0].bytes(), b"policy");
        assert_eq!(done[0].envelope().correlation, 102);
        assert_eq!(done[1].bytes().len(), MAX_FRAGMENT_BYTES + 9);
        assert!(done[1].bytes().iter().all(|b| *b == 0xa7));
        for message in done {
            let mut progress = tx
                .begin(
                    MessageKind::Reply,
                    message.envelope().class,
                    message.envelope().correlation,
                    message.bytes().len() as u64,
                    &message.bytes()[..message.bytes().len().min(MAX_FRAGMENT_BYTES)],
                )
                .unwrap();
            if !progress.complete() {
                tx.continue_message(&mut progress, &message.bytes()[MAX_FRAGMENT_BYTES..])
                    .unwrap();
            }
        }
        assert_eq!(bodies.work().unwrap().live_messages, 0);
    });
    let data = vec![0xa7; MAX_FRAGMENT_BYTES + 9];
    let mut large = sender
        .begin(
            MessageKind::Request,
            MessageClass::Save,
            101,
            data.len() as u64,
            &data[..MAX_FRAGMENT_BYTES],
        )
        .unwrap();
    assert!(!large.complete());
    let small = sender
        .begin(
            MessageKind::Request,
            MessageClass::Control,
            102,
            6,
            b"policy",
        )
        .unwrap();
    assert!(small.complete());
    sender
        .continue_message(&mut large, &data[MAX_FRAGMENT_BYTES..])
        .unwrap();
    assert!(large.complete());
    assert!(matches!(
        sender.continue_message(&mut large, b"x"),
        Err(FramingError::Frame(_))
    ));
    let mut replies = Reassembly::new(cfg(MessageKind::Reply)).unwrap();
    let mut done = Vec::new();
    while done.len() < 2 {
        let frame = receiver.receive().unwrap();
        if let Some(body) = replies.push(frame).unwrap() {
            done.push(body);
        }
    }
    assert_eq!(done[0].envelope().correlation, 102);
    assert_eq!(done[1].envelope().correlation, 101);
    assert_eq!(done[1].bytes(), data);
    assert_eq!(sender.work().fragments, 3);
    assert_eq!(sender.work().copied_bytes, data.len() as u64 + 6);
    worker.join().unwrap();
}
#[test]
fn authenticated_bad_header_fences_both_directions_and_retains_partial_body() {
    let (mut client, mut server) = pair();
    let mut tx = RecordSender::new(client.send).map_err(|(e, _)| e).unwrap();
    tx.begin(MessageKind::Request, MessageClass::Save, 1, 8, b"orig")
        .unwrap();
    let mut rx = RecordReceiver::new(server.receive);
    let mut partial = Reassembly::new(cfg(MessageKind::Request)).unwrap();
    partial.push(rx.receive().unwrap()).unwrap();
    tx.close().unwrap();
    assert!(matches!(rx.receive(), Err(FramingError::Native(_))));
    let retained = partial.drain_partial();
    assert_eq!(retained[0].bytes(), b"orig");
    assert!(!retained[0].complete());
    // A distinct authenticated channel supplies an invalid logical header.
    let (a, b) = pair();
    client = a;
    server = b;
    client.send.send(b"authenticated invalid header").unwrap();
    let mut bad = RecordReceiver::new(server.receive);
    assert!(matches!(
        bad.receive(),
        Err(FramingError::Rejected {
            frame: FrameError::Invalid(_),
            ..
        })
    ));
    assert!(matches!(
        server.send.send(b"after fence"),
        Err(ChannelError::Quarantined)
    ));
    assert!(matches!(
        bad.receive(),
        Err(FramingError::Native(ChannelError::Quarantined))
    ));
}
