//! Real application control publication around the Supervisor's idle rotation.
#![cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "support/supervisor.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/native.rs"]
mod sockets;

use fixture::Fixture;
use layerfs_bridge::{
    codec::{ReassemblyConfig, ReceiveBudget},
    contract::MessageKind,
    native::Connection,
};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_sdk::{
    client::{Attachment, AttachmentError, ClientRequest, ConsumerFence, Operation, ReplyView},
    runtime::{
        service::{InputFailure, ServiceClass},
        supervisor::*,
        RuntimeResult,
    },
};
use std::{
    fmt,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

// External functional watchdogs only; no runtime operation deadline is installed.
const WATCHDOG: Duration = Duration::from_secs(3);
fn end() -> Instant {
    Instant::now() + WATCHDOG
}
fn config() -> SupervisorConfig {
    let mut config = SupervisorConfig::default();
    config.service.connections = 3;
    config.output.owners = 3;
    config.output.messages = 11;
    config
}
fn consumer(connection: Connection) -> Attachment {
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
    match Attachment::new(connection, budget) {
        Ok(value) => value,
        Err((AttachmentError::Native(error), _original)) => {
            panic!("original consumer native startup refusal: {error:?}")
        }
        Err((AttachmentError::Frame(error), _original)) => {
            panic!("original consumer frame startup refusal: {error:?}")
        }
    }
}
fn attach(
    host: &mut Supervisor<'_, '_>,
    connection: Connection,
    branch: BranchId,
    tag: u8,
) -> AttachmentId {
    attached(host.attach(
        connection,
        WorkspaceId::from_authority([tag; 32]).unwrap(),
        branch,
    ))
}
fn attached(result: Result<AttachmentId, AttachFailure>) -> AttachmentId {
    match result {
        Ok(id) => id,
        Err(original) => match &original {
            AttachFailure::Runtime { error, .. } => {
                panic!("original unattempted attach runtime refusal: {error:?}")
            }
            AttachFailure::Input { error, .. } => match error {
                InputFailure::Native(error) => {
                    panic!("original attach input native cause: {error:?}")
                }
                InputFailure::Frame(error) => {
                    panic!("original attach input frame cause: {error:?}")
                }
                InputFailure::Refused(envelope) => {
                    panic!("original attach input refusal: {envelope:?}")
                }
                InputFailure::Detached => panic!("original attach input Detached"),
                InputFailure::Thread(error) => {
                    panic!("original attach input thread cause: {error:?}")
                }
            },
            AttachFailure::Output { error, .. } => {
                panic!("original attach output cause: {error:?}")
            }
        },
    }
}

struct AttachInput {
    connection: Connection,
    workspace: WorkspaceId,
    branch: BranchId,
}
enum Control {
    Attach(Box<AttachInput>),
    Fence(AttachmentId),
}
impl fmt::Debug for Control {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Attach(input) => out
                .debug_struct("Attach")
                .field("peer", &input.connection.peer)
                .field("workspace", &input.workspace)
                .field("branch", &input.branch)
                .finish(),
            Self::Fence(id) => out.debug_tuple("Fence").field(id).finish(),
        }
    }
}
#[derive(Debug, Eq, PartialEq)]
enum Ack {
    Attached(AttachmentId),
    FenceStarted(AttachmentId),
    Delivered(AttachmentId),
    Fenced(AttachmentId),
}
fn publish(queue: &SyncSender<Control>, wake: &SupervisorWakeHandle, original: Control) {
    // try_send returns the exact owned command on full/disconnected admission;
    // this helper never reconstructs or resubmits it. Notification is subsequent.
    queue.try_send(original).unwrap();
    wake.notify()
        .expect("original application notification error");
}
fn acknowledge(queue: &SyncSender<Ack>, value: Ack) {
    queue.try_send(value).unwrap();
}
fn receive_ack(queue: &Receiver<Ack>, expected: Ack) {
    assert_eq!(queue.recv_timeout(WATCHDOG).unwrap(), expected);
}
fn policy(consumer: &Attachment) {
    let original = consumer
        .calls()
        .call(ClientRequest::control(Operation::Policy, None, 0).unwrap())
        .unwrap();
    assert_eq!(original.envelope().correlation, 1);
    assert!(matches!(
        ReplyView::decode(original.bytes()).unwrap(),
        ReplyView::Policy(_)
    ));
    drop(original);
}
fn consumer_fence(consumer: &mut Attachment) -> ConsumerFence {
    consumer.fence();
    let deadline = end();
    loop {
        if let Some(original) = consumer.try_join().unwrap() {
            assert!(original.partial.is_empty());
            assert_eq!(original.receive.as_ref().unwrap().credited_bytes, 0);
            return original;
        }
        assert!(Instant::now() < deadline, "bounded original consumer fence");
        thread::yield_now();
    }
}
fn join(worker: JoinHandle<ConsumerFence>) -> ConsumerFence {
    let deadline = end();
    while !worker.is_finished() {
        assert!(Instant::now() < deadline, "bounded consumer thread exit");
        thread::yield_now();
    }
    // Only the actually finished thread is joined. Its call has finite native
    // fixture I/O and every coordination receive is bounded even if the host panics.
    worker.join().unwrap()
}

enum Applied {
    Attached(Result<AttachmentId, Box<AttachFailure>>),
    Fenced(AttachmentId, RuntimeResult<()>),
}
#[derive(Debug, Eq, PartialEq)]
enum ParkAction {
    NoPermit,
    DiscardedForControl,
    Waited(SupervisorWake),
}
struct AppTurn {
    event: Option<SupervisorEvent>,
    provider: Option<ProviderTurn>,
    applied: Option<Box<Applied>>,
    park: ParkAction,
    control_closed: bool,
}
fn serve_turn(
    host: &mut Supervisor<'_, '_>,
    controls: &Receiver<Control>,
    deadline: Instant,
) -> AppTurn {
    let before = host.work().turns;
    let dispatched: u64 = host.service_work().dispatched.iter().sum();
    let SupervisorTurn {
        event,
        attachment,
        provider,
        park,
        wake_error,
    } = host.step_observed();
    assert!(wake_error.is_none(), "original wake error: {wake_error:?}");
    if matches!(
        attachment.and_then(|turn| turn.wait),
        Some(SupervisorWait::WorkerJoin { .. })
    ) {
        assert!(park.is_none(), "actual pending joins never park");
    }
    if park.is_some() {
        assert!(event.is_none() && provider.is_none());
        assert!(!attachment.is_some_and(|turn| turn.progressed));
    }
    // Inspect application work on every normal turn, including active turns
    // without a permit, and after obtaining a permit before any wait.
    let mut control_closed = false;
    let (applied, park_action) = match controls.try_recv() {
        Ok(original) => {
            let action = if park.is_some() {
                ParkAction::DiscardedForControl
            } else {
                ParkAction::NoPermit
            };
            // This branch does not use the permit again, so its borrow ends
            // before the actual attach/fence transition.
            let applied = match original {
                Control::Attach(input) => {
                    let AttachInput {
                        connection,
                        workspace,
                        branch,
                    } = *input;
                    Applied::Attached(host.attach(connection, workspace, branch).map_err(Box::new))
                }
                Control::Fence(id) => Applied::Fenced(id, host.fence(id)),
            };
            (Some(Box::new(applied)), action)
        }
        Err(status) => {
            control_closed = matches!(status, TryRecvError::Disconnected);
            let action = match park {
                Some(permit) => ParkAction::Waited(permit.wait_until(deadline).unwrap()),
                None => {
                    thread::yield_now();
                    ParkAction::NoPermit
                }
            };
            (None, action)
        }
    };
    assert_eq!(host.work().turns, before + 1);
    assert_eq!(
        host.service_work().dispatched.iter().sum::<u64>() - dispatched,
        u64::from(provider.is_some()),
        "one unchanged original provider unit"
    );
    AppTurn {
        event,
        provider,
        applied,
        park: park_action,
        control_closed,
    }
}
fn original_idle_fence(fence: &AttachmentFence, id: AttachmentId) {
    assert_eq!(fence.attachment, id);
    assert!(
        fence.failure.is_none(),
        "explicit fence retains its own cause"
    );
    assert!(fence.request.is_none());
    assert_eq!(fence.service.cancelled, 0);
    assert!(fence.input.events.is_empty());
    let input = match &fence.input.worker {
        Ok(original) => original,
        Err(_) => panic!("original input worker panic retained in fence"),
    };
    assert!(input.partial.is_empty());
    assert!(input.undelivered.is_none());
    assert!(fence.output.receipts.is_empty());
    let output = match &fence.output.worker {
        Ok(original) => original,
        Err(_) => panic!("original output worker panic retained in fence"),
    };
    assert!(output.retained.is_empty());
    assert!(output.failure.is_none());
}
fn no_owners(host: &Supervisor<'_, '_>) {
    assert_eq!(host.work().attachments, 0);
    assert_eq!(host.input_owners(), 0);
    assert_eq!(host.service_work().outstanding, 0);
    assert_eq!(host.output_work().unwrap().credited_bytes, 0);
    assert_eq!(host.output_work().unwrap().owners, 0);
}

#[test]
fn queued_attach_survives_idle_latch_clear_and_serves_one_original_policy() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (idle, server) = sockets::pair();
    let idle_id = attach(&mut host, server, branch, 71);
    let mut idle = consumer(idle);
    let (client, server) = sockets::pair();
    let (send, controls) = mpsc::sync_channel(1);
    let (ack, acknowledgements) = mpsc::sync_channel(1);
    let (published, publication) = mpsc::sync_channel(1);
    let wake = host.wake_handle();
    let worker = thread::spawn(move || {
        let mut consumer = consumer(client);
        publish(
            &send,
            &wake,
            Control::Attach(Box::new(AttachInput {
                connection: server,
                workspace: WorkspaceId::from_authority([72; 32]).unwrap(),
                branch,
            })),
        );
        published.try_send(()).unwrap();
        let Ack::Attached(id) = acknowledgements.recv_timeout(WATCHDOG).unwrap() else {
            panic!("original attach acknowledgement")
        };
        policy(&consumer);
        // Do not fence a reply which was only received remotely: first await
        // the host's actual Delivery, preserving its original result/credit.
        receive_ack(&acknowledgements, Ack::Delivered(id));
        publish(&send, &wake, Control::Fence(id));
        receive_ack(&acknowledgements, Ack::Fenced(id));
        consumer_fence(&mut consumer)
    });
    publication.recv_timeout(WATCHDOG).unwrap();
    assert_eq!(host.work().headers, 0);
    // The command and notification precede the first round's latch clear.
    // The post-permit queue predicate, rather than the old hint, retains work.
    let first = serve_turn(&mut host, &controls, end());
    assert_eq!(first.park, ParkAction::DiscardedForControl);
    assert!(first.event.is_none() && first.provider.is_none());
    let Some(applied) = first.applied else {
        panic!("original queued attach")
    };
    let Applied::Attached(result) = *applied else {
        panic!("original attach command")
    };
    let id = attached(result.map_err(|original| *original));
    acknowledge(&ack, Ack::Attached(id));

    let deadline = end();
    let mut providers = 0;
    let mut deliveries = 0;
    let original = loop {
        let turn = serve_turn(&mut host, &controls, deadline);
        if let Some(provider) = turn.provider {
            providers += 1;
            assert_eq!(provider.attachment, id);
            assert_eq!(provider.correlation, 1);
        }
        if let Some(applied) = turn.applied {
            let Applied::Fenced(actual, result) = *applied else {
                panic!("no second attach or reconstructed input")
            };
            assert_eq!(actual, id);
            result.unwrap();
        }
        match turn.event {
            Some(SupervisorEvent::Delivered(original)) => {
                deliveries += 1;
                assert_eq!(original.attachment, id);
                assert_eq!(original.output.packet.correlation, 1);
                assert!(original.output.complete);
                assert!(original.completion.is_some() && original.refused.is_none());
                drop(original);
                acknowledge(&ack, Ack::Delivered(id));
            }
            Some(SupervisorEvent::Fenced(original)) => break original,
            None => (),
        }
        assert!(
            !turn.control_closed,
            "original producer still owns its queue"
        );
        assert!(Instant::now() < deadline, "bounded serving proof");
    };
    original_idle_fence(&original, id);
    acknowledge(&ack, Ack::Fenced(id));
    drop(original);
    drop(join(worker));
    assert_eq!(providers, 1);
    assert_eq!(deliveries, 1);
    assert_eq!(host.work().headers, 1);
    assert_eq!(host.work().received, 1);
    assert_eq!(
        host.service_work().dispatched[ServiceClass::Policy as usize],
        1
    );
    assert!(host.fence(id).is_err(), "joined original identity is stale");

    host.fence(idle_id).unwrap();
    let deadline = end();
    let original = loop {
        let turn = serve_turn(&mut host, &controls, deadline);
        assert!(turn.provider.is_none() && turn.applied.is_none());
        match turn.event {
            Some(SupervisorEvent::Fenced(original)) => break original,
            Some(SupervisorEvent::Delivered(_)) => panic!("idle owner has no request"),
            None => (),
        }
        assert!(Instant::now() < deadline, "bounded original idle fence");
    };
    original_idle_fence(&original, idle_id);
    drop(original);
    drop(consumer_fence(&mut idle));
    no_owners(&host);
}

#[test]
fn control_published_after_empty_predicate_wakes_and_other_owner_still_serves() {
    let mut fixture = Fixture::new(1);
    let branch = fixture.branch;
    let mut sessions = fixture.runtime.sessions();
    let mut host = Supervisor::new(&mut sessions, config()).unwrap();
    let (idle, server) = sockets::pair();
    let idle_id = attach(&mut host, server, branch, 73);
    let mut idle = consumer(idle);
    let (client, server) = sockets::pair();
    let id = attach(&mut host, server, branch, 74);
    let (send, controls) = mpsc::sync_channel(1);
    let (ack, acknowledgements) = mpsc::sync_channel(1);
    let (start, started) = mpsc::sync_channel(1);
    let (published, publication) = mpsc::sync_channel(1);
    let wake = host.wake_handle();
    let worker = thread::spawn(move || {
        let mut consumer = consumer(client);
        started.recv_timeout(WATCHDOG).unwrap();
        publish(&send, &wake, Control::Fence(idle_id));
        published.try_send(()).unwrap();
        receive_ack(&acknowledgements, Ack::FenceStarted(idle_id));
        policy(&consumer);
        receive_ack(&acknowledgements, Ack::Delivered(id));
        publish(&send, &wake, Control::Fence(id));
        receive_ack(&acknowledgements, Ack::Fenced(id));
        consumer_fence(&mut consumer)
    });
    {
        let turn = host.step_observed();
        assert!(!turn.progressed() && turn.park.is_none());
    }
    {
        let turn = host.step_observed();
        assert!(turn.wake_error.is_none());
        assert!(!turn.progressed());
        let park = turn.park.expect("complete two-owner idle rotation");
        assert!(park.reasons().input_header);
        assert!(matches!(controls.try_recv(), Err(TryRecvError::Empty)));
        // Publication happens only after the application's empty predicate.
        // The same consumer thread performs the subsequent real RPC.
        start.try_send(()).unwrap();
        publication.recv_timeout(WATCHDOG).unwrap();
        assert_eq!(park.wait_until(end()).unwrap(), SupervisorWake::Notified);
    }
    assert_eq!(host.work().headers, 0, "the wake dispatches no request");
    assert_eq!(host.service_work().submission_attempts, [0; 6]);
    let next = serve_turn(&mut host, &controls, end());
    // One of two idle owners was selected: no global permit yet. Application
    // work must be inspected here too, not only in the park branch.
    assert_eq!(next.park, ParkAction::NoPermit);
    assert!(next.event.is_none() && next.provider.is_none());
    let Some(applied) = next.applied else {
        panic!("original queued fence")
    };
    let Applied::Fenced(actual, result) = *applied else {
        panic!("original fence command")
    };
    assert_eq!(actual, idle_id);
    result.unwrap();
    acknowledge(&ack, Ack::FenceStarted(idle_id));

    let deadline = end();
    let mut providers = 0;
    let mut deliveries = 0;
    let mut idle_original = None;
    let mut live_original = None;
    while idle_original.is_none() || live_original.is_none() {
        let turn = serve_turn(&mut host, &controls, deadline);
        if let Some(provider) = turn.provider {
            providers += 1;
            assert_eq!(provider.attachment, id);
            assert_eq!(provider.correlation, 1);
        }
        if let Some(applied) = turn.applied {
            let Applied::Fenced(actual, result) = *applied else {
                panic!("no new input or reconstructed command")
            };
            assert_eq!(actual, id);
            result.unwrap();
        }
        match turn.event {
            Some(SupervisorEvent::Delivered(original)) => {
                deliveries += 1;
                assert_eq!(original.attachment, id);
                assert_eq!(original.output.packet.correlation, 1);
                assert!(original.output.complete);
                assert!(original.completion.is_some() && original.refused.is_none());
                drop(original);
                acknowledge(&ack, Ack::Delivered(id));
            }
            Some(SupervisorEvent::Fenced(original)) if original.attachment == idle_id => {
                original_idle_fence(&original, idle_id);
                assert!(idle_original.replace(original).is_none());
            }
            Some(SupervisorEvent::Fenced(original)) => {
                original_idle_fence(&original, id);
                assert!(live_original.replace(original).is_none());
                acknowledge(&ack, Ack::Fenced(id));
            }
            None => (),
        }
        if turn.control_closed {
            assert!(
                live_original.is_some(),
                "producer exited only after its exact fence"
            );
        }
        assert!(Instant::now() < deadline, "bounded serving proof");
    }
    drop(live_original.expect("actual serving owner's original joins"));
    drop(idle_original.expect("actual other owner's original joins"));
    drop(join(worker));
    drop(consumer_fence(&mut idle));
    assert_eq!(providers, 1);
    assert_eq!(deliveries, 1);
    assert_eq!(host.work().headers, 1);
    assert_eq!(host.work().received, 1);
    assert_eq!(
        host.service_work().dispatched[ServiceClass::Policy as usize],
        1
    );
    assert!(host.fence(idle_id).is_err());
    assert!(host.fence(id).is_err());
    no_owners(&host);
}
