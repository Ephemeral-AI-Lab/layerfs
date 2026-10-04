//! Count/cause fixture for locator retention at the existing 4096-entry bound.
#[path = "/Users/yifanxu/.codex/worktrees/save-vfs-amplification/layerfs/core/crates/layerfs-storage/tests/support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{policy, Storage};
use std::sync::Arc;

fn main() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let writer = Storage::new(metadata.clone()).unwrap();
    let mut objects: Vec<_> = (0..policy::READ_OBJECT_LIMIT as u64 + 1)
        .map(|n| FinalizedObject::new(ObjectRole::FileState,
            layerfs_content::object::codec::encode_bytes_object(&n.to_be_bytes()).unwrap()).unwrap())
        .collect();
    let save = writer.begin_save().unwrap();
    for object in &objects { save.accept(object.clone()).unwrap(); }
    save.finish().unwrap();
    objects.sort_by_key(FinalizedObject::id);
    let reading = Storage::new(metadata.clone()).unwrap();
    let reader = reading.reader().unwrap();
    let ids: Vec<_> = objects[..policy::READ_OBJECT_LIMIT].iter().map(FinalizedObject::id).collect();
    let heap_before_fill = LIVE.load(Ordering::Relaxed);
    let actual = reader.read_objects(&ids).unwrap();
    assert_eq!(actual, objects[..policy::READ_OBJECT_LIMIT].iter().map(|o|o.canonical().to_vec()).collect::<Vec<_>>());
    drop(actual);
    let fill_heap_delta = LIVE.load(Ordering::Relaxed) - heap_before_fill;
    metadata.calls.lock().unwrap().clear();
    let miss = &objects[policy::READ_OBJECT_LIMIT];
    assert_eq!(reader.read_objects(&[miss.id()]).unwrap(), vec![miss.canonical().to_vec()]);
    let retained = &objects[0];
    assert_eq!(reader.read_objects(&[retained.id()]).unwrap(), vec![retained.canonical().to_vec()]);
    for _ in 0..16 {
        for object in [&objects[1], &objects[0]] {
            assert_eq!(reader.read_objects(&[object.id()]).unwrap(), vec![object.canonical().to_vec()]);
        }
    }
    let requests: Vec<_> = metadata.calls.lock().unwrap().iter().filter(|(name,_)| *name == "locate").copied().collect();
    eprintln!("DIAGNOSTIC locator-pressure requests={requests:?}");
    println!("PUBLIC_READER_FILL_HEAP_DELTA {fill_heap_delta}");
    println!("PUBLIC_READER_WORK {:?}", reading.diagnostics());
}

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
static LIVE: AtomicUsize = AtomicUsize::new(0);
struct Alloc;
#[global_allocator]
static ALLOC: Alloc = Alloc;
unsafe impl GlobalAlloc for Alloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() { LIVE.fetch_add(layout.size(), Ordering::Relaxed); }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        System.dealloc(p, layout);
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = System.realloc(p, layout, size);
        if !q.is_null() {
            if size >= layout.size() { LIVE.fetch_add(size-layout.size(), Ordering::Relaxed); }
            else { LIVE.fetch_sub(layout.size()-size, Ordering::Relaxed); }
        }
        q
    }
}
