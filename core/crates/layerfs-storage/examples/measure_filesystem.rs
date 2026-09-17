//! Filesystem measurement in three modes: C1 only, C2 only, and integrated.
//!
//! `--mode c1` measures the database-free native operation. `--mode c2` receives
//! canonical objects that were prepared *outside* the timed region and measures
//! only admission. `--mode pipeline` measures construction, the handoff and the
//! final save in one region, with read-back reported separately. Every mode writes
//! into a fresh `--output` directory and prints actual nanosecond timings.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use layerfs_content::filesystem::references::backing::FileBacking;
use layerfs_content::filesystem::{
    build_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemRead,
    FilesystemResources, FilesystemRootId, InodeUpdate, LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
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

fn main() {
    let config = parse();
    std::fs::create_dir_all(&config.output).expect("output directory");
    let entries = match config.case.as_str() {
        "empty" => 0,
        "directory-update" | "inode-update" | "hardlink-move" | "subtree-remove" => 200,
        "attributes" => 32,
        other => panic!("unknown case {other}"),
    };
    let (bag, root, scope, new_inodes) = fixture(entries);
    let store_path = config.output.join("store.sqlite");
    let _ = std::fs::remove_file(&store_path);
    let store = layerfs_telemetry::timer::Timing::disabled("measure.store", |timing| {
        Store::create(
            &store_path,
            StoragePolicy::frozen_default(),
            timing.child("store"),
        )
    })
    .0
    .expect("store");
    let mut backing = FileBacking::new(&config.output);

    match config.mode.as_str() {
        "c1" => {
            let directories = [DirectoryUpdate {
                parent: 2,
                changes: (0..entries.min(10))
                    .map(|index| (name(&format!("f{index:04}")), Some(index as u64 + 3)))
                    .collect(),
            }];
            let input = FilesystemInput {
                base: Some(FilesystemRootId(root)),
                scope,
                root_serial: 1,
                directories: &directories,
                inodes: &[],
                new_inodes: &[],
                resources: FilesystemResources::default(),
            };
            let started = Instant::now();
            let mut sink = Bag::default();
            let result = disabled(|_| {
                let mut objects = FilesystemObjects::new(&bag, &mut sink);
                let backing_ref: &mut dyn layerfs_content::filesystem::references::backing::OrderingBacking = &mut backing;
                layerfs_content::filesystem::update_filesystem(&mut objects, &input, Some(backing_ref))
            })
            .expect("c1 update");
            let elapsed = started.elapsed();
            println!("mode c1 case {}", config.case);
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
                readback(&bag, root, entries)
            );
        }
        "c2" => {
            // Objects are prepared outside the timed region: this mode measures
            // admission only, and constructs no filesystem.
            let objects = bag
                .objects
                .iter()
                .map(|(_, (role, bytes))| {
                    FinalizedObject::new(*role, bytes.clone()).expect("finalized")
                })
                .collect::<Vec<_>>();
            let started = Instant::now();
            let (outcome, report) =
                layerfs_telemetry::timer::Timing::record("storage.save", |timing| {
                    let mut operation = store
                        .begin_save(timing.child("storage.begin"))
                        .map_err(|_| ContentError::IncompleteOperation)?;
                    for object in objects {
                        operation
                            .accept(object, timing.child("storage.accept"))
                            .map_err(|_| ContentError::IncompleteOperation)?;
                    }
                    operation
                        .finish(timing.child("storage.finish"))
                        .map_err(|_| ContentError::IncompleteOperation)
                });
            let elapsed = started.elapsed();
            let outcome = outcome.expect("save");
            println!("mode c2 case {}", config.case);
            println!("supplied_objects {}", bag.objects.len());
            println!("elapsed_ns {}", elapsed.as_nanos());
            println!(
                "inserted {} reused {} packs {} commits {} acknowledged {}",
                outcome.inserted,
                outcome.reused,
                outcome.packs_created,
                outcome.commits,
                outcome.acknowledged
            );
            println!(
                "full_records {} prefix_records {} pooled {}",
                outcome.full_records, outcome.prefix_records, outcome.pool.new_values
            );
            println!("report_nodes {}", report.node_count());
        }
        "pipeline" => {
            let directories = [DirectoryUpdate {
                parent: 2,
                changes: Vec::new(),
            }];
            let input = FilesystemInput {
                base: Some(FilesystemRootId(root)),
                scope,
                root_serial: 1,
                directories: &directories,
                inodes: &[],
                new_inodes: &[],
                resources: FilesystemResources::default(),
            };
            let started = Instant::now();
            let mut sink = Bag::default();
            let result = disabled(|_| {
                let mut objects = FilesystemObjects::new(&bag, &mut sink);
                let backing_ref: &mut dyn layerfs_content::filesystem::references::backing::OrderingBacking = &mut backing;
                layerfs_content::filesystem::update_filesystem(&mut objects, &input, Some(backing_ref))
            })
            .expect("pipeline update");
            let mut merged = bag.clone();
            merged.objects.extend(sink.objects.clone());
            let objects = merged
                .objects
                .iter()
                .map(|(_, (role, bytes))| {
                    FinalizedObject::new(*role, bytes.clone()).expect("finalized")
                })
                .collect::<Vec<_>>();
            let saved = disabled(|timing| {
                let mut operation = store
                    .begin_save(timing.child("storage.begin"))
                    .map_err(|_| ContentError::IncompleteOperation)?;
                for object in objects {
                    operation
                        .accept(object, timing.child("storage.accept"))
                        .map_err(|_| ContentError::IncompleteOperation)?;
                }
                operation
                    .finish(timing.child("storage.finish"))
                    .map_err(|_| ContentError::IncompleteOperation)
            })
            .expect("save");
            let elapsed = started.elapsed();
            println!("mode pipeline case {}", config.case);
            println!("root {}", result.root.0);
            println!(
                "elapsed_ns {} (construction, bounded handoff and save acknowledgement)",
                elapsed.as_nanos()
            );
            println!(
                "constructed objects_emitted {} bytes_emitted {} saved inserted {} reused {} admitted {}",
                result.counters.objects.objects_emitted,
                result.counters.objects.bytes_emitted,
                saved.inserted,
                saved.reused,
                saved.inserted + saved.reused
            );
            let read_started = Instant::now();
            let read = readback(&merged, result.root.0, entries);
            println!(
                "readback separately labelled elapsed_ns {} result {}",
                read_started.elapsed().as_nanos(),
                read
            );
        }
        other => panic!("unknown mode {other}"),
    }
    println!("prepared_objects {}", bag.objects.len());
    println!("new_inodes {}", new_inodes.len());
}

fn readback(bag: &Bag, root: ObjectId, entries: usize) -> String {
    match FilesystemRead::new(bag, FilesystemRootId(root)) {
        Ok(mut read) => {
            let listing = read
                .list(&LogicalPath::root(), None, 8, 4096)
                .expect("listing");
            let names = listing
                .entries
                .iter()
                .map(|(name, serial)| format!("{}->{serial}", name.as_str()))
                .collect::<Vec<_>>()
                .join(",");
            format!("top_level=[{names}] expected_files={entries}")
        }
        Err(error) => format!("read failed: {error}"),
    }
}
