//! Steps inside a kernel callback, timed on the receive worker that runs it
//! (#352 F-032, #356): owner jobs under the Workspace lock and the base reads
//! between them. The running callback's mount counters are current on the
//! thread; elsewhere, and off Linux, charging does nothing.
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Steps inside a callback, timed on the receive worker that runs it
/// (#352 F-032): owner jobs under the Workspace lock and the base reads
/// between them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Phase {
    /// LOOKUP/GETATTR/OPEN/OPENDIR owner visit job.
    ReadVisit,
    /// Base facts read for an undecided read visit.
    ReadSupply,
    /// Mutation owner visit job.
    MutateVisit,
    /// Base facts read for an undecided mutation visit.
    MutateSupply,
    /// The release job after a published mutation's reply.
    MutateRelease,
    /// READDIR window job.
    DirectoryVisit,
    /// Base listing read to finish a READDIR window.
    DirectoryFinish,
    /// FORGET owner job.
    Forget,
    /// RELEASEDIR owner job.
    CloseDirectory,
}
pub const PHASES: usize = Phase::CloseDirectory as usize + 1;
impl Phase {
    pub const NAMES: [&'static str; PHASES] = [
        "read_visit",
        "read_supply",
        "mutate_visit",
        "mutate_supply",
        "mutate_release",
        "directory_visit",
        "directory_finish",
        "forget",
        "close_directory",
    ];
}

/// One mount's phase counters.
#[derive(Default)]
pub struct Phases {
    counts: [AtomicU64; PHASES],
    nanos: [AtomicU64; PHASES],
}
impl Phases {
    pub fn counts(&self) -> [u64; PHASES] {
        std::array::from_fn(|index| self.counts[index].load(Ordering::Relaxed))
    }
    pub fn nanos(&self) -> [u64; PHASES] {
        std::array::from_fn(|index| self.nanos[index].load(Ordering::Relaxed))
    }
}

thread_local! {
    /// The phase counters of the callback this receive worker is running.
    static CURRENT: RefCell<Option<Arc<Phases>>> = const { RefCell::new(None) };
}

/// Makes `phases` current on this thread; returns the previous ones.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn enter(phases: Arc<Phases>) -> Option<Arc<Phases>> {
    CURRENT.with(|current| current.borrow_mut().replace(phases))
}
/// Restores what [`enter`] returned.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn restore(previous: Option<Arc<Phases>>) {
    CURRENT.with(|current| *current.borrow_mut() = previous);
}

/// Charges the time since `started` to `phase` of the running callback's
/// mount; nothing when no callback is running on this thread.
pub(crate) fn charge(phase: Phase, started: Instant) {
    let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    CURRENT.with(|current| {
        if let Some(phases) = current.borrow().as_ref() {
            phases.counts[phase as usize].fetch_add(1, Ordering::Relaxed);
            phases.nanos[phase as usize].fetch_add(elapsed, Ordering::Relaxed);
        }
    });
}
