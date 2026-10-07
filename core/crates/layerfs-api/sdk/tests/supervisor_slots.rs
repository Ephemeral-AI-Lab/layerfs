//! Public native ownership checks for sparse supervisor attachment rotation.
#![cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "support/supervisor.rs"]
mod fixture;
#[path = "support/native.rs"]
mod sockets;
use fixture::Fixture;
use layerfs_bridge::{
    codec::{Message, ReassemblyConfig, ReceiveBudget},
    contract::MessageKind,
    native::Connection,
};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_sdk::{
    client::{ClientReceiver, ClientRequest, ClientSender, Operation, PendingRequest, ReplyView},
    runtime::{service::ServiceClass, supervisor::*},
};
use std::{
    thread,
    time::{Duration, Instant},
};

const SLOTS: usize = 3;
const OUTPUT_MESSAGES: usize = 3 * SLOTS + 2;

fn config() -> SupervisorConfig {
    let mut config = SupervisorConfig::default();
    config.service.connections = SLOTS;
    config.output.owners = SLOTS;
    config.output.messages = OUTPUT_MESSAGES;
    config
}
struct Raw {
    send: ClientSender,
    receive: ClientReceiver,
}
impl Raw {
    fn new(connection: Connection) -> Self {
        let budget = ReceiveBudget::new(ReassemblyConfig {
            kind: MessageKind::Reply,
            messages: 8,
            demand_messages: 1,
            control_messages: 1,
            message_bytes: 34 << 20,
            bytes: 96 << 20,
            demand_reserve: 34 << 20,
            control_reserve: 64 << 10,
        })
        .unwrap();
        Self {
            send: ClientSender::new(connection.send)
                .map_err(|(error, _)| error)
                .unwrap(),
            receive: ClientReceiver::new(connection.receive, budget)
                .map_err(|_| "original client receive startup")
                .unwrap(),
        }
    }
    fn begin(&mut self) -> PendingRequest<'static> {
        self.send
            .begin(ClientRequest::control(Operation::Policy, None, 0).unwrap())
            .unwrap()
    }
    fn receive(&mut self) -> Message {
        self.receive
            .receive()
            .map_err(|_| "original client receive outcome")
            .unwrap()
    }
    fn grant(&mut self, pending: &mut PendingRequest<'_>) {
        let grant = self.receive();
        assert_eq!(grant.envelope().correlation, pending.correlation());
        assert!(matches!(
            ReplyView::decode(grant.bytes()).unwrap(),
            ReplyView::Granted
        ));
        self.send.send_body(pending, &grant).unwrap();
        drop(grant);
    }
    fn policy(&mut self, correlation: u64) {
        let reply = self.receive();
        assert_eq!(reply.envelope().correlation, correlation);
        assert!(matches!(
            ReplyView::decode(reply.bytes()).unwrap(),
            ReplyView::Policy(_)
        ));
        drop(reply);
    }
}
fn attach(
    host: &mut Supervisor<'_, '_>,
    connection: Connection,
    branch: BranchId,
    workspace: u8,
) -> AttachmentId {
    host.attach(
        connection,
        WorkspaceId::from_authority([workspace; 32]).unwrap(),
        branch,
    )
    .map_err(|_| "original attachment refusal")
    .unwrap()
}
fn until(host: &mut Supervisor<'_, '_>, condition: impl Fn(&Supervisor<'_, '_>) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition(host) {
        assert!(
            host.step().is_none(),
            "no final result at selected boundary"
        );
        assert!(Instant::now() < deadline, "bounded original phase wait");
        thread::sleep(Duration::from_millis(1));
    }
}
fn delivery(host: &mut Supervisor<'_, '_>, id: AttachmentId) -> Delivery {
    sockets::poll(|| match host.step() {
        Some(SupervisorEvent::Delivered(value)) => {
            assert_eq!(value.attachment, id);
            assert!(value.output.complete);
            Some(value)
        }
        Some(SupervisorEvent::Fenced(_)) => panic!("unexpected original fence"),
        None => None,
    })
}
fn fill_output(host: &mut Supervisor<'_, '_>, id: AttachmentId, client: &mut Raw) -> Vec<Delivery> {
    let mut held = Vec::new();
    for _ in 0..OUTPUT_MESSAGES {
        let admitted = host.output_work().unwrap().admitted;
        let mut pending = client.begin();
        until(host, |host| {
            host.output_work().unwrap().admitted == admitted + 1
        });
        client.grant(&mut pending);
        held.push(delivery(host, id));
        client.policy(pending.correlation());
    }
    assert_eq!(host.output_work().unwrap().messages, OUTPUT_MESSAGES);
    assert_eq!(host.service_work().outstanding, OUTPUT_MESSAGES);
    held
}
fn fence(host: &mut Supervisor<'_, '_>, id: AttachmentId) -> AttachmentFence {
    host.fence(id).unwrap();
    let fence = sockets::poll(|| host.try_join(id).unwrap());
    assert_eq!(fence.attachment, id);
    fence
}

#[test]
fn sparse_slot_gets_each_turn_and_fence_reuse_preserves_original_custody() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (_lower, server) = sockets::pair();
    let lower = attach(&mut host, server, branch, 21);
    let (client, server) = sockets::pair();
    let middle = attach(&mut host, server, branch, 22);
    let (_upper, server) = sockets::pair();
    let upper = attach(&mut host, server, branch, 23);
    drop(fence(&mut host, lower));
    drop(fence(&mut host, upper));
    assert_eq!(host.work().attachments, 1);
    assert_eq!(host.input_owners(), 1);
    let mut client = Raw::new(client);
    let held = fill_output(&mut host, middle, &mut client);

    let headers = host.work().headers;
    let pending = client.begin();
    until(&mut host, |host| host.work().headers == headers + 1);
    // Every output slot is caller-held. This header is known parked before grant,
    // so its readiness observation is deterministic, independent of worker timing.
    let attempts = host.service_work().submission_attempts;
    for _ in 0..5 {
        let before = host.work();
        assert!(host.step().is_none());
        assert_eq!(host.work().turns, before.turns + 1);
        assert_eq!(host.work().output_waits, before.output_waits + 1);
        assert_eq!(host.service_work().submission_attempts, attempts);
    }
    let original = fence(&mut host, middle);
    let request = original.request.as_ref().expect("original parked header");
    assert_eq!(request.envelope.correlation, pending.correlation());
    assert_eq!(request.header.operation, Operation::Policy);
    assert!(request.input.is_none());
    assert!(request.completion.is_none());
    assert_eq!(original.service.cancelled, 0);
    assert_eq!(host.work().attachments, 0);
    drop(original);
    assert_eq!(host.input_owners(), 0);
    assert_eq!(host.service_work().outstanding, OUTPUT_MESSAGES);
    assert_eq!(host.output_work().unwrap().messages, OUTPUT_MESSAGES);
    drop(held);
    assert_eq!(host.service_work().outstanding, 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
    assert!(host.fence(middle).is_err(), "retired identity is stale");
    let turns = host.work().turns;
    assert!(host.step().is_none());
    assert_eq!(host.work().turns, turns + 1);
    assert_eq!(host.service_work().submission_attempts, attempts);

    let (connection, server) = sockets::pair();
    let reused = attach(&mut host, server, branch, 24);
    assert_ne!(reused, lower, "reused first slot mints a new capability");
    assert_ne!(reused, middle);
    assert!(
        host.fence(lower).is_err(),
        "reused slot rejects old identity"
    );
    let mut client = Raw::new(connection);
    let mut pending = client.begin();
    let admitted = host.output_work().unwrap().admitted;
    until(&mut host, |host| {
        host.output_work().unwrap().admitted == admitted + 1
    });
    client.grant(&mut pending);
    drop(delivery(&mut host, reused));
    client.policy(pending.correlation());
    drop(fence(&mut host, reused));
    assert_eq!(host.input_owners(), 0);
    assert_eq!(host.service_work().outstanding, 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
}

#[test]
fn occupied_slots_rotate_across_hole_and_keep_one_phase_per_turn() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (first, server) = sockets::pair();
    let first_id = attach(&mut host, server, branch, 31);
    let (_hole, server) = sockets::pair();
    let hole = attach(&mut host, server, branch, 32);
    let (last, server) = sockets::pair();
    let last_id = attach(&mut host, server, branch, 33);
    drop(fence(&mut host, hole));
    let mut first = Raw::new(first);
    let mut last = Raw::new(last);
    let mut held = fill_output(&mut host, first_id, &mut first);
    let headers = host.work().headers;
    let mut first_pending = first.begin();
    until(&mut host, |host| host.work().headers == headers + 1);
    let mut last_pending = last.begin();
    until(&mut host, |host| host.work().headers == headers + 2);
    // Both original headers are now parked against caller-owned output slots.
    for _ in 0..4 {
        let waits = host.work().output_waits;
        assert!(host.step().is_none());
        assert_eq!(host.work().output_waits, waits + 1);
    }
    let admitted = host.output_work().unwrap().admitted;
    let attempts = host.service_work().submission_attempts;
    let waits = host.work().output_waits;
    drop(held.pop().unwrap());
    // The last header left the cursor after the last occupied slot. The next
    // turn grants only the first original header; the following turn skips the
    // hole and observes the last header's still-occupied output window.
    assert!(host.step().is_none());
    assert_eq!(host.output_work().unwrap().admitted, admitted + 1);
    assert_eq!(host.work().output_waits, waits);
    assert_eq!(host.service_work().submission_attempts, attempts);
    first.grant(&mut first_pending);
    assert!(host.step().is_none());
    assert_eq!(host.output_work().unwrap().admitted, admitted + 1);
    assert_eq!(host.work().output_waits, waits + 1);
    assert_eq!(host.service_work().submission_attempts, attempts);
    drop(held);
    until(&mut host, |host| {
        host.output_work().unwrap().admitted == admitted + 2
    });
    last.grant(&mut last_pending);
    let mut deliveries = Vec::new();
    sockets::poll(|| {
        let dispatched = host.service_work().dispatched;
        if let Some(event) = host.step() {
            match event {
                SupervisorEvent::Delivered(value) => {
                    assert!(value.output.complete);
                    assert!(!deliveries
                        .iter()
                        .any(|old: &Delivery| old.attachment == value.attachment));
                    deliveries.push(value);
                }
                SupervisorEvent::Fenced(_) => panic!("unexpected original fence"),
            }
        }
        let after = host.service_work().dispatched;
        assert!(after.iter().sum::<u64>() - dispatched.iter().sum::<u64>() <= 1);
        (deliveries.len() == 2).then_some(())
    });
    assert!(deliveries.iter().any(|value| value.attachment == first_id));
    assert!(deliveries.iter().any(|value| value.attachment == last_id));
    first.policy(first_pending.correlation());
    last.policy(last_pending.correlation());
    assert_eq!(host.service_work().outstanding, 2);
    drop(deliveries);
    assert_eq!(host.service_work().outstanding, 0);
    assert_eq!(
        host.service_work().dispatched[ServiceClass::Policy as usize],
        OUTPUT_MESSAGES as u64 + 2
    );
    drop(fence(&mut host, first_id));
    drop(fence(&mut host, last_id));
    assert_eq!(host.work().attachments, 0);
    assert_eq!(host.input_owners(), 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
}

#[test]
fn acquired_header_can_progress_and_wait_and_credit_release_wakes_original_attempt() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (client, server) = sockets::pair();
    let id = attach(&mut host, server, branch, 51);
    let mut client = Raw::new(client);
    let mut held = fill_output(&mut host, id, &mut client);
    let mut pending = client.begin();
    sockets::poll(|| {
        let turn = host.step_observed();
        assert!(turn.wake_error.is_none());
        let selected = turn.attachment.unwrap();
        if selected.progressed {
            assert_eq!(selected.stage, SupervisorStage::Header);
            assert_eq!(selected.correlation, Some(pending.correlation()));
            assert!(matches!(
                selected.wait,
                Some(SupervisorWait::OutputCredit { .. })
            ));
            assert!(turn.event.is_none());
            assert!(turn.provider.is_none());
            assert!(turn.park.is_none());
            Some(())
        } else {
            None
        }
    });
    {
        let turn = host.step_observed();
        assert!(!turn.progressed());
        let park = turn.park.expect("complete single occupied credit wait");
        assert!(park.reasons().output_credit);
        drop(held.pop().unwrap());
        assert_eq!(
            park.wait_until(Instant::now() + Duration::from_secs(3))
                .unwrap(),
            SupervisorWake::Notified
        );
    }
    {
        let turn = host.step_observed();
        let selected = turn.attachment.unwrap();
        assert!(selected.progressed);
        assert_eq!(selected.correlation, Some(pending.correlation()));
        assert!(selected.wait.is_none());
        assert!(
            turn.provider.is_none(),
            "grant is not a provider invocation"
        );
    }
    client.grant(&mut pending);
    drop(held);
    drop(delivery(&mut host, id));
    client.policy(pending.correlation());
    assert_eq!(host.work().headers, OUTPUT_MESSAGES as u64 + 1);
    assert_eq!(host.service_work().admitted, OUTPUT_MESSAGES as u64 + 1);
    drop(fence(&mut host, id));
    assert_eq!(host.service_work().outstanding, 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
}
