//! Count/cause through public persistence ports; never a performance gate.
use layerfs_content::ObjectId;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{location::PackInfo, pack::layout, port::*};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};
struct Counter;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ALLOC: AtomicU64 = AtomicU64::new(0);
static REALLOC: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            ALLOC.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(l.size() as u64, Ordering::Relaxed);
        }
        System.alloc(l)
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            ALLOC.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(l.size() as u64, Ordering::Relaxed);
        }
        System.alloc_zeroed(l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            REALLOC.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(n as u64, Ordering::Relaxed);
        }
        System.realloc(p, l, n)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
}
#[global_allocator]
static COUNTER: Counter = Counter;
struct FirstGroup;
impl PackReadPlan for FirstGroup {
    fn select(
        &mut self,
        info: PackInfo,
        prefix: &[u8],
    ) -> Result<PackReadChoice, PersistenceError> {
        let h = layout::parse_directory_header(prefix, info.length)
            .map_err(|_| PersistenceError::Malformed)?;
        let v =
            layout::directory_group_views(prefix, h).map_err(|_| PersistenceError::Malformed)?[0];
        Ok(PackReadChoice::Ranges(vec![PackRange {
            offset: v.start,
            length: v.end - v.start,
        }]))
    }
}
fn begin() {
    ALLOC.store(0, Ordering::Relaxed);
    REALLOC.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
}
fn finish(name: &str, calls: usize, checksum: u64, wall: u128) {
    ACTIVE.store(false, Ordering::Relaxed);
    println!("{{\"kind\":\"public-port-allocation-count-cause\",\"path\":\"{name}\",\"calls\":{calls},\"rust_allocations\":{},\"rust_reallocations\":{},\"rust_requested_allocation_bytes\":{},\"checksum\":{checksum},\"wall_ns\":{wall},\"cache_state\":\"uncontrolled; diagnostic only\",\"scope\":\"Rust global allocator requests within public calls; excludes C malloc, RSS, live memory and setup\"}}",ALLOC.load(Ordering::Relaxed),REALLOC.load(Ordering::Relaxed),BYTES.load(Ordering::Relaxed));
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("closed Store required")?;
    let db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut query =
        db.prepare("SELECT object_id FROM object_location ORDER BY object_id LIMIT 1024")?;
    let ids = query
        .query_map([], |r| r.get::<_, Vec<u8>>(0))?
        .map(|v| ObjectId::from_bytes(&v?).map_err(|_| rusqlite::Error::InvalidQuery))
        .collect::<Result<Vec<_>, _>>()?;
    drop(query);
    let mut query = db.prepare("SELECT pack_id FROM pack ORDER BY pack_id LIMIT 128")?;
    let packs = query
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(query);
    drop(db);
    let h = Handles::open_read_only(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        b"layerfs/issue286/retained-history/v3",
        [0x28; 32],
    )?;
    let mut out = Vec::with_capacity(1);
    let mut sum = 0u64;
    let t = Instant::now();
    begin();
    for id in &ids {
        h.storage.locate(&[*id], &mut out)?;
        if out.len() != 1 || out[0].location.object_id != *id {
            return Err("locator mismatch".into());
        }
        let row = out[0];
        sum = sum.wrapping_add(
            row.pack.length as u64
                + row.location.canonical_length as u64
                + row.location.group_number as u64
                + row.location.record_number as u64,
        );
    }
    finish("singleton-locator", ids.len(), sum, t.elapsed().as_nanos());
    let mut sum = 0u64;
    let t = Instant::now();
    begin();
    for id in &packs {
        let read = h.storage.read_scoped_pack(*id, &mut FirstGroup)?;
        let AcquiredPackRead::Units(read) = read else {
            return Err("expected scoped units".into());
        };
        let (info, prefix, ranges) = read.into_parts();
        sum = sum.wrapping_add(
            info.length as u64
                + prefix.iter().map(|b| u64::from(*b)).sum::<u64>()
                + ranges
                    .iter()
                    .map(|(_, b)| b.iter().map(|b| u64::from(*b)).sum::<u64>())
                    .sum::<u64>(),
        );
    }
    finish(
        "control-plus-first-group",
        packs.len(),
        sum,
        t.elapsed().as_nanos(),
    );
    Ok(())
}
