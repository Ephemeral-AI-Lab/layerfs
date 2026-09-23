//! Standalone #237 allocation diagnostic; not a public Init benchmark.
//! Run: rustc +1.85.1 -O prefix_probe_diagnostic.rs -o prefix_probe_diagnostic
//!      ./prefix_probe_diagnostic > prefix-probe-raw.tsv

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::io::{Read, Take};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::Instant;

struct CountingAllocator;
static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static MAX_REQUEST: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Relaxed);
            MAX_REQUEST.fetch_max(layout.size() as u64, Relaxed);
        }
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Relaxed);
            MAX_REQUEST.fetch_max(layout.size() as u64, Relaxed);
        }
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            REALLOCS.fetch_add(1, Relaxed);
            REALLOC_BYTES.fetch_add(size as u64, Relaxed);
            MAX_REQUEST.fetch_max(size as u64, Relaxed);
        }
        System.realloc(ptr, layout, size)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

struct SyntheticFile {
    remaining: usize,
    calls: u64,
    bytes: u64,
}

impl Read for SyntheticFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.calls += 1;
        let count = buf.len().min(self.remaining);
        buf[..count].fill(0xa5);
        self.remaining -= count;
        self.bytes += count as u64;
        Ok(count)
    }
}

struct Class {
    name: &'static str,
    count: usize,
    size: usize,
}

const CLASSES: [Class; 6] = [
    Class { name: "empty", count: 100, size: 0 },
    Class { name: "tiny325", count: 1_074, size: 325 },
    Class { name: "tiny326", count: 6_825, size: 326 },
    Class { name: "small", count: 1_500, size: 20_782 },
    Class { name: "medium", count: 500, size: 332_506 },
    Class { name: "anchor", count: 1, size: 100_000_000 },
];

fn reset_counters() {
    ALLOCS.store(0, Relaxed);
    REALLOCS.store(0, Relaxed);
    ALLOC_BYTES.store(0, Relaxed);
    REALLOC_BYTES.store(0, Relaxed);
    MAX_REQUEST.store(0, Relaxed);
}

fn run_class(cutoff: usize, initial: usize, class: &Class) {
    let mut elapsed_ns = 0_u128;
    let mut read_calls = 0_u64;
    let mut read_bytes = 0_u64;
    let mut min_initial_capacity = usize::MAX;
    let mut max_initial_capacity = 0;
    let mut min_final_capacity = usize::MAX;
    let mut max_final_capacity = 0;
    let mut digest = 0_u64;
    let expected = class.size.min(cutoff);
    let mut equal_bytes = true;
    let mut equal_dispatch = true;
    reset_counters();

    for _ in 0..class.count {
        let mut source = SyntheticFile { remaining: class.size, calls: 0, bytes: 0 };
        ENABLED.store(true, Relaxed);
        let start = Instant::now();
        let mut prefix = Vec::new();
        prefix.try_reserve_exact(initial).expect("initial reserve");
        let initial_capacity = prefix.capacity();
        let mut limited: Take<&mut SyntheticFile> = (&mut source).take(cutoff as u64);
        limited.read_to_end(&mut prefix).expect("prefix read");
        elapsed_ns += start.elapsed().as_nanos();
        ENABLED.store(false, Relaxed);

        min_initial_capacity = min_initial_capacity.min(initial_capacity);
        max_initial_capacity = max_initial_capacity.max(initial_capacity);
        min_final_capacity = min_final_capacity.min(prefix.capacity());
        max_final_capacity = max_final_capacity.max(prefix.capacity());
        read_calls += source.calls;
        read_bytes += source.bytes;
        equal_bytes &= prefix.len() == expected && prefix.iter().all(|byte| *byte == 0xa5);
        equal_dispatch &= (prefix.len() < cutoff) == (class.size < cutoff);
        digest = digest.wrapping_add(prefix.iter().map(|byte| *byte as u64).sum::<u64>());
        black_box(&prefix);
    }

    assert!(equal_bytes && equal_dispatch);
    assert_eq!(read_bytes, (expected * class.count) as u64);
    println!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        cutoff, initial, class.name, class.count, class.size, expected,
        elapsed_ns, ALLOCS.load(Relaxed), REALLOCS.load(Relaxed),
        ALLOC_BYTES.load(Relaxed), REALLOC_BYTES.load(Relaxed), MAX_REQUEST.load(Relaxed),
        min_initial_capacity, max_initial_capacity, min_final_capacity, max_final_capacity,
        read_calls, read_bytes, if class.size < cutoff { "inline" } else { "chunked" },
        if equal_bytes && equal_dispatch { "yes" } else { "no" }, digest
    );
}

fn main() {
    println!("cutoff\tinitial\tclass\tcount\tsize\tprefix_per_file\telapsed_ns\talloc_calls\trealloc_calls\talloc_requested_bytes\trealloc_requested_bytes\tmax_request\tinitial_capacity_min\tinitial_capacity_max\tfinal_capacity_min\tfinal_capacity_max\tread_calls\tread_bytes\tdispatch\tequal\tdigest");
    for cutoff in [131_072, 262_144, 524_288, 1_048_576] {
        for initial in [cutoff, 4_096, 512, 0] {
            for class in &CLASSES {
                run_class(cutoff, initial, class);
            }
        }
    }
}
