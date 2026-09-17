//! Failure paths: reads, output, malformed pages, late errors and cancellation.
//!
//! Every case asserts the same contract: the operation reports the failure once,
//! publishes no new root, leaves the earlier successful root readable, and owns no
//! ordering resources afterwards. A late failure may have emitted private pages;
//! it may not have emitted a root and it may not have retried.

mod support;

use layerfs_content::filesystem::{
    update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemRead,
    FilesystemResources, FilesystemRootId, InodeUpdate, LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectRole,
};
use support::filesystem::{
    count_role, resources, synthetic, value, FailingReader, RecordingBacking, Session, TempDir,
    TreeStore,
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
