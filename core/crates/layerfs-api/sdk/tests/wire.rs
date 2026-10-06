//! Real request/reply codecs and independently fenced authenticated I/O owners.
#[path = "support/native.rs"]
mod sockets;
use layerfs_bridge::{
    codec::{Reassembly, ReassemblyConfig, ReceiveBudget},
    contract::*,
    native::{RecordReceiver, RecordSender},
};
use layerfs_content::{encode_whole_file_payload, ObjectId, ObjectRole};
use layerfs_sdk::{
    client::*,
    runtime::{self, service::*},
};
fn config(kind: MessageKind) -> ReassemblyConfig {
    ReassemblyConfig {
        kind,
        messages: 8,
        demand_messages: 1,
        control_messages: 1,
        message_bytes: 34 << 20,
        bytes: 96 << 20,
        demand_reserve: 34 << 20,
        control_reserve: 64 << 10,
    }
}
fn output_config() -> OutputConfig {
    OutputConfig {
        owners: 1,
        messages: 8,
        demand_messages: 1,
        control_messages: 1,
        bytes: 16 << 20,
        demand_reserve: 2 << 20,
        control_reserve: 64 << 10,
    }
}
fn event(input: &mut NativeInput) -> InputEvent {
    sockets::poll(|| input.try_event().ok())
}
fn close_input(input: &mut NativeInput) -> InputFence {
    input.fence();
    let events = input.detach_events();
    let mut fence = sockets::poll(|| input.try_join());
    fence.events.extend(events);
    fence
}
#[test]
fn fixed_headers_validate_every_operation_and_refuse_wrong_envelope_before_allocation() {
    let save = SaveToken::from_bytes([9; 48]);
    let id = ObjectId::for_bytes(b"identity");
    let mut requests = vec![
        ClientRequest::objects(None, &[]),
        ClientRequest::objects(Some(save), &[]),
        ClientRequest::lengths(&[]),
        ClientRequest::stage(save, id, 19),
        ClientRequest::accept(save, id, ObjectRole::WholeFile, b"x"),
    ];
    for operation in [
        Operation::Policy,
        Operation::Binding,
        Operation::Begin,
        Operation::ReserveInodes,
        Operation::Finish,
        Operation::Abort,
        Operation::Completion,
        Operation::Release,
        Operation::Commit,
        Operation::Discard,
        Operation::History,
    ] {
        let owned = matches!(
            operation,
            Operation::Finish
                | Operation::Abort
                | Operation::Completion
                | Operation::Release
                | Operation::Commit
                | Operation::Discard
                | Operation::History
        );
        requests.push(
            ClientRequest::control(
                operation,
                owned.then_some(save),
                u64::from(operation == Operation::ReserveInodes),
            )
            .unwrap(),
        );
    }
    for request in requests {
        request.validate().unwrap();
        let bytes = request.header.encode().unwrap();
        assert_eq!(RequestHeader::decode(&bytes).unwrap(), request.header);
        let envelope = Envelope {
            message: 1,
            correlation: 1,
            kind: MessageKind::Request,
            class: request.header.operation.class(),
            total_bytes: request.header.total_bytes().unwrap(),
        };
        assert_eq!(
            runtime::check_header(envelope, &bytes).unwrap(),
            request.header
        );
        assert!(runtime::check_header(
            Envelope {
                total_bytes: u64::MAX,
                ..envelope
            },
            &bytes
        )
        .is_err());
        let mut invalid = bytes;
        invalid[7] = 1;
        assert!(RequestHeader::decode(&invalid).is_err());
    }
    let mut request = ClientRequest::accept(save, id, ObjectRole::WholeFile, b"x");
    request.header.value = u64::MAX;
    assert!(request.validate().is_err());
    let bytes = ClientRequest::accept(save, id, ObjectRole::WholeFile, b"x")
        .header
        .encode()
        .unwrap();
    let envelope = Envelope {
        message: 1,
        correlation: 1,
        kind: MessageKind::Request,
        class: MessageClass::Control,
        total_bytes: 105,
    };
    assert!(runtime::check_header(envelope, &bytes).is_err());
}
#[test]
fn output_permit_charges_retained_capacity_and_refuses_terminal_second_transfer() {
    let (_client, server) = sockets::pair();
    let pool = OutputPool::new(output_config()).unwrap();
    let mut output = pool.start(server.send).map_err(|_| "output start").unwrap();
    // Drop receipt delivery before the first send so worker exit is deterministic
    // after its original local write, independent of thread scheduling.
    drop(output.detach_receipts());
    output
        .try_submit(OutputPacket {
            correlation: 1,
            class: MessageClass::Control,
            bytes: b"LRG1\x01\x00\x00\x00".to_vec(),
        })
        .map_err(|_| "initial send")
        .unwrap();
    let joined = sockets::poll(|| output.try_join());
    let before = pool.work().unwrap();
    let mut bytes = Vec::with_capacity(4096);
    bytes.extend_from_slice(b"LRG1\x01\x00\x00\x00");
    let capacity = bytes.capacity();
    let permit = output
        .reserve(2, MessageClass::Control, capacity)
        .unwrap()
        .unwrap();
    assert!(pool.work().unwrap().reserved_bytes >= capacity);
    let (error, packet, permit) = output
        .submit_reserved(
            permit,
            OutputPacket {
                correlation: 2,
                class: MessageClass::Control,
                bytes,
            },
        )
        .expect_err("worker already exited");
    assert!(matches!(error, OutputAdmissionError::Detached));
    let terminal = pool.work().unwrap();
    assert_eq!(terminal.reserved_bytes, 0);
    assert_eq!(
        terminal.packet_capacity_bytes,
        before.packet_capacity_bytes + capacity
    );
    let (error, packet, permit) = output
        .submit_reserved(permit, packet)
        .expect_err("terminal reservation refuses replay");
    assert!(matches!(error, OutputAdmissionError::Frame(_)));
    assert_eq!(pool.work().unwrap().credited_bytes, terminal.credited_bytes);
    assert_eq!(
        pool.work().unwrap().packet_capacity_bytes,
        terminal.packet_capacity_bytes
    );
    assert_eq!(pool.work().unwrap().submissions, terminal.submissions);
    drop((packet, permit));
    assert_eq!(pool.work().unwrap().credited_bytes, before.credited_bytes);
    drop(joined);
    assert_eq!(pool.work().unwrap().credited_bytes, 0);
    assert_eq!(pool.work().unwrap().packet_capacity_bytes, 0);
}
#[test]
fn output_permit_refuses_large_capacity_even_when_wire_length_fits() {
    let (_client, server) = sockets::pair();
    let pool = OutputPool::new(output_config()).unwrap();
    let mut output = pool.start(server.send).map_err(|_| "output start").unwrap();
    let permit = output
        .reserve(1, MessageClass::Control, 8)
        .unwrap()
        .unwrap();
    let mut bytes = Vec::with_capacity(4096);
    bytes.extend_from_slice(b"LRG1\x01\x00\x00\x00");
    let before = pool.work().unwrap();
    let (error, packet, permit) = output
        .submit_reserved(
            permit,
            OutputPacket {
                correlation: 1,
                class: MessageClass::Control,
                bytes,
            },
        )
        .expect_err("retained allocation exceeds reservation");
    assert!(matches!(error, OutputAdmissionError::Frame(_)));
    assert_eq!(pool.work().unwrap().credited_bytes, before.credited_bytes);
    assert_eq!(pool.work().unwrap().submissions, before.submissions);
    assert_eq!(packet.bytes.len(), 8);
    assert!(packet.bytes.capacity() > 8);
    drop((packet, permit));
    output.fence();
    drop(output.detach_receipts());
    drop(sockets::poll(|| output.try_join()));
    assert_eq!(pool.work().unwrap().credited_bytes, 0);
}
#[test]
fn refused_native_header_never_allocates_or_copies_declared_canonical_body() {
    let (client, server) = sockets::pair();
    let budget = ReceiveBudget::new(config(MessageKind::Request)).unwrap();
    let pool = InputPool::new(1, budget.clone()).unwrap();
    let (mut input, _send, _peer) = pool.start(server).map_err(|_| "input start").unwrap();
    let canonical = vec![7; 1 << 20];
    let request = ClientRequest::accept(
        SaveToken::from_bytes([9; 48]),
        ObjectId::for_bytes(&canonical),
        ObjectRole::WholeFile,
        &canonical,
    );
    let mut sender = ClientSender::new(client.send).map_err(|(e, _)| e).unwrap();
    let pending = sender.begin(request).unwrap();
    let envelope = match event(&mut input) {
        InputEvent::Admission { envelope, header } => {
            assert_eq!(header.value, canonical.len() as u64);
            envelope
        }
        _ => panic!("header first"),
    };
    assert_eq!(budget.work().unwrap().allocation_attempts, 0);
    assert_eq!(budget.work().unwrap().copied_bytes, 0);
    input.decide(envelope, false).unwrap();
    let fence = sockets::poll(|| input.try_join());
    let report = fence.worker.map_err(|_| "worker panic").unwrap();
    assert!(matches!(report.failure,InputFailure::Refused(original) if original == envelope));
    assert!(report.partial.is_empty());
    assert_eq!(report.native.records, 1);
    assert_eq!(budget.work().unwrap().allocation_attempts, 0);
    assert!(!pending.body_sent());
    assert_eq!(pool.outstanding(), 1);
    drop(report);
    drop(input);
    assert_eq!(pool.outstanding(), 0);
}
#[test]
fn actual_grant_streams_canonical_input_and_disconnect_returns_partial_original() {
    let (client, server) = sockets::pair();
    let budget = ReceiveBudget::new(config(MessageKind::Request)).unwrap();
    let pool = InputPool::new(1, budget.clone()).unwrap();
    let (mut input, send, _) = pool.start(server).map_err(|_| "input start").unwrap();
    let mut host = RecordSender::new(send).map_err(|(e, _)| e).unwrap();
    let canonical = encode_whole_file_payload(&vec![0x72; 70000]).unwrap();
    let id = ObjectId::for_bytes(&canonical);
    let mut sender = ClientSender::new(client.send).map_err(|(e, _)| e).unwrap();
    let mut receiver = ClientReceiver::new(
        client.receive,
        ReceiveBudget::new(config(MessageKind::Reply)).unwrap(),
    )
    .map_err(|(e, _)| e)
    .unwrap();
    let mut pending = sender
        .begin(ClientRequest::accept(
            SaveToken::from_bytes([9; 48]),
            id,
            ObjectRole::WholeFile,
            &canonical,
        ))
        .unwrap();
    let envelope = match event(&mut input) {
        InputEvent::Admission { envelope, .. } => envelope,
        _ => panic!("header"),
    };
    assert_eq!(budget.work().unwrap().allocation_attempts, 0);
    let grant = runtime::encode_grant();
    host.begin(
        MessageKind::Reply,
        MessageClass::Control,
        envelope.correlation,
        grant.bytes.len() as u64,
        &grant.bytes,
    )
    .unwrap();
    input.decide(envelope, true).unwrap();
    let grant = receiver.receive().map_err(|_| "grant receive").unwrap();
    sender.send_body(&mut pending, &grant).unwrap();
    assert!(pending.body_sent());
    assert!(sender.send_body(&mut pending, &grant).is_err());
    let message = match event(&mut input) {
        InputEvent::Ready(message) => message,
        _ => panic!("complete original"),
    };
    let decoded = runtime::decode_request(message)
        .map_err(|(e, _)| e)
        .unwrap();
    assert_eq!(decoded.copied_bytes, canonical.len() as u64);
    match &decoded.request {
        Request::Accept {
            id: actual,
            canonical: body,
            ..
        } => {
            assert_eq!(*actual, id);
            assert_eq!(body, &canonical)
        }
        _ => panic!("accept"),
    }
    assert_eq!(budget.work().unwrap().live_messages, 1);
    drop(decoded);
    assert_eq!(budget.work().unwrap().live_messages, 0);
    // An explicitly granted second upload is fenced before its body arrives.
    let _partial = sender
        .begin(ClientRequest::accept(
            SaveToken::from_bytes([9; 48]),
            id,
            ObjectRole::WholeFile,
            &canonical,
        ))
        .unwrap();
    let envelope = match event(&mut input) {
        InputEvent::Admission { envelope, .. } => envelope,
        _ => panic!("second header"),
    };
    input.decide(envelope, true).unwrap();
    sockets::poll(|| (budget.work().unwrap().live_messages == 1).then_some(()));
    let fence = close_input(&mut input);
    let report = fence.worker.map_err(|_| "join").unwrap();
    assert_eq!(report.partial.len(), 1);
    assert!(!report.partial[0].complete());
    assert_eq!(report.partial[0].bytes().len(), REQUEST_HEADER_BYTES);
    assert_eq!(report.partial[0].envelope(), envelope);
    assert_eq!(budget.work().unwrap().live_messages, 1);
    drop(report);
    assert_eq!(budget.work().unwrap().live_messages, 0);
}
#[test]
fn output_rotates_ready_classes_and_caller_receipts_hold_real_shared_credit() {
    let (client, server) = sockets::pair();
    let pool = OutputPool::new(output_config()).unwrap();
    let mut output = pool.start(server.send).map_err(|(e, _)| e).unwrap();
    let mut receive = RecordReceiver::new(client.receive);
    let packet = |correlation, class, bytes: Vec<u8>| OutputPacket {
        correlation,
        class,
        bytes,
    };
    output
        .try_submit(packet(1, MessageClass::Control, vec![1]))
        .map_err(|(e, _)| e)
        .unwrap();
    assert_eq!(receive.receive().unwrap().bytes, &[1]);
    output
        .try_submit(packet(2, MessageClass::Control, vec![2]))
        .map_err(|(e, _)| e)
        .unwrap();
    assert_eq!(receive.receive().unwrap().bytes, &[2]);
    // Receipt one fills delivery. The second complete send parks on that bounded
    // queue, so all following jobs are ready together before class rotation.
    output
        .try_submit(packet(
            3,
            MessageClass::Save,
            vec![3; MAX_FRAGMENT_BYTES * 3 + 7],
        ))
        .map_err(|(e, _)| e)
        .unwrap();
    output
        .try_submit(packet(
            4,
            MessageClass::Demand,
            vec![4; MAX_FRAGMENT_BYTES * 2 + 9],
        ))
        .map_err(|(e, _)| e)
        .unwrap();
    output
        .try_submit(packet(5, MessageClass::Control, vec![5]))
        .map_err(|(e, _)| e)
        .unwrap();
    let first = sockets::poll(|| output.try_receipt().ok());
    assert_eq!(first.packet.correlation, 1);
    let mut bodies = Reassembly::new(config(MessageKind::Reply)).unwrap();
    let mut starts = Vec::new();
    let mut done = Vec::new();
    // The rotation cursor survives earlier control jobs. Demand and Save get
    // their first fragment before this control turn; control is never strict-first.
    for expected in [4, 3, 5] {
        let fragment = receive.receive().unwrap();
        assert_eq!(fragment.envelope.correlation, expected);
        assert_eq!(fragment.offset, 0);
        if expected == 5 {
            assert!(fragment.is_end());
            assert_eq!(fragment.bytes, &[5]);
        } else {
            starts.push(expected);
            assert!(bodies.push(fragment).unwrap().is_none());
        }
    }
    let second = sockets::poll(|| output.try_receipt().ok());
    assert_eq!(second.packet.correlation, 2);
    let mut receipts = vec![first, second];
    while done.len() < 2 {
        while let Ok(receipt) = output.try_receipt() {
            receipts.push(receipt);
        }
        let fragment = receive.receive().unwrap();
        if fragment.offset == 0 {
            starts.push(fragment.envelope.correlation)
        }
        if let Some(message) = bodies.push(fragment).unwrap() {
            done.push(message);
        }
    }
    assert_eq!(starts, vec![4, 3]);
    for message in &done {
        assert!(message
            .bytes()
            .iter()
            .all(|b| *b as u64 == message.envelope().correlation));
    }
    while receipts.len() < 5 {
        receipts.push(sockets::poll(|| output.try_receipt().ok()));
    }
    assert!(receipts
        .iter()
        .all(|r| r.complete && r.completed_bytes == r.packet.bytes.len() as u64));
    assert_eq!(pool.work().unwrap().messages, 5);
    drop(receipts);
    assert_eq!(pool.work().unwrap().messages, 0);
    output.fence();
    let queued = output.detach_receipts();
    assert!(queued.is_empty());
    let report = sockets::poll(|| output.try_join())
        .worker
        .map_err(|_| "worker panic")
        .unwrap();
    assert!(report.failure.is_none());
    assert!(report.retained.is_empty());
    drop(report);
    drop(output);
    assert_eq!(pool.work().unwrap().owners, 0);
}
#[test]
fn typed_refusal_preserves_nested_unknown_original_cleanup_and_opaque_io() {
    use layerfs_storage::{port::PersistenceError, StorageError};
    use std::sync::Arc;
    let error = layerfs_sdk::RuntimeError::Storage(Arc::new(StorageError::CleanupFailed {
        original: Box::new(StorageError::UnknownOutcome {
            original: Box::new(StorageError::from(PersistenceError::Uncertain)),
        }),
        cleanup: Box::new(StorageError::Io(std::io::Error::from_raw_os_error(28))),
    }));
    let reply = runtime::encode_refusal(&error, 64 << 10).unwrap();
    let node = match ReplyView::decode(&reply.bytes).unwrap() {
        ReplyView::Failure {
            origin: FailureOrigin::Admission,
            error,
        } => error,
        _ => panic!("typed refusal"),
    };
    assert_eq!(node.domain(), FailureDomain::Runtime);
    assert_eq!(node.code(), 8);
    let cleanup = child(node, 1);
    assert_eq!(cleanup.domain(), FailureDomain::Storage);
    assert_eq!(cleanup.code(), 14);
    assert_eq!(child(cleanup, 1).code(), 13);
    let io = child(child(cleanup, 2), 1);
    assert_eq!(io.domain(), FailureDomain::Io);
    assert!(matches!(
        io.field(2).unwrap().unwrap().value,
        FailureValue::Signed(28)
    ));
    let unknown = child(child(child(cleanup, 1), 1), 1);
    assert_eq!(unknown.domain(), FailureDomain::Storage);
    assert_eq!(unknown.code(), 2);
    let io = child(unknown, 1);
    let persistence = child(io, 4);
    assert_eq!(persistence.domain(), FailureDomain::Persistence);
    assert_eq!(persistence.code(), 5);
    let mut invalid = reply.bytes.clone();
    invalid.push(0);
    assert!(ReplyView::decode(&invalid).is_err());
}
fn child(node: RemoteFailure<'_>, tag: u8) -> RemoteFailure<'_> {
    match node.field(tag).unwrap().unwrap().value {
        FailureValue::Node(bytes) => RemoteFailure::decode(bytes).unwrap(),
        _ => panic!("nested node"),
    }
}
fn calls(client: layerfs_bridge::native::Connection) -> std::sync::Arc<Calls> {
    let fence = client.close_handle().unwrap();
    let peer = client.peer;
    let send = ClientSender::new(client.send).map_err(|(e, _)| e).unwrap();
    let receive = ClientReceiver::new(
        client.receive,
        ReceiveBudget::new(config(MessageKind::Reply)).unwrap(),
    )
    .map_err(|(e, _)| e)
    .unwrap();
    std::sync::Arc::new(Calls::new(send, receive, peer, fence))
}
#[test]
fn consumer_close_fences_a_blocked_call_without_waiting_for_its_mutex_or_replay() {
    let (client, server) = sockets::pair();
    let calls = calls(client);
    let worker_calls = calls.clone();
    let worker = std::thread::spawn(move || {
        worker_calls.call(ClientRequest::control(Operation::Policy, None, 0).unwrap())
    });
    let mut receive = RecordReceiver::new(server.receive);
    let header = receive.receive().unwrap();
    assert_eq!(header.envelope.correlation, 1);
    calls.close().unwrap();
    let failure = worker.join().unwrap().err().unwrap();
    assert_eq!(failure.phase, CallPhase::Grant);
    assert_eq!(failure.correlation, Some(1));
    let again = calls
        .call(ClientRequest::control(Operation::Policy, None, 0).unwrap())
        .err()
        .unwrap();
    assert_eq!(again.phase, CallPhase::Admission);
    assert_eq!(receive.work().records, 1);
    assert!(calls.drain_partial().unwrap().is_empty());
}
#[test]
fn consumer_wrong_result_retains_original_body_and_stops_before_another_call() {
    let (client, server) = sockets::pair();
    let calls = calls(client);
    let worker_calls = calls.clone();
    let worker = std::thread::spawn(move || {
        worker_calls.call(ClientRequest::control(Operation::Policy, None, 0).unwrap())
    });
    let mut receive = RecordReceiver::new(server.receive);
    let mut send = RecordSender::new(server.send).map_err(|(e, _)| e).unwrap();
    let header = receive.receive().unwrap();
    let correlation = header.envelope.correlation;
    let grant = runtime::encode_grant();
    send.begin(
        MessageKind::Reply,
        MessageClass::Control,
        correlation,
        grant.bytes.len() as u64,
        &grant.bytes,
    )
    .unwrap();
    let mut wrong = b"LRP1\x00\x02\x00\x00".to_vec();
    wrong.extend_from_slice(&1u64.to_be_bytes());
    wrong.extend_from_slice(&1u64.to_be_bytes());
    send.begin(
        MessageKind::Reply,
        MessageClass::Control,
        correlation,
        wrong.len() as u64,
        &wrong,
    )
    .unwrap();
    let failure = worker.join().unwrap().err().unwrap();
    assert_eq!(failure.phase, CallPhase::Reply);
    assert_eq!(failure.received.as_ref().unwrap().bytes(), wrong);
    assert!(calls
        .call(ClientRequest::control(Operation::Policy, None, 0).unwrap())
        .is_err());
    assert_eq!(receive.work().records, 1);
}
