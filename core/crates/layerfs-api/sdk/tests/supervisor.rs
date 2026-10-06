//! Authenticated socket supervision over the actual initialized macOS provider.
#![cfg(target_os = "macos")]
#[path = "support/supervisor.rs"]
mod fixture;
#[path = "support/native.rs"]
mod sockets;
use fixture::Fixture;
use layerfs_bridge::{
    codec::{ReassemblyConfig, ReceiveBudget},
    contract::MessageKind,
};
use layerfs_history::WorkspaceId;
use layerfs_sdk::{
    client::*,
    runtime::{service::*, supervisor::*},
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
fn consumer(connection: layerfs_bridge::native::Connection) -> Attachment {
    Attachment::new(connection, budget())
        .map_err(|_| "consumer refused")
        .unwrap()
}
fn pump(
    host: &mut Supervisor<'_, '_>,
    mut done: impl FnMut(&Supervisor<'_, '_>) -> bool,
) -> Vec<SupervisorEvent> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    while !done(host) {
        if let Some(event) = host.step() {
            events.push(event);
        }
        assert!(Instant::now() < deadline, "bounded supervisor wait");
        thread::sleep(Duration::from_millis(1));
    }
    events
}
fn fence(host: &mut Supervisor<'_, '_>, id: AttachmentId) -> AttachmentFence {
    host.fence(id).unwrap();
    sockets::poll(|| host.try_join(id).unwrap())
}
fn reply(calls: &Calls, request: ClientRequest<'_>) -> layerfs_bridge::codec::Message {
    calls.call(request).unwrap()
}
#[test]
fn real_supervisor_delivers_binding_same_save_objects_history_and_consumer_ports() {
    let mut fixture = Fixture::new(19);
    let branch = fixture.branch;
    let root = fixture.root;
    let payload = fixture.payload;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (client, server) = sockets::pair();
    let workspace = WorkspaceId::from_authority([2; 32]).unwrap();
    let id = host
        .attach(server, workspace, branch)
        .map_err(|_| "attach refused")
        .unwrap();
    let mut attachment = consumer(client);
    let calls = attachment.calls();
    let worker = thread::spawn(move || {
        let binding = reply(
            &calls,
            ClientRequest::control(Operation::Binding, None, 0).unwrap(),
        );
        assert!(
            matches!(ReplyView::decode(binding.bytes()).unwrap(), ReplyView::Binding(binding) if binding.workspace == workspace)
        );
        drop(binding);
        let begin = reply(
            &calls,
            ClientRequest::control(Operation::Begin, None, 0).unwrap(),
        );
        let save = match ReplyView::decode(begin.bytes()).unwrap() {
            ReplyView::Begun(save) => save,
            _ => panic!("begin"),
        };
        drop(begin);
        let canonical = layerfs_content::encode_whole_file_payload(b"same Save content").unwrap();
        let object = layerfs_content::ObjectId::for_bytes(&canonical);
        let accept = reply(
            &calls,
            ClientRequest::accept(
                save,
                object,
                layerfs_content::ObjectRole::WholeFile,
                &canonical,
            ),
        );
        assert!(
            matches!(ReplyView::decode(accept.bytes()).unwrap(), ReplyView::Accepted(id) if id == object)
        );
        drop(accept);
        let objects = reply(&calls, ClientRequest::objects(Some(save), &[object]));
        match ReplyView::decode(objects.bytes()).unwrap() {
            ReplyView::Objects(mut values) => assert_eq!(values.next().unwrap().1, canonical),
            _ => panic!("same Save objects"),
        }
        drop(objects);
        let finish = reply(
            &calls,
            ClientRequest::control(Operation::Finish, Some(save), 0).unwrap(),
        );
        match ReplyView::decode(finish.bytes()).unwrap() {
            ReplyView::Completion {
                save: got,
                receipt: Ok(receipt),
            } => {
                assert_eq!(got, save);
                assert_eq!(receipt.phase, layerfs_sdk::CompletionPhase::Finish);
                assert!(receipt.outcome.is_ok());
            }
            _ => panic!("known successful finish"),
        }
        drop(finish);
        let stage = reply(&calls, ClientRequest::stage(save, root, 1));
        match ReplyView::decode(stage.bytes()).unwrap() {
            ReplyView::History {
                save: got,
                receipt: Ok(receipt),
            } => {
                assert_eq!(got, save);
                let staged = receipt.stage.unwrap().unwrap();
                assert_eq!(staged.candidate_root, root);
                assert_eq!(staged.generation, 1);
                assert!(receipt.commit.is_none());
            }
            _ => panic!("known successful stage"),
        }
        drop(stage);
        let commit = reply(
            &calls,
            ClientRequest::control(Operation::Commit, Some(save), 0).unwrap(),
        );
        match ReplyView::decode(commit.bytes()).unwrap() {
            ReplyView::History {
                save: got,
                receipt: Ok(receipt),
            } => {
                assert_eq!(got, save);
                assert!(
                    matches!(receipt.commit.unwrap().unwrap(), layerfs_history::CommitStagedOutcome::UpToDate { root: got, .. } if got == root)
                );
            }
            _ => panic!("known UpToDate commit"),
        }
        drop(commit);
        let release = reply(
            &calls,
            ClientRequest::control(Operation::Release, Some(save), 0).unwrap(),
        );
        assert!(matches!(
            ReplyView::decode(release.bytes()).unwrap(),
            ReplyView::Released
        ));
        drop(release);
        use layerfs_content::AuthenticatedObjects;
        use layerfs_workspace::{FileLengths, InodeSerials};
        let objects = RemoteObjects::new(calls.clone(), None);
        assert_eq!(objects.read_canonical_batch(&[payload]).unwrap().len(), 1);
        assert_eq!(
            RemoteLengths::new(calls.clone())
                .file_length(payload)
                .unwrap(),
            19
        );
        assert_eq!(RemoteSerials::new(calls).reserve(4).unwrap().1, 4);
    });
    // Release each original delivery promptly; held completions intentionally
    // prevent explicit Save release rather than inventing remote-consumption ACKs.
    let deadline = Instant::now() + Duration::from_secs(8);
    while !worker.is_finished() {
        drop(host.step());
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap();
    assert!(host.service_work().admitted >= 11);
    let host_fence = fence(&mut host, id);
    assert_eq!(host_fence.service.cancelled, 0);
    attachment.fence();
    let consumer_fence = sockets::poll(|| attachment.try_join().unwrap());
    assert!(consumer_fence.partial.is_empty());
    assert_eq!(host.work().attachments, 0);
    let held_completion = usize::from(
        host_fence
            .request
            .as_ref()
            .is_some_and(|request| request.completion.is_some()),
    );
    assert_eq!(host.service_work().outstanding, held_completion);
    drop(host_fence);
    assert_eq!(host.service_work().outstanding, 0);
}
#[test]
fn peer_blocked_after_grant_cannot_hold_unrelated_policy_and_partial_input_returns_at_fence() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (blocked, server) = sockets::pair();
    let blocked_id = host
        .attach(
            server,
            WorkspaceId::from_authority([1; 32]).unwrap(),
            branch,
        )
        .map_err(|_| "attach")
        .unwrap();
    let mut send = ClientSender::new(blocked.send).map_err(|(e, _)| e).unwrap();
    let mut receive = ClientReceiver::new(blocked.receive, budget())
        .map_err(|(e, _)| e)
        .unwrap();
    let canonical = layerfs_content::encode_whole_file_payload(&vec![8; 70000]).unwrap();
    // Unknown Save is refused before receiving bytes. Use a valid begun Save first.
    let pending = send
        .begin(ClientRequest::control(Operation::Begin, None, 0).unwrap())
        .unwrap();
    pump(&mut host, |host| host.output_work().unwrap().admitted > 0);
    let grant = receive
        .receive()
        .map_err(|_| "original client receive failure")
        .unwrap();
    let mut pending = pending;
    send.send_body(&mut pending, &grant).unwrap();
    drop(grant);
    pump(&mut host, |host| host.service_work().dispatched[4] == 1);
    pump(&mut host, |host| host.output_work().unwrap().admitted >= 2);
    let begin = receive
        .receive()
        .map_err(|_| "original client receive failure")
        .unwrap();
    let save = match ReplyView::decode(begin.bytes()).unwrap() {
        ReplyView::Begun(save) => save,
        _ => panic!("begin"),
    };
    drop(begin);
    // Let host collect the previous send receipt before the next header.
    for _ in 0..6 {
        drop(host.step());
    }
    let _pending = send
        .begin(ClientRequest::accept(
            save,
            layerfs_content::ObjectId::for_bytes(&canonical),
            layerfs_content::ObjectRole::WholeFile,
            &canonical,
        ))
        .unwrap();
    pump(&mut host, |host| host.output_work().unwrap().admitted >= 3);
    let grant = receive
        .receive()
        .map_err(|_| "original client receive failure")
        .unwrap();
    assert!(matches!(
        ReplyView::decode(grant.bytes()).unwrap(),
        ReplyView::Granted
    ));
    drop(grant);
    let (client, server) = sockets::pair();
    let healthy_id = host
        .attach(
            server,
            WorkspaceId::from_authority([2; 32]).unwrap(),
            branch,
        )
        .map_err(|_| "attach")
        .unwrap();
    let healthy = consumer(client);
    let calls = healthy.calls();
    let worker = thread::spawn(move || {
        reply(
            &calls,
            ClientRequest::control(Operation::Policy, None, 0).unwrap(),
        )
    });
    drop(pump(&mut host, |_| worker.is_finished()));
    let policy = worker.join().unwrap();
    assert!(matches!(
        ReplyView::decode(policy.bytes()).unwrap(),
        ReplyView::Policy(_)
    ));
    drop(policy);
    let blocked_fence = fence(&mut host, blocked_id);
    assert_eq!(blocked_fence.service.cancelled, 0);
    assert_eq!(host.service_work().dispatched[3], 0);
    let report = blocked_fence
        .input
        .worker
        .as_ref()
        .map_err(|_| "input panic")
        .unwrap();
    assert_eq!(report.partial.len(), 1);
    assert_eq!(report.partial[0].bytes().len(), REQUEST_HEADER_BYTES);
    assert!(!report.partial[0].complete());
    assert_eq!(host.input_owners(), 2);
    drop(blocked_fence);
    assert_eq!(host.input_owners(), 1);
    drop(fence(&mut host, healthy_id));
}
#[test]
fn pre_body_denial_delivers_original_refusal_then_returns_joined_custody_without_dispatch() {
    let mut fixture = Fixture::new(1);
    let denied = fixture.denied.clone();
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (client, server) = sockets::pair();
    let id = host
        .attach(
            server,
            WorkspaceId::from_authority([4; 32]).unwrap(),
            branch,
        )
        .map_err(|_| "attach")
        .unwrap();
    let attachment = consumer(client);
    denied.set(true);
    let calls = attachment.calls();
    let worker = thread::spawn(move || {
        reply(
            &calls,
            ClientRequest::control(Operation::Begin, None, 0).unwrap(),
        )
    });
    let mut fences = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.is_finished() || host.work().attachments != 0 {
        if let Some(event) = host.step() {
            match event {
                SupervisorEvent::Fenced(fence) => fences.push(fence),
                _ => panic!("no dispatched delivery"),
            }
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    let refusal = worker.join().unwrap();
    assert!(
        matches!(ReplyView::decode(refusal.bytes()).unwrap(), ReplyView::Failure { origin: FailureOrigin::Admission, error } if error.domain() == FailureDomain::Runtime && error.code() == 1)
    );
    assert_eq!(host.service_work().admitted, 0);
    assert_eq!(host.service_work().dispatched, [0; 6]);
    assert_eq!(fences.len(), 1);
    assert_eq!(fences[0].attachment, id);
    assert!(matches!(
        fences[0].request.as_ref().unwrap().admission_error,
        Some(layerfs_sdk::RuntimeError::Denied)
    ));
    let input = fences[0]
        .input
        .worker
        .as_ref()
        .map_err(|_| "input panic")
        .unwrap();
    assert!(input.partial.is_empty());
    let receive = input.reassembly.as_ref().unwrap();
    assert_eq!(receive.allocation_attempts, 0);
    assert_eq!(receive.copied_bytes, 0);
    assert_eq!(host.input_owners(), 1);
    assert!(host.output_work().unwrap().credited_bytes > 0);
    drop(fences);
    assert_eq!(host.input_owners(), 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
}
#[test]
fn slow_output_does_not_hold_provider_service_and_fence_returns_dispatched_result() {
    let mut fixture = Fixture::new((128 << 10) - 1);
    let branch = fixture.branch;
    let payload = fixture.payload;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (blocked, server) = sockets::pair();
    let blocked_id = host
        .attach(
            server,
            WorkspaceId::from_authority([5; 32]).unwrap(),
            branch,
        )
        .map_err(|_| "attach")
        .unwrap();
    let mut send = ClientSender::new(blocked.send).map_err(|(e, _)| e).unwrap();
    let mut receive = ClientReceiver::new(blocked.receive, budget())
        .map_err(|(e, _)| e)
        .unwrap();
    // The frozen policy's WholeFile cutoff is 128KiB. Repeated valid demands
    // preserve an >8MiB original reply without bypassing the producer policy.
    let ids = [payload; 65];
    let mut pending = send.begin(ClientRequest::objects(None, &ids)).unwrap();
    drop(pump(&mut host, |host| {
        host.output_work().unwrap().admitted == 1
    }));
    let grant = receive
        .receive()
        .map_err(|_| "original client receive failure")
        .unwrap();
    send.send_body(&mut pending, &grant).unwrap();
    drop(grant);
    drop(pump(&mut host, |host| {
        host.output_work().unwrap().admitted >= 2
    }));
    let (client, server) = sockets::pair();
    let healthy_id = host
        .attach(
            server,
            WorkspaceId::from_authority([6; 32]).unwrap(),
            branch,
        )
        .map_err(|_| "attach")
        .unwrap();
    let healthy = consumer(client);
    let calls = healthy.calls();
    let worker = thread::spawn(move || {
        reply(
            &calls,
            ClientRequest::control(Operation::Policy, None, 0).unwrap(),
        )
    });
    drop(pump(&mut host, |_| worker.is_finished()));
    let policy = worker.join().unwrap();
    assert!(matches!(
        ReplyView::decode(policy.bytes()).unwrap(),
        ReplyView::Policy(_)
    ));
    drop(policy);
    let blocked_fence = fence(&mut host, blocked_id);
    let request = blocked_fence
        .request
        .as_ref()
        .expect("blocked result custody");
    assert!(matches!(
        request.completion.as_ref().unwrap().outcome(),
        ServiceOutcome::Dispatched(Ok(Response::Objects(_)))
    ));
    let report = blocked_fence
        .output
        .worker
        .as_ref()
        .map_err(|_| "output panic")
        .unwrap();
    assert_eq!(report.retained.len(), 1);
    assert!(!report.retained[0].complete);
    assert_eq!(report.retained[0].packet.correlation, pending.correlation());
    assert!(report.retained[0].completed_bytes < report.retained[0].packet.bytes.len() as u64);
    assert!(host.service_work().outstanding > 0);
    assert!(host.output_work().unwrap().packet_capacity_bytes > 8 << 20);
    drop(blocked_fence);
    // The healthy final send may still be awaiting its host receipt; fencing
    // retains that independent result without changing the blocked disposition.
    drop(fence(&mut host, healthy_id));
    assert_eq!(host.service_work().outstanding, 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
}
#[test]
fn idle_peer_does_not_hold_unrelated_service_and_held_delivery_keeps_both_credits() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (_idle, server) = sockets::pair();
    let idle_id = host
        .attach(
            server,
            WorkspaceId::from_authority([7; 32]).unwrap(),
            branch,
        )
        .map_err(|_| "attach")
        .unwrap();
    let (client, server) = sockets::pair();
    let healthy_id = host
        .attach(
            server,
            WorkspaceId::from_authority([8; 32]).unwrap(),
            branch,
        )
        .map_err(|_| "attach")
        .unwrap();
    let healthy = consumer(client);
    let calls = healthy.calls();
    let worker = thread::spawn(move || {
        reply(
            &calls,
            ClientRequest::control(Operation::Policy, None, 0).unwrap(),
        )
    });
    let mut delivery = None;
    let deadline = Instant::now() + Duration::from_secs(5);
    while delivery.is_none() || !worker.is_finished() {
        if let Some(event) = host.step() {
            match event {
                SupervisorEvent::Delivered(value) => delivery = Some(value),
                _ => panic!("healthy peer stopped"),
            }
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(host.service_work().outstanding, 1);
    assert_eq!(host.output_work().unwrap().messages, 1);
    assert_eq!(delivery.as_ref().unwrap().attachment, healthy_id);
    assert!(delivery.as_ref().unwrap().output.complete);
    drop(delivery);
    drop(worker.join().unwrap());
    assert_eq!(host.service_work().outstanding, 0);
    assert_eq!(host.output_work().unwrap().messages, 0);
    let idle = fence(&mut host, idle_id);
    assert!(idle
        .input
        .worker
        .as_ref()
        .map_err(|_| "input panic")
        .unwrap()
        .partial
        .is_empty());
    drop(idle);
    drop(fence(&mut host, healthy_id));
}
#[test]
fn fence_before_service_admission_returns_original_decoded_unattempted_input() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (client, server) = sockets::pair();
    let id = host
        .attach(
            server,
            WorkspaceId::from_authority([9; 32]).unwrap(),
            branch,
        )
        .map_err(|_| "attach")
        .unwrap();
    let consumer = consumer(client);
    let calls = consumer.calls();
    let worker = thread::spawn(move || {
        calls.call(ClientRequest::control(Operation::Begin, None, 0).unwrap())
    });
    drop(pump(&mut host, |host| host.work().received == 1));
    assert_eq!(host.service_work().admitted, 0);
    let fenced = fence(&mut host, id);
    assert_eq!(fenced.service.cancelled, 0);
    let request = fenced.request.as_ref().unwrap();
    assert!(matches!(
        request.input.as_ref().unwrap().request,
        Request::Begin
    ));
    assert!(request.completion.is_none());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !worker.is_finished() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    let failure = worker.join().unwrap().err().expect("lost original reply");
    assert_eq!(failure.phase, CallPhase::Reply);
    assert_eq!(failure.request.operation, Operation::Begin);
    assert_eq!(host.service_work().dispatched, [0; 6]);
}
