//! The terminal fence at dispatcher scope, through the public API only: what
//! `stop_service` does to parked, running and receive-waiting units, and that
//! the normal `stop_admission` does none of it. Every wait is bounded.
mod support;
use layerfs_fuse::{
    ports::Fence, DispatchError, MountQueue, RequestDisposition, HANDOFFS, RECEIVE_SLOTS,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    task::{Context, Poll},
    time::Duration,
};
use support::{finish, until, Event, EventFuture, Fixture};

const WAIT: Duration = Duration::from_secs(3);

/// A request waiting before an attempt: it ends itself once its fence is
/// stopped, and otherwise waits for its event like any parked continuation.
struct Unattempted {
    fence: Fence,
    event: EventFuture,
    polls: Arc<AtomicUsize>,
}
impl Future for Unattempted {
    type Output = RequestDisposition;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        if self.fence.stopped() {
            return Poll::Ready(RequestDisposition::Complete);
        }
        Pin::new(&mut self.event).poll(cx)
    }
}
/// A request waiting on a job it already submitted: only its event ends it.
struct Submitted {
    event: EventFuture,
    polls: Arc<AtomicUsize>,
}
impl Future for Submitted {
    type Output = RequestDisposition;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Pin::new(&mut self.event).poll(cx)
    }
}
fn handoff(queue: &MountQueue, future: impl Future<Output = RequestDisposition> + Send + 'static) {
    queue
        .receive()
        .unwrap()
        .admit(0)
        .unwrap()
        .handoff(Box::pin(future))
        .unwrap();
}

#[test]
fn stop_service_gives_every_parked_request_one_turn_and_stops_the_fence() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let fence = queue.fence();
    let (waiting, submitted) = (Arc::new(Event::default()), Arc::new(Event::default()));
    let (first, second) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    handoff(
        &queue,
        Unattempted {
            fence: fence.clone(),
            event: waiting.future(),
            polls: first.clone(),
        },
    );
    handoff(
        &queue,
        Submitted {
            event: submitted.future(),
            polls: second.clone(),
        },
    );
    until(|| queue.work().unwrap().parked == 2);
    assert!(!fence.stopped());
    assert!(!queue.work().unwrap().terminal);
    assert_eq!(
        (first.load(Ordering::SeqCst), second.load(Ordering::SeqCst)),
        (1, 1)
    );

    queue.stop_service().unwrap();
    // The unattempted request observed the stop in its extra turn and ended;
    // the submitted one took the same single turn and parked on its job again.
    until(|| {
        let work = queue.work().unwrap();
        work.admitted == 1 && work.parked == 1
    });
    let work = queue.work().unwrap();
    assert!(fence.stopped() && queue.fence().stopped() && work.terminal);
    assert_eq!((work.completed, work.retained, work.wakes), (1, 0, 0));
    assert_eq!(
        (first.load(Ordering::SeqCst), second.load(Ordering::SeqCst)),
        (2, 2)
    );
    // The dispatcher itself replies to nothing and counts no terminal reply.
    assert_eq!(fence.terminal_replies(), 0);
    match queue.receive().unwrap().admit(0) {
        Err(refused) => assert_eq!(refused.reason, DispatchError::Stopped),
        Ok(_) => panic!("a stopped mount admitted a request"),
    }
    // The wakeup the ended request once registered is stale, not a second run.
    waiting.waker().unwrap().wake();
    // The submitted job's own completion still decides that request.
    submitted.fire();
    until(|| queue.work().unwrap().admitted == 0);
    assert_eq!(queue.work().unwrap().completed, 2);
    assert_eq!(
        (first.load(Ordering::SeqCst), second.load(Ordering::SeqCst)),
        (2, 3)
    );
    // A second stop finds nothing parked and changes nothing.
    queue.stop_service().unwrap();
    queue.finish().unwrap();
    // A released lane is terminal: its fence reads stopped, with no count.
    assert_eq!(queue.stop_service(), Err(DispatchError::Stale));
    assert!(queue.fence().stopped());
    assert_eq!(queue.fence().terminal_replies(), 0);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn stop_admission_stops_no_fence_and_turns_no_parked_request() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let fence = queue.fence();
    let event = Arc::new(Event::default());
    let polls = Arc::new(AtomicUsize::new(0));
    handoff(
        &queue,
        Unattempted {
            fence: fence.clone(),
            event: event.future(),
            polls: polls.clone(),
        },
    );
    until(|| queue.work().unwrap().parked == 1);
    queue.stop_admission().unwrap();
    let work = queue.work().unwrap();
    assert!(work.terminal && !fence.stopped());
    assert_eq!((work.parked, work.queued, work.running), (1, 0, 0));
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    // The normal path ends an admitted request only by its own completion.
    event.fire();
    until(|| queue.work().unwrap().admitted == 0);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert!(!fence.stopped());
    queue.finish().unwrap();
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn stop_service_wakes_every_receive_capacity_waiter() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let events: Vec<_> = (0..HANDOFFS).map(|_| Arc::new(Event::default())).collect();
    for event in &events {
        handoff(&queue, event.future());
    }
    until(|| queue.work().unwrap().parked == HANDOFFS);
    // Every receive unit of the mount: one per receive loop.
    let received: Vec<_> = (0..RECEIVE_SLOTS)
        .map(|_| queue.receive().unwrap())
        .collect();
    let (send, recv) = mpsc::channel();
    // Stopping in Drop lets every blocked admission exit even if an assertion
    // below panics first; the threads have no other blocking work.
    struct Stop(MountQueue);
    impl Drop for Stop {
        fn drop(&mut self) {
            let _ = self.0.stop_service();
        }
    }
    let stop = Stop(queue.clone());
    let threads: Vec<_> = received
        .into_iter()
        .map(|received| {
            let send = send.clone();
            std::thread::spawn(move || {
                let failure = match received.admit(1) {
                    Err(failure) => failure,
                    Ok(_) => panic!("full handoff admitted"),
                };
                let _ = send.send(failure);
            })
        })
        .collect();
    let work = queue.work().unwrap();
    assert_eq!((work.received, work.admitted), (RECEIVE_SLOTS, HANDOFFS));
    drop(stop);
    let failures: Vec<_> = (0..RECEIVE_SLOTS)
        .map(|_| recv.recv_timeout(WAIT).unwrap())
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert!(failures
        .iter()
        .all(|failure| failure.reason == DispatchError::Stopped));
    // Each waiter still owns its receive unit until its one reply attempt.
    assert_eq!(queue.work().unwrap().received, RECEIVE_SLOTS);
    drop(failures);
    assert_eq!(queue.work().unwrap().received, 0);
    // Sixteen parked requests each took the stop's one turn and parked again.
    until(|| queue.work().unwrap().parked == HANDOFFS);
    assert_eq!(queue.work().unwrap().steps, 2 * HANDOFFS as u64);
    for event in events {
        event.fire();
    }
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn a_request_running_through_the_stop_gets_its_turn_afterwards() {
    /// Its first step is still executing when the mount stops and registers
    /// no wakeup at all: only the stop's own notification can run it again.
    struct Running {
        fence: Fence,
        entered: mpsc::Sender<()>,
        resume: mpsc::Receiver<()>,
        polls: Arc<AtomicUsize>,
    }
    impl Future for Running {
        type Output = RequestDisposition;
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            if self.polls.fetch_add(1, Ordering::SeqCst) == 0 {
                let _ = self.entered.send(());
                // Bounded: an abandoned test leaves this request parked.
                let _ = self.resume.recv_timeout(WAIT);
                return Poll::Pending;
            }
            if self.fence.stopped() {
                Poll::Ready(RequestDisposition::Complete)
            } else {
                Poll::Pending
            }
        }
    }
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let (entered, started) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let polls = Arc::new(AtomicUsize::new(0));
    // The first step runs on the thread that hands the request off, as it
    // does on a receive loop; the test observes it from outside.
    let receiver = {
        let queue = queue.clone();
        let running = Running {
            fence: queue.fence(),
            entered,
            resume: resumed,
            polls: polls.clone(),
        };
        std::thread::spawn(move || handoff(&queue, running))
    };
    started.recv_timeout(WAIT).unwrap();
    assert_eq!(queue.work().unwrap().running, 1);
    queue.stop_service().unwrap();
    assert_eq!(queue.work().unwrap().running, 1);
    resume.send(()).unwrap();
    receiver.join().unwrap();
    until(|| queue.work().unwrap().admitted == 0);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(queue.work().unwrap().completed, 1);
    queue.finish().unwrap();
    assert!(pool.stop().unwrap().clean());
}
