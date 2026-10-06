//! Consumer fences use independent shutdown while a caller owns the call mutex.
#[path = "support/native.rs"]
mod sockets;
use layerfs_bridge::{
    codec::{ReassemblyConfig, ReceiveBudget},
    contract::MessageKind,
};
use layerfs_sdk::client::{Attachment, CallPhase, ClientRequest, Operation};
use std::{
    thread,
    time::{Duration, Instant},
};
fn budget() -> ReceiveBudget {
    ReceiveBudget::new(ReassemblyConfig {
        kind: MessageKind::Reply,
        messages: 8,
        demand_messages: 1,
        control_messages: 1,
        message_bytes: 34 << 20,
        bytes: 96 << 20,
        demand_reserve: 34 << 20,
        control_reserve: 64 << 10,
    })
    .unwrap()
}
#[test]
fn consumer_fence_wakes_blocked_call_and_retains_original_failure() {
    let (client, mut host) = sockets::pair();
    let mut attachment = Attachment::new(client, budget())
        .map_err(|_| "attachment refused")
        .unwrap();
    let calls = attachment.calls();
    let worker = thread::spawn(move || {
        calls.call(ClientRequest::control(Operation::Policy, None, 0).unwrap())
    });
    // Receiving the original header proves the caller is waiting for its grant.
    host.receive.receive().unwrap();
    attachment.fence();
    let fence = sockets::poll(|| attachment.try_join().unwrap());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !worker.is_finished() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    let failure = worker.join().unwrap().err().expect("original call failed");
    assert_eq!(failure.phase, CallPhase::Grant);
    assert_eq!(failure.correlation, Some(1));
    assert_eq!(failure.request.operation, Operation::Policy);
    assert!(fence.partial.is_empty());
    assert!(fence.send_native.records > 0);
    assert!(attachment.try_join().is_err());
    let failure = attachment
        .calls()
        .call(ClientRequest::control(Operation::Policy, None, 0).unwrap())
        .err()
        .unwrap();
    assert_eq!(failure.phase, CallPhase::Admission);
    assert_eq!(failure.correlation, None);
}
#[test]
fn wrong_budget_kind_refuses_attachment_before_native_request() {
    let (client, _host) = sockets::pair();
    let mut config = budget().config();
    config.kind = MessageKind::Request;
    let (error, connection) = Attachment::new(client, ReceiveBudget::new(config).unwrap())
        .err()
        .expect("wrong reply budget refused");
    assert!(matches!(
        error,
        layerfs_sdk::client::AttachmentError::Frame(_)
    ));
    assert_eq!(connection.send.work().records, 0);
    assert_eq!(connection.receive.work().records, 0);
}
