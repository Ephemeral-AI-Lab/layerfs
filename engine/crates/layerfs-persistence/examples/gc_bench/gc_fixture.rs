//! Fixture construction and Store probes for the GC benchmark home.
//!
//! This module is a library module of [`examples/gc_bench.rs`](../gc_bench.rs),
//! not a Cargo example target: the benchmark declares it with
//! `#[path = "gc_bench/gc_fixture.rs"] mod gc_fixture;`.
//!
//! The fixture is built through the public persistence/storage APIs, so the
//! measurement exercises the same construction and compaction paths as a real
//! Store. Every live file is one whole-file object of `object_bytes` reached
//! through one `InodeLeaf` and one `FilesystemRoot`, published as its own
//! `LayerStack` genesis. The dead payload is whole-file objects that no retained
//! root reaches. Every pack holds one object, so `SELECT count(*), sum(length)
//! FROM pack` is the exact pack-byte population and the section 11 dead-byte
//! bound/refined fields can be checked against it.
//!
//! Only the Disposable (WAL/synchronous=OFF) profile is built and measured:
//! owner direction 2026-10-07 disabled Durable execution indefinitely.

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
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::{Storage, StoragePolicy};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

/// Binding key of the benchmark Store; it is not a product binding.
pub const BINDING: &[u8] = b"gc-bench-fixture-v1";
/// Cursor key of the benchmark Store; it is not a product cursor.
pub const CURSOR_KEY: [u8; 32] = [44; 32];

const BATCH: usize = 100;

#[derive(Clone, Copy, Debug)]
pub struct FixtureSpec {
    /// Live inode leaves. One group is `group_size` whole-file objects.
    pub groups: u64,
    /// Whole-file objects per inode leaf; the page cap is 100 rows including
    /// the directory root.
    pub group_size: u64,
    /// Payload bytes per whole-file object.
    pub object_bytes: usize,
}

impl FixtureSpec {
    pub fn live_objects(&self) -> u64 {
        self.groups.saturating_mul(self.group_size)
    }

    pub fn live_payload_bytes(&self) -> u64 {
        self.live_objects()
            .saturating_mul(u64::try_from(self.object_bytes).unwrap_or(u64::MAX))
    }

    /// A deliberately conservative bound on what the fixture and its rebuild
    /// can need on disk. The final 50 GB arm's receipt bound is 2.90x the old
    /// file; the runbook asks for >=3x free, which the caller checks.
    pub fn disk_headroom_bytes(&self) -> u64 {
        // 3.4x live plus 1 GiB: the 50 GB arm's receipt bound is 2.90x the old
        // file and the runbook asks for >=155 GB free, which this covers with
        // margin while staying close to the measured requirement.
        self.live_payload_bytes()
            .saturating_mul(34)
            .div_ceil(10)
            .saturating_add(1024 * 1024 * 1024)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PackStats {
    pub count: u64,
    pub total: u64,
    pub min: u64,
    pub max: u64,
}

pub fn create_store(path: &Path) -> SqliteHandles {
    SqliteHandles::create(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR_KEY,
            incarnation: 44,
        },
    )
    .expect("Store creation")
}

pub fn reopen(path: &Path) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        BINDING,
        CURSOR_KEY,
    )
    .expect("Store reopen")
}

/// Deterministic payload; the seed makes every object distinct. The first eight
/// bytes carry the object index so two seeds cannot collide by accident.
pub fn payload(seed: u64, len: usize) -> Vec<u8> {
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
    stack[8..].copy_from_slice(b"g44ben00");
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            stack: LayerStackId::from_authority(stack),
            name: HistoryName::new(&format!("gc-bench-{index:07}")).expect("name"),
            scope: scope_for_seed([1; 32]).object(),
            profile: profile_id(),
            genesis_root: root_id,
            logical: 0,
        })
        .expect("layerstack initialization");
}

/// Reads the exact pack population straight from the Store file. The read-only
/// connection is closed before any compaction starts.
pub fn pack_stats(path: &Path) -> PackStats {
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

pub fn physical_bytes(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Physical bytes of the Store plus its WAL/journal sidecars.
pub fn path_bytes(path: &Path) -> u64 {
    let mut total = physical_bytes(path);
    if let Some(name) = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    {
        if let Some(parent) = path.parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let file = entry.file_name().to_string_lossy().into_owned();
                    if file.starts_with(&name) && file != name {
                        total = total
                            .saturating_add(entry.metadata().map(|meta| meta.len()).unwrap_or(0));
                    }
                }
            }
        }
    }
    total
}

/// Builds the live half of the fixture and returns the wall time and the pack
/// population before any dead object is added.
pub fn build_live(store: &Path, spec: FixtureSpec) -> (f64, PackStats) {
    let handles = create_store(store);
    let started = Instant::now();
    let mut seed = 0u64;
    let progress_every = (spec.groups / 20).max(1);
    for group in 0..spec.groups {
        let payloads: Vec<Vec<u8>> = (0..spec.group_size)
            .map(|offset| payload(seed + offset, spec.object_bytes))
            .collect();
        seed += spec.group_size;
        let roots = save_whole_files(&handles, &payloads);
        publish_group(&handles, group, &roots);
        if group % progress_every == 0 || group + 1 == spec.groups {
            let written = (group + 1) as f64 * spec.group_size as f64 * spec.object_bytes as f64;
            let elapsed = started.elapsed().as_secs_f64().max(1e-9);
            eprintln!(
                "gc_bench: build live groups {}/{} ({:.3} GB, {:.1} MB/s)",
                group + 1,
                spec.groups,
                written / 1e9,
                written / 1e6 / elapsed
            );
            let _ = std::io::stderr().flush();
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    drop(handles);
    (seconds, pack_stats(store))
}

/// Saves `count` dead whole-file objects in batches.
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

#[derive(Clone, Copy, Debug)]
pub struct DeadShare {
    pub dead_bytes: u64,
    pub added_objects: u64,
    pub seconds: f64,
}

/// Adds dead whole-file objects until `dead_bytes * 1000 >= total * target_permille`.
/// `dead_bytes` is the current dead bound (0 when the Store is freshly
/// compacted). The function returns the measured dead bytes after the last add.
pub fn ensure_dead_share(
    store: &Path,
    target_permille: u64,
    dead_bytes: u64,
    seed_base: u64,
    object_bytes: usize,
) -> DeadShare {
    assert!(target_permille <= 1000, "dead share is a permille");
    let started = Instant::now();
    let mut dead = dead_bytes;
    let mut added = 0u64;
    let mut seed = seed_base;

    // Learn this fixture's exact one-object pack width instead of assuming the
    // pack framing: add one probe object and read the pack-total delta.
    let before = pack_stats(store);
    {
        let handles = reopen(store);
        add_dead(&handles, 1, seed, object_bytes);
    }
    seed = seed.saturating_add(1);
    added = added.saturating_add(1);
    let after = pack_stats(store);
    let unit = after.total.saturating_sub(before.total).max(1);
    dead = dead.saturating_add(after.total.saturating_sub(before.total));

    loop {
        let current = pack_stats(store);
        if current.total == 0
            || dead.saturating_mul(1000) >= current.total.saturating_mul(target_permille)
        {
            break;
        }
        let wanted = current.total.saturating_mul(target_permille) / 1000;
        let need = wanted.saturating_sub(dead).div_ceil(unit).max(1);
        let before_total = current.total;
        {
            let handles = reopen(store);
            add_dead(&handles, need, seed, object_bytes);
        }
        seed = seed.saturating_add(need);
        added = added.saturating_add(need);
        let after = pack_stats(store);
        dead = dead.saturating_add(after.total.saturating_sub(before_total));
        if added > 100_000_000 {
            panic!(
                "could not reach {target_permille} permille dead in a bounded number of objects"
            );
        }
    }
    DeadShare {
        dead_bytes: dead,
        added_objects: added,
        seconds: started.elapsed().as_secs_f64(),
    }
}
