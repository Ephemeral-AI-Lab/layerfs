//! Peak live memory of one whole-file edit, under a counting allocator.
//!
//! P1-14's observable. No frozen vehicle prints a memory figure for the edit
//! route: `measure_edits` prints emitted objects and bytes, and the whole-file
//! route deliberately reports `EditCounters::default()` (pinned by
//! `tests/edit_transitions.rs`), so a product counter is the wrong instrument.
//! This example is the sanctioned non-product location: it installs a counting
//! `GlobalAlloc`, builds the frozen `small` fixture (65,536-byte base, 512-byte
//! overwrite at a quarter of the cutoff), and reports the live requested bytes
//! before, the peak during, and the delta across one `apply_edits` call.
//!
//! Requested bytes, not RSS: the number is what the allocator was asked for and
//! is deterministic for a fixed input, which is what a before/after comparison
//! needs. It is a single process, one sample, and no release claim.
//!
//! Usage: `cargo +1.85.1 run --release --locked -p layerfs-content --example edit_memory_probe`

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use layerfs_telemetry::timer::Timing;

use layerfs_content::{
    apply_edits, construct_bytes, AuthenticatedObjects, ConstructionPolicy, ContentResult, Edit,
    EditRequest, EditStream, FinalizedConsumer, FinalizedObject, ObjectId, Replacements,
};

/// Live requested bytes, and the high-water mark since the last reset.
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// System allocator with a requested-byte ledger.
struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        if !moved.is_null() {
            let previous = LIVE.fetch_add(new_size, Ordering::Relaxed);
            let live = previous + new_size - layout.size();
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Starts a fresh high-water mark at the current live total.
fn reset_peak() -> usize {
    let live = LIVE.load(Ordering::Relaxed);
    PEAK.store(live, Ordering::Relaxed);
    live
}

/// Prepared base objects, served to the edit with demand accounting.
#[derive(Default)]
struct Provider {
    objects: Vec<FinalizedObject>,
    demanded: RefCell<Vec<ObjectId>>,
}

impl Provider {
    fn canonical(&self, id: ObjectId) -> Option<&[u8]> {
        self.objects
            .iter()
            .find(|object| object.id() == id)
            .map(|object| object.canonical())
    }
}

impl FinalizedConsumer for Provider {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.objects.push(object);
        Ok(())
    }
}

impl AuthenticatedObjects for Provider {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demanded.borrow_mut().extend_from_slice(ids);
        ids.iter()
            .map(|id| {
                self.canonical(*id)
                    .map(<[u8]>::to_vec)
                    .ok_or(layerfs_content::ContentError::MissingObject)
            })
            .collect()
    }
}

/// In-memory consumer for what the edit emits.
#[derive(Default)]
struct Collector {
    objects: Vec<FinalizedObject>,
}

impl FinalizedConsumer for Collector {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.objects.push(object);
        Ok(())
    }
}

fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..len)
        .map(|_| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            state as u8
        })
        .collect()
}

fn main() {
    let policy = ConstructionPolicy::frozen_default();
    let cutoff = policy.small_file_threshold_bytes() as usize;
    // The frozen `small` fixture: base = cutoff/2, overwrite 512 bytes at cutoff/4.
    let base = noise(cutoff / 2);
    let replacement = noise(512);
    let start = cutoff as u64 / 4;
    let mut provider = Provider::default();
    let constructed = Timing::disabled("build", |scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &base,
            &mut provider,
            scope.child("content"),
        )
    })
    .0
    .expect("fixture construction");
    let base_id = constructed.root;

    let mut replacements = Replacements::new();
    replacements.push(replacement);
    let stream = EditStream::new(base.len() as u64, vec![Edit::overwrite(start, start + 512)])
        .expect("valid stream");
    let mut collector = Collector::default();

    let baseline = reset_peak();
    let edited = Timing::disabled("edit", |scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &provider,
            EditRequest {
                root: base_id,
                edits: &stream,
                source: &replacements,
            },
            &mut collector,
            scope.child("edit"),
        )
    })
    .0
    .expect("whole-file edit");
    let peak = PEAK.load(Ordering::Relaxed);

    println!("probe: whole-file edit peak live requested bytes");
    println!("cutoff_bytes: {cutoff}");
    println!("base_bytes: {}", base.len());
    println!("edit: overwrite [{start}, {})", start + 512);
    println!("final_bytes: {}", edited.logical_len);
    println!("edited_root: {}", edited.root);
    println!("objects_written: {}", collector.objects.len());
    println!(
        "objects_written_bytes: {}",
        collector
            .objects
            .iter()
            .map(|object| object.canonical().len())
            .sum::<usize>()
    );
    println!("nodes_read: {}", provider.demanded.borrow().len());
    println!("baseline_live_bytes: {baseline}");
    println!("peak_live_bytes: {peak}");
    println!("peak_delta_bytes: {}", peak - baseline);
}
