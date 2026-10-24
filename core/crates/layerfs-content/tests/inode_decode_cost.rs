//! External allocation observations of the existing public inode-page decoder.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use layerfs_content::filesystem::inode::codec::{decode_inode_page, encode_inode_page, InodePage};
use layerfs_content::object::inode_leaf::{InodeKind, InodeLeafRow, InodeValue};
use layerfs_content::ObjectId;

#[derive(Clone, Copy, Default, Debug)]
struct Observation {
    armed: bool,
    target: usize,
    target_allocations: usize,
    allocations: usize,
    reallocations: usize,
}
thread_local! {
    static WATCH: Cell<Observation> = const { Cell::new(Observation {
        armed: false, target: 0, target_allocations: 0, allocations: 0, reallocations: 0,
    }) };
}
struct Observer;
#[global_allocator]
static ALLOCATOR: Observer = Observer;

fn observe(size: usize, reallocation: bool) {
    let _ = WATCH.try_with(|watch| {
        let mut row = watch.get();
        if row.armed {
            if reallocation {
                row.reallocations += 1;
            } else {
                row.allocations += 1;
                if size == row.target {
                    row.target_allocations += 1;
                }
            }
            watch.set(row);
        }
    });
}
unsafe impl GlobalAlloc for Observer {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            observe(layout.size(), false);
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            observe(layout.size(), false);
        }
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if !pointer.is_null() {
            observe(size, true);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }
}
struct Armed;
impl Armed {
    fn new(target: usize) -> Self {
        WATCH.with(|watch| {
            watch.set(Observation {
                armed: true,
                target,
                ..Observation::default()
            })
        });
        Self
    }
    fn finish(self) -> Observation {
        let observed = WATCH.with(Cell::get);
        drop(self);
        observed
    }
}
impl Drop for Armed {
    fn drop(&mut self) {
        WATCH.with(|watch| {
            let mut row = watch.get();
            row.armed = false;
            watch.set(row);
        });
    }
}

#[test]
fn typed_inode_decode_does_not_allocate_an_intermediate_raw_row_vector() {
    let mut outcomes = Vec::new();
    for count in [8_usize, 100] {
        let expected = InodePage::Leaf {
            entries: (1..=count as u64)
                .map(|serial| {
                    (
                        serial,
                        InodeValue {
                            kind: InodeKind::RegularFile,
                            namespace_ref_count: 1,
                            content_root: ObjectId::for_bytes(b"count-content"),
                            metadata_root: layerfs_content::object::inode_leaf::InodeMetadata::Root(
                                ObjectId::for_bytes(b"count-metadata"),
                            ),
                        },
                    )
                })
                .collect(),
        };
        let canonical = encode_inode_page(&expected).expect("fixture encodes");
        let target = count * std::mem::size_of::<InodeLeafRow>();
        let armed = Armed::new(target);
        let decoded = decode_inode_page(&canonical);
        let observed = armed.finish();
        assert_eq!(decoded.expect("fixture decodes"), expected);
        eprintln!("rows={count} raw_row_bytes={} target={target} target_allocations={} allocations={} reallocations={}",
            std::mem::size_of::<InodeLeafRow>(), observed.target_allocations,
            observed.allocations, observed.reallocations);
        outcomes.push((count, observed));
    }
    for (count, observed) in outcomes {
        assert_eq!(
            observed.target_allocations, 0,
            "{count} checked rows must not allocate a discarded raw-row representation"
        );
    }
}
