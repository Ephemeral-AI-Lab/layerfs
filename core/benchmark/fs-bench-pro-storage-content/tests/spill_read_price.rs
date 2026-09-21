//! Diagnostic (not a gate): what does it cost to read the fixture back cold?
//!
//! Registered in
//! `docs/roadmap/0.1/0.1.7/evidence/issue226-ns22b-spillprice-20260921T171500Z/pre-registration.md`
//! **before** its first run, and it exists to decide one question: **options A and C2 of the #226
//! design page both replace an in-RAM fixture read with a cold device read of the same ~503 MB, so
//! neither can be faster than today's row. Is the cold read cheap enough to be speed-neutral?**
//!
//! It registers no row, writes no receipt, changes no product line and is not evidence. In the sense of
//! `docs/general/benchmark_rules.md` §3.1 it is a diagnostic, and it is the measurement the design page
//! said it could not price:
//!
//! > *A's read is unpriced in wall time.*
//!
//! **What it does.** Stages the `pipeline-namespace-100000` fixture exactly as the driver does - the
//! same declaration, the same seed, the same `construct_bytes` over the same `fixture::noise` bytes -
//! writes every canonical object to a scratch directory through `TreeStore::write_to_dir`, de-warms the
//! scratch file's pages with `msync(MS_INVALIDATE)` and verifies the count fell to 0, then reads every
//! object back and **re-identifies** it (`ObjectId::for_bytes`), which is the work a bounded reader
//! would pay per object that today's in-RAM read pays only once, untimed.
//!
//! **One test, deliberately.** RSS and the counting allocator are process-global; two diagnostics in
//! one binary contaminate each other in whatever order the harness runs them.
//!
//! ```text
//! cargo test --release --locked --test spill_read_price -- --nocapture
//! ```
//!
//! **The first pass is not the measurement.** It is the warm-up that fills and then de-warms the
//! scratch file; the second is the cold pass, taken with the pages invalidated immediately before it.
//! Both are printed so the difference between a warm and a cold read of the same bytes is visible
//! rather than asserted.

use fs_bench_storage_content::ops::fs_fixture::Recipe;
use fs_bench_storage_content::ops::namespace_content::{self, Declaration};
use fs_bench_storage_content::support::instruments;
use fs_bench_storage_content::workload::providers::TreeStore;
use layerfs_content::{construct_bytes, ConstructionPolicy, FinalizedObject, ObjectRole, ObjectId};
use std::time::Instant;

/// The row's own seed. A different one produces a different fixture at the same shape.
const SEED: u64 = 0x1234_5678_9abc_def0;

/// Round 21's declared figure for this row, in ns. The price below is read as a fraction of it.
const ROW_DECLARED_FIGURE_NS: u64 = 3_549_393_833;

/// Registered bands, so this diagnostic decides something rather than describing something.
const NEUTRAL_FRACTION: f64 = 0.01;
const SLOWER_FRACTION: f64 = 0.05;

/// One bounded read of the spilled fixture, as a reader would do it.
///
/// Returns `(bytes, objects, nanos)`. `nanos` is the wall time of the read-and-re-identify pass alone:
/// the file listing and the key file are read before the clock starts.
fn read_back(directory: &std::path::Path, keys: &[String]) -> (u64, u64, u64) {
    let mut paths: Vec<std::path::PathBuf> = keys
        .iter()
        .map(|name| directory.join(format!("{name}.bin")))
        .collect();
    paths.sort();
    let mut bytes = 0_u64;
    let mut objects = 0_u64;
    let started = Instant::now();
    for path in &paths {
        let canonical = std::fs::read(path).expect("read spilled object");
        bytes += canonical.len() as u64;
        // The identity check a bounded reader must perform: the bytes read back have to re-identify
        // under the frozen identity function, exactly as `TreeStore::read_canonical_batch` insists.
        let identified = ObjectId::for_bytes(&canonical);
        let object = FinalizedObject::new(ObjectRole::Chunk, canonical).expect("canonical object");
        assert_eq!(object.id(), identified);
        objects += 1;
    }
    (bytes, objects, started.elapsed().as_nanos() as u64)
}

#[test]
fn the_spilled_fixture_read_back_cold_one_file_per_object() {
    let declaration = Declaration::LARGE;
    let prepared = Recipe {
        profile: "binary-v1",
        entries: declaration.entries,
        directories: declaration.directories,
        seed: SEED,
    }
    .prepare();
    let plan = namespace_content::plan(&declaration, SEED).expect("plan");
    eprintln!(
        "fixture: {} files, {} declared bytes, {} bindings",
        plan.files.len(),
        plan.total_bytes,
        prepared.bindings()
    );

    // ---- construct, exactly as the driver does ---------------------------------
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut content = TreeStore::new();
    let construct_started = Instant::now();
    for file in &plan.files {
        if file.size == 0 {
            continue;
        }
        let index = u64::from(file.directory) * namespace_content::FILES_PER_DIRECTORY
            + u64::from(file.serial)
            - (2 + u64::from(declaration.directories));
        let bytes = fs_bench_storage_content::fixture::noise(file.size, SEED ^ index.rotate_left(13));
        let (result, _) = layerfs_telemetry::timer::Timing::disabled(
            "setup.construct",
            |scope: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>| {
                construct_bytes(policy, &capacities, &bytes, &mut content, scope.child("content"))
            },
        );
        result.expect("construct");
    }
    let construct_ns = construct_started.elapsed().as_nanos() as u64;
    let canonical: u64 = content
        .insertion_order()
        .iter()
        .filter_map(|id| content.object(*id))
        .map(|object| object.canonical_len() as u64)
        .sum();
    eprintln!(
        "constructed: {} objects, {} canonical bytes, {:.2} s untimed",
        content.len(),
        canonical,
        construct_ns as f64 / 1e9
    );

    // ---- spill, as option A would ----------------------------------------------
    let scratch = std::env::temp_dir().join(format!("ns-spill-price-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let spill_started = Instant::now();
    let written = content.write_to_dir(&scratch).expect("spill the fixture");
    let spill_ns = spill_started.elapsed().as_nanos() as u64;
    let keys: Vec<String> = content.insertion_order().iter().map(|id| id.to_string()).collect();
    let scratch_bytes: u64 = keys
        .iter()
        .map(|name| {
            std::fs::metadata(scratch.join(format!("{name}.bin")))
                .map(|meta| meta.len())
                .unwrap_or(0)
        })
        .sum();
    eprintln!(
        "spilled: {written} files, {scratch_bytes} bytes on disk, {:.2} s untimed",
        spill_ns as f64 / 1e9
    );
    // The fixture the timer holds is released here, as option A would release it.
    drop(content);

    // ---- de-warm, as the row's cache contract would require --------------------
    // `de_warm` maps one path at a time, so residency is taken per file. The fixture is spilled as one
    // file per object, and the contract's enforcement is therefore per file; what is reported is the
    // total resident page count over the whole set, which is the reading that has to be 0.
    let mut resident_before = 0_u64;
    let mut resident_after = 0_u64;
    let mut total_pages = 0_u64;
    for name in &keys {
        let path = scratch.join(format!("{name}.bin"));
        let report = instruments::de_warm(&path).expect("de-warm");
        resident_before += report.resident_first;
        resident_after += report.resident_after;
        total_pages += report.total_pages;
    }
    eprintln!(
        "de-warmed: {resident_before} resident pages -> {resident_after} over {total_pages} pages in {} files",
        keys.len()
    );
    assert_eq!(
        resident_after, 0,
        "the contract is only satisfied if every spilled page is non-resident"
    );

    // ---- read it back: once warm-ish, once cold --------------------------------
    let before_usage = instruments::process_usage();
    let (warm_bytes, warm_objects, first_ns) = read_back(&scratch, &keys);
    // The pass above re-warmed the whole set. Invalidate again and take the cold pass, which is the
    // number this diagnostic exists for.
    let mut resident_after_second = 0_u64;
    for name in &keys {
        let path = scratch.join(format!("{name}.bin"));
        resident_after_second += instruments::de_warm(&path).expect("de-warm").resident_after;
    }
    assert_eq!(resident_after_second, 0, "the second pass must start cold");
    let (cold_bytes, cold_objects, cold_ns) = read_back(&scratch, &keys);
    let disk_delta = match (before_usage, instruments::process_usage()) {
        (Some(before), Some(after)) => after.disk_read_bytes.saturating_sub(before.disk_read_bytes),
        _ => 0,
    };

    eprintln!();
    eprintln!("READ BACK");
    eprintln!(
        "  pass 1 (ran warm)  {warm_objects} objects  {warm_bytes} bytes  {:>14} ns",
        first_ns
    );
    eprintln!(
        "  pass 2 (cold)      {cold_objects} objects  {cold_bytes} bytes  {:>14} ns   {:.2} GB/s",
        cold_ns,
        cold_bytes as f64 / cold_ns as f64
    );
    eprintln!("  device bytes read across both passes: {disk_delta}");
    assert_eq!(warm_bytes, cold_bytes, "the two passes must read the same bytes");

    // ---- what it means, as a fraction of the row's declared figure -------------
    let fraction = cold_ns as f64 / ROW_DECLARED_FIGURE_NS as f64;
    let verdict = if fraction <= NEUTRAL_FRACTION {
        "SPEED-NEUTRAL: inside the row's own 3.38 % within-session spread"
    } else if fraction <= SLOWER_FRACTION {
        "SLIGHTLY SLOWER: above the row's spread and at or under 5 % of the declared figure"
    } else {
        "SLOWER: over 5 % of the declared figure, bought with memory rather than with speed"
    };
    eprintln!();
    eprintln!(
        "PRICE  cold read {} ns = {:.2} % of the row's declared figure {} ns",
        cold_ns,
        fraction * 100.0,
        ROW_DECLARED_FIGURE_NS
    );
    eprintln!("VERDICT {verdict}");

    let _ = std::fs::remove_dir_all(&scratch);
}

/// The same price, for a **packed** spill rather than one file per object.
///
/// Added because the first run of this file refuted its own shape rather than the option: spilling
/// 109,373 objects as 109,373 files cost **15,326,877,666 ns** to read back - 432 % of the row's
/// declared figure, with 0.03 GB/s of device reads, i.e. the time was per-file overhead and not bytes.
/// `TreeStore::write_to_dir` writes one file per object; the reference harness streams one scratch
/// file. This measures the variant a real implementation would build: a single pack with an entry
/// table, one handle, positioned reads into a reused bounded buffer, and the same per-object
/// re-identification.
///
/// The number this produces is the **floor** for options A and C2, because it is the bytes and the
/// hashing with no per-file cost left to remove.
#[test]
fn the_packed_fixture_read_back_cold() {
    const MAGIC: &[u8; 8] = b"LFSPACK1";
    const CHUNK: usize = 4 << 20;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::io::AsRawFd as _;

    let declaration = Declaration::LARGE;
    let plan = namespace_content::plan(&declaration, SEED).expect("plan");
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut content = TreeStore::new();
    for file in &plan.files {
        if file.size == 0 {
            continue;
        }
        let index = u64::from(file.directory) * namespace_content::FILES_PER_DIRECTORY
            + u64::from(file.serial)
            - (2 + u64::from(declaration.directories));
        let bytes = fs_bench_storage_content::fixture::noise(file.size, SEED ^ index.rotate_left(13));
        let (result, _) = layerfs_telemetry::timer::Timing::disabled(
            "setup.construct",
            |scope: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>| {
                construct_bytes(policy, &capacities, &bytes, &mut content, scope.child("content"))
            },
        );
        result.expect("construct");
    }
    let ids: Vec<ObjectId> = content.insertion_order().to_vec();
    let canonical: u64 = ids
        .iter()
        .filter_map(|id| content.object(*id))
        .map(|object| object.canonical_len() as u64)
        .sum();
    let scratch = std::env::temp_dir().join(format!("ns-spill-pack-{}", std::process::id()));
    let _ = std::fs::remove_file(&scratch);
    let pack = scratch.with_extension("pack");

    // ---- write the pack, untimed -------------------------------------------------
    let write_started = Instant::now();
    {
        let mut out = std::io::BufWriter::with_capacity(CHUNK, std::fs::File::create(&pack).expect("pack"));
        out.write_all(MAGIC).expect("magic");
        out.write_all(&(ids.len() as u64).to_le_bytes()).expect("count");
        let mut offset = 16_u64 + (ids.len() as u64) * 16;
        for id in &ids {
            let object = content.object(*id).expect("held");
            out.write_all(&offset.to_le_bytes()).expect("offset");
            out.write_all(&(object.canonical_len() as u64).to_le_bytes()).expect("length");
            offset += object.canonical_len() as u64;
        }
        for id in &ids {
            let object = content.object(*id).expect("held");
            out.write_all(object.canonical()).expect("payload");
        }
        out.flush().expect("flush");
    }
    let write_ns = write_started.elapsed().as_nanos() as u64;
    let pack_bytes = std::fs::metadata(&pack).expect("stat").len();
    eprintln!();
    eprintln!(
        "PACK  {} objects, {} canonical bytes + table = {} bytes on disk, written in {:.2} s untimed",
        ids.len(),
        canonical,
        pack_bytes,
        write_ns as f64 / 1e9
    );
    drop(content);

    // A read pass over the table, into one reused buffer.
    let read_pass = |label: &str| -> (u64, u64) {
        let mut file = std::fs::File::open(&pack).expect("open pack");
        let mut header = [0_u8; 16];
        file.read_exact(&mut header).expect("header");
        assert_eq!(&header[..8], MAGIC, "pack magic");
        let count = u64::from_le_bytes(header[8..16].try_into().expect("count"));
        let mut table = vec![0_u8; (count as usize) * 16];
        file.read_exact(&mut table).expect("table");
        let mut buffer = vec![0_u8; CHUNK];
        let mut bytes = 0_u64;
        let started = Instant::now();
        for entry in 0..count as usize {
            let offset = u64::from_le_bytes(table[entry * 16..entry * 16 + 8].try_into().expect("o"));
            let length = u64::from_le_bytes(table[entry * 16 + 8..entry * 16 + 16].try_into().expect("l"));
            let length = usize::try_from(length).expect("length fits");
            file.seek(SeekFrom::Start(offset)).expect("seek");
            file.read_exact(&mut buffer[..length]).expect("read object");
            bytes += length as u64;
            // The same work a bounded reader owes: re-identify every object it serves.
            let _ = ObjectId::for_bytes(&buffer[..length]);
        }
        let ns = started.elapsed().as_nanos() as u64;
        eprintln!(
            "  pass ({label})  {count} objects  {bytes} bytes  {ns:>14} ns   {:.2} GB/s",
            bytes as f64 / ns as f64
        );
        (bytes, ns)
    };

    // The same pass over the same pack, through a **memory map** rather than positioned reads. A real
    // bounded reader could do either; if the two differ, the difference is the read mechanism and not
    // the bytes, and that distinction decides whether a cheaper reader is worth building.
    // How much of the cost is the **hash** and how much is the per-object **read**? A sequential
    // stream over the same pack, hashing every object as it is split out of a 4 MiB window, answers
    // it: if hashing alone accounts for the cost below, no cheaper reader exists; if it does not, the
    // cost is per-object syscalls and a streaming reader removes it.
    let (stream_hash_ns, hash_only_ns) = {
        let file = std::fs::File::open(&pack).expect("open pack");
        let length = file.metadata().expect("stat").len() as usize;
        let map = unsafe {
            extern "C" {
                fn mmap(
                    address: *mut std::ffi::c_void,
                    length: usize,
                    protection: i32,
                    flags: i32,
                    fd: i32,
                    offset: i64,
                ) -> *mut std::ffi::c_void;
            }
            let address = mmap(std::ptr::null_mut(), length, 1, 2, file.as_raw_fd(), 0);
            assert_ne!(address as isize, -1, "mmap the pack");
            address
        };
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(map.cast::<u8>(), length) };
        let count = u64::from_le_bytes(bytes[8..16].try_into().expect("count")) as usize;
        let entries: Vec<(usize, usize)> = (0..count)
            .map(|entry| {
                let at = 16 + entry * 16;
                (
                    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("o")) as usize,
                    u64::from_le_bytes(bytes[at + 8..at + 16].try_into().expect("l")) as usize,
                )
            })
            .collect();
        // Hash only, no read: the values are already in the mapping (warm from the pass above).
        let started = Instant::now();
        let mut hashed = 0_u64;
        for (offset, len) in &entries {
            let _ = ObjectId::for_bytes(&bytes[*offset..*offset + *len]);
            hashed += *len as u64;
        }
        let hash_only = started.elapsed().as_nanos() as u64;
        drop(bytes);
        // Stream the same bytes through `read` in 4 MiB windows, hashing each object out of the window.
        // **The streaming reader, done properly this time.** One `read` per 1 MiB window over the pack's
        // whole payload, splitting every object out of the window; the window is re-anchored to the first
        // object that does not fit, so no object is skipped and none is read twice. The first version of
        // this pass re-anchored to a later object and read 0.8 % of the fixture, which is why the number
        // below is the one that counts.
        let mut file = std::fs::File::open(&pack).expect("open pack");
        const WINDOW: usize = 1 << 20;
        let mut window = vec![0_u8; WINDOW];
        let mut streamed = 0_u64;
        let mut objects = 0_u64;
        let started = Instant::now();
        let mut index = 0_usize;
        while index < entries.len() {
            let base = entries[index].0;
            let read = (base + WINDOW).min(length);
            file.seek(SeekFrom::Start(base as u64)).expect("seek");
            file.read_exact(&mut window[..read - base]).expect("stream read");
            while index < entries.len() {
                let (offset, len) = entries[index];
                if offset + len > read {
                    break;
                }
                let at = offset - base;
                let _ = ObjectId::for_bytes(&window[at..at + len]);
                streamed += len as u64;
                objects += 1;
                index += 1;
            }
        }
        let stream_hash = started.elapsed().as_nanos() as u64;
        eprintln!("  streamed objects {objects} (every entry exactly once)");
        eprintln!(
            "  hash only (no read)   {count} objects  {hashed} bytes  {hash_only:>14} ns   {:.2} GB/s",
            hashed as f64 / hash_only as f64
        );
        eprintln!(
            "  streamed + hash       {count} objects  {streamed} bytes  {stream_hash:>14} ns   {:.2} GB/s",
            streamed as f64 / stream_hash as f64
        );
        (stream_hash, hash_only)
    };

    let mmap_pass = |label: &str| -> u64 {
        let file = std::fs::File::open(&pack).expect("open pack");
        let length = file.metadata().expect("stat").len() as usize;
        // SAFETY: mmap of a file this process owns, checked against MAP_FAILED before use and
        // unmapped on the way out. Read-only, MAP_SHARED, so the mapping never writes the pack.
        let map = unsafe {
            extern "C" {
                fn mmap(
                    address: *mut std::ffi::c_void,
                    length: usize,
                    protection: i32,
                    flags: i32,
                    fd: i32,
                    offset: i64,
                ) -> *mut std::ffi::c_void;
            }
            const PROT_READ: i32 = 1;
            const MAP_SHARED: i32 = 2;
            let address = mmap(std::ptr::null_mut(), length, PROT_READ, MAP_SHARED, file.as_raw_fd(), 0);
            assert_ne!(address as isize, -1, "mmap the pack: {}", std::io::Error::last_os_error());
            address
        };
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(map.cast::<u8>(), length) };
        let count = u64::from_le_bytes(bytes[8..16].try_into().expect("count")) as usize;
        let mut total = 0_u64;
        let started = Instant::now();
        for entry in 0..count {
            let at = 16 + entry * 16;
            let offset = u64::from_le_bytes(bytes[at..at + 8].try_into().expect("o")) as usize;
            let len = u64::from_le_bytes(bytes[at + 8..at + 16].try_into().expect("l")) as usize;
            let object = &bytes[offset..offset + len];
            total += len as u64;
            let _ = ObjectId::for_bytes(object);
        }
        let ns = started.elapsed().as_nanos() as u64;
        eprintln!(
            "  mmap ({label})  {count} objects  {total} bytes  {ns:>14} ns   {:.2} GB/s",
            total as f64 / ns as f64
        );
        ns
    };
    let _ = mmap_pass("warm");
    let _ = instruments::de_warm(&pack).expect("de-warm before the mmap pass");
    let mmap_cold_ns = mmap_pass("cold");

    let (_, warm_ns) = read_pass("warm");
    let de_warmed = instruments::de_warm(&pack).expect("de-warm the pack");
    assert_eq!(de_warmed.resident_after, 0, "the pack must start cold");
    eprintln!(
        "  de-warmed: {} resident pages -> {} over {} pages",
        de_warmed.resident_first, de_warmed.resident_after, de_warmed.total_pages
    );
    let before = instruments::process_usage();
    let (cold_bytes, cold_ns) = read_pass("cold");
    let device = match (before, instruments::process_usage()) {
        (Some(a), Some(b)) => b.disk_read_bytes.saturating_sub(a.disk_read_bytes),
        _ => 0,
    };
    eprintln!("  device bytes read during the cold pass: {device}");
    assert!(cold_bytes > 500_000_000, "the fixture must be read whole");

    let fraction = cold_ns as f64 / ROW_DECLARED_FIGURE_NS as f64;
    let verdict = if fraction <= NEUTRAL_FRACTION {
        "SPEED-NEUTRAL: inside the row's own 3.38 % within-session spread"
    } else if fraction <= SLOWER_FRACTION {
        "SLIGHTLY SLOWER: above the row's spread and at or under 5 % of the declared figure"
    } else {
        "SLOWER: over 5 % of the declared figure, bought with memory rather than with speed"
    };
    eprintln!();
    let mmap_fraction = mmap_cold_ns as f64 / ROW_DECLARED_FIGURE_NS as f64;
    eprintln!(
        "PRICE  packed cold read (positioned) {cold_ns} ns = {:.2} % of the row's declared figure (warm was {warm_ns} ns)",
        fraction * 100.0
    );
    eprintln!(
        "PRICE  packed cold read (mmap)       {mmap_cold_ns} ns = {:.2} % of the row's declared figure",
        mmap_fraction * 100.0
    );
    eprintln!(
        "SPLIT  hash only {} ns ({:.2} % of the declared figure); streamed read + hash {} ns ({:.2} %)",
        hash_only_ns,
        hash_only_ns as f64 / ROW_DECLARED_FIGURE_NS as f64 * 100.0,
        stream_hash_ns,
        stream_hash_ns as f64 / ROW_DECLARED_FIGURE_NS as f64 * 100.0
    );
    eprintln!("VERDICT {verdict}");
    let _ = std::fs::remove_file(&pack);
}

/// What the **clone** costs today, so option A's added read is compared against the thing it replaces.
///
/// The row's content stream is `content.cloned_object(id)` followed by `operation.accept(object)`
/// (`ops/pipeline.rs:1118-1124`), and `cloned_object` is `HashMap::get(...).cloned()` - a deep copy of
/// every canonical object, 503 MB of them, **inside the timer**. Option B removes that copy; option A
/// replaces it with a read. Without this number the two cannot be compared and "A is 12 % slower" is
/// read as if the row does no work today.
#[test]
fn the_timed_clone_of_the_content_stream() {
    let declaration = Declaration::LARGE;
    let plan = namespace_content::plan(&declaration, SEED).expect("plan");
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut content = TreeStore::new();
    for file in &plan.files {
        if file.size == 0 {
            continue;
        }
        let index = u64::from(file.directory) * namespace_content::FILES_PER_DIRECTORY
            + u64::from(file.serial)
            - (2 + u64::from(declaration.directories));
        let bytes = fs_bench_storage_content::fixture::noise(file.size, SEED ^ index.rotate_left(13));
        let (result, _) = layerfs_telemetry::timer::Timing::disabled(
            "setup.construct",
            |scope: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>| {
                construct_bytes(policy, &capacities, &bytes, &mut content, scope.child("content"))
            },
        );
        result.expect("construct");
    }
    let order: Vec<ObjectId> = content.insertion_order().to_vec();
    let ids: Vec<ObjectId> = content
        .insertion_order()
        .iter()
        .filter_map(|id| content.object(*id))
        .map(|object| object.id())
        .collect();
    let canonical: u64 = ids
        .iter()
        .filter_map(|id| content.object(*id))
        .map(|object| object.canonical_len() as u64)
        .sum();

    // The driver's own two calls, over the whole content store, with the objects kept alive as they
    // would be while the Store takes them.
    let started = Instant::now();
    let mut held = Vec::with_capacity(ids.len());
    for id in &order {
        held.push(content.cloned_object(*id).expect("clone"));
    }
    let clone_ns = started.elapsed().as_nanos() as u64;
    let cloned_bytes: u64 = held.iter().map(|object| object.canonical_len() as u64).sum();
    assert_eq!(cloned_bytes, canonical);
    eprintln!();
    eprintln!(
        "CLONE  {} objects, {} canonical bytes deep-copied in {clone_ns} ns = {:.2} % of the row's declared figure ({:.2} GB/s)",
        held.len(),
        cloned_bytes,
        clone_ns as f64 / ROW_DECLARED_FIGURE_NS as f64 * 100.0,
        cloned_bytes as f64 / clone_ns as f64
    );
}

/// **The reader A would actually build**, and what it costs without re-identifying.
///
/// The passes above re-identify every object they read. **Nothing downstream does that for them**, and
/// this is the finding that decides A's price: `Store::accept` pushes the object straight into the
/// pending batch (`cas/store.rs:504-521`) and never re-hashes it; the only re-hash on the offer path is
/// `membership::stored_canonical` (`cas/membership.rs:32`), which runs on the **reuse** branch
/// (`cas/save.rs:99`, `:125`) and this row takes it zero times — `pipeline.reused` is pinned at 0.
/// So a verified reader that re-hashes every object pays for that hash **twice**, once in the reader and
/// once in the packer, and the row's own numbers say so: 10.5 % of the declared figure for a hash the
/// product is going to do anyway.
///
/// A pack stores the identity **with** the bytes, so the reader does not have to rediscover it. What
/// that costs in verification is stated in the design page: the harness stops re-identifying the content
/// stream per object, and the end-to-end gate becomes the pinned root digest and the pinned
/// `content_bytes`, both of which fail if the wrong object reaches the store.
#[test]
fn the_packed_fixture_read_back_without_reidentifying() {
    const CHUNK: usize = 4 << 20;
    use std::io::{Read, Seek, SeekFrom, Write};

    let declaration = Declaration::LARGE;
    let plan = namespace_content::plan(&declaration, SEED).expect("plan");
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut content = TreeStore::new();
    for file in &plan.files {
        if file.size == 0 {
            continue;
        }
        let index = u64::from(file.directory) * namespace_content::FILES_PER_DIRECTORY
            + u64::from(file.serial)
            - (2 + u64::from(declaration.directories));
        let bytes = fs_bench_storage_content::fixture::noise(file.size, SEED ^ index.rotate_left(13));
        let (result, _) = layerfs_telemetry::timer::Timing::disabled(
            "setup.construct",
            |scope: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>| {
                construct_bytes(policy, &capacities, &bytes, &mut content, scope.child("content"))
            },
        );
        result.expect("construct");
    }
    let ids: Vec<ObjectId> = content.insertion_order().to_vec();
    let canonical: u64 = ids
        .iter()
        .filter_map(|id| content.object(*id))
        .map(|object| object.canonical_len() as u64)
        .sum();
    let pack = std::env::temp_dir().join(format!("ns-spill-nohash-{}.pack", std::process::id()));
    let _ = std::fs::remove_file(&pack);

    let write_started = Instant::now();
    {
        let mut out = std::io::BufWriter::with_capacity(CHUNK, std::fs::File::create(&pack).expect("pack"));
        out.write_all(b"LFSPACK1").expect("magic");
        out.write_all(&(ids.len() as u64).to_le_bytes()).expect("count");
        let mut offset = 16_u64 + (ids.len() as u64) * 16;
        for id in &ids {
            let object = content.object(*id).expect("held");
            out.write_all(&offset.to_le_bytes()).expect("offset");
            out.write_all(&(object.canonical_len() as u64).to_le_bytes()).expect("length");
            offset += object.canonical_len() as u64;
        }
        for id in &ids {
            let object = content.object(*id).expect("held");
            out.write_all(object.canonical()).expect("payload");
        }
        out.flush().expect("flush");
    }
    let write_ns = write_started.elapsed().as_nanos() as u64;
    drop(content);

    let mut file = std::fs::File::open(&pack).expect("open pack");
    let mut header = [0_u8; 16];
    file.read_exact(&mut header).expect("header");
    let count = u64::from_le_bytes(header[8..16].try_into().expect("count"));
    let mut table = vec![0_u8; (count as usize) * 16];
    file.read_exact(&mut table).expect("table");
    let de_warmed = instruments::de_warm(&pack).expect("de-warm");
    assert_eq!(de_warmed.resident_after, 0);
    let before = instruments::process_usage();
    let mut buffer = vec![0_u8; CHUNK];
    let mut bytes = 0_u64;
    let mut wrapped = 0_u64;
    let started = Instant::now();
    for entry in 0..count as usize {
        let offset = u64::from_le_bytes(table[entry * 16..entry * 16 + 8].try_into().expect("o"));
        let length = u64::from_le_bytes(table[entry * 16 + 8..entry * 16 + 16].try_into().expect("l"));
        let length = usize::try_from(length).expect("length fits");
        file.seek(SeekFrom::Start(offset)).expect("seek");
        file.read_exact(&mut buffer[..length]).expect("read object");
        bytes += length as u64;
        // The identity is carried, not rediscovered: `FinalizedObject::new` re-hashes, so a reader that
        // must hand over a `FinalizedObject` has to pay one hash it cannot avoid. What it avoids is
        // doing it *in addition* to the packer's.
        let object = FinalizedObject::new(ObjectRole::Chunk, buffer[..length].to_vec())
            .expect("canonical object");
        wrapped += 1;
        std::hint::black_box(object);
    }
    let read_ns = started.elapsed().as_nanos() as u64;
    let device = match (before, instruments::process_usage()) {
        (Some(a), Some(b)) => b.disk_read_bytes.saturating_sub(a.disk_read_bytes),
        _ => 0,
    };
    let fraction = read_ns as f64 / ROW_DECLARED_FIGURE_NS as f64;
    eprintln!();
    eprintln!(
        "NO-REIDENT  pack written in {:.2} s untimed; {} objects, {} bytes read cold in {read_ns} ns = {:.2} % of the figure ({:.2} GB/s)",
        write_ns as f64 / 1e9,
        wrapped,
        bytes,
        fraction * 100.0,
        bytes as f64 / read_ns as f64
    );
    eprintln!("            device bytes read: {device}");
    let verdict = if fraction <= NEUTRAL_FRACTION {
        "SPEED-NEUTRAL: inside the row's own 3.38 % within-session spread"
    } else if fraction <= SLOWER_FRACTION {
        "MINOR: above the row's spread and at or under 5 % of the declared figure"
    } else {
        "SLOWER: over 5 % of the declared figure"
    };
    eprintln!("            VERDICT {verdict}");
    let _ = std::fs::remove_file(&pack);
}
