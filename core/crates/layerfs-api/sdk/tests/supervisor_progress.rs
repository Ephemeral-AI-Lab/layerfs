//! Observed production turns and real bounded native publication/wait ownership.
#![cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "support/supervisor.rs"]
mod fixture;
#[path = "support/native.rs"]
mod sockets;
use fixture::Fixture;
use layerfs_bridge::{
    codec::{ReassemblyConfig, ReceiveBudget},
    contract::MessageKind,
};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_sdk::{
    client::{Attachment, ClientRequest, ClientSender, Operation, ReplyView},
    runtime::{
        service::{InputFailure, ServiceClass},
        supervisor::*,
    },
};
use std::{
    thread,
    time::{Duration, Instant},
};

fn config() -> SupervisorConfig {
    let mut config = SupervisorConfig::default();
    config.service.connections = 3;
    config.output.owners = 3;
    config
}
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
fn attach(
    host: &mut Supervisor<'_, '_>,
    server: layerfs_bridge::native::Connection,
    branch: BranchId,
    tag: u8,
) -> AttachmentId {
    host.attach(
        server,
        WorkspaceId::from_authority([tag; 32]).unwrap(),
        branch,
    )
    .map_err(|_| "original attach outcome")
    .unwrap()
}
fn observed_fence(host: &mut Supervisor<'_, '_>, id: AttachmentId) -> AttachmentFence {
    host.fence(id).unwrap();
    sockets::poll(|| {
        let turn = host.step_observed();
        assert!(turn.wake_error.is_none());
        let selected = turn.attachment.unwrap();
        assert_eq!(selected.attachment, id);
        assert_eq!(selected.stage, SupervisorStage::Fence);
        assert!(turn.park.is_none(), "a pending original join never parks");
        match turn.event {
            Some(SupervisorEvent::Fenced(fence)) => {
                assert!(selected.progressed);
                assert!(selected.wait.is_none());
                Some(fence)
            }
            Some(SupervisorEvent::Delivered(_)) => panic!("no delivery after explicit fence"),
            None => {
                assert!(matches!(
                    selected.wait,
                    Some(SupervisorWait::WorkerJoin { .. })
                ));
                None
            }
        }
    })
}

#[test]
fn real_policy_reports_six_original_phase_advances_and_one_provider_unit() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (client, server) = sockets::pair();
    let id = attach(&mut host, server, branch, 61);
    let mut consumer = Attachment::new(client, budget())
        .map_err(|_| "original consumer startup")
        .unwrap();
    let calls = consumer.calls();
    let worker = thread::spawn(move || {
        calls.call(ClientRequest::control(Operation::Policy, None, 0).unwrap())
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut phases = Vec::new();
    let mut dispatches = 0;
    let delivery = loop {
        let turn = host.step_observed();
        assert!(turn.wake_error.is_none());
        let selected = turn.attachment.unwrap();
        assert_eq!(selected.attachment, id);
        if selected.progressed {
            assert_eq!(selected.correlation, Some(1));
            phases.push(selected.stage);
            assert!(turn.park.is_none());
        }
        if let Some(provider) = turn.provider {
            dispatches += 1;
            assert_eq!(provider.attachment, id);
            assert_eq!(provider.correlation, 1);
            assert_eq!(selected.stage, SupervisorStage::Ready);
            assert!(turn.park.is_none());
        }
        match turn.event {
            Some(SupervisorEvent::Delivered(delivery)) => break delivery,
            Some(SupervisorEvent::Fenced(_)) => panic!("original policy connection fenced"),
            None => (),
        }
        if let Some(park) = turn.park {
            assert_eq!(park.wait_until(deadline).unwrap(), SupervisorWake::Notified);
        } else {
            thread::yield_now();
        }
        assert!(Instant::now() < deadline, "bounded original policy phases");
    };
    assert_eq!(
        phases,
        vec![
            SupervisorStage::Header,
            SupervisorStage::Grant,
            SupervisorStage::Body,
            SupervisorStage::Ready,
            SupervisorStage::Result,
            SupervisorStage::Reply
        ]
    );
    assert_eq!(dispatches, 1);
    assert_eq!(
        host.service_work().dispatched[ServiceClass::Policy as usize],
        1
    );
    assert_eq!(host.service_work().outstanding, 1);
    assert!(delivery.output.complete);
    assert_eq!(delivery.output.packet.correlation, 1);
    sockets::poll(|| worker.is_finished().then_some(()));
    let reply = worker.join().unwrap().unwrap();
    assert_eq!(reply.envelope().correlation, 1);
    assert!(matches!(
        ReplyView::decode(reply.bytes()).unwrap(),
        ReplyView::Policy(_)
    ));
    drop(reply);
    drop(delivery);
    drop(observed_fence(&mut host, id));
    consumer.fence();
    let fence = sockets::poll(|| consumer.try_join().unwrap());
    assert!(fence.partial.is_empty());
    assert_eq!(fence.receive.unwrap().credited_bytes, 0);
    assert_eq!(host.service_work().outstanding, 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
}

#[test]
fn whole_idle_rotation_and_notify_before_wait_preserve_wait_only_deadline() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (first, server) = sockets::pair();
    let first_id = attach(&mut host, server, branch, 62);
    let (last, server) = sockets::pair();
    let last_id = attach(&mut host, server, branch, 63);
    let wake = host.wake_handle();
    {
        let turn = host.step_observed();
        assert!(!turn.progressed());
        assert_eq!(
            turn.attachment.unwrap().wait,
            Some(SupervisorWait::InputHeader)
        );
        assert!(turn.park.is_none(), "selected wait is not global park");
    }
    {
        let turn = host.step_observed();
        assert!(!turn.progressed());
        let park = turn.park.expect("complete occupied idle rotation");
        assert!(park.reasons().input_header);
        wake.notify().unwrap();
        assert_eq!(
            park.wait_until(Instant::now() + Duration::from_secs(3))
                .unwrap(),
            SupervisorWake::Notified
        );
    }
    assert!(host.step_observed().park.is_none());
    {
        let turn = host.step_observed();
        let park = turn.park.expect("another complete idle rotation");
        assert_eq!(
            park.wait_until(Instant::now()).unwrap(),
            SupervisorWake::DeadlineReached
        );
    }
    assert_eq!(host.work().attachments, 2, "wait bound closes no owner");
    assert_eq!(host.service_work().submission_attempts, [0; 6]);
    drop(first);
    drop(last);
    // Each explicit fence returns only its original workers/custody. Observe
    // try_join directly because the unrelated live attachment still rotates.
    host.fence(first_id).unwrap();
    drop(sockets::poll(|| host.try_join(first_id).unwrap()));
    drop(observed_fence(&mut host, last_id));
    assert_eq!(host.input_owners(), 0);
    assert_eq!(host.work().attachments, 0);
    let turn = host.step_observed();
    let park = turn.park.expect("empty initialized registry may wait");
    assert!(park.reasons().no_attachments);
    assert_eq!(
        park.wait_until(Instant::now()).unwrap(),
        SupervisorWake::DeadlineReached
    );
}

#[test]
fn native_header_publication_after_park_wakes_without_replacing_original_fence() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (client, server) = sockets::pair();
    let id = attach(&mut host, server, branch, 64);
    let mut send = ClientSender::new(client.send)
        .map_err(|(error, _)| error)
        .unwrap();
    let pending;
    {
        let turn = host.step_observed();
        let park = turn.park.expect("idle original input owner");
        pending = send
            .begin(ClientRequest::control(Operation::Policy, None, 0).unwrap())
            .unwrap();
        assert_eq!(
            park.wait_until(Instant::now() + Duration::from_secs(3))
                .unwrap(),
            SupervisorWake::Notified
        );
    }
    {
        let turn = host.step_observed();
        let selected = turn.attachment.unwrap();
        assert_eq!(selected.attachment, id);
        assert_eq!(selected.stage, SupervisorStage::Header);
        assert!(selected.progressed);
        assert_eq!(selected.correlation, Some(pending.correlation()));
        assert!(turn.event.is_none());
        assert!(turn.provider.is_none());
        assert!(turn.park.is_none());
    }
    let fence = observed_fence(&mut host, id);
    let original = fence.request.as_ref().unwrap();
    assert_eq!(original.envelope.correlation, pending.correlation());
    assert_eq!(original.header.operation, Operation::Policy);
    assert!(original.completion.is_none());
    assert_eq!(fence.service.cancelled, 0);
    assert_eq!(host.service_work().submission_attempts, [0; 6]);
    drop(fence);
    assert_eq!(host.input_owners(), 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
}

#[test]
fn actual_input_sender_closure_wakes_park_and_preserves_original_native_failure() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (client, server) = sockets::pair();
    let id = attach(&mut host, server, branch, 65);
    {
        let turn = host.step_observed();
        let park = turn.park.expect("original idle header receiver");
        drop(client);
        assert_eq!(
            park.wait_until(Instant::now() + Duration::from_secs(3))
                .unwrap(),
            SupervisorWake::Notified
        );
    }
    let fence = sockets::poll(|| {
        let turn = host.step_observed();
        assert!(turn.wake_error.is_none());
        assert!(turn.park.is_none(), "original failure/join is not parked");
        match turn.event {
            Some(SupervisorEvent::Fenced(fence)) => Some(fence),
            Some(SupervisorEvent::Delivered(_)) => panic!("no original request was sent"),
            None => {
                assert!(matches!(
                    turn.attachment.unwrap().wait,
                    Some(SupervisorWait::WorkerJoin { .. })
                ));
                None
            }
        }
    });
    assert_eq!(fence.attachment, id);
    assert!(matches!(
        &fence.failure,
        Some(SupervisorFailure::InputStopped)
    ));
    assert!(fence.request.is_none());
    let report = fence
        .input
        .worker
        .as_ref()
        .map_err(|_| "original input panic")
        .unwrap();
    assert!(matches!(&report.failure, InputFailure::Native(_)));
    assert!(report.partial.is_empty());
    assert!(report.native.record_io_attempts >= 1);
    assert_eq!(host.service_work().submission_attempts, [0; 6]);
    drop(fence);
    assert_eq!(host.input_owners(), 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
}
