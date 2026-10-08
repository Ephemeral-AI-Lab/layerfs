//! Public pending-job futures: original outcomes, wakeup races and credit custody.
use layerfs_daemon::{
    Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Pending, Response,
};
use layerfs_overlay::{Inode, InodeKind, ProfileConfig, Publication, Route};
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-future-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Event {
    sender: SyncSender<()>,
    wakes: AtomicUsize,
}
impl Wake for Event {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
        // Coalesce runnable notifications; never block the SQL publisher.
        let _ = self.sender.try_send(());
    }
}
fn event() -> (Arc<Event>, Receiver<()>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    (
        Arc::new(Event {
            sender,
            wakes: AtomicUsize::new(0),
        }),
        receiver,
    )
}
fn poll(pending: &mut Pending, event: &Arc<Event>) -> Poll<Result<Completion, OwnerError>> {
    let waker = Waker::from(event.clone());
    Pin::new(pending).poll(&mut Context::from_waker(&waker))
}
fn finish(mut pending: Pending) -> Completion {
    let (event, receiver) = event();
    let deadline = Instant::now() + WAIT;
    loop {
        match poll(&mut pending, &event) {
            Poll::Ready(result) => return result.unwrap(),
            Poll::Pending => receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("pending job did not wake"),
        }
    }
}
fn submit(client: &OwnerClient, route: Option<Route>, command: Command) -> Pending {
    client
        .try_submit(route, command)
        .unwrap_or_else(|(error, command)| panic!("admission of {command:?}: {error:?}"))
}
fn start(temp: &Temp) -> (Owner, OwnerClient, Route) {
    let owner = Owner::start(
        &temp.0.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let done = finish(submit(
        &client,
        None,
        Command::Open {
            incarnation: [71; 32],
            base_root: [72; 32],
        },
    ));
    let route = match done.result() {
        Ok(Response::Opened(route)) => *route,
        other => panic!("open: {other:?}"),
    };
    (owner, client, route)
}
fn hold_capture(client: &OwnerClient, route: Route) -> (Publication, Pending) {
    let done = finish(submit(
        client,
        Some(route),
        Command::Publish {
            inode: Inode {
                serial: 2,
                kind: InodeKind::File,
                mode: 0o644,
                mtime_seconds: 1,
                mtime_nanoseconds: 2,
                nlink: 1,
                size: 0,
                inherited_cutoff: 0,
                born: 0,
                entries: 0,
            },
            name: None,
            cell: None,
        },
    ));
    let ticket = match done.result() {
        Ok(Response::Published(ticket)) => *ticket,
        other => panic!("publish: {other:?}"),
    };
    (ticket, submit(client, Some(route), Command::Capture))
}
fn release(client: &OwnerClient, route: Route, ticket: Publication) {
    let done = finish(submit(client, Some(route), Command::ReplyAttempted(ticket)));
    assert!(matches!(done.result(), Ok(Response::Done)));
}
fn observed(client: &OwnerClient, check: impl Fn(&layerfs_daemon::OwnerWork) -> bool) {
    let deadline = Instant::now() + WAIT;
    loop {
        if check(&client.diagnostics().unwrap()) {
            return;
        }
        assert!(Instant::now() < deadline, "owner observation deadline");
        std::thread::yield_now();
    }
}

#[test]
fn publication_wakes_the_parked_future_and_keeps_credit_until_disposal() {
    let temp = Temp::new();
    let (owner, client, route) = start(&temp);
    let (ticket, mut pending) = hold_capture(&client, route);
    let (event, receiver) = event();
    assert!(poll(&mut pending, &event).is_pending());
    assert!(receiver.try_recv().is_err());
    release(&client, route, ticket);
    receiver.recv_timeout(WAIT).unwrap();
    let done = match poll(&mut pending, &event) {
        Poll::Ready(Ok(done)) => done,
        other => panic!("original capture: {other:?}"),
    };
    assert!(matches!(done.result(), Ok(Response::Captured(_))));
    assert!(matches!(
        poll(&mut pending, &event),
        Poll::Ready(Err(OwnerError::Disconnected))
    ));
    drop(pending);
    observed(&client, |work| work.outstanding == 1);
    assert!(client.diagnostics().unwrap().credited_bytes > 0);
    drop(done);
    observed(&client, |work| {
        work.outstanding == 0 && work.credited_bytes == 0
    });
    assert_eq!(event.wakes.load(Ordering::SeqCst), 1);
    owner.stop().unwrap();
}

#[test]
fn publication_before_registration_is_ready_without_another_event() {
    let temp = Temp::new();
    let (owner, client, route) = start(&temp);
    let before = client.diagnostics().unwrap().completed;
    let mut pending = submit(&client, Some(route), Command::State);
    observed(&client, |work| work.completed != before);
    // The aggregate precedes publication. A later synchronous FIFO State result
    // establishes that the earlier publisher has returned, without taking it.
    drop(finish(submit(&client, Some(route), Command::State)));
    let (event, receiver) = event();
    let done = match poll(&mut pending, &event) {
        Poll::Ready(Ok(done)) => done,
        other => panic!("already published: {other:?}"),
    };
    assert!(matches!(done.result(), Ok(Response::State(_))));
    assert_eq!(event.wakes.load(Ordering::SeqCst), 0);
    assert!(receiver.try_recv().is_err());
    drop(done);
    drop(pending);
    owner.stop().unwrap();
}

#[test]
fn repoll_replaces_the_task_and_publication_releases_the_registration() {
    let temp = Temp::new();
    let (owner, client, route) = start(&temp);
    let (ticket, mut pending) = hold_capture(&client, route);
    let (first, _) = event();
    let first_weak = Arc::downgrade(&first);
    assert!(poll(&mut pending, &first).is_pending());
    drop(first);
    assert!(first_weak.upgrade().is_some());
    let (last, receiver) = event();
    assert!(poll(&mut pending, &last).is_pending());
    assert!(first_weak.upgrade().is_none());
    let last_weak = Arc::downgrade(&last);
    release(&client, route, ticket);
    receiver.recv_timeout(WAIT).unwrap();
    drop(last);
    // A subsequent owner result follows return from the original wake callback.
    drop(finish(submit(&client, Some(route), Command::State)));
    assert!(last_weak.upgrade().is_none());
    let done = finish(pending);
    assert!(matches!(done.result(), Ok(Response::Captured(_))));
    drop(done);
    owner.stop().unwrap();
}

#[test]
fn stop_wakes_a_future_with_its_original_unattempted_command() {
    let temp = Temp::new();
    let (owner, client, route) = start(&temp);
    let (_, mut pending) = hold_capture(&client, route);
    let (event, receiver) = event();
    assert!(poll(&mut pending, &event).is_pending());
    owner.stop().unwrap();
    receiver.recv_timeout(WAIT).unwrap();
    let done = finish(pending);
    match done.result() {
        Err(OwnerError::Unattempted { cause, command }) => {
            assert!(matches!(**cause, OwnerError::Stopped));
            assert!(matches!(**command, Command::Capture));
        }
        other => panic!("stopped original: {other:?}"),
    }
    assert_eq!(client.diagnostics().unwrap().outstanding, 1);
    drop(done);
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
}

#[test]
fn concurrent_publication_and_registration_never_require_polling_for_progress() {
    let temp = Temp::new();
    let (owner, client, route) = start(&temp);
    // Distinct original jobs race the independent SQL publisher. Each pending
    // return is followed by an actual event, never repeated speculative polls.
    for _ in 0..64 {
        let done = finish(submit(&client, Some(route), Command::State));
        assert!(matches!(done.result(), Ok(Response::State(_))));
    }
    owner.stop().unwrap();
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
}
