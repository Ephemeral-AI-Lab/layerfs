//! Independent review diagnostics for the LayerFS C1 core and Stage 5 filesystem.
//!
//! Public API only. No product source is modified, included or recompiled from
//! here, and no test-only hook is used. Each probe prints a label so its output
//! binds to the code path it exercised.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemRead, FilesystemResources, FilesystemRootId, InodeScope,
    InodeUpdate, LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
    ObjectRole,
};

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
    ObjectId::for_bytes(format!("layerfs/review-diag/{label}").as_bytes())
}

fn scope() -> InodeScope {
    scope_for_seed([0x91; 32])
}

struct Fixture {
    bag: Bag,
    root: ObjectId,
}

/// Builds root (serial 1) with directory d (serial 2) holding `files` files.
fn fixture(files: usize) -> Fixture {
    fixture_checked(files).expect("fixture build")
}

/// The same build, with its result returned instead of panicking.
fn fixture_checked(files: usize) -> ContentResult<Fixture> {
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
    }?;
    bag.objects.extend(sink.objects);
    Ok(Fixture {
        bag,
        root: result.root.0,
    })
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
    let dir = std::env::temp_dir().join(format!("layerfs-review-diag-{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("backing directory");
    dir
}

/// P1: ordering work and simultaneously owned ordering bytes under forced spills.
fn probe_ordering_scaling() {
    println!("== P1 ordering scaling, maximum_pending_records = 64, base 4000 files ==");
    println!(
        "{:>6} {:>8} {:>13} {:>10} {:>10} {:>6} {:>11} {:>11} {:>11} {:>9}",
        "pairs", "spilled", "elapsed_ns", "rows_read", "rows_writ", "runs", "peak_owned", "held_now", "peak_back", "emitted"
    );
    for pairs in [250usize, 500, 1000, 2000] {
        let fixture = fixture(4000);
        let directories = rename_pairs(pairs);
        let input = FilesystemInput {
            base: Some(FilesystemRootId(fixture.root)),
            scope: scope(),
            root_serial: 1,
            directories: &directories,
            inodes: &[],
            new_inodes: &[],
            resources: FilesystemResources {
                maximum_pending_records: 64,
                ..FilesystemResources::default()
            },
        };
        let mut backing = FileBacking::new(backing_dir(&format!("p1-{pairs}")));
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
            "{:>6} {:>8} {:>13} {:>10} {:>10} {:>6} {:>11} {:>11} {:>11} {:>9}",
            pairs,
            result.counters.references.rows_spilled,
            elapsed,
            work.rows_read,
            work.rows_written,
            work.runs_created,
            work.peak_run_bytes,
            backing.held_bytes(),
            backing.peak_bytes(),
            result.counters.objects.objects_emitted
        );
    }
}

/// P2: a legal directory rename against the declared cycle-check entry ceiling.
fn probe_cycle_check_ceiling() {
    println!("== P2 legal rename of an existing directory vs MAXIMUM_CYCLE_CHECK_ENTRIES ==");
    for files in [4094usize, 4095, 4096, 4097, 4200] {
        let fixture = match fixture_checked(files) {
            Ok(fixture) => fixture,
            Err(error) => {
                println!("files {files:>5} -> BUILD REFUSED {error}");
                continue;
            }
        };
        let directories = [DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), None), (name("e"), Some(2))],
        }];
        let input = FilesystemInput {
            base: Some(FilesystemRootId(fixture.root)),
            scope: scope(),
            root_serial: 1,
            directories: &directories,
            inodes: &[],
            new_inodes: &[],
            resources: FilesystemResources::default(),
        };
        let mut backing = FileBacking::new(backing_dir(&format!("p2-{files}")));
        let mut sink = Bag::default();
        let result = {
            let reader = fixture.bag.clone();
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            let backing_ref: &mut dyn OrderingBacking = &mut backing;
            update_filesystem(&mut objects, &input, Some(backing_ref))
        };
        match result {
            Ok(result) => println!(
                "files {:>5} -> OK      validation entries_examined {} objects_read {}",
                files,
                result.counters.validation.entries_examined,
                result.counters.validation.objects_read
            ),
            Err(error) => println!("files {files:>5} -> REFUSED {error}"),
        }
    }
}

/// P3: listing bounds that cannot fit one row, and zero limits.
fn probe_listing_bounds() {
    println!("== P3 listing byte and count bounds ==");
    let fixture = fixture(8);
    for (count, bytes) in [
        (4usize, 4096usize),
        (4, 64),
        (4, 16),
        (4, 11),
        (4, 10),
        (4, 0),
        (0, 4096),
    ] {
        let path = LogicalPath::new("d").expect("path");
        let mut read =
            FilesystemRead::new(&fixture.bag, FilesystemRootId(fixture.root)).expect("read");
        match read.list(&path, None, count, bytes) {
            Ok(page) => println!(
                "count {count:>2} max_bytes {bytes:>4} -> OK entries {} continuation {}",
                page.entries.len(),
                page.continuation.is_some()
            ),
            Err(error) => println!("count {count:>2} max_bytes {bytes:>4} -> ERR {error}"),
        }
    }
    // Progressive pagination: four names per page until the directory is drained.
    let path = LogicalPath::new("d").expect("path");
    let mut read = FilesystemRead::new(&fixture.bag, FilesystemRootId(fixture.root)).expect("read");
    let mut after: Option<PathName> = None;
    let mut total = 0usize;
    let mut pages = 0usize;
    loop {
        let page = read
            .list(&path, after.as_ref(), 4, 4096)
            .expect("progressive listing");
        pages += 1;
        total += page.entries.len();
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
        if pages > 16 {
            break;
        }
    }
    println!("progressive pagination: {pages} pages, {total} entries");
}

/// P4: declared resource ceilings are enforced.
fn probe_resource_ceilings() {
    println!("== P4 declared resource ceilings ==");
    let fixture = fixture(64);
    let directories = rename_pairs(32);
    let cases: [(&str, FilesystemResources); 5] = [
        (
            "ordering_bytes=81",
            FilesystemResources {
                ordering_bytes: 81,
                ..FilesystemResources::default()
            },
        ),
        (
            "maximum_pending_records=1",
            FilesystemResources {
                maximum_pending_records: 1,
                ..FilesystemResources::default()
            },
        ),
        (
            "scratch_bytes=1023",
            FilesystemResources {
                scratch_bytes: 1023,
                ..FilesystemResources::default()
            },
        ),
        (
            "merge_buffer_bytes=0",
            FilesystemResources {
                merge_buffer_bytes: 0,
                ..FilesystemResources::default()
            },
        ),
        (
            "base_read_batch=0",
            FilesystemResources {
                base_read_batch: 0,
                ..FilesystemResources::default()
            },
        ),
    ];
    for (label, resources) in cases {
        let input = FilesystemInput {
            base: Some(FilesystemRootId(fixture.root)),
            scope: scope(),
            root_serial: 1,
            directories: &directories,
            inodes: &[],
            new_inodes: &[],
            resources,
        };
        let mut backing = FileBacking::new(backing_dir(&format!("p4-{}", label.replace('=', "-"))));
        let mut sink = Bag::default();
        let result = {
            let reader = fixture.bag.clone();
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            let backing_ref: &mut dyn OrderingBacking = &mut backing;
            update_filesystem(&mut objects, &input, Some(backing_ref))
        };
        match result {
            Ok(result) => println!(
                "{label:<26} -> OK spills {} peak_pending {}",
                result.counters.references.rows_spilled, result.counters.references.peak_pending
            ),
            Err(error) => println!("{label:<26} -> ERR {error}"),
        }
    }
}

/// P5: the inode serial range at its boundary.
fn probe_serial_range() {
    println!("== P5 inode serial range ==");
    for serial in [i64::MAX as u64, i64::MAX as u64 + 1] {
        let directories = [DirectoryUpdate {
            parent: 1,
            changes: vec![(name("f"), Some(serial))],
        }];
        let inodes = [
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
                serial,
                value: InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 0,
                    content_root: synthetic("file-content"),
                    metadata_root: synthetic("file-meta"),
                },
            },
        ];
        let input = FilesystemInput {
            base: None,
            scope: scope(),
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &[1, serial],
            resources: FilesystemResources::default(),
        };
        let reader = Bag::default();
        let mut sink = Bag::default();
        let result = {
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            build_filesystem(&mut objects, &input, None)
        };
        match result {
            Ok(_) => println!("binding serial {serial} -> OK"),
            Err(error) => println!("binding serial {serial} -> ERR {error}"),
        }
    }
}

/// P6: the attribute value bound reached through the public emit path.
fn probe_attribute_value_bound() {
    println!("== P6 attribute value bound ==");
    use layerfs_content::filesystem::attributes::emit_value;
    for bytes in [4usize, 1024 * 1024, 1024 * 1024 + 1] {
        let reader = Bag::default();
        let mut sink = Bag::default();
        let payload = vec![0x5au8; bytes];
        let result = {
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            emit_value(&mut objects, &payload)
        };
        match result {
            Ok(root) => println!("value bytes {bytes:>8} -> OK root {}", &root.to_string()[..16]),
            Err(error) => println!("value bytes {bytes:>8} -> ERR {error}"),
        }
    }
}

/// P7: an update that restates the base must not rewrite the tree.
fn probe_noop() {
    println!("== P7 restated bindings ==");
    let fixture = fixture(64);
    let directories = [DirectoryUpdate {
        parent: 2,
        changes: (0..64)
            .map(|index| (name(&format!("f{index:05}")), Some(index as u64 + 3)))
            .collect(),
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(fixture.root)),
        scope: scope(),
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let mut backing = FileBacking::new(backing_dir("p7"));
    let mut sink = Bag::default();
    let result = {
        let reader = fixture.bag.clone();
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        let backing_ref: &mut dyn OrderingBacking = &mut backing;
        update_filesystem(&mut objects, &input, Some(backing_ref))
    };
    match result {
        Ok(result) => println!(
            "restated root equals base: {} emitted {}
",
            result.root.0 == fixture.root, result.counters.objects.objects_emitted
        ),
        Err(error) => println!("ERR {error}"),
    }
}


/// P8: grow a tree past the whole-tree ceiling across operations, then rebind it.
fn probe_growth_past_ceiling() {
    println!("== P8 entry ceiling is per operation ==");
    let fixture = fixture_checked(4095).expect("base build");
    println!("base build: 4095 entries OK");
    // A second operation adds 100 more regular-file bindings to d. No stored
    // directory is rebound, so the whole-tree walk is not entered.
    let base_files = 4095usize;
    let extra_files = 100usize;
    let mut inodes: Vec<InodeUpdate> = Vec::new();
    let mut new_inodes: Vec<u64> = Vec::new();
    let mut changes: Vec<(PathName, Option<u64>)> = Vec::new();
    for index in 0..extra_files {
        let serial = (base_files + index) as u64 + 3;
        inodes.push(InodeUpdate {
            serial,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: synthetic(&format!("extra-{index:05}")),
                metadata_root: synthetic("file-meta"),
            },
        });
        new_inodes.push(serial);
        changes.push((name(&format!("g{index:05}")), Some(serial)));
    }
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    let directories = [DirectoryUpdate {
        parent: 2,
        changes,
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(fixture.root)),
        scope: scope(),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let mut backing = FileBacking::new(backing_dir("p8-add"));
    let mut sink = Bag::default();
    let grown = {
        let reader = fixture.bag.clone();
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        let backing_ref: &mut dyn OrderingBacking = &mut backing;
        update_filesystem(&mut objects, &input, Some(backing_ref))
    };
    let grown = match grown {
        Ok(result) => {
            println!(
                "add 100 entries: OK, validation entries_examined {}",
                result.counters.validation.entries_examined
            );
            result
        }
        Err(error) => {
            println!("add 100 entries: REFUSED {error}");
            return;
        }
    };
    // Now the same tree holds 4195 entries. Rebinding the directory itself
    // enters the whole-tree walk.
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("d"), None), (name("e"), Some(2))],
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(grown.root.0)),
        scope: scope(),
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let mut backing = FileBacking::new(backing_dir("p8-rename"));
    let mut grown_bag = fixture.bag.clone();
    grown_bag.objects.extend(sink.objects);
    let mut sink = Bag::default();
    let renamed = {
        let reader = grown_bag.clone();
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        let backing_ref: &mut dyn OrderingBacking = &mut backing;
        update_filesystem(&mut objects, &input, Some(backing_ref))
    };
    match renamed {
        Ok(result) => println!(
            "rename the 4195-entry directory: OK entries_examined {}",
            result.counters.validation.entries_examined
        ),
        Err(error) => println!("rename the 4195-entry directory: REFUSED {error}"),
    }
}

fn main() {
    let which: Vec<String> = std::env::args().skip(1).collect();
    let run = |label: &str| which.is_empty() || which.iter().any(|item| item == label);
    println!("stage15-diag: public-API review diagnostics");
    if run("p1") {
        probe_ordering_scaling();
    }
    if run("p2") {
        probe_cycle_check_ceiling();
    }
    if run("p3") {
        probe_listing_bounds();
    }
    if run("p4") {
        probe_resource_ceilings();
    }
    if run("p5") {
        probe_serial_range();
    }
    if run("p6") {
        probe_attribute_value_bound();
    }
    if run("p7") {
        probe_noop();
    }
    if run("p8") {
        probe_growth_past_ceiling();
    }
}
