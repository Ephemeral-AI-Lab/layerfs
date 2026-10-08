//! External public-API checks of bounded event-driven request custody.
mod support;
use layerfs_fuse::{
    DispatchError, FailureView, NextTurn, RequestDisposition, HANDOFFS, MAX_INPUT_BYTES,
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
fn two_borrowed_waiters_wake_on_terminal_without_releasing_original_credit() {
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
    let received = [queue.receive().unwrap(), queue.receive().unwrap()];
    assert!(matches!(queue.receive(), Err(DispatchError::Capacity)));
    assert_eq!(
        queue.work().unwrap().owned_input_bytes,
        HANDOFFS * MAX_INPUT_BYTES
    );
    let (send, recv) = mpsc::channel();
    // A terminal fence in Drop makes both blocking admissions exit even if a
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
    let failures: Vec<_> = (0..2)
        .map(|_| recv.recv_timeout(Duration::from_secs(3)).unwrap())
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert!(failures
        .iter()
        .all(|error| error.reason == DispatchError::Stopped));
    assert_eq!(queue.work().unwrap().received, 2);
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
