//! Holds of real daemon resources for mounted proofs. Each is taken through
//! the product's public API, is an ordinary owner of what it holds, and gives
//! it back through a guard on every path, a panic of the test included.
//!
//! - `Leases`: every reader of the Store's fixed read set, each an ordinary
//!   granted `ReadLease`. While they are held, anything that needs the Store
//!   waits for reader admission; nothing is blocked inside a provider call.
//! - `Credits`: completions of real read-only owner jobs on one Workspace's
//!   lane. A kept completion keeps its job slot and its credited bytes, as
//!   any caller-held result does, until it is dropped.
//!
//! Taking a hold is one bounded readiness wait; it panics with the service's
//! own counters when the hold cannot be staged. A hold is returned by
//! `release` or by drop. The file is standalone: the including test declares
//! it as `#[path = "support/holds.rs"] mod holds;`.
use layerfs_bridge::control::WorkspaceToken;
use layerfs_daemon::{
    control::Service,
    store::{ReadLease, ReadTicket, Store},
    Command, Completion, OwnerClient, OwnerError,
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::Route;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread::{self, Thread},
    time::{Duration, Instant},
};

/// The read-admission lane the test's own leases are taken on. No Workspace
/// of a test may be bound with tag 255.
pub const HOLDER: [u8; 32] = [255; 32];

/// The owner route of a bound Workspace, for `Credits`.
pub fn route(service: &Service, token: WorkspaceToken) -> Route {
    service.operation(token).unwrap().workspace().route()
}

struct Unpark(Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
/// The ticket's grant, or `None` at the deadline. Dropping the ticket there
/// cancels only that unstarted admission.
fn granted(mut ticket: ReadTicket, deadline: Instant) -> Option<ReadLease> {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    loop {
        match Pin::new(&mut ticket).poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(Ok(lease)) => return Some(lease),
            Poll::Ready(Err(error)) => panic!("read lease hold refused: {error:?}"),
            Poll::Pending => {
                let now = Instant::now();
                if now >= deadline {
                    return None;
                }
                thread::park_timeout(deadline - now);
            }
        }
    }
}

/// Every reader of one Store, leased to the test.
pub struct Leases {
    held: Vec<ReadLease>,
}
impl Leases {
    /// Leases the whole read set, waiting at most `wait` for readers that
    /// are in use. Requires that no reader is quarantined.
    pub fn all(store: &Store, wait: Duration) -> Self {
        let readers = store.read_handles();
        assert_eq!(
            store.read_work().quarantined,
            0,
            "a quarantined reader cannot be held"
        );
        let deadline = Instant::now() + wait;
        let lane = WorkspaceId::from_authority(HOLDER).unwrap();
        let mut held: Vec<ReadLease> = Vec::with_capacity(readers);
        for _ in 0..readers {
            let ticket = store.read_ticket(Some(lane)).unwrap_or_else(|error| {
                panic!("read lease hold: {error:?} {:?}", store.read_work())
            });
            let lease = granted(ticket, deadline).unwrap_or_else(|| {
                panic!(
                    "read lease hold: {} of {readers} readers granted within {wait:?}: {:?}",
                    held.len(),
                    store.read_work()
                )
            });
            assert!(
                held.iter().all(|other| other.index() != lease.index()),
                "one reader leased twice"
            );
            held.push(lease);
        }
        let work = store.read_work();
        assert!(
            work.leased == readers && work.assigned == 0,
            "the whole read set is leased: {work:?}"
        );
        Self { held }
    }
    pub fn count(&self) -> usize {
        self.held.len()
    }
    /// Returns every reader to fair admission.
    pub fn release(mut self) {
        self.give_back();
    }
    fn give_back(&mut self) {
        let held = std::mem::take(&mut self.held);
        if held.is_empty() {
            return;
        }
        if thread::panicking() {
            // The product quarantines a lease that an unwinding thread drops,
            // as a poisoned session. This hold is the test's own and no
            // provider call failed, so a thread that is not unwinding returns
            // it: a failing test must not also take the read set out of
            // service under whatever it left parked.
            let _ = thread::spawn(move || drop(held)).join();
        } else {
            drop(held);
        }
    }
}
impl Drop for Leases {
    fn drop(&mut self) {
        self.give_back();
    }
}

/// Completions of the test's own read-only jobs on one Workspace lane.
pub struct Credits {
    held: Vec<Completion>,
}
impl Credits {
    /// `count` Lifecycle-class credits: `State` observations, kept.
    pub fn lifecycle(client: &OwnerClient, route: Route, count: usize, wait: Duration) -> Self {
        Self::of(client, route, count, wait, || Command::State)
    }
    /// `count` ordinary credits (Read class): local `Resources` observations,
    /// kept.
    pub fn ordinary(client: &OwnerClient, route: Route, count: usize, wait: Duration) -> Self {
        Self::of(client, route, count, wait, || Command::Resources {
            global: false,
        })
    }
    fn of(
        client: &OwnerClient,
        route: Route,
        count: usize,
        wait: Duration,
        command: fn() -> Command,
    ) -> Self {
        let deadline = Instant::now() + wait;
        let expired = |what: &str| {
            assert!(
                Instant::now() < deadline,
                "owner credit hold: {what} within {wait:?}: {:?}",
                client.diagnostics()
            );
            thread::sleep(Duration::from_millis(1));
        };
        let mut held = Vec::with_capacity(count);
        while held.len() < count {
            // A slot whose result was just dropped is free only once its
            // publisher has let go as well: a bounded readiness wait.
            let pending = match client.try_submit(Some(route), command()) {
                Ok(pending) => pending,
                Err((OwnerError::AdmissionFull, _)) => {
                    expired("no free slot");
                    continue;
                }
                Err((error, command)) => panic!("owner credit hold: {command:?}: {error:?}"),
            };
            let done = loop {
                match pending.try_complete().unwrap() {
                    Some(done) => break done,
                    None => expired("the job did not complete"),
                }
            };
            assert!(done.result().is_ok(), "owner credit hold: {done:?}");
            held.push(done);
        }
        Self { held }
    }
    pub fn count(&self) -> usize {
        self.held.len()
    }
    /// Drops every kept completion; each slot is free once its publisher has
    /// let go too, which the owner's `outstanding` count shows.
    pub fn release(self) {}
}
