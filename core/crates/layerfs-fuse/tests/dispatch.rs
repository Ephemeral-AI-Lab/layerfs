//! External public-API checks of bounded event-driven request custody.
mod support;
use layerfs_fuse::{
    DispatchError, FailureView, LeaveReceiver, MountQueue, NextTurn, RequestDisposition,
    RequestFuture, HANDOFFS, MAX_INPUT_BYTES, RECEIVE_SLOTS,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    task::{Context, Poll},
    thread::ThreadId,
    time::{Duration, Instant},
};
use support::{finish, until, Event, Fixture};

#[test]
fn notification_registration_does_not_keep_a_disposed_request_alive() {
    struct Parked {
        event: support::EventFuture,
        drops: Arc<AtomicUsize>,
    }
    impl Future for Parked {
        type Output = RequestDisposition;
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            Pin::new(&mut self.event).poll(cx)
        }
    }
    impl Drop for Parked {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    let fixture = Fixture::new();
    let pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let event = Arc::new(Event::default());
    let drops = Arc::new(AtomicUsize::new(0));
    queue
        .receive()
        .unwrap()
        .admit(0)
        .unwrap()
        .handoff(Box::pin(Parked {
            event: event.future(),
            drops: drops.clone(),
        }))
        .unwrap();
    until(|| queue.work().unwrap().parked == 1);
    let old = event.waker().unwrap();
    drop(pool);
    assert_eq!(
        drops.load(Ordering::SeqCst),
        0,
        "queue retains original after worker joins"
    );
    drop(queue);
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "notification may not form a strong ownership cycle"
    );
    old.wake();
    event.fire();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn fixed_workers_share_mounts_and_survive_zero_mounts() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(2);
    assert_eq!(pool.work().configured_workers, 4);
    assert_eq!(pool.work().entered_workers, 4);
    assert_eq!(pool.work().live_workers, 4);
    let first = fixture.mount(1);
    let a = pool.register(first).unwrap();
    assert!(matches!(
        pool.register(first),
        Err(DispatchError::DuplicateMount)
    ));
    let b = pool.register(fixture.mount(2)).unwrap();
    assert!(matches!(
        pool.register(fixture.mount(3)),
        Err(DispatchError::Capacity)
    ));
    assert!(matches!(pool.stop(), Err(DispatchError::Busy)));
    assert!(!pool.work().stopping);
    for queue in [&a, &b] {
        queue
            .receive()
            .unwrap()
            .admit(0)
            .unwrap()
            .handoff(Box::pin(async {
                for _ in 0..16 {
                    NextTurn::default().await;
                }
                RequestDisposition::Complete
            }))
            .unwrap();
        finish(queue);
    }
    assert_eq!(pool.work().mounts, 0);
    assert_eq!(pool.work().live_workers, 4);
    let again = pool.register(first).unwrap();
    assert!(matches!(a.receive(), Err(DispatchError::Stale)));
    assert_eq!(a.stop_admission(), Err(DispatchError::Stale));
    finish(&again);
    let stopped = pool.stop().unwrap();
    assert!(stopped.clean());
    assert_eq!(stopped.joins.len(), 4);
    assert!(pool.stop().unwrap().clean());
    assert_eq!(pool.work().live_workers, 0);
}

#[test]
fn every_receive_unit_waiter_wakes_on_terminal_without_releasing_original_credit() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let events: Vec<_> = (0..HANDOFFS).map(|_| Arc::new(Event::default())).collect();
    for event in &events {
        queue
            .receive()
            .unwrap()
            .admit(MAX_INPUT_BYTES)
            .unwrap()
            .handoff(Box::pin(event.future()))
            .unwrap();
    }
    until(|| queue.work().unwrap().parked == HANDOFFS);
    // Every receive unit of the mount: one per receive loop.
    let received: Vec<_> = (0..RECEIVE_SLOTS)
        .map(|_| queue.receive().unwrap())
        .collect();
    assert!(matches!(queue.receive(), Err(DispatchError::Capacity)));
    assert_eq!(
        queue.work().unwrap().owned_input_bytes,
        HANDOFFS * MAX_INPUT_BYTES
    );
    let (send, recv) = mpsc::channel();
    // A terminal fence in Drop makes every blocking admission exit even if a
    // preceding assertion panics. Their test threads have no other blocking work.
    struct Fence(layerfs_fuse::MountQueue);
    impl Drop for Fence {
        fn drop(&mut self) {
            let _ = self.0.stop_admission();
        }
    }
    let fence = Fence(queue.clone());
    let threads: Vec<_> = received
        .into_iter()
        .map(|received| {
            let send = send.clone();
            std::thread::spawn(move || {
                let failure = match received.admit(1) {
                    Err(failure) => failure,
                    Ok(_) => panic!("full handoff admitted"),
                };
                send.send(failure).unwrap();
            })
        })
        .collect();
    assert_eq!(queue.finish(), Err(DispatchError::Busy));
    drop(fence);
    let failures: Vec<_> = (0..RECEIVE_SLOTS)
        .map(|_| recv.recv_timeout(Duration::from_secs(3)).unwrap())
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert!(failures
        .iter()
        .all(|error| error.reason == DispatchError::Stopped));
    assert_eq!(queue.work().unwrap().received, RECEIVE_SLOTS);
    assert_eq!(queue.work().unwrap().admitted, HANDOFFS);
    drop(failures);
    assert_eq!(queue.work().unwrap().received, 0);
    for event in events {
        event.fire();
    }
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn completion_event_resumes_without_another_request_and_old_wakers_are_stale() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let token = fixture.mount(1);
    let queue = pool.register(token).unwrap();
    let event = Arc::new(Event::default());
    queue
        .receive()
        .unwrap()
        .admit(19)
        .unwrap()
        .handoff(Box::pin(event.future()))
        .unwrap();
    until(|| queue.work().unwrap().parked == 1);
    let old_waker = event.waker().unwrap();
    event.fire();
    until(|| queue.work().unwrap().completed == 1);
    assert_eq!(queue.work().unwrap().owned_input_bytes, 0);
    let next = Arc::new(Event::default());
    queue
        .receive()
        .unwrap()
        .admit(23)
        .unwrap()
        .handoff(Box::pin(next.future()))
        .unwrap();
    until(|| queue.work().unwrap().parked == 1);
    let before = queue.work().unwrap();
    old_waker.wake_by_ref();
    assert_eq!(queue.work().unwrap(), before);
    next.fire();
    finish(&queue);
    let new_queue = pool.register(token).unwrap();
    old_waker.wake_by_ref();
    assert_eq!(new_queue.work().unwrap().wakes, 0);
    finish(&new_queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn wake_during_poll_is_coalesced_and_never_lost() {
    struct SelfWake(usize);
    impl Future for SelfWake {
        type Output = RequestDisposition;
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.0 == 0 {
                return Poll::Ready(RequestDisposition::Complete);
            }
            self.0 -= 1;
            for _ in 0..32 {
                cx.waker().wake_by_ref();
            }
            Poll::Pending
        }
    }
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    queue
        .receive()
        .unwrap()
        .admit(0)
        .unwrap()
        .handoff(Box::pin(SelfWake(64)))
        .unwrap();
    until(|| queue.work().unwrap().completed == 1);
    let work = queue.work().unwrap();
    assert_eq!(work.steps, 65);
    assert_eq!(work.wakes, 64 * 32);
    assert_eq!(work.admitted, 0);
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn rejected_copy_capacity_keeps_receive_ownership_and_permit_cancel_is_exact() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let failure = match queue.receive().unwrap().admit(usize::MAX) {
        Err(error) => error,
        Ok(_) => panic!("unbounded input admitted"),
    };
    assert_eq!(failure.reason, DispatchError::Capacity);
    assert_eq!(queue.work().unwrap().received, 1);
    assert_eq!(queue.work().unwrap().admitted, 0);
    drop(failure);
    let permit = queue.receive().unwrap().admit(31).unwrap();
    assert_eq!(queue.work().unwrap().owned_input_bytes, 31);
    drop(permit);
    assert_eq!(queue.work().unwrap().admitted, 0);
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

struct RetainedFuture {
    drops: Arc<AtomicUsize>,
    panic: bool,
}
impl Future for RetainedFuture {
    type Output = RequestDisposition;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.panic {
            std::panic::panic_any(8128_u64);
        }
        Poll::Ready(RequestDisposition::Retained(Box::new(
            std::io::Error::other("original"),
        )))
    }
}
impl Drop for RetainedFuture {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn panic_and_failure_retain_original_future_and_do_not_stop_other_mounts() {
    for panic in [false, true] {
        let fixture = Fixture::new();
        let mut pool = fixture.pool(2);
        let queue = pool.register(fixture.mount(1)).unwrap();
        let other = pool.register(fixture.mount(2)).unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        queue
            .receive()
            .unwrap()
            .admit(29)
            .unwrap()
            .handoff(Box::pin(RetainedFuture {
                drops: drops.clone(),
                panic,
            }))
            .unwrap();
        until(|| queue.work().unwrap().retained == 1);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert!(queue.work().unwrap().terminal);
        assert_eq!(queue.work().unwrap().owned_input_bytes, 29);
        queue
            .inspect_retained(0, |failure| match failure {
                FailureView::Panic(payload) => {
                    assert!(panic);
                    assert_eq!(payload.downcast_ref::<u64>(), Some(&8128));
                }
                FailureView::Request(error) => {
                    assert!(!panic);
                    assert_eq!(error.to_string(), "original");
                }
            })
            .unwrap();
        assert_eq!(queue.finish(), Err(DispatchError::Busy));
        assert!(matches!(pool.stop(), Err(DispatchError::Busy)));
        other
            .receive()
            .unwrap()
            .admit(0)
            .unwrap()
            .handoff(Box::pin(async { RequestDisposition::Complete }))
            .unwrap();
        finish(&other);
        assert_eq!(pool.work().live_workers, 4);
        drop(pool);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert_eq!(queue.work().unwrap().retained, 1);
        drop(queue);
        drop(other);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn handoff_after_pool_shutdown_retains_original_and_returns_terminal_error() {
    let fixture = Fixture::new();
    let pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let permit = queue.receive().unwrap().admit(11).unwrap();
    drop(pool);
    let drops = Arc::new(AtomicUsize::new(0));
    assert_eq!(
        permit.handoff(Box::pin(RetainedFuture {
            drops: drops.clone(),
            panic: false
        })),
        Err(DispatchError::Stopped)
    );
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(queue.work().unwrap().retained, 1);
    assert_eq!(queue.finish(), Err(DispatchError::Busy));
    queue
        .inspect_retained(0, |failure| match failure {
            FailureView::Request(error) => assert_eq!(
                error.downcast_ref::<DispatchError>(),
                Some(&DispatchError::Stopped)
            ),
            FailureView::Panic(_) => panic!("unpolled future cannot panic"),
        })
        .unwrap();
    drop(queue);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn quiescence_wait_is_event_driven_and_expiry_disposes_nothing() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let event = Arc::new(Event::default());
    queue
        .receive()
        .unwrap()
        .admit(0)
        .unwrap()
        .handoff(Box::pin(event.future()))
        .unwrap();
    until(|| queue.work().unwrap().parked == 1);
    let held = queue.receive().unwrap();
    let expired = queue
        .wait_quiescent(Instant::now() + Duration::from_millis(20))
        .unwrap();
    assert_eq!(
        (expired.received, expired.admitted, expired.parked),
        (1, 1, 1)
    );
    assert_eq!(queue.finish(), Err(DispatchError::Busy));
    // The waiter owns its own deadline: it returns without this thread's help.
    let waiter = {
        let queue = queue.clone();
        std::thread::spawn(move || queue.wait_quiescent(Instant::now() + Duration::from_secs(3)))
    };
    drop(held);
    event.fire();
    let drained = waiter.join().unwrap().unwrap();
    assert_eq!((drained.received, drained.admitted), (0, 0));
    assert_eq!(drained.completed, 1);
    queue.stop_admission().unwrap();
    queue.finish().unwrap();
    assert_eq!(
        queue.wait_quiescent(Instant::now()),
        Err(DispatchError::Stale)
    );
    assert!(pool.stop().unwrap().clean());
}

/// Where each step of one request ran, and how often the request ended.
#[derive(Clone, Default)]
struct Trace {
    steps: Arc<Mutex<Vec<(ThreadId, bool)>>>,
    drops: Arc<AtomicUsize>,
}
impl Trace {
    /// The thread of every step so far, in order.
    fn threads(&self) -> Vec<ThreadId> {
        self.steps
            .lock()
            .unwrap()
            .iter()
            .map(|step| step.0)
            .collect()
    }
    /// Whether each step so far ran on a pool worker.
    fn workers(&self) -> Vec<bool> {
        self.steps
            .lock()
            .unwrap()
            .iter()
            .map(|step| step.1)
            .collect()
    }
    fn dropped(&self) -> usize {
        self.drops.load(Ordering::SeqCst)
    }
}
/// A request that records the thread of each of its steps.
struct Traced<F> {
    trace: Trace,
    inner: Pin<Box<F>>,
}
impl<F: Future<Output = RequestDisposition>> Future for Traced<F> {
    type Output = RequestDisposition;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let thread = std::thread::current();
        let worker = thread
            .name()
            .is_some_and(|name| name.starts_with("layerfs-fuse-"));
        self.trace.steps.lock().unwrap().push((thread.id(), worker));
        self.inner.as_mut().poll(cx)
    }
}
impl<F> Drop for Traced<F> {
    fn drop(&mut self) {
        self.trace.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn traced(
    trace: &Trace,
    request: impl Future<Output = RequestDisposition> + Send + 'static,
) -> RequestFuture {
    Box::pin(Traced {
        trace: trace.clone(),
        inner: Box::pin(request),
    })
}
/// Receives, admits and hands off one request: its first step runs here.
fn hand(queue: &MountQueue, request: RequestFuture) {
    queue
        .receive()
        .unwrap()
        .admit(0)
        .unwrap()
        .handoff(request)
        .unwrap();
}
/// A request parked on its own event after its first step on this thread.
fn parked(queue: &MountQueue) -> (Arc<Event>, Trace) {
    let event = Arc::new(Event::default());
    let trace = Trace::default();
    let waiting = event.future();
    hand(
        queue,
        traced(&trace, async move {
            waiting.await;
            RequestDisposition::Complete
        }),
    );
    (event, trace)
}

#[test]
fn a_step_made_runnable_inside_a_first_step_is_run_by_the_receiving_thread() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let me = std::thread::current().id();
    let (event, waiter) = parked(&queue);
    let parked_work = queue.work().unwrap();
    assert_eq!(
        (parked_work.parked, parked_work.steps, parked_work.wakes),
        (1, 1, 0)
    );

    // The waker of the parked request is called from inside another
    // request's first step, on the thread that received it.
    let waking = Trace::default();
    hand(
        &queue,
        traced(&waking, async move {
            event.fire();
            RequestDisposition::Complete
        }),
    );
    // Nothing is waited for: when the handoff returns, the receiving thread
    // has run its own step and the one it made runnable. No step was queued
    // for the pool and no pool worker ran one.
    let work = queue.work().unwrap();
    assert_eq!(
        (
            work.completed,
            work.admitted,
            work.queued,
            work.running,
            work.parked
        ),
        (2, 0, 0, 0, 0)
    );
    assert_eq!((work.steps, work.wakes), (3, 1));
    assert_eq!(waking.threads(), [me]);
    assert_eq!(waiter.threads(), [me, me]);
    assert_eq!(waiter.workers(), [false, false]);
    assert_eq!((waiter.dropped(), waking.dropped()), (1, 1));
    assert_eq!((pool.work().queued, pool.work().running), (0, 0));

    // The thread keeps a step only while it runs one. The same thread,
    // running no step, wakes a parked request for the pool as before.
    let (event, waiter) = parked(&queue);
    event.fire();
    until(|| queue.work().unwrap().completed == 3);
    assert_eq!(waiter.workers(), [false, true]);
    assert_ne!(waiter.threads()[1], me);
    let work = queue.work().unwrap();
    assert_eq!((work.steps, work.wakes), (5, 2));
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn a_wake_from_a_thread_that_runs_no_request_step_goes_to_a_pool_worker() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let me = std::thread::current().id();
    let (event, waiter) = parked(&queue);
    // A plain thread, as the daemon's owner thread is to the dispatcher. It
    // only fires the event and ends.
    let plain = std::thread::spawn(move || {
        event.fire();
        std::thread::current().id()
    })
    .join()
    .unwrap();
    until(|| queue.work().unwrap().completed == 1);
    let threads = waiter.threads();
    assert_eq!(threads.len(), 2);
    assert_eq!(waiter.workers(), [false, true]);
    assert!(threads[1] != plain && threads[1] != me);
    let work = queue.work().unwrap();
    assert_eq!((work.steps, work.wakes, work.admitted), (2, 1, 0));
    assert_eq!(waiter.dropped(), 1);
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn the_receiving_thread_keeps_one_step_and_its_kept_step_keeps_none() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let me = std::thread::current().id();

    // One first step makes two parked requests runnable: the first is this
    // thread's, the second is queued for the pool.
    let (first_event, first) = parked(&queue);
    let (second_event, second) = parked(&queue);
    hand(
        &queue,
        Box::pin(async move {
            first_event.fire();
            second_event.fire();
            RequestDisposition::Complete
        }),
    );
    assert_eq!(first.threads(), [me, me]);
    until(|| queue.work().unwrap().completed == 3);
    assert_eq!(second.workers(), [false, true]);
    let work = queue.work().unwrap();
    assert_eq!((work.steps, work.wakes, work.admitted), (5, 2, 0));

    // The kept step runs here as a receiving step and keeps nothing: what
    // it makes runnable is queued for the pool.
    let (last_event, last) = parked(&queue);
    let middle_event = Arc::new(Event::default());
    let middle = Trace::default();
    let waiting = middle_event.future();
    hand(
        &queue,
        traced(&middle, async move {
            waiting.await;
            last_event.fire();
            RequestDisposition::Complete
        }),
    );
    hand(
        &queue,
        Box::pin(async move {
            middle_event.fire();
            RequestDisposition::Complete
        }),
    );
    assert_eq!(middle.threads(), [me, me]);
    until(|| queue.work().unwrap().completed == 6);
    assert_eq!(last.workers(), [false, true]);
    let work = queue.work().unwrap();
    assert_eq!((work.steps, work.wakes, work.admitted), (10, 4, 0));
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn a_kept_step_on_the_receiving_thread_still_leaves_it_before_provider_work() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let me = std::thread::current().id();
    let event = Arc::new(Event::default());
    let waiter = Trace::default();
    let waiting = event.future();
    hand(
        &queue,
        traced(&waiter, async move {
            waiting.await;
            // What follows stands for provider I/O.
            LeaveReceiver::default().await;
            RequestDisposition::Complete
        }),
    );
    hand(
        &queue,
        Box::pin(async move {
            event.fire();
            RequestDisposition::Complete
        }),
    );
    // The kept step ran here and yielded at the boundary; the rest is a
    // pool worker's.
    assert_eq!(waiter.threads()[..2], [me, me]);
    until(|| queue.work().unwrap().completed == 2);
    assert_eq!(waiter.workers(), [false, false, true]);
    let work = queue.work().unwrap();
    // The event's wake and the boundary's own.
    assert_eq!((work.steps, work.wakes, work.admitted), (4, 2, 0));
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn a_worker_runs_the_steps_it_makes_runnable_as_its_own_next_steps() {
    const CHAIN: usize = HANDOFFS - 1;
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let me = std::thread::current().id();
    // Every request of the chain waits for its event and then fires the
    // next one's from inside its own step.
    let events: Vec<_> = (0..=CHAIN).map(|_| Arc::new(Event::default())).collect();
    let traces: Vec<_> = (0..CHAIN).map(|_| Trace::default()).collect();
    for (index, trace) in traces.iter().enumerate() {
        let waiting = events[index].future();
        let next = events[index + 1].clone();
        hand(
            &queue,
            traced(trace, async move {
                waiting.await;
                next.fire();
                RequestDisposition::Complete
            }),
        );
    }
    assert_eq!(queue.work().unwrap().parked, CHAIN);
    // Fired by a thread that runs no step: the first link is a worker's.
    events[0].fire();
    until(|| queue.work().unwrap().completed == CHAIN as u64);
    let worker = traces[0].threads()[1];
    assert_ne!(worker, me);
    for (index, trace) in traces.iter().enumerate() {
        // Each link after the first was kept by the worker that woke it.
        assert_eq!(trace.threads(), [me, worker], "link {index}");
        assert_eq!(trace.workers(), [false, true], "link {index}");
        assert_eq!(trace.dropped(), 1, "link {index}");
    }
    let work = queue.work().unwrap();
    // The last link fires an event nobody waits on.
    assert_eq!(
        (work.steps, work.wakes, work.admitted),
        (2 * CHAIN as u64, CHAIN as u64, 0)
    );
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn a_worker_gives_a_kept_step_to_the_pool_before_provider_work() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let event = Arc::new(Event::default());
    let waiter = Trace::default();
    let waiting = event.future();
    let (finished, seen) = mpsc::channel();
    hand(
        &queue,
        traced(&waiter, async move {
            waiting.await;
            let _ = finished.send(());
            RequestDisposition::Complete
        }),
    );
    let waking = Trace::default();
    let observed = Arc::new(Mutex::new(None));
    let report = observed.clone();
    hand(
        &queue,
        traced(&waking, async move {
            // Off the receiving thread: the next step is a worker's.
            LeaveReceiver::default().await;
            event.fire();
            // Before what stands for provider I/O, the worker gives up the
            // step it kept. Another worker finishes it while this step is
            // still running: a bounded wait inside the step observes that.
            LeaveReceiver::default().await;
            *report.lock().unwrap() = Some(seen.recv_timeout(Duration::from_secs(3)).is_ok());
            RequestDisposition::Complete
        }),
    );
    until(|| queue.work().unwrap().completed == 2);
    assert_eq!(
        *observed.lock().unwrap(),
        Some(true),
        "the kept step waited for the step that woke it"
    );
    assert_eq!(waking.workers(), [false, true]);
    assert_eq!(waiter.workers(), [false, true]);
    assert_ne!(waiter.threads()[1], waking.threads()[1]);
    let work = queue.work().unwrap();
    assert_eq!((work.steps, work.wakes, work.admitted), (4, 2, 0));
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn a_step_kept_while_the_dispatcher_stops_is_retained_and_not_run_again() {
    // The step is kept by the receiving thread, then by a pool worker.
    for on_worker in [false, true] {
        let fixture = Fixture::new();
        let pool = fixture.pool(1);
        let queue = pool.register(fixture.mount(1)).unwrap();
        let me = std::thread::current().id();
        let (event, waiter) = parked(&queue);

        // The pool is stopped by its own thread when the waking step says
        // so. That thread ends by itself if it is never told.
        let (go, told) = mpsc::channel::<()>();
        let (stopped, joined) = mpsc::channel::<()>();
        let stopper = std::thread::spawn(move || {
            let _ = told.recv_timeout(Duration::from_secs(5));
            drop(pool);
            let _ = stopped.send(());
        });
        let waking = Trace::default();
        let observer = queue.clone();
        let reached = Arc::new(Mutex::new(false));
        let report = reached.clone();
        hand(
            &queue,
            traced(&waking, async move {
                if on_worker {
                    LeaveReceiver::default().await;
                }
                // The parked request becomes this thread's kept step.
                event.fire();
                // The stop arrives while this step still runs: bounded.
                let _ = go.send(());
                let deadline = Instant::now() + Duration::from_secs(3);
                while Instant::now() < deadline {
                    if observer.work().is_ok_and(|work| work.terminal) {
                        *report.lock().unwrap() = true;
                        break;
                    }
                    std::thread::yield_now();
                }
                RequestDisposition::Complete
            }),
        );
        joined
            .recv_timeout(Duration::from_secs(10))
            .expect("the pool stopped");
        stopper.join().unwrap();
        assert!(*reached.lock().unwrap(), "on_worker={on_worker}");
        assert_eq!(waking.workers().last(), Some(&on_worker));

        // The waking request ended; the kept one is retained with the
        // stop as its failure. It was not run again and is not disposed.
        let work = queue.work().unwrap();
        assert_eq!(
            (
                work.completed,
                work.admitted,
                work.retained,
                work.queued,
                work.parked,
                work.running
            ),
            (1, 1, 1, 0, 0, 0),
            "on_worker={on_worker}"
        );
        assert_eq!(work.wakes, if on_worker { 2 } else { 1 });
        assert_eq!(waiter.threads(), [me], "on_worker={on_worker}");
        assert_eq!(waiter.dropped(), 0);
        assert_eq!(waking.dropped(), 1);
        queue
            .inspect_retained(0, |failure| match failure {
                FailureView::Request(error) => assert_eq!(
                    error.downcast_ref::<DispatchError>(),
                    Some(&DispatchError::Stopped)
                ),
                FailureView::Panic(_) => panic!("the kept step was never run"),
            })
            .unwrap();
        assert_eq!(queue.finish(), Err(DispatchError::Busy));
        drop(queue);
        assert_eq!(waiter.threads(), [me]);
        assert_eq!(waiter.dropped(), 1);
    }
}

#[test]
fn a_terminal_fence_raised_during_the_waking_step_runs_the_kept_step_exactly_once() {
    let fixture = Fixture::new();
    let pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let me = std::thread::current().id();
    let (event, waiter) = parked(&queue);
    let (_, bystander) = parked(&queue);
    let fenced = queue.clone();
    hand(
        &queue,
        Box::pin(async move {
            event.fire();
            // Forced teardown gives every parked request one more turn. The
            // kept one is already this thread's: it gets one turn, not two.
            fenced.stop_service().unwrap();
            RequestDisposition::Complete
        }),
    );
    assert_eq!(waiter.threads(), [me, me]);
    assert_eq!(waiter.dropped(), 1);
    // The other parked request got its one more turn from the pool and
    // parked again on its event.
    until(|| bystander.threads().len() == 2 && queue.work().unwrap().parked == 1);
    assert_eq!(bystander.workers(), [false, true]);
    let work = queue.work().unwrap();
    assert_eq!(
        (work.completed, work.admitted, work.parked, work.queued),
        (2, 1, 1, 0)
    );
    // Waiter 2, bystander 2, waking 1.
    assert_eq!((work.steps, work.wakes), (5, 1));
    assert!(queue.fence().stopped());
    drop(pool);
    assert_eq!(bystander.dropped(), 0);
    drop(queue);
    assert_eq!(bystander.dropped(), 1);
}

#[test]
fn repeated_collisions_complete_every_request_once_with_exact_step_counts() {
    const ROUNDS: u64 = 200;
    let fixture = Fixture::new();
    let mut pool = fixture.pool(1);
    let queue = pool.register(fixture.mount(1)).unwrap();
    let me = std::thread::current().id();
    let ended = Arc::new(AtomicUsize::new(0));
    for round in 1..=ROUNDS {
        // One request woken inside a first step on this thread, one woken
        // inside a later step on a worker, by the same third request.
        let (here_event, here) = parked(&queue);
        let (there_event, there) = parked(&queue);
        let waking = Trace::default();
        hand(
            &queue,
            traced(&waking, async move {
                here_event.fire();
                LeaveReceiver::default().await;
                there_event.fire();
                RequestDisposition::Complete
            }),
        );
        assert_eq!(here.threads(), [me, me], "round {round}");
        until(|| queue.work().unwrap().completed == 3 * round);
        // The request woken on the worker was that worker's next step.
        assert_eq!(there.threads(), [me, waking.threads()[1]], "round {round}");
        assert_eq!(waking.workers(), [false, true], "round {round}");
        for trace in [&here, &there, &waking] {
            ended.fetch_add(trace.dropped(), Ordering::SeqCst);
        }
    }
    let work = queue.work().unwrap();
    // Per round: two steps of each request; the two events' wakes and the
    // boundary's own.
    assert_eq!(
        (work.completed, work.steps, work.wakes, work.admitted),
        (3 * ROUNDS, 6 * ROUNDS, 3 * ROUNDS, 0)
    );
    assert_eq!(ended.load(Ordering::SeqCst) as u64, 3 * ROUNDS);
    finish(&queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn a_step_of_another_mount_of_the_pool_is_kept_and_counted_in_its_own_lane() {
    let fixture = Fixture::new();
    let mut pool = fixture.pool(2);
    let waiting_queue = pool.register(fixture.mount(1)).unwrap();
    let waking_queue = pool.register(fixture.mount(2)).unwrap();
    let me = std::thread::current().id();
    let (event, waiter) = parked(&waiting_queue);
    hand(
        &waking_queue,
        Box::pin(async move {
            event.fire();
            RequestDisposition::Complete
        }),
    );
    // Both are done when the handoff returns, each in its own mount's
    // counts: the wake and the kept step belong to the woken request.
    assert_eq!(waiter.threads(), [me, me]);
    let waiting = waiting_queue.work().unwrap();
    assert_eq!(
        (
            waiting.completed,
            waiting.steps,
            waiting.wakes,
            waiting.admitted
        ),
        (1, 2, 1, 0)
    );
    let waking = waking_queue.work().unwrap();
    assert_eq!(
        (
            waking.completed,
            waking.steps,
            waking.wakes,
            waking.admitted
        ),
        (1, 1, 0, 0)
    );
    finish(&waiting_queue);
    finish(&waking_queue);
    assert!(pool.stop().unwrap().clean());
}

#[test]
fn a_step_of_another_pool_is_never_kept_and_runs_on_its_own_pool() {
    let fixture = Fixture::new();
    let (mut waiting_pool, mut waking_pool) = (fixture.pool(1), fixture.pool(1));
    let waiting_queue = waiting_pool.register(fixture.mount(1)).unwrap();
    let waking_queue = waking_pool.register(fixture.mount(2)).unwrap();
    let me = std::thread::current().id();
    let (event, waiter) = parked(&waiting_queue);
    hand(
        &waking_queue,
        Box::pin(async move {
            event.fire();
            RequestDisposition::Complete
        }),
    );
    // The woken request belongs to another pool: a worker of that pool
    // continues it, and nothing of it stays with this thread.
    until(|| waiting_queue.work().unwrap().completed == 1);
    let threads = waiter.threads();
    assert_eq!(threads.len(), 2);
    assert_eq!(threads[0], me);
    assert_ne!(threads[1], me);
    assert_eq!(waking_queue.work().unwrap().completed, 1);
    finish(&waiting_queue);
    finish(&waking_queue);
    assert!(waiting_pool.stop().unwrap().clean());
    assert!(waking_pool.stop().unwrap().clean());
}
