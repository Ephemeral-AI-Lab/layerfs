//! Requested-allocation observation around the real System allocator.
//!
//! This external test helper never changes, refuses or fabricates allocations.
//! A fixed observer ledger records selected-thread Rust allocation requests and
//! follows their actual deallocation, including after capture stops. It measures
//! requested layouts, not allocator metadata, C-library heap, RSS or page cache.
//! Observer trace buffers and independently prepared fixtures/output have separate
//! ownership and are deliberately outside capture.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

thread_local! {
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
}

const SLOTS: usize = 2048;
static WATCHING: AtomicBool = AtomicBool::new(false);
static LEDGER: Mutex<Ledger> = Mutex::new(Ledger::new());

#[derive(Clone, Copy)]
struct Allocation {
    pointer: usize,
    size: usize,
    class: usize,
}

const EMPTY: Allocation = Allocation {
    pointer: 0,
    size: 0,
    class: 0,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    /// Classes: exact encoded sizes, exact decoded-entry sizes, other requests.
    pub live: [usize; 3],
    pub peak: [usize; 3],
    pub peak_total: usize,
    pub requested_total: usize,
    pub allocation_events: usize,
    pub reallocations: usize,
    pub largest_request: usize,
    pub overflow: bool,
}

impl Snapshot {
    pub fn live_total(self) -> usize {
        self.live.iter().sum()
    }
}

struct Ledger {
    slots: [Allocation; SLOTS],
    encoded: [usize; 2],
    decoded: [usize; 2],
    snapshot: Snapshot,
}

impl Ledger {
    const fn new() -> Self {
        Self {
            slots: [EMPTY; SLOTS],
            encoded: [0; 2],
            decoded: [0; 2],
            snapshot: Snapshot {
                live: [0; 3],
                peak: [0; 3],
                peak_total: 0,
                requested_total: 0,
                allocation_events: 0,
                reallocations: 0,
                largest_request: 0,
                overflow: false,
            },
        }
    }

    fn add(&mut self, pointer: *mut u8, size: usize) {
        if pointer.is_null() {
            return;
        }
        let class = if self.encoded.contains(&size) {
            0
        } else if self.decoded.contains(&size) {
            1
        } else {
            2
        };
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.pointer == 0) {
            *slot = Allocation {
                pointer: pointer as usize,
                size,
                class,
            };
            self.snapshot.live[class] += size;
            self.snapshot.peak[class] = self.snapshot.peak[class].max(self.snapshot.live[class]);
            self.snapshot.peak_total = self.snapshot.peak_total.max(self.snapshot.live_total());
        } else {
            self.snapshot.overflow = true;
        }
        self.snapshot.requested_total += size;
        self.snapshot.allocation_events += 1;
        self.snapshot.largest_request = self.snapshot.largest_request.max(size);
    }

    fn remove(&mut self, pointer: *mut u8) -> bool {
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.pointer == pointer as usize)
        {
            self.snapshot.live[slot.class] -= slot.size;
            *slot = EMPTY;
            true
        } else {
            false
        }
    }
}

fn capturing() -> bool {
    CAPTURE.try_with(Cell::get).unwrap_or(false)
}

fn record(pointer: *mut u8, size: usize) {
    if capturing() {
        if let Ok(mut ledger) = LEDGER.lock() {
            ledger.add(pointer, size);
        }
    }
}

/// The allocator installed only in this external integration-test executable.
pub struct ObservedSystem;

unsafe impl GlobalAlloc for ObservedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        record(pointer, layout.size());
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record(pointer, layout.size());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if WATCHING.load(Ordering::Relaxed) {
            if let Ok(mut ledger) = LEDGER.lock() {
                ledger.remove(pointer);
            }
        }
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() && WATCHING.load(Ordering::Relaxed) {
            if let Ok(mut ledger) = LEDGER.lock() {
                let owned = ledger.remove(pointer);
                if owned || capturing() {
                    ledger.snapshot.reallocations += 1;
                    ledger.add(result, size);
                }
            }
        }
        result
    }
}

/// One selected-thread capture, followed by explicit exact release observation.
pub struct Observation;

impl Observation {
    pub fn start(encoded: [usize; 2], decoded: [usize; 2]) -> Self {
        assert!(!capturing(), "one allocation capture at a time");
        {
            let mut ledger = LEDGER.lock().expect("external allocation ledger");
            assert_eq!(
                ledger.snapshot.live_total(),
                0,
                "previous owners must release"
            );
            *ledger = Ledger::new();
            ledger.encoded = encoded;
            ledger.decoded = decoded;
        }
        WATCHING.store(true, Ordering::Relaxed);
        CAPTURE.with(|capture| capture.set(true));
        Self
    }

    pub fn stop(&self) -> Snapshot {
        CAPTURE.with(|capture| capture.set(false));
        LEDGER.lock().expect("external allocation ledger").snapshot
    }

    /// Call after every captured allocation owner is actually dropped.
    pub fn released(self) -> Snapshot {
        let snapshot = self.stop();
        WATCHING.store(false, Ordering::Relaxed);
        snapshot
    }
}

impl Drop for Observation {
    fn drop(&mut self) {
        CAPTURE.with(|capture| capture.set(false));
    }
}

/// Excludes only external observation bookkeeping from capture; product code
/// never runs while this guard is alive.
pub struct Pause(bool);

impl Pause {
    pub fn new() -> Self {
        Self(CAPTURE.with(|capture| capture.replace(false)))
    }
}

impl Default for Pause {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Pause {
    fn drop(&mut self) {
        CAPTURE.with(|capture| capture.set(self.0));
    }
}
