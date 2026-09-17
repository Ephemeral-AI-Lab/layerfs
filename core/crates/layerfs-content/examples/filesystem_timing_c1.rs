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
use layerfs_content::filesystem::attributes::patch::{
    apply_patches, visit_keys, AttributePatch, AttributePatchWork,
};
use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::attributes::value::emit_value;
use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::{
    build_filesystem_timed, update_filesystem_timed, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemPhases, FilesystemRead, FilesystemResources, FilesystemResult,
    FilesystemRootId, InodeUpdate, LogicalPath, ObjectWork, PathName,
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
        let parts = object.into_parts();
        let (id, role, bytes) = (parts.id, parts.role, parts.canonical);
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
    Option<ObjectId>,
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
    let mut inodes = inodes;
    let mut stored_attribute_tree = None;
    if case == "attributes" {
        // The patch route edits a stored attribute tree, so the base tree has to
        // carry one: the root inode's metadata root is a real tree holding the
        // portable mode entry, built here with the same public building blocks
        // the patch uses. Without it `apply_patches` would be handed a root no
        // provider can serve and the case would fail instead of measuring.
        let mut attribute_sink = Bag::default();
        let reader = bag.clone();
        let metadata_root = {
            let mut objects = FilesystemObjects::new(&reader, &mut attribute_sink);
            let mode = emit_value(
                &mut objects,
                &PortableMetadata {
                    mode: 0o755,
                    mtime_seconds: 1_700_000_000,
                    mtime_nanoseconds: 1,
                }
                .mode_bytes(InodeKind::Directory)
                .expect("mode bytes"),
            )
            .expect("mode value");
            layerfs_content::filesystem::attributes::build::build_attribute_tree(
                &mut objects,
                vec![Ok(AttributeEntry {
                    key: AttributeKey::new("portable".to_owned(), b"mode".to_vec())
                        .expect("mode key"),
                    value_root: mode,
                })]
                .into_iter(),
            )
            .expect("attribute tree")
            .0
        };
        bag.objects.extend(attribute_sink.objects);
        for update in &mut inodes {
            if update.serial == 1 {
                update.value.metadata_root = metadata_root;
            }
        }
        stored_attribute_tree = Some(metadata_root);
    }
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
    (bag, result.root.0, new_inodes, scope, stored_attribute_tree)
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

/// One timed operation's own result, object work and attribute change.
struct TimedRun {
    result: FilesystemResult,
    sink: Bag,
    attribute: Option<AttributeRun>,
}

/// The attribute change one `attributes` run performed inside the timed region.
struct AttributeRun {
    /// Root the patch returned.
    root: ObjectId,
    /// Patch work.
    work: AttributePatchWork,
    /// The metadata root the patch addressed.
    read_page_canonical: ObjectId,
    /// The inode page the root inode was looked up in.
    inode_page: ObjectId,
    /// Object work the patch's own boundary charged.
    objects: ObjectWork,
}

/// The inode page whose lookup resolved the root inode for this run.
fn inode_lookup_page(attribute: &AttributeRun) -> ObjectId {
    attribute.inode_page
}

/// Resolves the root inode's serial through the public inode page grammar.
///
/// The patch route needs the metadata root the updated tree carries, and the
/// route to it is the inode table: this walks the table from its root, decoding
/// each page with the public codec until it finds the serial, so the identity the
/// row reports is the page it actually read.
fn root_inode(reader: &Bag, table: ObjectId) -> ContentResult<(InodeValue, ObjectId)> {
    use layerfs_content::filesystem::inode::codec::{decode_inode_page, InodePage};
    let canonical = reader.read_canonical(table)?;
    match decode_inode_page(&canonical)? {
        InodePage::Leaf { entries, .. } => entries
            .into_iter()
            .find(|(key, _)| *key == 1)
            .map(|(_, value)| (value, table))
            .ok_or(ContentError::MissingObject),
        InodePage::Branch { children, .. } => children
            .into_iter()
            .find(|(key, _)| *key >= 1)
            .map(|(_, child)| root_inode(reader, child))
            .unwrap_or(Err(ContentError::MissingObject)),
    }
}

/// Reads the key this run set back through the patch and value read paths.
///
/// Two public operations, both bounded: the key is looked up in the tree the
/// patch returned, and the value root that lookup answers is read as bytes. A
/// tree that does not carry the key fails the run instead of printing a row.
fn attribute_readback(bag: &Bag, patched_root: ObjectId) -> String {
    let key = AttributeKey::new("user.example".to_owned(), b"note".to_vec())
        .expect("the key this run wrote");
    let mut found = None;
    let mut visit = |candidate: &AttributeKey, value: &ObjectId| -> ContentResult<()> {
        if candidate == &key {
            found = Some(*value);
        }
        Ok(())
    };
    if let Err(error) = visit_keys(bag, patched_root, &mut visit) {
        return format!("key walk failed: {error}");
    }
    let Some(value_root) = found else {
        return "attribute absent".to_owned();
    };
    match layerfs_content::filesystem::attributes::value::read_value(
        bag,
        value_root,
        layerfs_content::filesystem::limits::MAXIMUM_ATTRIBUTE_VALUE_BYTES,
    ) {
        Ok(stored) if stored == b"timed attribute value" => {
            format!(
                "{} bytes {:?}",
                stored.len(),
                String::from_utf8_lossy(&stored)
            )
        }
        Ok(stored) => format!("value differs: {} bytes", stored.len()),
        Err(error) => format!("value read failed: {error}"),
    }
}

fn main() {
    let config = parse();
    std::fs::create_dir_all(&config.output).expect("output directory");
    let (mut bag, root, _serials, scope, root_attribute_tree) = base(&config.case);
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
    let (outcome, report) = layerfs_telemetry::timer::Timing::record(
        "filesystem.update",
        |timing| -> ContentResult<TimedRun> {
            let reader = bag.clone();
            let mut sink = Bag::default();
            let phases = FilesystemPhases::new(timing);
            let updated = {
                let mut objects = FilesystemObjects::new(&reader, &mut sink);
                let backing_ref: &mut dyn OrderingBacking = &mut backing;
                let result =
                    update_filesystem_timed(&mut objects, &input, Some(backing_ref), &phases)?;
                (result, objects.work())
            };
            // `attributes` is the case named for an attribute change, so the
            // change happens **inside** this timed region: the patch addresses the
            // attribute tree the updated root inode carries, reads its stored page
            // through the provider, emits the new value and rebuilds the tree. Its
            // work is charged to the same row counters as the update's, so a row
            // that printed "0 objects read, 1 object emitted" for a base re-emit
            // cannot print that for this case.
            let attribute = match root_attribute_tree {
                Some(_) => {
                    let staged = {
                        let mut staged = reader.clone();
                        staged.objects.extend(sink.objects.clone());
                        staged
                    };
                    let (value, inode_page) = root_inode(&staged, updated.0.value.inode_table())?;
                    let (patched_root, patch_work) = {
                        let mut objects = FilesystemObjects::new(&staged, &mut sink);
                        let patched = apply_patches(
                            &staged,
                            &mut objects,
                            value.metadata_root,
                            &[AttributePatch::Set {
                                key: AttributeKey::new("user.example".to_owned(), b"note".to_vec())
                                    .map_err(|_| ContentError::InvalidRecord("attribute key"))?,
                                value: b"timed attribute value".to_vec(),
                            }],
                        )?;
                        (patched, objects.work())
                    };
                    Some(AttributeRun {
                        root: patched_root.0,
                        work: patched_root.1,
                        read_page_canonical: value.metadata_root,
                        inode_page,
                        objects: patch_work,
                    })
                }
                None => None,
            };
            Ok(TimedRun {
                result: updated.0,
                sink,
                attribute,
            })
        },
    );
    let elapsed = started.elapsed();
    let run = outcome.expect("update");
    let result = run.result;
    bag.objects.extend(run.sink.objects);
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
    let objects = match &run.attribute {
        Some(attribute) => ObjectWork {
            objects_read: result.counters.objects.objects_read + attribute.objects.objects_read,
            read_waves: result.counters.objects.read_waves + attribute.objects.read_waves,
            bytes_read: result.counters.objects.bytes_read + attribute.objects.bytes_read,
            objects_emitted: result.counters.objects.objects_emitted
                + attribute.objects.objects_emitted,
            bytes_emitted: result.counters.objects.bytes_emitted + attribute.objects.bytes_emitted,
        },
        None => result.counters.objects,
    };
    text.push_str(&format!(
        "objects read {} waves {} bytes {} emitted {} bytes {}\n",
        objects.objects_read,
        objects.read_waves,
        objects.bytes_read,
        objects.objects_emitted,
        objects.bytes_emitted
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
    if let Some(attribute) = &run.attribute {
        // Every canonical read this case performed: the inode page the root inode
        // is looked up in, and the attribute page the patch addressed. Without
        // that second boundary the row would print zero reads for work that read
        // a stored page.
        text.push_str(&format!(
            "attribute reads: inode page lookup {}, base attribute page {}\n",
            inode_lookup_page(attribute),
            attribute.read_page_canonical
        ));
        // The patch's own counters sit beside the update's, and the patched value
        // is read back through the public text route: the root inode's stored
        // attribute tree is asked for the key this run set, so the row proves the
        // timed work produced the value it names.
        let stored = attribute_readback(&bag, attribute.root);
        text.push_str(&format!(
            "attribute_patch: base tree {} set {} removed {} preserved {} base entries {} base pages {} values emitted {} | readback {stored}\n",
            attribute.read_page_canonical,
            attribute.work.set,
            attribute.work.removed,
            attribute.work.preserved,
            attribute.work.base_entries,
            attribute.work.base_pages,
            attribute.work.values_emitted
        ));
    }
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

    println!("prepared_objects {}", bag.objects.len());
}
