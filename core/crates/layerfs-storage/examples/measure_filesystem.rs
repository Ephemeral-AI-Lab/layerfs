//! Filesystem measurement in three modes: C1 only, C2 only, and integrated.
//!
//! `--mode c1` measures the database-free native operation and opens no Store.
//! `--mode c2` receives canonical objects that were prepared *outside* the timed
//! region and measures only admission. `--mode pipeline` measures construction,
//! the bounded handoff and the save acknowledgement in one region, then reads the
//! persisted tree back through a **reopened** Store, labelled separately.
//!
//! Each `--case` names its own fixture size and its own edit; the row prints both,
//! so a pipeline row's root identifies the change set that produced it. A run
//! whose timing report was clipped by the recorder's node budget is a failure, not
//! a row: the process exits non-zero and says which tree was clipped.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::{
    build_filesystem, update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemRead, FilesystemResources, FilesystemResult, FilesystemRootId, InodeUpdate,
    LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_storage::{SaveHandoff, SaveOutcome, StorageError, StoragePolicy, Store};
use layerfs_telemetry::timer::{Timing, TimingReport};

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
        let (id, role, bytes, _) = object.into_parts();
        self.objects.insert(id, (role, bytes));
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
    ObjectId::for_bytes(format!("layerfs/stage5-measure/{label}").as_bytes())
}

struct Config {
    mode: String,
    case: String,
    output: PathBuf,
}

impl Config {
    fn store_path(&self) -> PathBuf {
        self.output.join("store.sqlite")
    }
}

fn parse() -> Config {
    let mut mode = "c1".to_owned();
    let mut case = "directory-update".to_owned();
    let mut output = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--mode" => mode = arguments.next().unwrap_or_default(),
            "--case" => case = arguments.next().unwrap_or_default(),
            "--output" => output = arguments.next().map(PathBuf::from),
            other => panic!("unexpected argument {other}"),
        }
    }
    Config {
        mode,
        case,
        output: output.expect("--output FRESH_DIRECTORY"),
    }
}

fn disabled<T>(
    body: impl FnOnce(
        &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    ) -> ContentResult<T>,
) -> ContentResult<T> {
    layerfs_telemetry::timer::Timing::disabled("measure.operation", body).0
}

/// Fixture size `--case` selects.
fn entries_for(case: &str) -> usize {
    match case {
        "empty" => 0,
        "attributes" => 32,
        "directory-update" | "inode-update" | "hardlink-move" | "subtree-remove" => 200,
        other => panic!("unknown case {other}"),
    }
}

/// What `--case` selects, printed with the row.
fn banner(case: &str) -> &'static str {
    match case {
        "empty" => "update: none; the fixture tree is re-emitted unchanged",
        "directory-update" => "update: rename the first 10 names in /d (directory pages change)",
        "inode-update" => "update: replace 10 inode values under unchanged bindings",
        "hardlink-move" => "update: move 10 files from /d to / (additions before removals)",
        "subtree-remove" => "update: remove 10 bindings from /d (counts released)",
        "attributes" => "update: rename 4 names in /d and replace their inode values",
        other => panic!("unknown case {other}"),
    }
}

/// The real change set `--case` selects.
///
/// Four case names used to share one fixture *and* one edit - the first ten
/// bindings restated - so every `--mode pipeline` row printed the identical root
/// and every `--mode c1` row performed the identical update. Each case now names
/// its own edit of the same fixture, and `banner` says which one ran.
fn changes(case: &str, entries: usize) -> (Vec<DirectoryUpdate>, Vec<InodeUpdate>) {
    let window = entries.min(10);
    let unbind = |index: usize| (name(&format!("f{index:04}")), None);
    let rebind =
        |label: &str, index: usize| (name(&format!("{label}{index:04}")), Some(index as u64 + 3));
    let refile = |index: usize, metadata: &str| InodeUpdate {
        serial: index as u64 + 3,
        value: InodeValue {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 0,
            content_root: synthetic(&format!("content-{index:04}-v2")),
            metadata_root: synthetic(metadata),
        },
    };
    let (mut directories, mut inodes): (Vec<DirectoryUpdate>, Vec<InodeUpdate>) = match case {
        "empty" => (Vec::new(), Vec::new()),
        "directory-update" => (
            vec![DirectoryUpdate {
                parent: 2,
                changes: (0..window)
                    .flat_map(|index| [unbind(index), rebind("r", index)])
                    .collect(),
            }],
            Vec::new(),
        ),
        "inode-update" => (
            Vec::new(),
            (0..window)
                .map(|index| refile(index, "file-meta"))
                .collect(),
        ),
        "hardlink-move" => (
            vec![
                DirectoryUpdate {
                    parent: 1,
                    changes: (0..window).map(|index| rebind("m", index)).collect(),
                },
                DirectoryUpdate {
                    parent: 2,
                    changes: (0..window).map(unbind).collect(),
                },
            ],
            Vec::new(),
        ),
        "subtree-remove" => (
            vec![DirectoryUpdate {
                parent: 2,
                changes: (0..window).map(unbind).collect(),
            }],
            Vec::new(),
        ),
        "attributes" => (
            vec![DirectoryUpdate {
                parent: 2,
                changes: (0..window.min(4))
                    .flat_map(|index| [unbind(index), rebind("a", index)])
                    .collect(),
            }],
            (0..window.min(4))
                .map(|index| refile(index, "attr-meta"))
                .collect(),
        ),
        other => panic!("unknown case {other}"),
    };
    // The final-state bindings are strict: a directory update lists its changed
    // names in order, so a rename contributes both of its names into their sorted
    // places rather than appending the new one after the old.
    for update in &mut directories {
        update.changes.sort_by(|left, right| left.0.cmp(&right.0));
    }
    inodes.sort_by_key(|update| update.serial);
    (directories, inodes)
}

/// Deterministic fixture: `entries` files under `/d`, all identities fixed.
fn fixture(
    entries: usize,
) -> (
    Bag,
    ObjectId,
    layerfs_content::filesystem::InodeScope,
    Vec<u64>,
) {
    let scope = layerfs_content::filesystem::scope_for_seed([0x44; 32]);
    let mut bag = Bag::default();
    let mut inodes = vec![
        InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("unused"),
                metadata_root: synthetic("root-meta"),
            },
        },
        InodeUpdate {
            serial: 2,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("unused"),
                metadata_root: synthetic("dir-meta"),
            },
        },
    ];
    let mut new_inodes = vec![1_u64, 2];
    for index in 0..entries {
        inodes.push(InodeUpdate {
            serial: index as u64 + 3,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: synthetic(&format!("content-{index:04}")),
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
            changes: (0..entries)
                .map(|index| (name(&format!("f{index:04}")), Some(index as u64 + 3)))
                .collect(),
        },
    ];
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let reader = bag.clone();
    let mut sink = Bag::default();
    let result = disabled(|_| {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        build_filesystem(&mut objects, &input, None)
    })
    .expect("fixture build");
    bag.objects.extend(sink.objects);
    (bag, result.root.0, scope, new_inodes)
}

/// Creates the run's fresh Store. Only the modes that measure admission call it.
///
/// A `--mode c1` row therefore opens no database at all, which is what the
/// measurement contract says about it.
fn fresh_store(config: &Config) -> Store {
    let path = config.store_path();
    let _ = std::fs::remove_file(&path);
    Timing::disabled("measure.store", |timing| {
        Store::create(
            &path,
            StoragePolicy::frozen_default(),
            timing.child("store"),
        )
    })
    .0
    .expect("store")
}

/// Reopens the run's Store for the read-back half of an integrated row.
fn reopened_store(config: &Config) -> Store {
    Timing::disabled("measure.store.open", |timing| {
        Store::open(config.store_path(), timing.child("store"))
    })
    .0
    .expect("reopened store")
}

/// Prints the report size and refuses to leave a clipped row unremarked.
fn require_complete(report: &TimingReport) -> Result<(), String> {
    if report.is_incomplete() {
        return Err(format!(
            "timings: INCOMPLETE - the node budget clipped this tree to {} nodes; the run is not \
             a measured row and must not be quoted",
            report.node_count()
        ));
    }
    println!("report_nodes {}", report.node_count());
    Ok(())
}

fn main() -> Result<(), String> {
    let config = parse();
    std::fs::create_dir_all(&config.output).expect("output directory");
    let entries = entries_for(&config.case);
    let (bag, root, scope, new_inodes) = fixture(entries);
    let (directories, inodes) = changes(&config.case, entries);
    let mut backing = FileBacking::new(&config.output);
    println!("mode {} case {}", config.mode, config.case);
    println!("fixture entries {entries}");
    println!("{}", banner(&config.case));
    let input = FilesystemInput {
        base: Some(FilesystemRootId(root)),
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    match config.mode.as_str() {
        "c1" => run_c1(&bag, &input, &mut backing, entries),
        "c2" => run_c2(&config, &bag),
        "pipeline" => run_pipeline(&config, &bag, &input, &mut backing, entries),
        other => panic!("unknown mode {other}"),
    }?;
    println!("prepared_objects {}", bag.objects.len());
    println!("new_inodes {}", new_inodes.len());
    Ok(())
}

/// The database-free native operation: no Store is created or opened here.
fn run_c1(
    bag: &Bag,
    input: &FilesystemInput<'_>,
    backing: &mut FileBacking,
    entries: usize,
) -> Result<(), String> {
    let started = Instant::now();
    let mut sink = Bag::default();
    let result = disabled(|_| {
        let mut objects = FilesystemObjects::new(bag, &mut sink);
        let backing_ref: &mut dyn OrderingBacking = backing;
        update_filesystem(&mut objects, input, Some(backing_ref))
    })
    .expect("c1 update");
    let elapsed = started.elapsed();
    let mut merged = bag.clone();
    merged.objects.extend(sink.objects);
    println!("root {}", result.root.0);
    println!("elapsed_ns {}", elapsed.as_nanos());
    println!(
        "boundary objects_read {} waves {} emitted {} bytes_emitted {}",
        result.counters.objects.objects_read,
        result.counters.objects.read_waves,
        result.counters.objects.objects_emitted,
        result.counters.objects.bytes_emitted
    );
    println!(
        "sorted directories pages_read {} pages_created {} inodes pages_read {} pages_created {}",
        result.counters.directories.pages_read,
        result.counters.directories.pages_created,
        result.counters.inodes.pages_read,
        result.counters.inodes.pages_created
    );
    println!("end-to-end ns {}", elapsed.as_nanos());
    println!(
        "readback separately labelled: {}",
        readback(&merged, result.root.0, entries)
    );
    Ok(())
}

/// Admission only: the objects are prepared before the timed region.
fn run_c2(config: &Config, bag: &Bag) -> Result<(), String> {
    let store = fresh_store(config);
    let objects = bag
        .objects
        .iter()
        .map(|(_, (role, bytes))| FinalizedObject::new(*role, bytes.clone()).expect("finalized"))
        .collect::<Vec<_>>();
    let started = Instant::now();
    let (outcome, report) = Timing::record("storage.save", |timing| {
        let mut operation = store.begin_save(timing.child("storage.begin"))?;
        timing
            .child("storage.accept")
            .run(|_accept| -> Result<(), StorageError> {
                for object in objects {
                    operation.accept(object)?;
                }
                Ok(())
            })?;
        operation.finish(timing.child("storage.finish"))
    });
    let elapsed = started.elapsed();
    let outcome = outcome.map_err(|error| format!("c2 save failed: {error}"))?;
    println!("elapsed_ns {}", elapsed.as_nanos());
    println!(
        "inserted {} reused {} packs {} commits {}",
        outcome.inserted, outcome.reused, outcome.packs_created, outcome.commits
    );
    println!(
        "full_records {} prefix_records {} pooled {}",
        outcome.full_records, outcome.prefix_records, outcome.pool.new_values
    );
    require_complete(&report)
}

/// Integrated: real C1 construction, the bounded handoff and the save, in one
/// region; the persisted tree is then read back through a reopened Store.
fn run_pipeline(
    config: &Config,
    bag: &Bag,
    input: &FilesystemInput<'_>,
    backing: &mut FileBacking,
    entries: usize,
) -> Result<(), String> {
    let store = fresh_store(config);
    // Setup, outside every timed region: the fixture tree is persisted so the
    // update has a real base to address, exactly as a Workspace would supply one.
    // Only the update's own construction, handoff and save are measured.
    let setup_started = Instant::now();
    let setup = Timing::disabled("measure.setup", |timing| {
        let mut operation = store.begin_save(timing.child("storage.begin"))?;
        for (role, bytes) in bag.objects.values() {
            operation.accept(FinalizedObject::new(*role, bytes.clone())?)?;
        }
        operation.finish(timing.child("storage.finish"))
    })
    .0;
    let setup = setup.map_err(|error| format!("fixture setup failed: {error}"))?;
    println!(
        "setup untimed elapsed_ns {} inserted {}",
        setup_started.elapsed().as_nanos(),
        setup.inserted
    );
    let started = Instant::now();
    let (result, report): (
        Result<(FilesystemResult, SaveOutcome), StorageError>,
        TimingReport,
    ) = Timing::record("c1c2.pipeline", |timing| {
        let mut operation = store.begin_save(timing.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        let constructed = {
            let mut objects = FilesystemObjects::new(bag, &mut handoff);
            let backing_ref: &mut dyn OrderingBacking = backing;
            update_filesystem(&mut objects, input, Some(backing_ref))
        };
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        let constructed = constructed?;
        let outcome = operation.finish(timing.child("storage.finish"))?;
        Ok((constructed, outcome))
    });
    let elapsed = started.elapsed();
    let (result, outcome) = result.map_err(|error| format!("pipeline failed: {error}"))?;
    println!("root {}", result.root.0);
    println!(
        "elapsed_ns {} (construction, bounded handoff and save acknowledgement)",
        elapsed.as_nanos()
    );
    println!(
        "constructed objects_emitted {} bytes_emitted {} saved inserted {} reused {} admitted {}",
        result.counters.objects.objects_emitted,
        result.counters.objects.bytes_emitted,
        outcome.inserted,
        outcome.reused,
        outcome.inserted + outcome.reused
    );
    require_complete(&report)?;
    drop(store);

    // Read-back is its own labelled region, and it goes through a REOPENED Store:
    // the row has to describe a persisted tree, not a copy this process still
    // holds in memory.
    let reopened = reopened_store(config);
    let read_started = Instant::now();
    let read = store_readback(&reopened, result.root.0, entries);
    println!(
        "readback separately labelled elapsed_ns {} result {}",
        read_started.elapsed().as_nanos(),
        read
    );
    Ok(())
}

/// Reads a built tree back through the in-memory bag, for the C1-only row.
fn readback(bag: &Bag, root: ObjectId, entries: usize) -> String {
    match FilesystemRead::new(bag, FilesystemRootId(root)) {
        Ok(mut read) => listing(&mut read, entries),
        Err(error) => format!("read failed: {error}"),
    }
}

/// Reads a persisted tree back through a reopened Store, over the product bridge.
fn store_readback(store: &Store, root: ObjectId, entries: usize) -> String {
    let provider = layerfs_storage::StoreProvider::new(store);
    match FilesystemRead::new(&provider, FilesystemRootId(root)) {
        Ok(mut read) => listing(&mut read, entries),
        Err(error) => format!("read failed: {error}"),
    }
}

fn listing(read: &mut FilesystemRead<'_>, entries: usize) -> String {
    let top = names_of(read, &LogicalPath::root());
    let inner = names_of(read, &LogicalPath::new("d").expect("absolute path"));
    format!("top_level=[{top}] d_first4=[{inner}] fixture_entries={entries}")
}

fn names_of(read: &mut FilesystemRead<'_>, path: &LogicalPath) -> String {
    match read.list(path, None, 4, 4096) {
        Ok(page) => page
            .entries
            .iter()
            .map(|(name, serial)| format!("{}->{serial}", name.as_str()))
            .collect::<Vec<_>>()
            .join(","),
        Err(error) => format!("list failed: {error}"),
    }
}
