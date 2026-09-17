//! Phase-local heap accounting for the Stages 3-4 paths, plus a labelled RSS scope.
//!
//! This is measurement code in an external target, never a product hook. It
//! installs a counting global allocator and brackets each real product phase with
//! it, so every row reports:
//!
//! * `current` - bytes the process still holds when the phase ended;
//! * `peak`    - the largest number of bytes held *inside* the phase, measured
//!               against the residency the phase started from;
//! * `allocs`  - allocation calls made inside the phase;
//! * `charged` - bytes those calls requested in total.
//!
//! The counts are requested sizes: allocator metadata, alignment padding and any
//! `mmap` the allocator serves outside `alloc` are not included, and the numbers
//! are therefore lower bounds on the process's true heap. Residency is sampled
//! separately and reported as what it is - a sampled whole-process figure, never a
//! phase-local heap number. A lifetime high-water is *not* produced here: `ru_maxrss`
//! needs a platform interface this batch does not add a dependency for, so the
//! receipt says `null` with that reason and the sample window is stated instead.
//!
//! Usage:
//!   memory_ledger --output FRESH_DIR [--pooled-leaves 24] [--pooled-rows 100]
//!
//! The output directory must not exist; one run writes its receipt there. Every
//! phase runs the real product entry point, and each fixture is prepared before
//! the phase that uses it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
};
use layerfs_content::{
    construct_bytes, AdvisoryPredecessors, ConstructionPolicy, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::{StoragePolicy, Store};
use layerfs_telemetry::timer::Timing;

type Failure = Box<dyn std::error::Error>;

/// Bytes the process currently holds.
static CURRENT: AtomicUsize = AtomicUsize::new(0);
/// Bytes held above the phase's starting residency.
static PEAK: AtomicUsize = AtomicUsize::new(0);
/// Residency the current phase started from.
static BASE: AtomicUsize = AtomicUsize::new(0);
/// Allocation calls observed.
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
/// Bytes requested by those calls.
static CHARGED: AtomicUsize = AtomicUsize::new(0);

/// Counting allocator over the system allocator.
struct Counting;

impl Counting {
    fn record(size: usize) {
        let current = CURRENT.fetch_add(size, Ordering::Relaxed) + size;
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        CHARGED.fetch_add(size, Ordering::Relaxed);
        let base = BASE.load(Ordering::Relaxed);
        if current > base {
            PEAK.fetch_max(current - base, Ordering::Relaxed);
        }
    }
}

// SAFETY: every call forwards to the system allocator with the same layout it was
// given, and the counters are plain relaxed atomics that cannot fail.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            Counting::record(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let resized = unsafe { System.realloc(pointer, layout, new_size) };
        if !resized.is_null() {
            CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
            Counting::record(new_size);
        }
        resized
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// One measured phase.
struct Phase {
    label: &'static str,
    current: usize,
    peak: usize,
    allocations: u64,
    charged: u64,
    rss_before: Option<u64>,
    rss_after: Option<u64>,
}

/// Opens a phase: residency the phase starts from, counters at zero.
fn begin() -> (usize, u64, u64) {
    let base = CURRENT.load(Ordering::Relaxed);
    BASE.store(base, Ordering::Relaxed);
    PEAK.store(0, Ordering::Relaxed);
    (
        CURRENT.load(Ordering::Relaxed),
        ALLOCATIONS.load(Ordering::Relaxed) as u64,
        CHARGED.load(Ordering::Relaxed) as u64,
    )
}

/// Closes a phase and records what it cost.
fn end(label: &'static str, start: (usize, u64, u64), rss_before: Option<u64>) -> Phase {
    let (_, allocations, charged) = start;
    Phase {
        label,
        current: CURRENT.load(Ordering::Relaxed),
        peak: PEAK.load(Ordering::Relaxed),
        allocations: ALLOCATIONS.load(Ordering::Relaxed) as u64 - allocations,
        charged: CHARGED.load(Ordering::Relaxed) as u64 - charged,
        rss_before,
        rss_after: sample_rss(),
    }
}

/// Samples this process's resident set in bytes.
///
/// The sample is a whole-process figure taken at a phase boundary, so it is a
/// lower bound on the phase's true residency: something that peaked between two
/// samples is not seen. The sampling tool and its resolution are part of the
/// receipt.
fn sample_rss() -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let kilobytes: u64 = text.trim().parse().ok()?;
    Some(kilobytes * 1024)
}

/// In-memory consumer that keeps every emitted canonical object.
///
/// It is the fixture for the save phase, so the objects it retains are part of the
/// phase that follows rather than of the construction phase that produced them.
#[derive(Default)]
struct Collected {
    objects: Vec<FinalizedObject>,
}

impl FinalizedConsumer for Collected {
    fn accept(&mut self, object: FinalizedObject) -> layerfs_content::ContentResult<()> {
        self.objects.push(object);
        Ok(())
    }
}

fn disabled<T, E>(
    body: impl FnOnce(
        &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    ) -> Result<T, E>,
) -> Result<T, E> {
    Timing::disabled("memory.phase", body).0
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

fn pooled_value(seed: u64) -> [u8; INODE_VALUE_BYTES] {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&bytes),
        metadata_root: ObjectId::for_bytes(&[seed as u8; 8]),
    })
}

fn pooled_leaf(first_serial: u64, values: &[[u8; INODE_VALUE_BYTES]]) -> FinalizedObject {
    let rows = values
        .iter()
        .enumerate()
        .map(|(index, value)| InodeLeafRow {
            serial: first_serial + index as u64,
            value: *value,
        })
        .collect::<Vec<_>>();
    let canonical = InodeLeaf {
        subtree_bytes: rows.len() as u64 * INODE_VALUE_BYTES as u64,
        rows,
    }
    .encode()
    .expect("canonical leaf");
    FinalizedObject::new(ObjectRole::InodeLeaf, canonical).expect("finalized leaf")
}

fn main() {
    // D2: the whole-command wall time is printed by the tool itself.
    let started = std::time::Instant::now();
    let result = run();
    println!("wall_seconds: {:.6}", started.elapsed().as_secs_f64());
    if let Err(failure) = result {
        eprintln!("memory_ledger: {failure}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Failure> {
    let mut output = None;
    let mut leaves = 24_u64;
    let mut rows = 100_usize;
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--output" => output = Some(PathBuf::from(value)),
            "--pooled-leaves" => leaves = value.parse()?,
            "--pooled-rows" => rows = value.parse()?,
            other => return Err(format!("unknown flag {other}").into()),
        }
    }
    let output = output.ok_or("--output is required; it must not exist")?;
    std::fs::create_dir(&output).map_err(|error| format!("--output: {error}"))?;
    for path in [
        output.join("memory-ledger.txt"),
        output.join("memory-ledger.json"),
    ] {
        if path.exists() {
            return Err(format!("{} already exists; use a fresh directory", path.display()).into());
        }
    }
    if rows == 0 || rows > 100 {
        return Err("--pooled-rows must be 1..=100".into());
    }

    let receipt = output.join("memory-ledger.txt");
    let mut phases: Vec<Phase> = Vec::new();
    let policy = ConstructionPolicy::frozen_default();

    // Fixtures are prepared before the phases that consume them.
    let chunked_bytes = noise(1_048_576);
    let pooled_values: Vec<[u8; INODE_VALUE_BYTES]> =
        (0..leaves * rows as u64).map(pooled_value).collect();

    // Phase 1: complete construction of a 1 MiB chunked file (C1 only).
    let mut collected = Collected::default();
    let started = begin();
    let rss = sample_rss();
    let constructed = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &chunked_bytes,
            &mut collected,
            scope.child("content"),
        )
    })?;
    phases.push(end("c1.construct.chunked-1mib", started, rss));
    let emitted = collected.objects.len() as u64;
    let chunked_objects: Vec<FinalizedObject> = collected.objects.drain(..).collect();
    assert_eq!(chunked_objects.len() as u64, emitted);

    // Phase 2: save that object set through the real C2 path, including the Store
    // creation the run needs.
    let store_path = output.join("chunked.sqlite");
    let started = begin();
    let rss = sample_rss();
    let store = disabled(|scope| {
        Store::create(
            &store_path,
            StoragePolicy::frozen_default(),
            scope.child("store"),
        )
    })?;
    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in chunked_objects {
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    })?;
    phases.push(end("c2.save.chunked-1mib", started, rss));
    let rooted =
        disabled(|scope| store.read_batch(&[constructed.root], scope.child("storage.read")))?;
    let chunked_read_bytes = rooted.0.iter().map(Vec::len).sum::<usize>();

    // Phase 3: the pooled lane at the requested shape. The first leaf runs against
    // a cold Store (the index synchronizes from the catalogue) and the rest reuse
    // that work, so the two are measured separately.
    let pooled_path = output.join("pooled.sqlite");
    let started = begin();
    let rss = sample_rss();
    let pooled = disabled(|scope| {
        Store::create(
            &pooled_path,
            StoragePolicy::frozen_default(),
            scope.child("store"),
        )
    })?;
    let mut previous: Option<ObjectId> = None;
    let mut inserted = 0_u64;
    let mut cold_phase = None;
    let mut warm_start = started;
    let mut warm_rss = rss;
    for step in 0..leaves {
        if step == 1 {
            // The first leaf is finished: close the cold phase and start the warm
            // one from the residency it left behind.
            cold_phase = Some(end("c2.save.pooled.cold", started, rss));
            warm_rss = sample_rss();
            warm_start = begin();
        }
        let first = (step * rows as u64) as usize;
        let values = &pooled_values[first..first + rows];
        let mut object = pooled_leaf(1 + step * rows as u64, values);
        if let Some(base) = previous {
            let mut predecessors = AdvisoryPredecessors::new();
            predecessors.push(base, PredecessorProvenance::OriginalBase)?;
            object = object.with_predecessors(predecessors);
        }
        let id = object.id();
        let saved = disabled(|scope| {
            let mut operation = pooled.begin_save(scope.child("storage.begin"))?;
            operation.accept(object)?;
            operation.finish(scope.child("storage.finish"))
        })?;
        inserted += saved.inserted;
        previous = Some(id);
    }
    phases.push(end("c2.save.pooled.warm", warm_start, warm_rss));
    if let Some(cold) = cold_phase {
        phases.push(cold);
    }
    let index_entries = pooled.pool_index_entries();
    let index_bytes = pooled.pool_index_bytes();

    // Phase 4: read one pooled leaf back.
    let deepest = previous.expect("at least one pooled leaf");
    let started = begin();
    let rss = sample_rss();
    let read = disabled(|scope| pooled.read_batch(&[deepest], scope.child("storage.read")))?;
    phases.push(end("c2.read.pooled", started, rss));
    let read_bytes = read.0.iter().map(Vec::len).sum::<usize>();

    // Phase 5: an abandoned save and its one cleanup attempt.
    let started = begin();
    let rss = sample_rss();
    let candidate_bytes;
    {
        let mut operation = disabled(|scope| pooled.begin_save(scope.child("storage.begin")))?;
        disabled(|_scope| operation.accept(pooled_leaf(900_000, &pooled_values[..rows])))?;
        candidate_bytes = operation.candidate_index_bytes();
        disabled(|scope| operation.abort(scope.child("storage.abort")))?;
    }
    phases.push(end("c2.abandon.cleanup", started, rss));

    // The receipt: identities, the phases, the live counters and the profile facts.
    let mut text = BufWriter::new(
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&receipt)?,
    );
    writeln!(text, "LayerFS memory ledger - Stages 3-4 candidate")?;
    writeln!(
        text,
        "profile: dev (debug) test build; LAYERFS_CONSTRUCTION_WORKERS={}",
        std::env::var("LAYERFS_CONSTRUCTION_WORKERS").unwrap_or_else(|_| "unset".into())
    )?;
    writeln!(
        text,
        "fixture: chunked {} bytes of deterministic noise; pooled {} leaves x {} rows = {} values",
        chunked_bytes.len(),
        leaves,
        rows,
        pooled_values.len()
    )?;
    writeln!(
        text,
        "construction cutoff: {}",
        policy.small_file_threshold_bytes()
    )?;
    writeln!(
        text,
        "built objects: {emitted}; pooled inserted rows: {inserted}; chunked root read: {chunked_read_bytes} bytes; pooled leaf read: {read_bytes} bytes"
    )?;
    writeln!(
        text,
        "pool_index_entries: {index_entries}; pool_index_bytes: {index_bytes}"
    )?;
    writeln!(
        text,
        "candidate_index_bytes: {candidate_bytes} (fixed declared cache, reported live by the operation that held it)"
    )?;
    writeln!(text, "sqlite profile: journal_mode=MEMORY synchronous=OFF temp_store=MEMORY foreign_keys=ON busy_timeout=0; cache_size and mmap_size are the host library's defaults (no pragma is set); the engine maxima are environment-dependent because the build links the system libsqlite3")?;
    writeln!(text, "rss: sampled with `ps -o rss= -p <pid>` at phase boundaries only; a sample is a whole-process figure, so it is a lower bound on the phase peak and never a phase-local heap number")?;
    writeln!(text, "lifetime_high_water: null - reason: ru_maxrss needs a platform interface (libc) this batch does not add as a dependency; the sampled figures above are the alternative and their window is stated")?;
    writeln!(text)?;
    writeln!(
        text,
        "{:<28} {:>10} {:>10} {:>10} {:>12} {:>12} {:>12}",
        "phase", "current", "peak", "allocs", "charged", "rss_before", "rss_after"
    )?;
    for phase in &phases {
        writeln!(
            text,
            "{:<28} {:>10} {:>10} {:>10} {:>12} {:>12} {:>12}",
            phase.label,
            phase.current,
            phase.peak,
            phase.allocations,
            phase.charged,
            phase
                .rss_before
                .map_or("null".to_string(), |value| value.to_string()),
            phase
                .rss_after
                .map_or("null".to_string(), |value| value.to_string()),
        )?;
    }
    text.flush()?;

    let mut json = BufWriter::new(
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output.join("memory-ledger.json"))?,
    );
    writeln!(json, "{{")?;
    writeln!(json, "  \"profile\": \"dev\",")?;
    writeln!(json, "  \"chunked_bytes\": {},", chunked_bytes.len())?;
    writeln!(json, "  \"pooled_leaves\": {leaves},")?;
    writeln!(json, "  \"pooled_rows\": {rows},")?;
    writeln!(json, "  \"pool_index_entries\": {index_entries},")?;
    writeln!(json, "  \"pool_index_bytes\": {index_bytes},")?;
    writeln!(json, "  \"lifetime_high_water\": null,")?;
    writeln!(json, "  \"phases\": [")?;
    for (index, phase) in phases.iter().enumerate() {
        let comma = if index + 1 == phases.len() { "" } else { "," };
        writeln!(
            json,
            "    {{\"phase\": \"{}\", \"current\": {}, \"peak\": {}, \"allocs\": {}, \"charged\": {}, \"rss_before\": {}, \"rss_after\": {}}}{comma}",
            phase.label,
            phase.current,
            phase.peak,
            phase.allocations,
            phase.charged,
            phase.rss_before.map_or("null".to_string(), |value| value.to_string()),
            phase.rss_after.map_or("null".to_string(), |value| value.to_string()),
        )?;
    }
    writeln!(json, "  ]")?;
    writeln!(json, "}}")?;
    json.flush()?;

    print!("{}", std::fs::read_to_string(&receipt)?);
    Ok(())
}
