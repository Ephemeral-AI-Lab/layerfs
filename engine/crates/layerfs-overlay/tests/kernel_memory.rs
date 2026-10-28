//! One allocation observation of 5,000 remembered regular files. Requested
//! Rust allocation sizes include both kernel-reference and inode-custody maps
//! and their touched-entry journals. SQLite's C heap, allocator rounding,
//! process RSS and kernel caches are outside this measurement.
use layerfs_overlay::{Inode, InodeKind, KernelDecision, Overlay, ProfileConfig};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicIsize, Ordering},
};

struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
// SAFETY: unchanged arguments go to the system allocator. Only successful
// requested layouts are added, and the same layouts are subtracted at free.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() {
            LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout);
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn remembered_regular_file_allocation_is_measured_without_a_population_cap() {
    let path = std::env::temp_dir().join(format!("layerfs-kernel-memory-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let db = Overlay::create(&path.join("overlay.sqlite"), ProfileConfig::default()).unwrap();
    db.open_workspace([2; 32]).unwrap();
    let mount = db.create_kernel_mount(1).unwrap();
    let lookup = |serial| {
        let outcome = db.observe_kernel_visit(mount, 1, None, true, |_, _| {
            Ok(KernelDecision::Finished {
                inode: Some(Inode {
                    serial,
                    kind: InodeKind::File,
                    mode: 0o644,
                    mtime_seconds: 0,
                    mtime_nanoseconds: 0,
                    nlink: 1,
                    size: 0,
                    inherited_cutoff: 0,
                    born: 0,
                    entries: 0,
                    subdirs: 0,
                }),
                value: serial,
            })
        });
        assert_eq!(outcome.result.unwrap(), None);
        assert_eq!(outcome.decision, Some(serial));
    };
    // Warm only fixed statement/journal setup for this allocation diagnostic.
    // This is not a cold phase or a timing/performance qualification.
    lookup(2);
    db.kernel_forget(mount, 2, 1).unwrap();
    let initial = db.resources().unwrap();
    let bytes_before = LIVE.load(Ordering::Relaxed);
    for serial in 3..5003 {
        lookup(serial);
    }
    let bytes_after = LIVE.load(Ordering::Relaxed);
    let held = db.resources().unwrap();
    assert_eq!(held.counts, initial.counts, "lookup changes no SQLite row");
    assert_eq!(held.open.kernel_entries, initial.open.kernel_entries + 5000);
    assert_eq!(held.open.file_custody, initial.open.file_custody + 5000);
    assert_eq!(held.open.directory_parents, initial.open.directory_parents);
    assert!(bytes_after > bytes_before);
    println!(
        "KERNEL_MEMORY entries=5000 rust_requested_bytes={} bytes_per_entry={} kernel_entries={} custody_entries={} scope=kernel_and_custody_maps_with_retained_journal_capacity sqlite_c_heap=excluded allocator_rounding=excluded rss=not_measured",
        bytes_after - bytes_before,
        (bytes_after - bytes_before) as f64 / 5000.0,
        held.open.kernel_entries,
        held.open.file_custody,
    );
    // A real FORGET ends exactly one of those references. No total-cap admission
    // substitutes for serving the last entry.
    db.kernel_forget(mount, 5002, 1).unwrap();
    db.revoke_kernel_mount(mount).unwrap();
    db.close().unwrap();
    db.close_database().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
