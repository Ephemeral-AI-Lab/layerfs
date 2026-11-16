//! AC-57 section 13 capacity arm: a real, large SQLite Store compaction measurement.
//!
//! The acceptance matrix's 50 GB arms were extrapolated from the 28.2 ms tiny
//! fixture. This opt-in test builds a Store of the requested size through the
//! public persistence/storage APIs, measures the `none` path at ~9% wholly dead
//! bytes and the `rebuild` path at ~10%, and records wall time, the section 11
//! receipt values and the T10.3 phase durations.
//!
//! It is ignored by default: `cargo test` never allocates more than a few MiB.
//! Run it with a TMPDIR that has at least 3x the target bytes free:
//!
//! ```text
//! CAPACITY_MEASURE=1 TMPDIR=/var/tmp/g61 UPDATE_CAPACITY=1 \
//!   cargo test -p layerfs-persistence --release \
//!   --test sqlite_compact_capacity -- --ignored --nocapture
//! ```
//!
//! Knobs: `CAPACITY_GROUPS` (live inode leaves, default 701),
//! `CAPACITY_GROUP` (files per inode leaf, default 99; the page cap is 100 rows including the root directory),
//! `CAPACITY_DEAD_OBJECTS` (initial dead objects, default 6770),
//! `CAPACITY_OBJECT_BYTES` (default 131071, the largest whole-file
//! payload), `CAPACITY_RUNS` (default 3 per arm), `CAPACITY_DURABLE=1` (adds
//! one WAL/FULL rebuild after the WAL/OFF runs), `CAPACITY_KEEP=1` (keeps the
//! temporary Store) and `CAPACITY_OUT` (fixture path override).
//!
//! Fixture shape: every live file is the largest whole-file object (131071 B)
//! reached through one `InodeLeaf` of `CAPACITY_GROUP` file entries and one
//! `FilesystemRoot`, whose `LayerStack` genesis publishes it. The dead payload
//! is whole-file objects no retained root reaches. Every pack holds one object,
//! so `SELECT count(*), sum(length) FROM pack` gives the exact pack byte total
//! and the section 11 `deadBytesBound` can be checked against it. Production
//! saves would add a directory namespace over these inodes; that changes the
//! object count a little, not the bytes compaction copies.
#![cfg(target_os = "linux")]

mod support;

use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::directory::{encode_directory_page, DirectoryPage};
use layerfs_content::filesystem::inode::{encode_inode_page, InodePage};
use layerfs_content::filesystem::root::profile_id;
use layerfs_content::filesystem::{scope_for_seed, FilesystemRoot};
use layerfs_content::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    HistoryCatalog, HistoryCatalogConfig, HistoryName, LayerStackId, StackInitialization,
};
use layerfs_persistence::compact_metrics::{CompactMetrics, CompactMetricsCapture};
use layerfs_persistence::{
    CompactPath, CompactRequest, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{Storage, StoragePolicy};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::Temp;

const BINDING: &[u8] = b"sqlite-compact-capacity-measure";
const DEFAULT_GROUPS: u64 = 701;
const DEFAULT_GROUP: u64 = 99;
const DEFAULT_DEAD_OBJECTS: u64 = 6_770;
const DEFAULT_OBJECT_BYTES: usize = 131_071;
const BATCH: usize = 100;

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn profile_label(profile: SqlitePersistenceProfile) -> &'static str {
    match profile {
        SqlitePersistenceProfile::Durable => "SqlitePersistenceProfile::Durable (WAL/FULL)",
        SqlitePersistenceProfile::Disposable => "SqlitePersistenceProfile::Disposable (WAL/OFF)",
    }
}

fn create_store(path: &Path, profile: SqlitePersistenceProfile) -> SqliteHandles {
    SqliteHandles::create(
        PersistenceConfig::sqlite(path.to_path_buf()).with_sqlite_profile(profile),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .expect("Store creation")
}

fn reopen(path: &Path, profile: SqlitePersistenceProfile) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path.to_path_buf()).with_sqlite_profile(profile),
        BINDING,
        [71; 32],
    )
    .expect("Store reopen")
}

/// Deterministic payload; the seed makes every object distinct. The first eight
/// bytes carry the object index so two seeds cannot collide by accident.
fn payload(seed: u64, len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    out[..8].copy_from_slice(&seed.to_le_bytes());
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    for chunk in out[8..].chunks_exact_mut(8) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        chunk.copy_from_slice(&state.to_le_bytes());
    }
    out
}

fn whole_file(bytes: &[u8]) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).expect("whole-file payload"),
    )
    .expect("finalized object")
}

/// Saves a batch of already-finalized objects in one Save.
fn save_batch(handles: &SqliteHandles, objects: Vec<FinalizedObject>) -> Vec<ObjectId> {
    let storage = Storage::new(handles.storage.clone()).expect("storage");
    let save = storage.begin_save().expect("save");
    let mut ids = Vec::with_capacity(objects.len());
    for object in objects {
        ids.push(object.id());
        save.accept(object).expect("accept");
    }
    save.finish().expect("save finish");
    ids
}

fn save_whole_files(handles: &SqliteHandles, payloads: &[Vec<u8>]) -> Vec<ObjectId> {
    let objects = payloads.iter().map(|bytes| whole_file(bytes)).collect();
    save_batch(handles, objects)
}

/// Publishes one `InodeLeaf` of whole-file content roots and its
/// `FilesystemRoot` as the genesis of its own LayerStack.
fn publish_group(handles: &SqliteHandles, index: u64, roots: &[ObjectId]) {
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: Vec::new(),
        })
        .expect("directory"),
    )
    .expect("directory leaf");
    let directory_id = directory.id();
    let mut entries = Vec::with_capacity(roots.len() + 1);
    entries.push((
        1,
        InodeValue {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: directory_id,
            metadata_root: InodeMetadata::Inline(PortableMetadata {
                mode: 0o755,
                mtime_seconds: i64::try_from(index).unwrap_or(0),
                mtime_nanoseconds: 0,
            }),
        },
    ));
    for (offset, root) in roots.iter().enumerate() {
        entries.push((
            u64::try_from(offset + 2).expect("inode serial"),
            InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: *root,
                metadata_root: InodeMetadata::Inline(PortableMetadata {
                    mode: 0o644,
                    mtime_seconds: i64::try_from(index).unwrap_or(0),
                    mtime_nanoseconds: 0,
                }),
            },
        ));
    }
    let mut references = Vec::with_capacity(roots.len() + 1);
    references.push(directory_id);
    references.extend_from_slice(roots);
    let leaf = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf { entries }).expect("inode page"),
    )
    .expect("inode leaf")
    .with_references(references);
    let leaf_id = leaf.id();
    let root = FilesystemRoot::new(profile_id(), scope_for_seed([1; 32]), 1, leaf_id)
        .expect("filesystem root");
    let root_object = FinalizedObject::new(
        ObjectRole::FilesystemRoot,
        root.encode().expect("root encode"),
    )
    .expect("filesystem root object")
    .with_references(vec![leaf_id]);
    let root_id = root_object.id();
    save_batch(handles, vec![directory, leaf, root_object]);

    let mut stack = [0u8; 16];
    stack[..8].copy_from_slice(&index.to_le_bytes());
    stack[8..].copy_from_slice(b"g61cap00");
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            stack: LayerStackId::from_authority(stack),
            name: HistoryName::new(&format!("capacity-{index:07}")).expect("name"),
            scope: scope_for_seed([1; 32]).object(),
            profile: profile_id(),
            genesis_root: root_id,
            logical: 0,
        })
        .expect("layerstack initialization");
}

#[derive(Clone, Copy, Debug, Default)]
struct PackStats {
    count: u64,
    total: u64,
    min: u64,
    max: u64,
}

/// Reads the exact pack population straight from the Store file. The read-only
/// connection is closed before any compaction starts.
fn pack_stats(path: &Path) -> PackStats {
    let open = |flags| rusqlite::Connection::open_with_flags(path, flags);
    let connection = open(rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .or_else(|_| open(rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE))
        .expect("Store probe");
    let row: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT count(*), COALESCE(sum(length),0), COALESCE(min(length),0), \
             COALESCE(max(length),0) FROM pack",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("pack statistics");
    PackStats {
        count: u64::try_from(row.0).unwrap_or(0),
        total: u64::try_from(row.1).unwrap_or(0),
        min: u64::try_from(row.2).unwrap_or(0),
        max: u64::try_from(row.3).unwrap_or(0),
    }
}

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

#[derive(Clone, Debug)]
struct Measured {
    micros: u64,
    path: &'static str,
    generation_before: u64,
    generation_after: u64,
    dead_bytes_bound: u64,
    dead_bytes_refined: u64,
    physical_shrink: u64,
    peak_temp_bytes: u64,
    old_file_bytes: u64,
    new_file_bytes: u64,
    added_objects: u64,
    pack_before: PackStats,
    pack_after: PackStats,
    phases_nanos: Vec<(&'static str, u64)>,
}

fn measured(path: &Path, profile: SqlitePersistenceProfile, op: u8, added: u64) -> Measured {
    let handles = reopen(path, profile);
    let pack_before = pack_stats(path);
    let old_file_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let capture = CompactMetricsCapture::start();
    let started = Instant::now();
    let receipt = handles.compact(&request(op)).expect("compaction");
    let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    let metrics: CompactMetrics = capture.finish();
    let new_file_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let pack_after = pack_stats(path);
    Measured {
        micros,
        path: match receipt.path {
            CompactPath::None => "none",
            CompactPath::Rebuild => "rebuild",
        },
        generation_before: receipt.generation_before,
        generation_after: receipt.generation_after,
        dead_bytes_bound: receipt.dead_bytes_bound,
        dead_bytes_refined: receipt.dead_bytes_refined,
        physical_shrink: receipt.physical_shrink,
        peak_temp_bytes: receipt.peak_temp_bytes,
        old_file_bytes,
        new_file_bytes,
        added_objects: added,
        pack_before,
        pack_after,
        phases_nanos: phase_nanos(&metrics),
    }
}

fn phase_nanos(metrics: &CompactMetrics) -> Vec<(&'static str, u64)> {
    let mut phases: Vec<(&'static str, u64)> = metrics
        .samples()
        .into_iter()
        .filter(|sample| sample.name == "compact.phase.duration_ns")
        .filter(|sample| sample.label("statistic") == Some("sum"))
        .filter_map(|sample| Some((sample.label("phase")?, sample.value)))
        .collect();
    phases.sort_by_key(|(phase, _)| *phase);
    phases
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn json_u64_array(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn phase_value(run: &Measured, phase: &str) -> u64 {
    run.phases_nanos
        .iter()
        .find(|(name, _)| *name == phase)
        .map(|(_, value)| *value)
        .unwrap_or(0)
}

/// One arm as a JSON object, with the per-run values and the median/range.
fn json_arm(name: &str, runs: &[Measured]) -> String {
    let micros: Vec<u64> = runs.iter().map(|run| run.micros).collect();
    let old: Vec<u64> = runs.iter().map(|run| run.old_file_bytes).collect();
    let new: Vec<u64> = runs.iter().map(|run| run.new_file_bytes).collect();
    let dead: Vec<u64> = runs.iter().map(|run| run.dead_bytes_bound).collect();
    let refined: Vec<u64> = runs.iter().map(|run| run.dead_bytes_refined).collect();
    let shrink: Vec<u64> = runs.iter().map(|run| run.physical_shrink).collect();
    let peak: Vec<u64> = runs.iter().map(|run| run.peak_temp_bytes).collect();
    let generation_before: Vec<u64> = runs.iter().map(|run| run.generation_before).collect();
    let generation_after: Vec<u64> = runs.iter().map(|run| run.generation_after).collect();
    let packs_before: Vec<u64> = runs.iter().map(|run| run.pack_before.count).collect();
    let packs_after: Vec<u64> = runs.iter().map(|run| run.pack_after.count).collect();
    let added: Vec<u64> = runs.iter().map(|run| run.added_objects).collect();
    let phases = ["mark", "build", "verify", "install"]
        .iter()
        .map(|phase| {
            let values: Vec<u64> = runs.iter().map(|run| phase_value(run, phase)).collect();
            format!("\n      \"{phase}\": [{}]", json_u64_array(&values))
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "\n  \"{name}\": {{\n    \"path\": \"{}\",\n    \"runs\": {},\n    \"micros\": [{}],\n    \
         \"medianMicros\": {},\n    \"minMicros\": {},\n    \"maxMicros\": {},\n    \
         \"deadBytesBound\": [{}],\n    \"deadBytesRefined\": [{}],\n    \
         \"physicalShrink\": [{}],\n    \"peakTempBytes\": [{}],\n    \
         \"oldFileBytes\": [{}],\n    \"newFileBytes\": [{}],\n    \
         \"generationBefore\": [{}],\n    \"generationAfter\": [{}],\n    \
         \"packCountBefore\": [{}],\n    \"packCountAfter\": [{}],\n    \
         \"addedDeadObjects\": [{}],\n    \"phasesNanos\": {{{}\n    }}\n  }}",
        runs[0].path,
        runs.len(),
        json_u64_array(&micros),
        median(&micros),
        micros.iter().min().copied().unwrap_or(0),
        micros.iter().max().copied().unwrap_or(0),
        json_u64_array(&dead),
        json_u64_array(&refined),
        json_u64_array(&shrink),
        json_u64_array(&peak),
        json_u64_array(&old),
        json_u64_array(&new),
        json_u64_array(&generation_before),
        json_u64_array(&generation_after),
        json_u64_array(&packs_before),
        json_u64_array(&packs_after),
        json_u64_array(&added),
        phases,
    )
}

fn rustc_version() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn cpus() -> u64 {
    std::thread::available_parallelism()
        .map(|value| u64::try_from(value.get()).unwrap_or(0))
        .unwrap_or(0)
}

fn mem_bytes() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let rest = line.strip_prefix("MemTotal:")?;
                let kib: u64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
                Some(kib.saturating_mul(1024))
            })
        })
        .unwrap_or(0)
}

fn update_fixture(path: &Path, json: &str) {
    std::fs::write(path, json.as_bytes()).expect("fixture write");
}

/// Saves `count` dead whole-file objects in batches and returns how many it added.
fn add_dead(handles: &SqliteHandles, count: u64, seed_base: u64, object_bytes: usize) {
    let mut left = count;
    let mut seed = seed_base;
    while left > 0 {
        let take = left.min(BATCH as u64);
        let payloads: Vec<Vec<u8>> = (0..take)
            .map(|offset| payload(seed + offset, object_bytes))
            .collect();
        let _ = save_whole_files(handles, &payloads);
        seed += take;
        left -= take;
    }
}

/// Adds the smallest number of whole-file dead objects that makes the pack-byte
/// dead share reach 10%. Returns `(dead_bytes, objects_added, next_seed)`.
fn cross_ten(
    store: &Path,
    profile: SqlitePersistenceProfile,
    dead_bytes: u64,
    mut seed: u64,
    object_bytes: usize,
) -> (u64, u64, u64) {
    // One object is one singleton pack: measure its width instead of assuming
    // the pack framing.
    let before = pack_stats(store);
    {
        let handles = reopen(store, profile);
        add_dead(&handles, 1, seed, object_bytes);
        drop(handles);
    }
    seed += 1;
    let after = pack_stats(store);
    let unit = after.total.saturating_sub(before.total);
    let mut dead = dead_bytes.saturating_add(unit);
    let mut added = 1u64;
    loop {
        let before = pack_stats(store);
        if before.total > 0 && dead.saturating_mul(10) >= before.total {
            break;
        }
        let need = before
            .total
            .saturating_sub(dead.saturating_mul(10))
            .div_ceil(unit.saturating_mul(9))
            .max(1);
        let handles = reopen(store, profile);
        add_dead(&handles, need, seed, object_bytes);
        drop(handles);
        seed += need;
        added += need;
        let after = pack_stats(store);
        dead = dead.saturating_add(after.total.saturating_sub(before.total));
        if added > 10_000_000 {
            panic!("could not cross the 10% threshold");
        }
    }
    (dead, added, seed)
}

/// The opt-in section 13 capacity measurement. See the module docs.
#[test]
#[ignore = "opt-in 50 GB capacity measurement: set CAPACITY_MEASURE=1 and a large TMPDIR"]
fn capacity_measure() {
    if std::env::var_os("CAPACITY_MEASURE").is_none() {
        eprintln!("capacity_measure: skipped; set CAPACITY_MEASURE=1 to run the section 13 arm");
        return;
    }
    let groups = env_u64("CAPACITY_GROUPS", DEFAULT_GROUPS);
    let group_size = env_u64("CAPACITY_GROUP", DEFAULT_GROUP);
    let initial_dead = env_u64("CAPACITY_DEAD_OBJECTS", DEFAULT_DEAD_OBJECTS);
    let runs_per_arm = env_u64("CAPACITY_RUNS", 3).max(1);
    let object_bytes = usize::try_from(env_u64(
        "CAPACITY_OBJECT_BYTES",
        DEFAULT_OBJECT_BYTES as u64,
    ))
    .expect("object bytes");
    let live_objects = groups.saturating_mul(group_size);
    let durable = std::env::var_os("CAPACITY_DURABLE").is_some();
    let keep = std::env::var_os("CAPACITY_KEEP").is_some();

    let temp = Temp::new("sqlite-compact-capacity");
    let store = temp.join("store.sqlite");
    eprintln!(
        "capacity_measure: {} groups x {} whole-file objects of {} B + {} dead objects under {}",
        groups,
        group_size,
        object_bytes,
        initial_dead,
        temp.path().display()
    );

    // Build the live payload through the public APIs: whole-file objects reached
    // through an inode leaf and a filesystem root, published as a stack genesis.
    let handles = create_store(&store, SqlitePersistenceProfile::Disposable);
    let build_started = Instant::now();
    let mut seed = 0u64;
    for group in 0..groups {
        let payloads: Vec<Vec<u8>> = (0..group_size)
            .map(|offset| payload(seed + offset, object_bytes))
            .collect();
        seed += group_size;
        let roots = save_whole_files(&handles, &payloads);
        publish_group(&handles, group, &roots);
        if group % 50 == 0 || group + 1 == groups {
            let elapsed = build_started.elapsed().as_secs_f64();
            let written = (group + 1) as f64 * group_size as f64 * object_bytes as f64;
            eprintln!(
                "capacity_measure: build live groups {}/{} ({:.2} GB, {:.2} GB/s)",
                group + 1,
                groups,
                written / 1e9,
                written / 1e9 / elapsed.max(1e-9)
            );
            let _ = std::io::stderr().flush();
        }
    }
    add_dead(&handles, initial_dead, 1_000_000_000, object_bytes);
    let fixture_build_seconds = build_started.elapsed().as_secs_f64();
    let built = pack_stats(&store);
    eprintln!(
        "capacity_measure: fixture built in {:.1} s; {} packs, {:.3} GB, pack length {}..{}",
        fixture_build_seconds,
        built.count,
        built.total as f64 / 1e9,
        built.min,
        built.max
    );
    drop(handles);

    // The `none` path at ~9%: mark only, repeated for the E3 median and range.
    let mut none_runs = Vec::new();
    for run in 0..u8::try_from(runs_per_arm).expect("run count") {
        let measured = measured(&store, SqlitePersistenceProfile::Disposable, 90 + run, 0);
        assert_eq!(measured.path, "none", "~9% dead takes the none path");
        eprintln!(
            "capacity_measure: none run {} -> {} us, dead {} B, shrink {} B",
            run + 1,
            measured.micros,
            measured.dead_bytes_bound,
            measured.physical_shrink
        );
        none_runs.push(measured);
    }

    // The `rebuild` path at the 10% threshold, then again after re-adding enough
    // dead packs for the refined decision on each compacted Store.
    let mut rebuild_runs = Vec::new();
    let mut dead_bytes = none_runs[0].dead_bytes_bound;
    let mut dead_seed = 2_000_000_000u64;
    for run in 0..u8::try_from(runs_per_arm).expect("run count") {
        let (_crossed, added, seed) = cross_ten(
            &store,
            SqlitePersistenceProfile::Disposable,
            dead_bytes,
            dead_seed,
            object_bytes,
        );
        dead_seed = seed;
        let measured = measured(
            &store,
            SqlitePersistenceProfile::Disposable,
            100 + run,
            added,
        );
        assert_eq!(measured.path, "rebuild", "10% dead takes the rebuild path");
        eprintln!(
            "capacity_measure: rebuild run {} -> {} us, dead {} B, shrink {} B, peak {} B",
            run + 1,
            measured.micros,
            measured.dead_bytes_bound,
            measured.physical_shrink,
            measured.peak_temp_bytes
        );
        dead_bytes = 0;
        rebuild_runs.push(measured);
    }

    // Optional WAL/FULL sensitivity run: production's default profile.
    let mut durable_run = None;
    if durable {
        let (_crossed, added, _seed) = cross_ten(
            &store,
            SqlitePersistenceProfile::Durable,
            dead_bytes,
            dead_seed,
            object_bytes,
        );
        let measured = measured(&store, SqlitePersistenceProfile::Durable, 120, added);
        assert_eq!(measured.path, "rebuild");
        eprintln!(
            "capacity_measure: durable rebuild -> {} us, dead {} B, shrink {} B, peak {} B",
            measured.micros,
            measured.dead_bytes_bound,
            measured.physical_shrink,
            measured.peak_temp_bytes
        );
        durable_run = Some(measured);
    }

    let none_dead = none_runs[0].dead_bytes_bound;
    let none_total = none_runs[0].pack_before.total;
    let rebuild_dead = rebuild_runs[0].dead_bytes_bound;
    let rebuild_total = rebuild_runs[0].pack_before.total;
    let mut arms = format!(
        "{},{}",
        json_arm("none", &none_runs),
        json_arm("rebuild", &rebuild_runs)
    );
    if let Some(run) = &durable_run {
        arms.push_str(&format!(
            ",\n  \"durableRebuild\": {{\n    \"path\": \"rebuild\",\n    \"runs\": 1,\n    \
             \"micros\": [{}],\n    \"deadBytesBound\": [{}],\n    \
             \"physicalShrink\": [{}],\n    \"peakTempBytes\": [{}],\n    \
             \"addedDeadObjects\": [{}],\n    \"phasesNanos\": {{\n      \"mark\": [{}],\n      \
             \"build\": [{}],\n      \"verify\": [{}],\n      \"install\": [{}]\n    }}\n  }}",
            run.micros,
            run.dead_bytes_bound,
            run.physical_shrink,
            run.peak_temp_bytes,
            run.added_objects,
            phase_value(run, "mark"),
            phase_value(run, "build"),
            phase_value(run, "verify"),
            phase_value(run, "install"),
        ));
    }
    let json = format!(
        "{{\n  \"schema\": \"t12.3a-sqlite-compact-capacity/1\",\n  \
         \"actualBytes\": {},\n  \"actualGib\": \"{:.3}\",\n  \
         \"packCount\": {},\n  \"packLengthMin\": {},\n  \"packLengthMax\": {},\n  \
         \"objectBytes\": {},\n  \"groups\": {},\n  \"groupSize\": {},\n  \
         \"liveObjects\": {},\n  \"initialDeadObjects\": {},\n  \
         \"deadFractionPermille\": {},\n  \"fixtureBuildSeconds\": {:.1},\n  \
         \"noneDeadBytes\": {},\n  \"noneTotalBytes\": {},\n  \
         \"rebuildDeadBytes\": {},\n  \"rebuildTotalBytes\": {},\n  \
         \"profile\": \"{}\",\n  \"platform\": \"{}-{}\",\n  \
         \"rustc\": \"{}\",\n  \"cpus\": {},\n  \"memBytes\": {},{}\n}}\n",
        built.total,
        built.total as f64 / 1e9,
        built.count,
        built.min,
        built.max,
        object_bytes,
        groups,
        group_size,
        live_objects,
        initial_dead,
        none_dead.saturating_mul(1000) / none_total.max(1),
        fixture_build_seconds,
        none_dead,
        none_total,
        rebuild_dead,
        rebuild_total,
        profile_label(SqlitePersistenceProfile::Disposable),
        std::env::consts::OS,
        std::env::consts::ARCH,
        rustc_version(),
        cpus(),
        mem_bytes(),
        arms,
    );
    println!("{json}");
    if std::env::var_os("UPDATE_CAPACITY").is_some() {
        let out = std::env::var_os("CAPACITY_OUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../../../engine/crates/layerfs-persistence/tests/fixtures/\
                 sqlite_compact_capacity_measure.json",
                )
            });
        update_fixture(&out, &json);
        eprintln!("capacity_measure: wrote {}", out.display());
    }
    if keep {
        eprintln!("capacity_measure: kept {}", temp.path().display());
        std::mem::forget(temp);
    } else {
        eprintln!("capacity_measure: removing {}", temp.path().display());
    }
}

// ---- the peak-temp receipt covers the whole run's temp artifacts ------------

/// The maximum simultaneous extent of the rebuild's temp artifacts one sample
/// saw: the pre-truncate staged copy, its rollback journal, the visited scratch
/// and SQLite's unlinked rewrite temp database.
#[derive(Clone, Copy, Debug, Default)]
struct TempFootprint {
    total: u64,
    staged: u64,
    journal: u64,
    visited: u64,
    rewrite: u64,
}

/// Samples the run's temp artifacts from a second thread. The staged file,
/// its `-journal` and the visited scratch are visible in the Store's directory;
/// SQLite writes the rewrite's temp database to the process temp directory and
/// unlinks it, so it is reachable only through `/proc/self/fd` (Linux).
struct FootprintSampler {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    total: Arc<AtomicU64>,
    staged: Arc<AtomicU64>,
    journal: Arc<AtomicU64>,
    visited: Arc<AtomicU64>,
    rewrite: Arc<AtomicU64>,
}

impl FootprintSampler {
    fn start(store: &Path) -> Self {
        let name = store
            .file_name()
            .map(|file| file.to_string_lossy().into_owned())
            .unwrap_or_default();
        let directory = store
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let stop = Arc::new(AtomicBool::new(false));
        let total = Arc::new(AtomicU64::new(0));
        let staged = Arc::new(AtomicU64::new(0));
        let journal = Arc::new(AtomicU64::new(0));
        let visited = Arc::new(AtomicU64::new(0));
        let rewrite = Arc::new(AtomicU64::new(0));
        let handle = {
            let stop = Arc::clone(&stop);
            let total = Arc::clone(&total);
            let staged_at = Arc::clone(&staged);
            let journal_at = Arc::clone(&journal);
            let visited_at = Arc::clone(&visited);
            let rewrite_at = Arc::clone(&rewrite);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let (mut staged, mut journal, mut visited) = (0u64, 0u64, 0u64);
                    if let Ok(entries) = std::fs::read_dir(&directory) {
                        for entry in entries.flatten() {
                            let file = entry.file_name().to_string_lossy().into_owned();
                            let Some(suffix) = file.strip_prefix(&name) else {
                                continue;
                            };
                            let Some(suffix) = suffix.strip_prefix('.') else {
                                continue;
                            };
                            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                            if suffix == "compact" {
                                staged = staged.max(size);
                            } else if suffix.starts_with("compact-journal")
                                || suffix.starts_with("compact-mj")
                            {
                                journal = journal.max(size);
                            } else if suffix.starts_with("compact-visited") {
                                visited = visited.max(size);
                            }
                        }
                    }
                    let mut rewrite = 0u64;
                    if let Ok(fds) = std::fs::read_dir("/proc/self/fd") {
                        for fd in fds.flatten() {
                            let Ok(link) = std::fs::read_link(fd.path()) else {
                                continue;
                            };
                            if !link.to_string_lossy().contains("etilqs_") {
                                continue;
                            }
                            if let Ok(meta) = std::fs::metadata(fd.path()) {
                                rewrite = rewrite.max(meta.len());
                            }
                        }
                    }
                    staged_at.fetch_max(staged, Ordering::Relaxed);
                    journal_at.fetch_max(journal, Ordering::Relaxed);
                    visited_at.fetch_max(visited, Ordering::Relaxed);
                    rewrite_at.fetch_max(rewrite, Ordering::Relaxed);
                    total.fetch_max(staged + journal + visited + rewrite, Ordering::Relaxed);
                    std::thread::sleep(Duration::from_micros(200));
                }
            })
        };
        Self {
            stop,
            handle: Some(handle),
            total,
            staged,
            journal,
            visited,
            rewrite,
        }
    }

    fn finish(mut self) -> TempFootprint {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        TempFootprint {
            total: self.total.load(Ordering::Relaxed),
            staged: self.staged.load(Ordering::Relaxed),
            journal: self.journal.load(Ordering::Relaxed),
            visited: self.visited.load(Ordering::Relaxed),
            rewrite: self.rewrite.load(Ordering::Relaxed),
        }
    }
}

/// The §11 peak-temp receipt on a moderate fixture. The run really does create
/// the pre-truncate staged copy, a rollback journal (the prune and the rewrite)
/// and SQLite's unlinked rewrite temp database, and the receipt must bound all
/// of them; the old `new + visited` formula fails the sampled journal. The
/// fixture is ~40 MB, not the 50 GB T-G66 arm: the same code path, sampled.
#[test]
fn the_peak_temp_receipt_covers_the_staged_copy_and_the_rollback_journal() {
    const GROUPS: u64 = 16;
    const GROUP: u64 = 48;
    const OBJECT_BYTES: usize = 40 * 1024;
    const DEAD_OBJECTS: u64 = 240;

    let temp = Temp::new("sqlite-compact-peak-temp");
    let store = temp.join("store.sqlite");
    let handles = create_store(&store, SqlitePersistenceProfile::Disposable);
    let mut seed = 0u64;
    for group in 0..GROUPS {
        let payloads: Vec<Vec<u8>> = (0..GROUP)
            .map(|offset| payload(seed + offset, OBJECT_BYTES))
            .collect();
        seed += GROUP;
        let roots = save_whole_files(&handles, &payloads);
        publish_group(&handles, group, &roots);
    }
    add_dead(&handles, DEAD_OBJECTS, 1_000_000_000, OBJECT_BYTES);
    drop(handles);

    let sampler = FootprintSampler::start(&store);
    let handles = reopen(&store, SqlitePersistenceProfile::Disposable);
    let receipt = handles.compact(&request(0x7a)).expect("compaction");
    let footprint = sampler.finish();

    assert_eq!(
        receipt.path,
        CompactPath::Rebuild,
        "the fixture must take the rebuild path"
    );
    assert!(
        footprint.staged > 0,
        "the staged copy must be sampled: {footprint:?}"
    );
    assert!(
        footprint.journal > 0,
        "the rollback journal must be sampled: {footprint:?}"
    );
    assert!(
        footprint.rewrite > 0,
        "the unlinked rewrite temp database must be sampled: {footprint:?}"
    );
    assert!(
        footprint.visited > 0,
        "the visited scratch must be sampled: {footprint:?}"
    );
    let new_bytes = std::fs::metadata(&store).expect("installed Store").len();
    eprintln!(
        "peak-temp fixture: new file {new_bytes} B, sampled temp footprint {footprint:?}, \
         receipt {} B",
        receipt.peak_temp_bytes
    );
    assert!(
        receipt.peak_temp_bytes >= new_bytes + footprint.journal + footprint.visited,
        "peakTempBytes {} must cover the new file {} plus the sampled {}-byte journal \
         and {}-byte visited scratch",
        receipt.peak_temp_bytes,
        new_bytes,
        footprint.journal,
        footprint.visited
    );
    assert!(
        receipt.peak_temp_bytes >= footprint.total,
        "peakTempBytes {} must bound the sampled {}-byte temp footprint: {footprint:?}",
        receipt.peak_temp_bytes,
        footprint.total
    );
}
