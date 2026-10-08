//! Admission waits are event-driven and preserve each original command.
use layerfs_daemon::{
    Admission, Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Pending, Response,
};
use layerfs_overlay::{DirectoryEntry, Inode, InodeKind, ProfileConfig, Route};
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(3);
struct Fixture {
    path: PathBuf,
    owner: Option<Owner>,
    client: OwnerClient,
    route: Route,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-admission-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let owner = Owner::start(
            &path.join("overlay"),
            ProfileConfig::default(),
            OwnerConfig {
                namespaces: 1,
                jobs_per_namespace: 1,
                lifecycle_jobs_per_namespace: 1,
                ..OwnerConfig::default()
            },
        )
        .unwrap();
        let client = owner.client();
        let done = finish(
            client
                .try_submit(
                    None,
                    Command::Open {
                        incarnation: [91; 32],
                        base_root: [92; 32],
                    },
                )
                .unwrap(),
        );
        let route = match done.result() {
            Ok(Response::Opened(route)) => *route,
            other => panic!("open: {other:?}"),
        };
        drop(done);
        Self {
            path,
            owner: Some(owner),
            client,
            route,
        }
    }
    fn hold(&self) -> Completion {
        let (event, receiver) = event(None);
        finish(admitted(
            self.waiting(Command::Inode(900)),
            &event,
            &receiver,
        ))
    }
    fn waiting(&self, command: Command) -> Admission {
        self.client
            .submit_when_available(Some(self.route), command)
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.owner.take());
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
struct Event {
    sender: mpsc::SyncSender<()>,
    wakes: AtomicUsize,
    client: Option<OwnerClient>,
}
impl Wake for Event {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        // Reentrancy would deadlock if credit notification held the queue lock.
        if let Some(client) = &self.client {
            client.diagnostics().unwrap();
        }
        self.wakes.fetch_add(1, Ordering::SeqCst);
        let _ = self.sender.try_send(());
    }
}
fn event(client: Option<OwnerClient>) -> (Arc<Event>, mpsc::Receiver<()>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    (
        Arc::new(Event {
            sender,
            wakes: AtomicUsize::new(0),
            client,
        }),
        receiver,
    )
}
fn poll<F: Future + Unpin>(future: &mut F, event: &Arc<Event>) -> Poll<F::Output> {
    let waker = Waker::from(event.clone());
    Pin::new(future).poll(&mut Context::from_waker(&waker))
}
fn finish(mut pending: Pending) -> Completion {
    let (event, receiver) = event(None);
    let deadline = Instant::now() + WAIT;
    loop {
        match poll(&mut pending, &event) {
            Poll::Ready(done) => return done.unwrap(),
            Poll::Pending => receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("completion did not wake"),
        }
    }
}
fn admitted(mut waiting: Admission, event: &Arc<Event>, receiver: &mpsc::Receiver<()>) -> Pending {
    let deadline = Instant::now() + WAIT;
    loop {
        match poll(&mut waiting, event) {
            Poll::Ready(done) => return done.unwrap(),
            Poll::Pending => receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("admission did not wake"),
        }
    }
}

fn idle(client: &OwnerClient) {
    let deadline = Instant::now() + WAIT;
    while client.diagnostics().unwrap().outstanding != 0 {
        assert!(
            Instant::now() < deadline,
            "publisher did not release final credit"
        );
        std::thread::yield_now();
    }
}

#[test]
fn held_completion_release_wakes_and_admits_original_once() {
    let fixture = Fixture::new();
    let held = fixture.hold();
    let before = fixture.client.diagnostics().unwrap().admitted;
    let mut waiting = fixture.waiting(Command::Inode(123));
    let (event, receiver) = event(Some(fixture.client.clone()));
    assert!(poll(&mut waiting, &event).is_pending());
    assert!(poll(&mut waiting, &event).is_pending());
    assert!(receiver.try_recv().is_err());
    assert_eq!(fixture.client.diagnostics().unwrap().admitted, before);
    drop(held);
    receiver.recv_timeout(WAIT).unwrap();
    let done = finish(admitted(waiting, &event, &receiver));
    assert!(matches!(done.result(), Ok(Response::Inode(None))));
    assert_eq!(fixture.client.diagnostics().unwrap().admitted, before + 1);
    assert_eq!(fixture.client.diagnostics().unwrap().outstanding, 1);
    drop(done);
    idle(&fixture.client);
}

#[test]
fn release_before_first_poll_is_observed_without_future_event() {
    let fixture = Fixture::new();
    let held = fixture.hold();
    let mut waiting = fixture.waiting(Command::Inode(555));
    drop(held);
    // The publisher drops its own final credit after the waiter observes the
    // completion; the release this test is about must be complete before the
    // first poll, with no later event to deliver it.
    idle(&fixture.client);
    let (event, _) = event(None);
    let pending = match poll(&mut waiting, &event) {
        Poll::Ready(Ok(pending)) => pending,
        _ => panic!("available credit was not admitted"),
    };
    assert!(matches!(
        finish(pending).result(),
        Ok(Response::Inode(None))
    ));
    assert_eq!(event.wakes.load(Ordering::SeqCst), 0);
}

#[test]
fn stopped_waiter_returns_the_same_unattempted_input() {
    let mut fixture = Fixture::new();
    let held = fixture.hold();
    let before = fixture.client.diagnostics().unwrap().admitted;
    let mut waiting = fixture.waiting(Command::Inode(0xdead));
    let (event, receiver) = event(None);
    assert!(poll(&mut waiting, &event).is_pending());
    fixture.owner.take().unwrap().stop().unwrap();
    receiver.recv_timeout(WAIT).unwrap();
    assert!(matches!(
        poll(&mut waiting, &event),
        Poll::Ready(Err((OwnerError::Stopped, Command::Inode(0xdead))))
    ));
    assert_eq!(fixture.client.diagnostics().unwrap().admitted, before);
    assert!(matches!(held.result(), Ok(Response::Inode(None))));
}

#[test]
fn fixed_registrations_replace_wakers_and_cancel_without_losing_input() {
    let fixture = Fixture::new();
    let held = fixture.hold();
    let mut first = fixture.waiting(Command::Inode(41));
    let second = fixture.waiting(Command::Inode(42));
    assert!(matches!(
        fixture
            .client
            .submit_when_available(Some(fixture.route), Command::Inode(43)),
        Err((OwnerError::AdmissionFull, Command::Inode(43)))
    ));
    let (old, _) = event(None);
    let weak = Arc::downgrade(&old);
    assert!(poll(&mut first, &old).is_pending());
    drop(old);
    let (last, receiver) = event(None);
    assert!(poll(&mut first, &last).is_pending());
    assert!(weak.upgrade().is_none());
    assert!(matches!(second.into_command(), Some(Command::Inode(42))));
    let mut replacement = fixture.waiting(Command::Inode(44));
    let (cancelled, _) = event(None);
    let cancelled_weak = Arc::downgrade(&cancelled);
    assert!(poll(&mut replacement, &cancelled).is_pending());
    drop(cancelled);
    drop(replacement);
    assert!(cancelled_weak.upgrade().is_none());
    drop(held);
    receiver.recv_timeout(WAIT).unwrap();
    drop(finish(admitted(first, &last, &receiver)));
    idle(&fixture.client);
}

#[test]
fn credit_release_races_registration_without_a_polling_fallback() {
    let fixture = Fixture::new();
    for serial in 1..=64 {
        let held = fixture.hold();
        let waiting = fixture.waiting(Command::Inode(serial));
        let (event, receiver) = event(None);
        let release = std::thread::spawn(move || drop(held));
        let done = finish(admitted(waiting, &event, &receiver));
        assert!(matches!(done.result(), Ok(Response::Inode(None))));
        release.join().unwrap();
    }
}

#[test]
fn a_command_larger_than_the_entire_budget_is_refused_without_waiting() {
    let fixture = Fixture::new();
    let before = fixture.client.diagnostics().unwrap().admitted;
    let command = Command::Publish {
        inode: Inode {
            serial: 2,
            kind: InodeKind::File,
            mode: 0o644,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            nlink: 1,
            size: 0,
            inherited_cutoff: 0,
            born: 0,
            entries: 0,
        },
        name: Some(DirectoryEntry {
            inherited: false,
            parent: 1,
            name: vec![1; 8 * 1024 * 1024],
            serial: Some(2),
        }),
        cell: None,
    };
    let original = match fixture
        .client
        .submit_when_available(Some(fixture.route), command)
    {
        Err((OwnerError::AdmissionFull, command)) => command,
        _ => panic!("permanently oversized command was allowed to wait"),
    };
    assert!(
        matches!(original, Command::Publish { name: Some(entry), .. } if entry.name.len() == 8 * 1024 * 1024)
    );
    assert_eq!(fixture.client.diagnostics().unwrap().admitted, before);
}

struct PanickingTask(mpsc::SyncSender<()>);
impl Wake for PanickingTask {
    fn wake(self: Arc<Self>) {
        let _ = self.0.try_send(());
        panic!("caller task panicked during completion notification");
    }
}

#[test]
fn worker_loss_fences_admission_and_retains_the_original_join_failure() {
    let mut fixture = Fixture::new();
    let (event, receiver) = event(None);
    let published = finish(admitted(
        fixture.waiting(Command::Publish {
            inode: Inode {
                serial: 2,
                kind: InodeKind::File,
                mode: 0o644,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
                nlink: 1,
                size: 0,
                inherited_cutoff: 0,
                born: 0,
                entries: 0,
            },
            name: None,
            cell: None,
        }),
        &event,
        &receiver,
    ));
    let publication = match published.result() {
        Ok(Response::Published(publication)) => *publication,
        other => panic!("publish: {other:?}"),
    };
    drop(published);
    let mut capture = admitted(fixture.waiting(Command::Capture), &event, &receiver);
    let (panicked, observed_panic) = mpsc::sync_channel(1);
    let waker = Waker::from(Arc::new(PanickingTask(panicked)));
    assert!(Pin::new(&mut capture)
        .poll(&mut Context::from_waker(&waker))
        .is_pending());
    let mut waiting = fixture.waiting(Command::Inode(987));
    assert!(poll(&mut waiting, &event).is_pending());
    drop(finish(admitted(
        fixture.waiting(Command::ReplyAttempted(publication)),
        &event,
        &receiver,
    )));
    observed_panic.recv_timeout(WAIT).unwrap();
    // The captured outcome was published before its notification panicked.
    // Preserve it; worker loss does not reinterpret it as an unattempted job.
    assert!(matches!(
        finish(capture).result(),
        Ok(Response::Captured(_))
    ));
    let deadline = Instant::now() + WAIT;
    let original = loop {
        match poll(&mut waiting, &event) {
            Poll::Ready(result) => break result,
            Poll::Pending => receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("lost owner did not wake admission"),
        }
    };
    match original {
        Err((OwnerError::Stopped, Command::Inode(987))) => {}
        Ok(pending) => {
            // Admission may race the exit fence, but no departed worker may
            // leave that accepted original job without a terminal outcome.
            let done = finish(pending);
            assert!(
                matches!(done.result(), Err(OwnerError::Unattempted { cause, command }) if matches!(cause.as_ref(), OwnerError::Stopped) && matches!(command.as_ref(), Command::Inode(987)))
            );
        }
        _ => panic!("lost owner changed the original command"),
    }
    let Err(OwnerError::WorkerPanicked(payload)) = fixture.owner.take().unwrap().stop() else {
        panic!("original worker panic was lost");
    };
    let payload = payload
        .into_inner()
        .unwrap_or_else(|error| error.into_inner());
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"caller task panicked during completion notification")
    );
    idle(&fixture.client);
}
