//! Phase 0 ordering probe (P0-3 `order.default` / `order.forced64`).
//!
//! Structural provenance: the fixture and the rename batch are the Stage 5
//! round-4 `s5term grid` probe's own shapes (4,000-file base, `fNNNNN` ->
//! `rNNNNN` rename pairs, one update operation), re-implemented here because that
//! client lives under a frozen historical evidence directory and is deliberately
//! not edited. The grid rows it produced are cited as the historical comparator;
//! the numbers in this directory are this tree's own single sample.
//!
//! This client uses only public entry points of the `core` packages, is not part
//! of the product workspace, and nothing in the repository imports it.
//!
//! Probes:
//! - `order <pairs> <pending>`: one update operation at a chosen
//!   `maximum_pending_records`, printing the counters the Phase 1 items verify
//!   against (`rows_read`, `rows_written`, runs, spills, peaks).
//! - `c2 <rows> <mode>`: one C2 save of `rows` supplied canonical pooled-metadata
//!   objects through `Store::begin_save` / `accept` / `finish`, plus the reachable
//!   SQLite observables for the spilling question.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemResources, FilesystemRootId, InodeScope, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
    ObjectRole,
};
use layerfs_storage::{StoragePolicy, Store};

#[derive(Clone, Debug, Default)]
struct Bag {
    objects: BTreeMap<ObjectId, (ObjectRole, Vec<u8>)>,
}

impl Bag {
    fn get(&self, id: ObjectId) -> ContentResult<Vec<u8>> {
        self.objects
            .get(&id)
            .map(|(_, bytes)| bytes.clone())
            .ok_or(ContentError::MissingObject)
    }
}

impl FinalizedConsumer for Bag {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let parts = object.into_parts();
        self.objects
            .insert(parts.id, (parts.role, parts.canonical));
        Ok(())
    }
}

impl AuthenticatedObjects for Bag {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter().map(|id| self.get(*id)).collect()
    }
}

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn synthetic(label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("layerfs/s5term/{label}").as_bytes())
}

fn scope() -> InodeScope {
    scope_for_seed([0x41; 32])
}

struct Fixture {
    bag: Bag,
    root: ObjectId,
}

/// Builds root (serial 1) with directory d (serial 2) holding `files` files.
fn fixture(files: usize) -> Fixture {
    let mut bag = Bag::default();
    let mut inodes: Vec<InodeUpdate> = vec![
        InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("root-content"),
                metadata_root: synthetic("root-meta"),
            },
        },
        InodeUpdate {
            serial: 2,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("d-content"),
                metadata_root: synthetic("d-meta"),
            },
        },
    ];
    let mut new_inodes: Vec<u64> = vec![1, 2];
    for index in 0..files {
        inodes.push(InodeUpdate {
            serial: index as u64 + 3,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: synthetic(&format!("content-{index:05}")),
                metadata_root: synthetic("file-meta"),
            },
        });
        new_inodes.push(index as u64 + 3);
    }
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), Some(2))],
        },
        DirectoryUpdate {
            parent: 2,
            changes: (0..files)
                .map(|index| (name(&format!("f{index:05}")), Some(index as u64 + 3)))
                .collect(),
        },
    ];
    let input = FilesystemInput {
        base: None,
        scope: scope(),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let reader = bag.clone();
    let mut sink = Bag::default();
    // The build is itself an operation: above the default pending-row ceiling it
    // needs a caller-owned ordering backing, exactly like any other operation.
    let mut backing = FileBacking::new(backing_dir("fixture"));
    let result = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        let backing_ref: &mut dyn OrderingBacking = &mut backing;
        build_filesystem(&mut objects, &input, Some(backing_ref))
    }
    .expect("fixture build");
    bag.objects.extend(sink.objects);
    Fixture {
        bag,
        root: result.root.0,
    }
}

/// A rename batch over the first `pairs` names of d.
fn rename_pairs(pairs: usize) -> Vec<DirectoryUpdate> {
    let mut changes: Vec<(PathName, Option<u64>)> = Vec::with_capacity(pairs * 2);
    for index in 0..pairs {
        changes.push((name(&format!("f{index:05}")), None));
        changes.push((name(&format!("r{index:05}")), Some(index as u64 + 3)));
    }
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    vec![DirectoryUpdate {
        parent: 2,
        changes,
    }]
}

fn backing_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-phase0-{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("backing directory");
    dir
}

/// One measured update at the requested pending ceiling.
fn measured_update(fixture: &Fixture, pairs: usize, pending: usize, label: &str) -> u128 {
    let directories = rename_pairs(pairs);
    let input = FilesystemInput {
        base: Some(FilesystemRootId(fixture.root)),
        scope: scope(),
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources {
            maximum_pending_records: pending,
            ..FilesystemResources::default()
        },
    };
    let mut backing = FileBacking::new(backing_dir(label));
    let started = Instant::now();
    let mut sink = Bag::default();
    let result = {
        let reader = fixture.bag.clone();
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        let backing_ref: &mut dyn OrderingBacking = &mut backing;
        update_filesystem(&mut objects, &input, Some(backing_ref))
    };
    let elapsed = started.elapsed().as_nanos();
    let result = result.expect("update");
    let work = result.counters.references.runs;
    println!(
        "order pairs {} pending {} spilled {} elapsed_ns {} rows_read {} rows_written {} runs_created {} merges {} peak_run_bytes {} peak_live_runs {} peak_pending {} held_now {} peak_backing {} emitted {} dir_pages_read {} dir_pages_created {} dir_pages_reused {} dir_scratch {} ino_pages_read {} ino_pages_created {} ino_pages_reused {} ino_scratch {} objects_read {} read_waves {} bytes_read {} rows_touched {} final_values {} final_removals {}",
        pairs,
        pending,
        result.counters.references.rows_spilled,
        elapsed,
        work.rows_read,
        work.rows_written,
        work.runs_created,
        work.merges,
        work.peak_run_bytes,
        work.peak_live_runs,
        result.counters.references.peak_pending,
        backing.held_bytes(),
        backing.peak_bytes(),
        result.counters.objects.objects_emitted,
        result.counters.directories.pages_read,
        result.counters.directories.pages_created,
        result.counters.directories.pages_reused,
        result.counters.directories.peak_scratch_bytes,
        result.counters.inodes.pages_read,
        result.counters.inodes.pages_created,
        result.counters.inodes.pages_reused,
        result.counters.inodes.peak_scratch_bytes,
        result.counters.objects.objects_read,
        result.counters.objects.read_waves,
        result.counters.objects.bytes_read,
        result.counters.references.rows_touched,
        result.counters.references.final_values,
        result.counters.references.final_removals,
    );
    elapsed
}

fn probe_order(files: usize, pairs: usize, pending: usize) {
    let fixture = fixture(files);
    let label = format!("order-{pairs}-{pending}");
    measured_update(&fixture, pairs, pending, &label);
}

/// One canonical pooled-metadata leaf object: one inode value with a unique
/// content root, so `rows` of them are `rows` distinct canonical objects that all
/// land in the ordinary lane.
fn leaf_canonical(index: usize) -> Vec<u8> {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&(index as u64).to_be_bytes());
    let leaf = layerfs_content::object::inode_leaf::InodeLeaf {
        subtree_bytes: layerfs_content::object::inode_leaf::LEAF_ROW_BYTES as u64,
        rows: vec![layerfs_content::object::inode_leaf::InodeLeafRow {
            serial: 1,
            value: layerfs_content::object::inode_leaf::encode_inode_value(
                layerfs_content::object::inode_leaf::InodeValue {
                    kind: layerfs_content::object::inode_leaf::InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(&bytes),
                    metadata_root: ObjectId::for_bytes(&bytes),
                },
            ),
        }],
    };
    leaf.encode().expect("leaf encode")
}

/// One finalized synthetic inode leaf with a distinct canonical body.
fn leaf_object(index: usize) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::InodeLeaf, leaf_canonical(index)).expect("finalized object")
}

/// Reads the reachable SQLite observables on `connection`.
fn observables(connection: &rusqlite::Connection) -> String {
    let cache_size: i64 = connection
        .query_row("PRAGMA cache_size", [], |row| row.get(0))
        .expect("cache_size");
    let cache_spill: i64 = connection
        .query_row("PRAGMA cache_spill", [], |row| row.get(0))
        .expect("cache_spill");
    let page_size: i64 = connection
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .expect("page_size");
    let journal: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("journal_mode");
    let mut current: std::os::raw::c_int = 0;
    let mut high: std::os::raw::c_int = 0;
    let mut reset: std::os::raw::c_int = 0;
    let spill_ok = unsafe {
        rusqlite::ffi::sqlite3_db_status(
            connection.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_CACHE_SPILL,
            &mut current,
            &mut high,
            1,
        ) == rusqlite::ffi::SQLITE_OK
    };
    let spill_value = current;
    let write_ok = unsafe {
        rusqlite::ffi::sqlite3_db_status(
            connection.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_CACHE_WRITE,
            &mut current,
            &mut reset,
            1,
        ) == rusqlite::ffi::SQLITE_OK
    };
    let write_value = current;
    let used_ok = unsafe {
        rusqlite::ffi::sqlite3_db_status(
            connection.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_CACHE_USED,
            &mut current,
            &mut reset,
            1,
        ) == rusqlite::ffi::SQLITE_OK
    };
    format!(
        "pragma cache_size {} cache_spill {} page_size {} journal_mode {} | dbstatus cache_spill ok={} value={} | cache_write ok={} value={} | cache_used ok={} value={}",
        cache_size, cache_spill, page_size, journal, spill_ok, spill_value, write_ok, write_value,
        used_ok, current,
    )
}

/// One C2 save of `rows` supplied canonical objects through the public
/// `begin_save` / `accept` / `finish` surface.
///
/// `cache` selects the connection profile *of the connection that performs the
/// save*, which is the only connection whose `SQLITE_DBSTATUS_CACHE_SPILL`
/// counter can answer the spilling question. Two shapes exist:
///
/// - `default`: `Store::create` owns its connection and applies the declared
///   product profile (no `cache_size`, no `cache_spill` pragma).
/// - `diagnostic-large-cache`: a labelled diagnostic that, on a connection this
///   harness owns, applies the *reference* profile (`cache_size = -32768`,
///   `cache_spill = OFF`) and then drives the same row/transaction shape through
///   the same engine. It is an upper-bound comparison for the effect of the cache
///   setting, not a product path, and it is never a gate sample.
fn probe_c2(rows: usize, cache: &str) {
    let dir = backing_dir(&format!("c2-{rows}-{cache}"));
    let store_path = dir.join("store.sqlite");
    // Untimed: the supplied canonical objects. The timed region is the Store's
    // creation, the save and its acknowledgement.
    let objects: Vec<FinalizedObject> = (0..rows).map(leaf_object).collect();
    let mut total_bytes = 0_u64;
    for object in &objects {
        total_bytes += object.canonical_len() as u64;
    }

    if cache == "diagnostic-default-cache"
        || cache == "diagnostic-large-cache"
        || cache == "diagnostic-small-cache"
    {
        let profile = if cache == "diagnostic-large-cache" {
            // The reference's profile: 32 MiB and spilling OFF.
            "PRAGMA journal_mode = MEMORY; PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY; \
             PRAGMA cache_size = -32768; PRAGMA cache_spill = OFF;"
        } else if cache == "diagnostic-small-cache" {
            // V7's live control for the spill counter (P0-2's `spillcontrol`,
            // re-run in this directory): a deliberately tiny 8-page cache with
            // spilling ON, the same row shape, one transaction. A nonzero value
            // here is what makes a zero on the product's connection a
            // measurement rather than a dead read.
            "PRAGMA journal_mode = MEMORY; PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY; \
             PRAGMA cache_size = 8; PRAGMA cache_spill = ON;"
        } else {
            // Core's declared profile as it stands: no cache_size, no cache_spill,
            // so SQLite's own defaults apply (2 MiB, spilling ON).
            "PRAGMA journal_mode = MEMORY; PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;"
        };
        let connection = rusqlite::Connection::open(&store_path).expect("diagnostic store");
        connection
            .execute_batch(profile)
            .expect("diagnostic profile");
        connection
            .execute_batch(
                "CREATE TABLE rows_objects (object_id BLOB PRIMARY KEY, canonical_length INTEGER, \
                 pack_id INTEGER, group_number INTEGER, record_number INTEGER) STRICT;",
            )
            .expect("diagnostic schema");
        let started = Instant::now();
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .expect("begin");
        {
            let mut statement = connection
                .prepare("INSERT INTO rows_objects VALUES (?1,?2,?3,?4,?5)")
                .expect("prepare");
            for (index, object) in objects.iter().enumerate() {
                statement
                    .execute(rusqlite::params![
                        object.id().as_bytes().to_vec(),
                        object.canonical_len() as i64,
                        1_i64,
                        (index / 256) as i64,
                        (index % 256) as i64
                    ])
                    .expect("insert");
            }
        }
        connection.execute_batch("COMMIT").expect("commit");
        let elapsed = started.elapsed().as_nanos();
        println!(
            "c2 rows {} canonical_bytes {} cache_arm {} elapsed_ns {} commits 1 | {} | note diagnostic: harness-owned connection, same single-row INSERT shape and one transaction; the two diagnostic arms differ ONLY in cache_size/cache_spill",
            rows,
            total_bytes,
            cache,
            elapsed,
            observables(&connection),
        );
        return;
    }

    let started = Instant::now();
    let (saved, _) = layerfs_telemetry::timer::Timing::disabled("c2.save", |scope| {
        let store = Store::create(
            &store_path,
            StoragePolicy::frozen_default(),
            scope.child("store.create"),
        )?;
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in objects {
            operation.accept(object)?;
        }
        // V7: the cache profile of the connection that performed the save, read
        // back from that connection one statement before acknowledgement.
        let profile = operation.connection_profile()?;
        let outcome = operation.finish(scope.child("storage.finish"))?;
        Ok::<_, layerfs_storage::StorageError>((store, outcome, profile))
    });
    let (store, outcome, profile) = saved.expect("save");
    let _ = &store;
    let elapsed = started.elapsed().as_nanos();
    println!(
        "c2 rows {} canonical_bytes {} cache_arm {} elapsed_ns {} inserted {} reused {} packs_created {} pack_appends {} commits {} statements {} presence_queries {} full_records {} prefix_records {} pool_leaves {} pool_new_values {} pool_groups {} pool_delta_leaves {} pool_trials {}",
        rows,
        total_bytes,
        cache,
        elapsed,
        outcome.inserted,
        outcome.reused,
        outcome.packs_created,
        outcome.pack_appends,
        outcome.commits,
        outcome.statements,
        outcome.presence_queries,
        outcome.full_records,
        outcome.prefix_records,
        outcome.pool.leaves,
        outcome.pool.new_values,
        outcome.pool.groups,
        outcome.pool.delta_leaves,
        outcome.pool.trials,
    );
    println!(
        "c2 save-connection profile page_size {} cache_size {} cache_spill {} mmap_size {} | note read back on the save's own connection by SaveOperation::connection_profile(); the page-cache spill counter is NOT here - reading SQLITE_DBSTATUS needs FFI and this crate allows unsafe in one audited module only (see V7's receipt)",
        profile.page_size, profile.cache_size, profile.cache_spill, profile.mmap_size,
    );
    let connection = rusqlite::Connection::open(&store_path).expect("inspection connection");
    println!(
        "c2 inspection-connection {} | note the spill counter below belongs to THIS connection, not the save's: the product exposes no public surface for the save connection, so a nonzero value here is not the save's spill count",
        observables(&connection),
    );
}

/// `c2-references <rows>`: two saves that price the wave-level presence batch
/// (`P2-5`).
///
/// The first save stores `rows` leaves. The second offers `rows` *different*
/// leaves, each naming one of those stored leaves as a direct reference - an
/// identity the wave did not offer and has not resolved, which is exactly what the
/// availability check has to ask the engine about. Per-object presence queries
/// price one question per dependent; a wave-level batch prices one for the wave.
fn probe_references(rows: usize) {
    let dir = backing_dir(&format!("c2-references-{rows}"));
    let store_path = dir.join("store.sqlite");
    let targets: Vec<FinalizedObject> = (0..rows).map(leaf_object).collect();
    let target_ids: Vec<ObjectId> = targets.iter().map(|object| object.id()).collect();

    let started = Instant::now();
    let saved = layerfs_telemetry::timer::Timing::disabled("c2.references", |scope| {
        let store = Store::create(
            &store_path,
            StoragePolicy::frozen_default(),
            scope.child("store.create"),
        )?;
        let mut operation = store.begin_save(scope.child("targets.begin"))?;
        for object in targets {
            operation.accept(object)?;
        }
        let targets_outcome = operation.finish(scope.child("targets.finish"))?;
        let mut operation = store.begin_save(scope.child("dependents.begin"))?;
        for (index, target) in target_ids.iter().enumerate() {
            let dependent =
                FinalizedObject::new(ObjectRole::InodeLeaf, leaf_canonical(index + rows))?
                    .with_references(vec![*target]);
            operation.accept(dependent)?;
        }
        let dependents_outcome = operation.finish(scope.child("dependents.finish"))?;
        Ok::<_, layerfs_storage::StorageError>((store, targets_outcome, dependents_outcome))
    });
    let (store, targets_outcome, dependents_outcome) = saved.0.expect("save");
    let _ = &store;
    let elapsed = started.elapsed().as_nanos();
    println!(
        "c2-references rows {} elapsed_ns {} targets inserted {} targets presence_queries {} dependents inserted {} dependents presence_queries {} | note the dependent wave names {} stored objects it does not offer, so the availability check must ask about each of them",
        rows,
        elapsed,
        targets_outcome.inserted,
        targets_outcome.presence_queries,
        dependents_outcome.inserted,
        dependents_outcome.presence_queries,
        target_ids.len(),
    );
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("order") => {
            let files: usize = arguments[1].parse().expect("files");
            let pairs: usize = arguments[2].parse().expect("pairs");
            let pending: usize = arguments[3].parse().expect("pending");
            probe_order(files, pairs, pending);
        }
        Some("c2") => {
            let rows: usize = arguments[1].parse().expect("rows");
            let cache = arguments.get(2).map(String::as_str).unwrap_or("default");
            probe_c2(rows, cache);
        }
        Some("c2-references") => {
            let rows: usize = arguments[1].parse().expect("rows");
            probe_references(rows);
        }
        other => panic!("unsupported probe {other:?}: expected `order` or `c2`"),
    }
}
