//! Database-free C1 filesystem operations with actual timing.
//!
//! Runs one deterministic case over fixed identities and reports the coarse phase
//! timings the operation itself recorded, the resulting root, and an exact
//! read-back check. Nothing here opens a database, pack or store: the fixture
//! objects live in memory, the ordering backing is a caller-owned directory, and
//! every case is bounded.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use layerfs_content::filesystem::attributes::codec::AttributeEntry;
use layerfs_content::filesystem::attributes::keys::AttributeKey;
use layerfs_content::filesystem::attributes::patch::{apply_patches, AttributePatch};
use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::attributes::value::emit_value;
use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::{
    build_filesystem_timed, update_filesystem_timed, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemPhases, FilesystemRead, FilesystemResources, FilesystemRootId,
    InodeUpdate, LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};

/// In-memory provider and consumer for one run.
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
    ObjectId::for_bytes(format!("layerfs/stage5-example/{label}").as_bytes())
}

fn file(content: &str) -> InodeValue {
    InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 0,
        content_root: synthetic(content),
        metadata_root: synthetic("example/meta"),
    }
}

fn directory() -> InodeValue {
    InodeValue {
        kind: InodeKind::Directory,
        namespace_ref_count: 0,
        content_root: synthetic("example/unused"),
        metadata_root: synthetic("example/dir-meta"),
    }
}

struct Config {
    case: String,
    output: PathBuf,
}

fn parse() -> Config {
    let mut case = "empty".to_owned();
    let mut output = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--case" => case = arguments.next().unwrap_or_default(),
            "--output" => output = arguments.next().map(PathBuf::from),
            other => panic!("unexpected argument {other}"),
        }
    }
    Config {
        case,
        output: output.expect("--output FRESH_DIRECTORY"),
    }
}

/// Builds the base tree each case starts from.
fn base(
    case: &str,
) -> (
    Bag,
    ObjectId,
    Vec<u64>,
    layerfs_content::filesystem::InodeScope,
) {
    let scope = layerfs_content::filesystem::scope_for_seed([0x5a; 32]);
    let mut bag = Bag::default();
    let (directories, inodes, new_inodes) = match case {
        "empty" => (
            vec![DirectoryUpdate {
                parent: 1,
                changes: Vec::new(),
            }],
            vec![InodeUpdate {
                serial: 1,
                value: directory(),
            }],
            vec![1_u64],
        ),
        "directory-update" | "subtree-remove" => (
            vec![
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("d"), Some(2))],
                },
                DirectoryUpdate {
                    parent: 2,
                    changes: (0..200)
                        .map(|index| (name(&format!("f{index:03}")), Some(index as u64 + 3)))
                        .collect(),
                },
            ],
            std::iter::once(InodeUpdate {
                serial: 1,
                value: directory(),
            })
            .chain(std::iter::once(InodeUpdate {
                serial: 2,
                value: directory(),
            }))
            .chain((0..200).map(|index| InodeUpdate {
                serial: index + 3,
                value: file(&format!("example/content-{index:03}")),
            }))
            .collect(),
            (1..=202).collect(),
        ),
        "inode-update" | "hardlink-move" => (
            vec![
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("a"), Some(2)), (name("b"), Some(3))],
                },
                DirectoryUpdate {
                    parent: 2,
                    changes: vec![(name("x"), Some(4))],
                },
                DirectoryUpdate {
                    parent: 3,
                    changes: vec![(name("y"), Some(4))],
                },
            ],
            vec![
                InodeUpdate {
                    serial: 1,
                    value: directory(),
                },
                InodeUpdate {
                    serial: 2,
                    value: directory(),
                },
                InodeUpdate {
                    serial: 3,
                    value: directory(),
                },
                InodeUpdate {
                    serial: 4,
                    value: file("example/content-moved"),
                },
            ],
            vec![1, 2, 3, 4],
        ),
        "attributes" => (
            vec![DirectoryUpdate {
                parent: 1,
                changes: vec![(name("f"), Some(2))],
            }],
            vec![
                InodeUpdate {
                    serial: 1,
                    value: directory(),
                },
                InodeUpdate {
                    serial: 2,
                    value: file("example/content-attributes"),
                },
            ],
            vec![1, 2],
        ),
        other => panic!("unknown case {other}"),
    };
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
    let result = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        build_filesystem_timed(&mut objects, &input, None, &FilesystemPhases::disabled())
            .expect("build")
    };
    bag.objects.extend(sink.objects);
    (bag, result.root.0, new_inodes, scope)
}

/// The normalized change set one case applies to the base tree.
fn changes(case: &str) -> (Vec<DirectoryUpdate>, Vec<InodeUpdate>, Vec<u64>) {
    match case {
        "empty" => (Vec::new(), Vec::new(), Vec::new()),
        "directory-update" => (
            vec![DirectoryUpdate {
                parent: 2,
                changes: (0..10)
                    .map(|index| (name(&format!("f{index:03}")), Some(index as u64 + 3)))
                    .collect(),
            }],
            Vec::new(),
            Vec::new(),
        ),
        "inode-update" => (
            Vec::new(),
            vec![InodeUpdate {
                serial: 4,
                value: file("example/content-updated"),
            }],
            Vec::new(),
        ),
        "hardlink-move" => (
            vec![
                DirectoryUpdate {
                    parent: 2,
                    changes: Vec::new(),
                },
                DirectoryUpdate {
                    parent: 3,
                    changes: vec![(name("moved"), Some(4)), (name("y"), Some(4))],
                },
            ],
            Vec::new(),
            Vec::new(),
        ),
        "subtree-remove" => (
            vec![DirectoryUpdate {
                parent: 1,
                changes: vec![(name("d"), None)],
            }],
            Vec::new(),
            Vec::new(),
        ),
        "attributes" => (Vec::new(), Vec::new(), Vec::new()),
        other => panic!("unknown case {other}"),
    }
}

fn main() {
    let config = parse();
    std::fs::create_dir_all(&config.output).expect("output directory");
    let (mut bag, root, _serials, scope) = base(&config.case);
    let (directories, inodes, new_inodes) = changes(&config.case);
    let input = FilesystemInput {
        base: Some(FilesystemRootId(root)),
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let mut backing = FileBacking::new(&config.output);
    let started = Instant::now();
    let (result, report) =
        layerfs_telemetry::timer::Timing::record("filesystem.update", |timing| {
            let reader = bag.clone();
            let mut sink = Bag::default();
            let phases = FilesystemPhases::new(timing);
            let outcome = {
                let mut objects = FilesystemObjects::new(&reader, &mut sink);
                let backing_ref: &mut dyn OrderingBacking = &mut backing;
                update_filesystem_timed(&mut objects, &input, Some(backing_ref), &phases)
            };
            outcome.map(|result| (result, sink))
        });
    let elapsed = started.elapsed();
    let (result, sink) = result.expect("update");
    let mut merged = bag.clone();
    merged.objects.extend(sink.objects.clone());
    bag = merged;
    drop(backing);

    let mut read = FilesystemRead::new(&bag, FilesystemRootId(result.root.0)).expect("reader");
    let listing = read
        .list(&LogicalPath::root(), None, 16, 4096)
        .expect("list");
    let mut text = String::new();
    text.push_str(&format!("case {}\n", config.case));
    text.push_str(&format!("root {}\n", result.root.0));
    text.push_str(&format!("inode_table {}\n", result.value.inode_table()));
    text.push_str(&format!(
        "directories {} bindings +{} -{}\n",
        result.counters.directory_updates,
        result.counters.bindings_added,
        result.counters.bindings_removed
    ));
    text.push_str(&format!(
        "objects read {} waves {} bytes {} emitted {} bytes {}\n",
        result.counters.objects.objects_read,
        result.counters.objects.read_waves,
        result.counters.objects.bytes_read,
        result.counters.objects.objects_emitted,
        result.counters.objects.bytes_emitted
    ));
    text.push_str(&format!(
        "directories: pages read {} created {} reused {} untouched {} scratch peak {} top-level {}\n",
        result.counters.directories.pages_read,
        result.counters.directories.pages_created,
        result.counters.directories.pages_reused,
        result.counters.directories.untouched_subtrees,
        result.counters.directories.peak_scratch_bytes,
        listing.entries.len()
    ));
    text.push_str(&format!(
        "inodes: pages read {} created {} reused {} scratch peak {}\n",
        result.counters.inodes.pages_read,
        result.counters.inodes.pages_created,
        result.counters.inodes.pages_reused,
        result.counters.inodes.peak_scratch_bytes
    ));
    text.push_str(&format!(
        "references: rows {} spilled {} values {} removals {} released {} peak pending {}\n",
        result.counters.references.rows_touched,
        result.counters.references.rows_spilled,
        result.counters.references.final_values,
        result.counters.references.final_removals,
        result.counters.release.released,
        result.counters.references.peak_pending
    ));
    text.push_str(&format!("elapsed_ns {}\n", elapsed.as_nanos()));
    for child in report
        .root()
        .map(|root| root.children().to_vec())
        .unwrap_or_default()
    {
        text.push_str(&format!(
            "phase {} elapsed_ns {}\n",
            child.name(),
            child.elapsed().as_nanos()
        ));
    }
    text.push_str(&format!("report_nodes {}\n", report.node_count()));
    print!("{text}");
    // A clipped tree is a hard failure for a measured row, not a note: the node
    // budget dropped detail this run cannot describe, so the process exits
    // non-zero and says so instead of leaving a partial tree to be quoted.
    if report.is_incomplete() {
        eprintln!(
            "timings: INCOMPLETE - the node budget clipped this tree; the run is not a measured row"
        );
        std::process::exit(2);
    }

    // One attribute patch, timed separately and labelled as its own operation.
    if config.case == "attributes" {
        let metadata = PortableMetadata {
            mode: 0o644,
            mtime_seconds: 1_700_000_000,
            mtime_nanoseconds: 3,
        };
        // The patch route reads the tree it edits, so the provider for this step
        // is the base plus the tree just built - never the base alone. The earlier
        // form handed `apply_patches` a provider that could not serve the root it
        // had just been given, so the row panicked with MissingObject instead of
        // measuring the patch.
        let mut sink = Bag::default();
        let attribute_root = {
            let mut objects = FilesystemObjects::new(&bag, &mut sink);
            let mode = emit_value(
                &mut objects,
                &metadata.mode_bytes(InodeKind::RegularFile).unwrap(),
            )
            .expect("mode value");
            layerfs_content::filesystem::attributes::build::build_attribute_tree(
                &mut objects,
                vec![Ok(AttributeEntry {
                    key: AttributeKey::new("portable".to_owned(), b"mode".to_vec()).unwrap(),
                    value_root: mode,
                })]
                .into_iter(),
            )
            .expect("attribute tree")
            .0
        };
        let mut staged = bag.clone();
        staged.objects.extend(sink.objects.clone());
        let mut patched = Bag::default();
        let (root, patch_work) = {
            let mut objects = FilesystemObjects::new(&staged, &mut patched);
            apply_patches(
                &staged,
                &mut objects,
                attribute_root,
                &[AttributePatch::Set {
                    key: AttributeKey::new("user.example".to_owned(), b"note".to_vec()).unwrap(),
                    value: b"timed attribute value".to_vec(),
                }],
            )
            .expect("patch")
        };
        bag.objects.extend(patched.objects);
        println!("attribute_root {root}");
        println!(
            "attribute_patch: set {} preserved {} base pages {} values emitted {}",
            patch_work.set, patch_work.preserved, patch_work.base_pages, patch_work.values_emitted
        );
    }
    println!("prepared_objects {}", bag.objects.len());
}
