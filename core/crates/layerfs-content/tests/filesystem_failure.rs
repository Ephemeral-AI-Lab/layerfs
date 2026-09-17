//! Failure paths: reads, output, malformed pages, late errors and cancellation.
//!
//! Every case asserts the same contract: the operation reports the failure once,
//! publishes no new root, leaves the earlier successful root readable, and owns no
//! ordering resources afterwards. A late failure may have emitted private pages;
//! it may not have emitted a root and it may not have retried.

mod support;

use layerfs_content::filesystem::attributes::value::emit_value;
use layerfs_content::filesystem::{
    build_filesystem, update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemRead, FilesystemResources, FilesystemRootId, InodeUpdate, LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectRole,
};
use support::filesystem::{
    count_role, resources, synthetic, value, with_objects, FailingReader, RecordingBacking,
    Session, TempDir, TreeStore,
};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn regular(content: &str) -> InodeValue {
    value(
        InodeKind::RegularFile,
        synthetic(content),
        synthetic("failure/meta"),
    )
}

fn dir_value() -> InodeValue {
    value(
        InodeKind::Directory,
        synthetic("failure/unused"),
        synthetic("failure/dir-meta"),
    )
}

/// A consumer that refuses one exact object and counts what it was offered.
struct RefusingConsumer {
    refuse_at: u64,
    offered: u64,
    accepted: u64,
    accepted_roots: usize,
}

impl FinalizedConsumer for RefusingConsumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.offered += 1;
        if self.offered == self.refuse_at {
            return Err(ContentError::OutputRejected);
        }
        self.accepted += 1;
        if object.role() == ObjectRole::FilesystemRoot {
            self.accepted_roots += 1;
        }
        Ok(())
    }
}

/// A tree with one directory holding `count` files, built and saved in memory.
fn base_tree(count: usize) -> (Session, u64, Vec<u64>) {
    let mut session = Session::new(1).expect("empty");
    let directory = session.allocate();
    let serials = (0..count).map(|_| session.allocate()).collect::<Vec<_>>();
    let mut inodes = vec![InodeUpdate {
        serial: directory,
        value: dir_value(),
    }];
    let mut new = vec![directory];
    for (index, serial) in serials.iter().enumerate() {
        inodes.push(InodeUpdate {
            serial: *serial,
            value: regular(&format!("failure/content-{index:03}")),
        });
        new.push(*serial);
    }
    inodes.sort_by_key(|update| update.serial);
    new.sort_unstable();
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), Some(directory))],
        },
        DirectoryUpdate {
            parent: directory,
            changes: serials
                .iter()
                .enumerate()
                .map(|(index, serial)| (name(&format!("f{index:03}")), Some(*serial)))
                .collect(),
        },
    ];
    session
        .apply(&directories, &inodes, &new)
        .expect("base tree");
    (session, directory, serials)
}

#[test]
fn a_read_failure_at_each_boundary_publishes_no_root_and_keeps_the_old_one() {
    let (session, directory, serials) = base_tree(24);
    let updates = [DirectoryUpdate {
        parent: directory,
        changes: serials
            .iter()
            .take(3)
            .enumerate()
            .map(|(index, serial)| (name(&format!("f{index:03}")), Some(*serial)))
            .collect(),
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &updates,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let temp = TempDir::new("failure-reads");
    for budget in [0_u64, 1, 2, 3, 5] {
        let mut backing = RecordingBacking::new(temp.path());
        let reader = FailingReader::new(&session.store, budget);
        let mut sink = TreeStore::new();
        let outcome = {
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            update_filesystem(&mut objects, &input, Some(&mut backing))
        };
        assert!(
            outcome.is_err(),
            "a provider that fails after {budget} reads must fail the operation"
        );
        assert_eq!(
            count_role(&sink, ObjectRole::FilesystemRoot),
            0,
            "budget {budget}: no root may be published after a failed read"
        );
        // The earlier successful root is untouched and still readable.
        let mut read = FilesystemRead::new(&session.store, FilesystemRootId(session.root))
            .expect("old root still loads");
        assert_eq!(
            read.list(&LogicalPath::new("d").unwrap(), None, 64, 8192)
                .expect("old listing")
                .entries
                .len(),
            24
        );
        // The attempt owned no ordering resources afterwards.
        assert_eq!(backing.held_bytes(), 0);
        assert!(!backing.owns_storage() || backing.cleanup_failed());
    }
}

#[test]
fn a_refusing_consumer_ends_the_operation_once_without_a_root() {
    let (session, directory, serials) = base_tree(24);
    // A real change: twelve names move, so directory pages, the inode table and
    // the root all change and several objects are emitted.
    let mut changes = serials
        .iter()
        .take(12)
        .enumerate()
        .map(|(index, _)| (name(&format!("f{index:03}")), None))
        .collect::<Vec<_>>();
    changes.extend(
        serials
            .iter()
            .take(12)
            .enumerate()
            .map(|(index, serial)| (name(&format!("g{index:03}")), Some(*serial))),
    );
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    let updates = [DirectoryUpdate {
        parent: directory,
        changes,
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &updates,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    // First learn how many objects this exact update emits when nothing refuses.
    let mut counting = TreeStore::new();
    let clean = {
        let mut objects = FilesystemObjects::new(&session.store, &mut counting);
        update_filesystem(&mut objects, &input, None)
    }
    .expect("clean update");
    let emitted = counting.order().len() as u64;
    assert!(emitted >= 3, "the fixture must emit several objects");
    assert_eq!(count_role(&counting, ObjectRole::FilesystemRoot), 1);

    // Refuse each object in turn: every refusal must end the operation once, and
    // no root may be accepted.
    for refuse_at in 1..=emitted {
        let temp = TempDir::new("failure-consumer");
        let mut backing = RecordingBacking::new(temp.path());
        let mut consumer = RefusingConsumer {
            refuse_at,
            offered: 0,
            accepted: 0,
            accepted_roots: 0,
        };
        let outcome = {
            let mut objects = FilesystemObjects::new(&session.store, &mut consumer);
            update_filesystem(&mut objects, &input, Some(&mut backing))
        };
        assert!(
            matches!(outcome, Err(ContentError::OutputRejected)),
            "refusing object {refuse_at} of {emitted} must fail the operation"
        );
        assert_eq!(
            consumer.offered, refuse_at,
            "the operation must stop at the first refused object, not retry"
        );
        assert_eq!(consumer.accepted, refuse_at - 1);
        assert_eq!(
            consumer.accepted_roots, 0,
            "no root object was accepted after a refusal at {refuse_at}"
        );
        assert_eq!(backing.held_bytes(), 0);
        assert!(!backing.owns_storage() || backing.cleanup_failed());
    }
    let _ = clean;
}

#[test]
fn a_malformed_or_unauthenticated_demanded_page_is_rejected_without_a_root() {
    let (session, directory, serials) = base_tree(8);
    let updates = [DirectoryUpdate {
        parent: directory,
        changes: serials
            .iter()
            .take(2)
            .enumerate()
            .map(|(index, serial)| (name(&format!("f{index:03}")), Some(*serial)))
            .collect(),
    }];
    let table_root = session.value.inode_table();

    // 1. The demanded inode-table page is served under the wrong identity.
    for damage in ["byte flipped", "truncated", "trailing byte"] {
        let mut damaged = session.store.clone();
        let mut bytes = session
            .store
            .canonical(table_root)
            .expect("table bytes")
            .to_vec();
        match damage {
            "byte flipped" => bytes[13 + 12] = 1,
            "truncated" => {
                bytes.truncate(bytes.len() - 1);
            }
            _ => bytes.push(0),
        }
        damaged.overwrite(table_root, bytes);
        let input = FilesystemInput {
            base: Some(FilesystemRootId(session.root)),
            scope: session.scope,
            root_serial: 1,
            directories: &updates,
            inodes: &[],
            new_inodes: &[],
            resources: resources(),
        };
        let mut sink = TreeStore::new();
        let outcome = {
            let mut objects = FilesystemObjects::new(&damaged, &mut sink);
            update_filesystem(&mut objects, &input, None)
        };
        assert!(
            outcome.is_err(),
            "{damage}: a page that does not authenticate must fail the operation"
        );
        assert_eq!(count_role(&sink, ObjectRole::FilesystemRoot), 0);
    }

    // 2. A demanded root object that decodes to a foreign or truncated grammar.
    let original = session
        .store
        .canonical(session.root)
        .expect("root bytes")
        .to_vec();
    for damage in ["profile", "truncated", "trailing"] {
        let mut bytes = original.clone();
        match damage {
            "profile" => bytes[13 + 12] ^= 0x40,
            "truncated" => {
                bytes.truncate(13 + 8);
            }
            _ => bytes.push(0),
        }
        let mut damaged = session.store.clone();
        let id = damaged.insert(ObjectRole::FilesystemRoot, bytes);
        let input = FilesystemInput {
            base: Some(FilesystemRootId(id)),
            scope: session.scope,
            root_serial: 1,
            directories: &updates,
            inodes: &[],
            new_inodes: &[],
            resources: resources(),
        };
        let mut sink = TreeStore::new();
        let outcome = {
            let mut objects = FilesystemObjects::new(&damaged, &mut sink);
            update_filesystem(&mut objects, &input, None)
        };
        assert!(
            outcome.is_err(),
            "{damage}: a malformed demanded root must fail the operation"
        );
        assert_eq!(count_role(&sink, ObjectRole::FilesystemRoot), 0);
    }

    // The earlier successful root is still readable through its own provider.
    let mut read =
        FilesystemRead::new(&session.store, FilesystemRootId(session.root)).expect("old root");
    assert!(read.stat(&LogicalPath::root()).is_ok());
}

/// One refused input: a label, its directory changes, values and new identities.
type RefusedInput = (
    &'static str,
    Vec<DirectoryUpdate>,
    Vec<InodeUpdate>,
    Vec<u64>,
);

#[test]
fn invalid_input_is_refused_before_anything_is_emitted() {
    let (session, directory, serials) = base_tree(6);
    let cases: Vec<RefusedInput> = vec![
        (
            "unsorted",
            vec![DirectoryUpdate {
                parent: directory,
                changes: vec![
                    (name("f001"), Some(serials[1])),
                    (name("f000"), Some(serials[0])),
                ],
            }],
            Vec::new(),
            Vec::new(),
        ),
        (
            "duplicate",
            vec![DirectoryUpdate {
                parent: directory,
                changes: vec![(name("f000"), Some(serials[0])), (name("f000"), None)],
            }],
            Vec::new(),
            Vec::new(),
        ),
        (
            "unknown parent",
            vec![DirectoryUpdate {
                parent: 9_999,
                changes: vec![(name("x"), Some(serials[0]))],
            }],
            Vec::new(),
            Vec::new(),
        ),
        (
            "reused identity",
            vec![DirectoryUpdate {
                parent: directory,
                changes: vec![(name("f000"), Some(serials[0]))],
            }],
            Vec::new(),
            vec![serials[0]],
        ),
    ];
    for (label, directories, inodes, new_inodes) in cases {
        let input = FilesystemInput {
            base: Some(FilesystemRootId(session.root)),
            scope: session.scope,
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: resources(),
        };
        let mut sink = TreeStore::new();
        let outcome = {
            let mut objects = FilesystemObjects::new(&session.store, &mut sink);
            update_filesystem(&mut objects, &input, None)
        };
        assert!(outcome.is_err(), "{label} must be refused");
        assert_eq!(
            sink.order().len(),
            0,
            "{label}: a refused input must emit nothing at all"
        );
        let mut read =
            FilesystemRead::new(&session.store, FilesystemRootId(session.root)).expect("old root");
        assert!(
            read.stat(&LogicalPath::root()).is_ok(),
            "{label}: old root intact"
        );
    }
}

#[test]
fn cancellation_leaves_no_owned_ordering_resources_and_repeatable_state() {
    let (session, directory, serials) = base_tree(16);
    let updates = [DirectoryUpdate {
        parent: directory,
        changes: serials
            .iter()
            .take(4)
            .enumerate()
            .map(|(index, serial)| (name(&format!("f{index:03}")), Some(*serial)))
            .collect(),
    }];
    let temp = TempDir::new("failure-cancellation");
    // A failing reader aborts the attempt after a few demanded pages.
    let mut backing = RecordingBacking::new(temp.path());
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &updates,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources {
            maximum_pending_records: 1,
            ..resources()
        },
    };
    let reader = FailingReader::new(&session.store, 1);
    let mut sink = TreeStore::new();
    let outcome = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing))
    };
    assert!(outcome.is_err());
    let observed = backing.counters();
    assert!(
        observed.releases <= 1,
        "cancellation must not release the same backing twice"
    );
    assert_eq!(backing.held_bytes(), 0, "no ordering bytes stay owned");
    assert!(!backing.owns_storage(), "no run file survives cancellation");
    drop(backing);

    // A real change runs cleanly afterwards: a failed attempt leaves no poisoned
    // state behind.
    let removal = [DirectoryUpdate {
        parent: directory,
        changes: vec![(name("f000"), None)],
    }];
    let removal_input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &removal,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources {
            maximum_pending_records: 1,
            ..resources()
        },
    };
    let mut backing = RecordingBacking::new(temp.path());
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&session.store, &mut sink);
        update_filesystem(&mut objects, &removal_input, Some(&mut backing))
    }
    .expect("a clean attempt after a cancelled one");
    assert_eq!(count_role(&sink, ObjectRole::FilesystemRoot), 1);
    assert_ne!(result.root.0, session.root);
    assert_eq!(backing.counters().releases, 1);
    assert!(!backing.owns_storage());
}

#[test]
fn a_binding_serial_outside_the_stored_range_is_refused() {
    // The compact profile stores one serial in eight bytes and the tree grammar
    // requires it below `i64::MAX`. The root's identity is checked by its own
    // constructor; a serial that enters through a binding or a supplied value was
    // written into leaf bytes and refused later by a page decoder.
    let scope = layerfs_content::filesystem::scope_for_seed([0x44; 32]);
    let too_large = (i64::MAX as u64) + 1;
    for (directories, inodes, new_inodes) in [
        (
            vec![DirectoryUpdate {
                parent: 1,
                changes: vec![(name("x"), Some(too_large))],
            }],
            vec![InodeUpdate {
                serial: 1,
                value: dir_value(),
            }],
            vec![1_u64, too_large],
        ),
        (
            vec![DirectoryUpdate {
                parent: 1,
                changes: Vec::new(),
            }],
            vec![
                InodeUpdate {
                    serial: 1,
                    value: dir_value(),
                },
                InodeUpdate {
                    serial: too_large,
                    value: regular("failure/serial"),
                },
            ],
            vec![1_u64, too_large],
        ),
    ] {
        let input = FilesystemInput {
            base: None,
            scope,
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: resources(),
        };
        let mut store = TreeStore::new();
        let outcome = with_objects(&mut store, |objects| {
            build_filesystem(objects, &input, None)
        });
        assert!(
            matches!(outcome, Err(ContentError::InvalidRecord("inode serial"))),
            "a serial above the stored range must be refused: {outcome:?}"
        );
    }
}

#[test]
fn the_attribute_value_bound_is_the_chunk_maximum_at_its_boundary() {
    // The declared bound is the largest value one extent-only value root can
    // carry, and that figure is the chunk grammar's maximum. What this case pins
    // is the **boundary**, both sides of it: the last accepted value is emitted
    // and read back whole, and the first refused value is refused by the declared
    // bound rather than by whichever codec happened to run first. A case that
    // wrote some smaller value could not discriminate at the bound at all.
    let limit = layerfs_content::filesystem::limits::MAXIMUM_ATTRIBUTE_VALUE_BYTES;
    assert_eq!(
        limit,
        layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES,
        "the declared value bound is the chunk maximum, so a value it accepts has a \
         representation and a value it refuses has none"
    );

    let mut store = TreeStore::new();
    let at_limit = vec![0x5a_u8; limit];
    let outcome = with_objects(&mut store, |objects| emit_value(objects, &at_limit));
    let root = match outcome {
        Ok(root) => root,
        Err(error) => panic!("the value exactly at the declared bound must be accepted: {error}"),
    };
    let read_back = layerfs_content::filesystem::attributes::value::read_value(&store, root, limit)
        .expect("the value at the bound is read back whole");
    assert_eq!(
        read_back.len(),
        limit,
        "the read path returns exactly what was written"
    );
    assert!(
        read_back.iter().all(|byte| *byte == 0x5a),
        "the read path returns the written bytes, not a prefix"
    );

    let mut store = TreeStore::new();
    let over = vec![0x5a_u8; limit + 1];
    let outcome = with_objects(&mut store, |objects| emit_value(objects, &over));
    assert!(
        matches!(
            outcome,
            Err(ContentError::ObjectLimitExceeded { limit: refused, actual })
                if refused == limit && actual == limit + 1
        ),
        "one byte over the declared bound must be refused by the declared bound: {outcome:?}"
    );
}

/// R26/R33: a branch that misstates its summary is refused by the tree engine.
///
/// The role and flag negatives are codec-level and live in `filesystem_codec`.
/// These two are structural and live in the update engine: the parent's declared
/// `(subtree_count, subtree_bytes)` against the sum of the children it actually
/// read, and its own level and separator key against each child. Neither had any
/// case at all.
///
/// Reaching them needs a tree whose **identities are consistent**: the provider
/// contract requires a page served under an id whose bytes do not hash to it to be
/// refused as `IdentityMismatch`, so a damaged page cannot simply be written over
/// the old one. The damaged page is therefore re-encoded with the product's own
/// encoder, inserted under its own new identity, and the chain above it is moved
/// in two public steps: the inode leaf's value for the directory is re-encoded to
/// name the new page, and the filesystem root is rebuilt with
/// `FilesystemRoot::with_inode_table`. The directory holds 900 hard links to one
/// file inode so the inode table stays a single leaf and the chain has exactly
/// those two steps.
#[test]
fn a_branch_that_misstates_its_summary_is_refused_by_the_tree_engine() {
    use layerfs_content::filesystem::directory::codec::{
        decode_directory_page, encode_directory_page, DirectoryPage,
    };
    use layerfs_content::filesystem::inode::codec::{
        decode_inode_page, encode_inode_page, InodePage,
    };
    use layerfs_content::filesystem::root::FilesystemRoot;

    for (damage, expected) in [
        ("subtree count", "tree subtree summary"),
        ("level", "tree child summary"),
    ] {
        let mut session = Session::new(1).expect("empty");
        let directory = session.allocate();
        let file = session.allocate();
        let names = (0..900)
            .map(|index| name(&format!("f{index:04}")))
            .collect::<Vec<_>>();
        let directories = [
            DirectoryUpdate {
                parent: 1,
                changes: vec![(name("d"), Some(directory))],
            },
            DirectoryUpdate {
                parent: directory,
                changes: names.iter().map(|n| (n.clone(), Some(file))).collect(),
            },
        ];
        let inodes = [
            InodeUpdate {
                serial: directory,
                value: dir_value(),
            },
            InodeUpdate {
                serial: file,
                value: regular("failure/shared-content"),
            },
        ];
        let new = vec![directory, file];
        session
            .apply(&directories, &inodes, &new)
            .expect("base tree of 900 hard links");

        // The directory's inode value names its content root, which must be a
        // branch page for this directory to have one at all.
        let table_id = session.value.inode_table();
        let InodePage::Leaf { entries } =
            decode_inode_page(session.store.canonical(table_id).expect("inode table"))
                .expect("inode table leaf")
        else {
            panic!("a three-inode table is one leaf");
        };
        let content_root = entries
            .iter()
            .find(|(serial, _)| *serial == directory)
            .map(|(_, value)| value.content_root)
            .expect("the directory's inode value");
        let DirectoryPage::Branch {
            level,
            subtree_count,
            subtree_bytes,
            children,
        } = decode_directory_page(session.store.canonical(content_root).expect("content root"))
            .expect("a 900-entry directory has a branch")
        else {
            panic!("a 900-entry directory has a branch");
        };
        let damaged = match damage {
            "subtree count" => DirectoryPage::Branch {
                level,
                // One entry more than the children hold. The encoder accepts it
                // (it only requires the total to cover the row count); the engine
                // compares it with the children it read.
                subtree_count: subtree_count + 1,
                subtree_bytes,
                children,
            },
            _ => DirectoryPage::Branch {
                // A level its children cannot be one below.
                level: level + 1,
                subtree_count,
                subtree_bytes,
                children,
            },
        };
        let damaged_id = session.store.insert(
            ObjectRole::DirectoryBranch,
            encode_directory_page(&damaged).expect("re-encode the damaged branch"),
        );
        assert_ne!(damaged_id, content_root, "{damage}: a new identity");

        let moved_table = InodePage::Leaf {
            entries: entries
                .into_iter()
                .map(|(serial, mut value)| {
                    if serial == directory {
                        value.content_root = damaged_id;
                    }
                    (serial, value)
                })
                .collect(),
        };
        let moved_table_id = session.store.insert(
            ObjectRole::InodeLeaf,
            encode_inode_page(&moved_table).expect("re-encode the inode leaf"),
        );
        let moved_root =
            FilesystemRoot::decode(session.store.canonical(session.root).expect("root bytes"))
                .expect("root")
                .with_inode_table(moved_table_id);
        let moved_root_id = session.store.insert(
            ObjectRole::FilesystemRoot,
            moved_root.encode().expect("root"),
        );

        let updates = [DirectoryUpdate {
            parent: directory,
            changes: vec![(name("f0000"), Some(file))],
        }];
        let input = FilesystemInput {
            base: Some(FilesystemRootId(moved_root_id)),
            scope: session.scope,
            root_serial: 1,
            directories: &updates,
            inodes: &[],
            new_inodes: &[],
            resources: resources(),
        };
        let mut sink = TreeStore::new();
        let outcome = {
            let mut objects = FilesystemObjects::new(&session.store, &mut sink);
            update_filesystem(&mut objects, &input, None)
        };
        assert!(
            matches!(&outcome, Err(ContentError::InvalidRecord(what)) if *what == expected),
            "{damage}: expected InvalidRecord({expected:?}), got {outcome:?}"
        );
        assert_eq!(
            count_role(&sink, ObjectRole::FilesystemRoot),
            0,
            "{damage}: a refused operation publishes no root"
        );

        let mut read =
            FilesystemRead::new(&session.store, FilesystemRootId(session.root)).expect("old root");
        assert!(
            read.stat(&LogicalPath::new("d").expect("path")).is_ok(),
            "{damage}: the earlier root is untouched"
        );
    }
}
