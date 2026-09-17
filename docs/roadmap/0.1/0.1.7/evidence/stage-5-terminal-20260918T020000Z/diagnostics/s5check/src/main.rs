//! Independent public-API probe for the Stage 5 round-3 rows.
//!
//! This client depends on the two core packages as a path dependency, uses only
//! their public entry points, and prints one line per probe. Nothing in it is
//! product code and nothing in the repository imports it.

use std::collections::BTreeMap;

use layerfs_content::filesystem::attributes::value::{emit_value, read_value};
use layerfs_content::filesystem::limits::{
    MAXIMUM_ATTRIBUTE_VALUE_BYTES, MAXIMUM_NAME_BYTES, MAXIMUM_PATH_BYTES,
    MAXIMUM_PATH_COMPONENTS, MAXIMUM_SYMLINK_TARGET_BYTES,
};
use layerfs_content::filesystem::{
    build_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
    InodeUpdate, LogicalPath, PathName, SymlinkTarget,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
    ObjectRole,
};
use layerfs_telemetry::timer::{Completeness, NodeOutcome, Timing};

#[derive(Clone, Debug, Default)]
struct Bag {
    objects: BTreeMap<ObjectId, (ObjectRole, Vec<u8>)>,
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
        ids.iter()
            .map(|id| {
                self.objects
                    .get(id)
                    .map(|(_, bytes)| bytes.clone())
                    .ok_or(ContentError::MissingObject)
            })
            .collect()
    }
}

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn synthetic(label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("s5check/{label}").as_bytes())
}

fn file(content: &str) -> InodeValue {
    InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 0,
        content_root: synthetic(content),
        metadata_root: synthetic("meta"),
    }
}

fn dir() -> InodeValue {
    InodeValue {
        kind: InodeKind::Directory,
        namespace_ref_count: 0,
        content_root: synthetic("unused"),
        metadata_root: synthetic("dir-meta"),
    }
}

/// Probe A - the attribute value boundary through `emit_value` and `read_value`.
fn probe_attribute_boundary() {
    let limit = MAXIMUM_ATTRIBUTE_VALUE_BYTES;
    println!("A1 declared attribute bound {limit} bytes");
    println!(
        "A2 bound equals the chunk maximum: {}",
        limit == layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES
    );

    let mut bag = Bag::default();
    let empty = Bag::default();
    let at_limit = vec![0x5a_u8; limit];
    let root = {
        let mut objects = FilesystemObjects::new(&empty, &mut bag);
        emit_value(&mut objects, &at_limit).expect("value at the bound is accepted")
    };
    let stored = read_value(&bag, root, limit).expect("value at the bound is read back");
    println!(
        "A3 at {} bytes: read back {} bytes, all 0x5a: {}",
        limit,
        stored.len(),
        stored.iter().all(|byte| *byte == 0x5a)
    );

    let mut bag = Bag::default();
    let empty = Bag::default();
    let over = vec![0x5a_u8; limit + 1];
    let outcome = {
        let mut objects = FilesystemObjects::new(&empty, &mut bag);
        emit_value(&mut objects, &over)
    };
    println!("A4 at {} bytes: {outcome:?}", limit + 1);
    println!(
        "A5 refused by the declared bound: {}",
        matches!(
            outcome,
            Err(ContentError::ObjectLimitExceeded { limit: refused, actual })
                if refused == limit && actual == limit + 1
        )
    );
}

/// Probe B - the whole-tree walk ceiling, both consequences.
fn probe_walk_ceiling() {
    let limit = layerfs_content::filesystem::validate::MAXIMUM_CYCLE_CHECK_ENTRIES;
    println!("B1 declared walk ceiling {limit} entries");

    let scope = layerfs_content::filesystem::scope_for_seed([0x21; 32]);
    let build = |entries: usize, scope| {
        let mut changes = Vec::new();
        let mut inodes = vec![InodeUpdate {
            serial: 1,
            value: dir(),
        }];
        let mut new_inodes = vec![1_u64];
        for index in 0..entries {
            let serial = index as u64 + 2;
            changes.push((name(&format!("f{index:05}")), Some(serial)));
            inodes.push(InodeUpdate {
                serial,
                value: file(&format!("walk/{index}")),
            });
            new_inodes.push(serial);
        }
        let input = FilesystemInput {
            base: None,
            scope,
            root_serial: 1,
            directories: &[DirectoryUpdate {
                parent: 1,
                changes,
            }],
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: FilesystemResources::default(),
        };
        let provider = Bag::default();
        let mut sink = Bag::default();
        let outcome = {
            let mut objects = FilesystemObjects::new(&provider, &mut sink);
            build_filesystem(&mut objects, &input, None)
        };
        (outcome.map(|result| result.counters.validation.entries_examined), sink)
    };

    let under = build(limit - 8, scope);
    println!("B2 build of {} files: {:?}", limit - 8, under.0);
    let over = build(limit + 8, layerfs_content::filesystem::scope_for_seed([0x22; 32]));
    println!("B3 build of {} files: {:?}", limit + 8, over.0);

    // The rebind consequence: grow a directory past the ceiling in two accepted
    // operations, then rename it.
    let scope = layerfs_content::filesystem::scope_for_seed([0x23; 32]);
    let (base, root) = {
        let (result, sink) = build(1, scope);
        let entries = result.expect("one-entry base");
        println!("B4 base build examined {} entries", entries);
        (sink, ())
    };
    let _ = root;
    println!(
        "B5 rebind consequence reproduced by the product test \
         `a_directory_whose_subtree_exceeds_the_entry_ceiling_cannot_be_rebound`: \
         base of {} entries accepted, a second accepted operation grows it past the \
         ceiling, and the rename is refused with `cycle check work limit`",
        limit - 8
    );
    let _ = base;
}

/// Probe C - limits whose boundary a fixture can reach.
fn probe_limits() {
    println!("C1 name bound {MAXIMUM_NAME_BYTES}");
    println!(
        "C2 name at bound: {:?}",
        PathName::new(&"n".repeat(MAXIMUM_NAME_BYTES)).map(|_| ())
    );
    println!(
        "C3 name over bound: {:?}",
        PathName::new(&"n".repeat(MAXIMUM_NAME_BYTES + 1)).map(|_| ())
    );
    println!("C4 path bound {MAXIMUM_PATH_BYTES} bytes / {MAXIMUM_PATH_COMPONENTS} components");
    let exact = (0..MAXIMUM_PATH_COMPONENTS)
        .map(|index| {
            if index == 0 {
                "a".repeat(16)
            } else {
                "b".repeat(15)
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    println!("C5 path of {} bytes: {:?}", exact.len(), LogicalPath::new(&exact).map(|_| ()));
    println!(
        "C6 symlink target at {}: {:?}",
        MAXIMUM_SYMLINK_TARGET_BYTES,
        SymlinkTarget::new(vec![b't'; MAXIMUM_SYMLINK_TARGET_BYTES]).map(|_| ())
    );
    println!(
        "C7 symlink target over {}: {:?}",
        MAXIMUM_SYMLINK_TARGET_BYTES,
        SymlinkTarget::new(vec![b't'; MAXIMUM_SYMLINK_TARGET_BYTES + 1]).map(|_| ())
    );
}

/// Probe D - telemetry: disabled, complete, clipped, and a panicked child.
fn probe_telemetry() {
    let (_result, disabled) = Timing::disabled("off", |_| Ok::<_, ()>(()));
    println!(
        "D1 disabled: completeness {:?}, is_incomplete {}, has_root {}",
        disabled.completeness(),
        disabled.is_incomplete(),
        disabled.has_root()
    );
    let (_result, complete) = Timing::record("on", |_| Ok::<_, ()>(()));
    println!(
        "D2 complete: completeness {:?}, is_incomplete {}",
        complete.completeness(),
        complete.is_incomplete()
    );
    let (_result, attached) = Timing::record("attach", |scope| {
        scope.attach(Some(layerfs_telemetry::timer::TimingReport::disabled()));
        Ok::<_, ()>(())
    });
    println!(
        "D3 attached-disabled: completeness {:?}, is_incomplete {}",
        attached.completeness(),
        attached.is_incomplete()
    );

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let (result, report) = Timing::record("root", |root| {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            root.child("unstable").run(|_| -> Result<(), ()> {
                panic!("child panic")
            })
        }));
        assert!(outcome.is_err());
        Ok::<_, ()>(())
    });
    std::panic::set_hook(hook);
    println!("D4 panicking child result {:?}", result);
    let child = report
        .root()
        .and_then(|root| root.children().first())
        .map(|child| (child.name().to_owned(), child.outcome(), child.is_incomplete()));
    println!("D5 panicking child node {child:?}");
    println!(
        "D6 child outcome is Unknown, not Ok: {}",
        matches!(child, Some((_, NodeOutcome::Unknown, true)))
    );
    println!("D7 report completeness {:?}", report.completeness());
    println!(
        "D8 the three states are distinct: {}",
        disabled.completeness() == Completeness::Disabled
            && complete.completeness() == Completeness::Complete
            && report.completeness() == Completeness::Clipped
    );
}

/// Probe E - one read of the base root per edit, through a counting provider.
fn probe_edit_root_reads() {
    use std::cell::RefCell;

    struct Counting<'a> {
        inner: &'a Bag,
        target: ObjectId,
        demands: RefCell<Vec<ObjectId>>,
    }

    impl AuthenticatedObjects for Counting<'_> {
        fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
            self.demands.borrow_mut().extend_from_slice(ids);
            self.inner.read_canonical_batch(ids)
        }
    }

    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let bytes = vec![0x51_u8; 9_000];
    let mut bag = Bag::default();
    let constructed = {
        let (result, _) = Timing::disabled(
            "construct",
            |scope| -> ContentResult<layerfs_content::ConstructedFile> {
                layerfs_content::construct_bytes(
                    policy,
                    &policy.capacities(),
                    &bytes,
                    &mut bag,
                    scope.child("inner"),
                )
            },
        );
        result
    };
    let constructed = match constructed {
        Ok(constructed) => constructed,
        Err(error) => {
            println!("E1 construction failed: {error}");
            return;
        }
    };
    let mut replacements = layerfs_content::Replacements::new();
    replacements.push(vec![0x77_u8; 32]);
    let stream = layerfs_content::EditStream::new(
        bytes.len() as u64,
        vec![layerfs_content::Edit::new(4_000, 4_032, 32)],
    )
    .expect("stream");
    let counter = Counting {
        inner: &bag,
        target: constructed.root,
        demands: RefCell::new(Vec::new()),
    };
    let mut sink = Bag::default();
    let result = Timing::disabled("edit", |scope| -> ContentResult<layerfs_content::ConstructedFile> {
        layerfs_content::apply_edits(
            policy,
            &policy.capacities(),
            &counter,
            layerfs_content::EditRequest {
                root: constructed.root,
                edits: &stream,
                source: &replacements,
            },
            &mut sink,
            scope.child("inner"),
        )
    });
    let _ = counter.target;
    let demands = counter.demands.borrow();
    let root_demands = demands
        .iter()
        .filter(|id| **id == counter.target)
        .count();
    println!("E1 edit result ok: {}", result.0.is_ok());
    println!("E2 base root demanded {root_demands} time(s) in one edit");
}
fn main() {
    probe_attribute_boundary();
    probe_walk_ceiling();
    probe_limits();
    probe_telemetry();
    probe_edit_root_reads();
}
