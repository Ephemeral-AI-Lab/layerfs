//! Whole-root topology qualification over real canonical trees.
//!
//! Positive roots are built by the public filesystem operations. Each refused
//! root is real canonical bytes the operations would never emit: its pages are
//! encoded by the public codecs and stored under their true identities, so the
//! refusal comes from the graph and not from a damaged object.
mod support;

use layerfs_content::filesystem::directory::{encode_directory_page, DirectoryPage};
use layerfs_content::filesystem::inode::{encode_inode_page, InodePage};
use layerfs_content::filesystem::{
    profile_id, qualify_root, scope_for_seed, DirectoryUpdate, FilesystemRoot, FilesystemRootId,
    InodeScope, InodeUpdate, QualificationWork, QualifiedRoot, RootContext,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    ConstructionRecordApply, ConstructionRecordChange, ConstructionRecordExpected,
    ConstructionRecordKey, ContentError, ContentResult, IndexedConstructionBacking, ObjectId,
    ObjectRole,
};
use std::collections::BTreeMap;
use support::filesystem::{name_of, synthetic, value, FailingReader, Session, TreeStore};

/// The production adapter's fixed job envelope.
const ENVELOPE: usize = 64 * 1024;

/// In-memory records with the neutral port's contract: one sorted unique
/// guarded atomic batch inside the retained-capacity envelope.
#[derive(Default)]
struct Records {
    values: BTreeMap<ConstructionRecordKey, Vec<u8>>,
    gets: u64,
    batches: u64,
    peak_batch_bytes: usize,
    peak_value_bytes: usize,
    fail_batch: Option<u64>,
}
impl IndexedConstructionBacking for Records {
    fn contains(&mut self, key: ConstructionRecordKey) -> ContentResult<bool> {
        Ok(self.values.contains_key(&key))
    }
    fn get(&mut self, key: ConstructionRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.gets += 1;
        Ok(self.values.get(&key).cloned())
    }
    fn apply(
        &mut self,
        changes: Vec<ConstructionRecordChange>,
    ) -> ContentResult<ConstructionRecordApply> {
        self.batches += 1;
        if self.fail_batch == Some(self.batches) {
            return Err(ContentError::ProviderFailure {
                what: "fixture original batch refusal",
            });
        }
        let mut bytes = changes.capacity() * std::mem::size_of::<ConstructionRecordChange>();
        let mut previous = None;
        for change in &changes {
            assert!(previous < Some(change.key), "batch order");
            previous = Some(change.key);
            if let ConstructionRecordExpected::ExactBytes(old) = &change.expected {
                bytes += old.capacity();
            }
            let value = change.value.as_ref().map_or(0, Vec::capacity);
            bytes += value;
            self.peak_value_bytes = self.peak_value_bytes.max(value);
        }
        assert!(bytes <= ENVELOPE, "batch retains {bytes} bytes");
        self.peak_batch_bytes = self.peak_batch_bytes.max(bytes);
        for (index, change) in changes.iter().enumerate() {
            let actual = self.values.get(&change.key);
            let holds = match &change.expected {
                ConstructionRecordExpected::Missing => actual.is_none(),
                ConstructionRecordExpected::ExactBytes(old) => actual == Some(old),
            };
            if !holds {
                return Ok(ConstructionRecordApply::NotApplied {
                    index,
                    key: change.key,
                    actual: actual.cloned(),
                });
            }
        }
        for change in changes {
            match change.value {
                Some(value) => self.values.insert(change.key, value),
                None => self.values.remove(&change.key),
            };
        }
        Ok(ConstructionRecordApply::Applied)
    }
    fn first_keys(&mut self, _: u32, _: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        panic!("qualification enumerates no record keys")
    }
}

fn scope() -> InodeScope {
    scope_for_seed([0x11; 32])
}
fn context(root: ObjectId) -> RootContext {
    RootContext {
        root: FilesystemRootId(root),
        scope: scope(),
        profile: profile_id(),
        root_serial: Some(1),
    }
}
fn qualify(
    store: &TreeStore,
    root: ObjectId,
) -> (ContentResult<QualifiedRoot>, QualificationWork, Records) {
    let (mut records, mut work) = (Records::default(), QualificationWork::default());
    let result = qualify_root(store, &mut records, &context(root), &mut work);
    (result, work, records)
}
fn refused(store: &TreeStore, root: ObjectId) -> ContentError {
    let (result, _, _) = qualify(store, root);
    result.expect_err("an unqualified root returned a proof")
}

/// One crafted inode: serial, kind, declared count and, for a directory, its
/// name-ordered listing.
type Crafted<'a> = (u64, InodeKind, u64, &'a [(&'a str, u64)]);
const FILE: InodeKind = InodeKind::RegularFile;
const DIR: InodeKind = InodeKind::Directory;

fn crafted_value(store: &mut TreeStore, inode: &Crafted<'_>) -> InodeValue {
    let (serial, kind, declared, listing) = *inode;
    let content_root = if kind == DIR {
        let entries = listing
            .iter()
            .map(|(name, serial)| (name_of(name), *serial))
            .collect();
        let page = encode_directory_page(&DirectoryPage::Leaf { entries }).expect("listing");
        store.insert(ObjectRole::DirectoryLeaf, page)
    } else {
        synthetic(&format!("qualify/content/{serial}"))
    };
    InodeValue {
        kind,
        namespace_ref_count: declared,
        content_root,
        metadata_root: synthetic("qualify/metadata"),
    }
}
fn crafted_root(store: &mut TreeStore, table: ObjectId, root_serial: u64) -> ObjectId {
    let root = FilesystemRoot::new(profile_id(), scope(), root_serial, table).expect("root");
    store.insert(
        ObjectRole::FilesystemRoot,
        root.encode().expect("root bytes"),
    )
}
/// Stores a single-leaf tree exactly as described, valid or not.
fn craft(inodes: &[Crafted<'_>]) -> (TreeStore, ObjectId) {
    let mut store = TreeStore::new();
    let entries = inodes
        .iter()
        .map(|inode| (inode.0, crafted_value(&mut store, inode)))
        .collect();
    let table = encode_inode_page(&InodePage::Leaf { entries }).expect("inode table");
    let table = store.insert(ObjectRole::InodeLeaf, table);
    let root = crafted_root(&mut store, table, 1);
    (store, root)
}

fn regular(label: &str) -> InodeValue {
    value(FILE, synthetic(label), synthetic("qualify/file-meta"))
}
fn directory() -> InodeValue {
    value(
        DIR,
        synthetic("qualify/unused"),
        synthetic("qualify/dir-meta"),
    )
}

/// A real multi-page tree: 700 files in one directory, a nested chain, an
/// empty directory, a symlink and one file bound three times in two parents.
fn built() -> (Session, u64, u64, u64) {
    let mut session = Session::new(1).expect("empty");
    let wide = session.allocate();
    let deep: Vec<u64> = (0..3).map(|_| session.allocate()).collect();
    let empty = session.allocate();
    let link = session.allocate();
    let shared = session.allocate();
    let files: Vec<u64> = (0..700).map(|_| session.allocate()).collect();
    let mut wide_names: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(index, serial)| (name_of(&format!("file-{index:04}")), Some(*serial)))
        .collect();
    wide_names.push((name_of("shared-a"), Some(shared)));
    wide_names.push((name_of("shared-b"), Some(shared)));
    wide_names.sort();
    let mut directories = vec![
        DirectoryUpdate {
            parent: 1,
            changes: vec![
                (name_of("deep"), Some(deep[0])),
                (name_of("empty"), Some(empty)),
                (name_of("link"), Some(link)),
                (name_of("wide"), Some(wide)),
            ],
        },
        DirectoryUpdate {
            parent: wide,
            changes: wide_names,
        },
        DirectoryUpdate {
            parent: deep[0],
            changes: vec![(name_of("inner"), Some(deep[1]))],
        },
        DirectoryUpdate {
            parent: deep[1],
            changes: vec![(name_of("innermost"), Some(deep[2]))],
        },
        DirectoryUpdate {
            parent: deep[2],
            changes: vec![(name_of("shared-c"), Some(shared))],
        },
        DirectoryUpdate {
            parent: empty,
            changes: Vec::new(),
        },
    ];
    directories.sort_by_key(|update| update.parent);
    let mut inodes: Vec<InodeUpdate> = [wide, deep[0], deep[1], deep[2], empty]
        .into_iter()
        .map(|serial| InodeUpdate {
            serial,
            value: directory(),
        })
        .collect();
    inodes.push(InodeUpdate {
        serial: link,
        value: value(
            InodeKind::Symlink,
            synthetic("qualify/target"),
            synthetic("qualify/link-meta"),
        ),
    });
    inodes.extend(files.iter().chain([&shared]).map(|serial| InodeUpdate {
        serial: *serial,
        value: regular(&format!("qualify/file/{serial}")),
    }));
    inodes.sort_by_key(|update| update.serial);
    let new_inodes: Vec<u64> = inodes.iter().map(|update| update.serial).collect();
    session
        .apply(&directories, &inodes, &new_inodes)
        .expect("built tree");
    (session, 1 + new_inodes.len() as u64, 6, 4 + 702 + 3)
}

#[test]
fn a_built_multi_page_root_qualifies_within_bounded_windows() {
    let (session, inodes, directories, bindings) = built();
    let (result, work, records) = qualify(&session.store, session.root);
    let proof = result.expect("built root qualifies");
    assert_eq!(
        (proof.inodes(), proof.directories(), proof.bindings()),
        (inodes, directories, bindings)
    );
    assert_eq!(proof.id(), FilesystemRootId(session.root));
    assert_eq!(proof.root(), session.value);
    assert_eq!(
        (work.inodes, work.directories, work.bindings),
        (inodes, directories, bindings)
    );
    // Several inode leaves and several listing windows were really read.
    assert!(work.inode.pages_read > 8, "{work:?}");
    assert!(work.directory.pages_read > 12, "{work:?}");
    // One context record, one row per inode and one queue entry per
    // non-root directory; nothing is enumerated or held resident.
    assert_eq!(records.values.len() as u64, 1 + inodes + (directories - 1));
    assert_eq!(work.record_batches, records.batches);
    assert_eq!(work.record_reads, records.gets);
    assert!(work.peak_batch_changes <= 128, "{work:?}");
    assert!(records.peak_batch_bytes <= ENVELOPE);
    assert_eq!(records.peak_value_bytes, 105);
    println!(
        "S9_R2_QUALIFY inodes={inodes} directories={directories} bindings={bindings} \
         inode_pages={} directory_pages={} record_reads={} record_batches={} \
         record_changes={} peak_batch_changes={} peak_batch_bytes={}",
        work.inode.pages_read,
        work.directory.pages_read,
        work.record_reads,
        work.record_batches,
        work.record_changes,
        work.peak_batch_changes,
        records.peak_batch_bytes
    );
}

#[test]
fn a_proof_admits_only_its_own_root_and_context() {
    let (session, ..) = built();
    let (result, _, _) = qualify(&session.store, session.root);
    let proof = result.expect("built root qualifies");
    let own = context(session.root);
    assert_eq!(proof.admit(&own), Ok(session.value));
    assert_eq!(
        proof.admit(&RootContext {
            root_serial: None,
            ..own
        }),
        Ok(session.value)
    );
    let (other, other_root) = craft(&[(1, DIR, 0, &[])]);
    let (other_proof, _, _) = qualify(&other, other_root);
    let other_proof = other_proof.expect("empty root qualifies");
    for (wrong, what) in [
        (context(other_root), "qualified root identity"),
        (
            RootContext {
                scope: scope_for_seed([0x22; 32]),
                ..own
            },
            "qualified root scope/profile",
        ),
        (
            RootContext {
                profile: synthetic("qualify/foreign-profile"),
                ..own
            },
            "qualified root scope/profile",
        ),
        (
            RootContext {
                root_serial: Some(2),
                ..own
            },
            "qualified root serial",
        ),
    ] {
        assert_eq!(
            proof.admit(&wrong),
            Err(ContentError::ScopeMismatch { what })
        );
    }
    assert_eq!(
        other_proof.admit(&own),
        Err(ContentError::ScopeMismatch {
            what: "qualified root identity"
        })
    );
}

#[test]
fn a_root_of_another_context_is_refused_before_any_record() {
    let (session, ..) = built();
    for (wrong, what) in [
        (
            RootContext {
                scope: scope_for_seed([0x22; 32]),
                ..context(session.root)
            },
            "qualified root scope/profile",
        ),
        (
            RootContext {
                profile: synthetic("qualify/foreign-profile"),
                ..context(session.root)
            },
            "qualified root scope/profile",
        ),
        (
            RootContext {
                root_serial: Some(9),
                ..context(session.root)
            },
            "qualified root serial",
        ),
    ] {
        let (mut records, mut work) = (Records::default(), QualificationWork::default());
        assert_eq!(
            qualify_root(&session.store, &mut records, &wrong, &mut work),
            Err(ContentError::ScopeMismatch { what })
        );
        assert!(records.values.is_empty() && records.batches == 0);
        assert_eq!(work, QualificationWork::default());
    }
    // An object that is not a filesystem root is not a root of any context.
    let (store, _) = craft(&[(1, DIR, 0, &[])]);
    let not_a_root =
        FilesystemRoot::decode(store.canonical(context(session.root).root.0).unwrap_or(&[]));
    assert!(not_a_root.is_err());
    assert_eq!(
        refused(&store, synthetic("qualify/absent-root")),
        ContentError::MissingObject
    );
}

#[test]
fn a_binding_to_a_serial_outside_the_table_is_refused() {
    let (store, root) = craft(&[(1, DIR, 0, &[("gone", 7), ("kept", 2)]), (2, FILE, 1, &[])]);
    assert_eq!(
        refused(&store, root),
        ContentError::InvalidRecord("qualified binding names a missing inode")
    );
    // The dangling name may also sit below the root.
    let (store, root) = craft(&[(1, DIR, 0, &[("d", 2)]), (2, DIR, 1, &[("gone", 9)])]);
    assert_eq!(
        refused(&store, root),
        ContentError::InvalidRecord("qualified binding names a missing inode")
    );
}

#[test]
fn a_directory_bound_twice_is_refused_in_one_parent_or_two() {
    let alias = ContentError::InvalidRecord("qualified directory has more than one binding");
    let (store, root) = craft(&[(1, DIR, 0, &[("a", 2), ("b", 2)]), (2, DIR, 1, &[])]);
    assert_eq!(refused(&store, root), alias);
    let (store, root) = craft(&[
        (1, DIR, 0, &[("a", 2), ("b", 3)]),
        (2, DIR, 1, &[("shared", 4)]),
        (3, DIR, 1, &[("shared", 4)]),
        (4, DIR, 1, &[]),
    ]);
    assert_eq!(refused(&store, root), alias);
}

#[test]
fn every_cycle_is_refused() {
    // A descendant names its ancestor: the ancestor gains a second binding.
    let (store, root) = craft(&[
        (1, DIR, 0, &[("a", 2)]),
        (2, DIR, 1, &[("b", 3)]),
        (3, DIR, 1, &[("back", 2)]),
    ]);
    assert_eq!(
        refused(&store, root),
        ContentError::InvalidRecord("qualified directory has more than one binding")
    );
    // A directory names itself.
    let (store, root) = craft(&[(1, DIR, 0, &[("a", 2)]), (2, DIR, 1, &[("self", 2)])]);
    assert_eq!(
        refused(&store, root),
        ContentError::InvalidRecord("qualified directory has more than one binding")
    );
    // Any name for the root directory closes a cycle through the root.
    let (store, root) = craft(&[(1, DIR, 0, &[("a", 2)]), (2, DIR, 1, &[("up", 1)])]);
    assert_eq!(
        refused(&store, root),
        ContentError::InvalidRecord("qualified binding names the root directory")
    );
    // An island whose two directories bind only each other is never reached,
    // so no walk follows it and the count closure refuses it.
    let (store, root) = craft(&[
        (1, DIR, 0, &[("a", 2)]),
        (2, FILE, 1, &[]),
        (3, DIR, 1, &[("to-four", 4)]),
        (4, DIR, 1, &[("to-three", 3)]),
    ]);
    let (result, work, _) = qualify(&store, root);
    assert_eq!(
        result,
        Err(ContentError::InvalidRecord("qualified inode binding count"))
    );
    assert_eq!((work.directories, work.bindings), (1, 1));
}

#[test]
fn unreachable_and_miscounted_inodes_are_refused() {
    let closure = ContentError::InvalidRecord("qualified inode binding count");
    // No directory names inode 3.
    let (store, root) = craft(&[
        (1, DIR, 0, &[("a", 2)]),
        (2, FILE, 1, &[]),
        (3, FILE, 1, &[]),
    ]);
    assert_eq!(refused(&store, root), closure);
    // An unreachable empty directory.
    let (store, root) = craft(&[(1, DIR, 0, &[]), (2, DIR, 1, &[])]);
    assert_eq!(refused(&store, root), closure);
    // Declared two, bound once.
    let (store, root) = craft(&[(1, DIR, 0, &[("a", 2)]), (2, FILE, 2, &[])]);
    assert_eq!(refused(&store, root), closure);
    // Declared one, bound twice.
    let (store, root) = craft(&[(1, DIR, 0, &[("a", 2), ("b", 2)]), (2, FILE, 1, &[])]);
    assert_eq!(
        refused(&store, root),
        ContentError::InvalidRecord("qualified binding count exceeds the inode")
    );
    // The exact declared count qualifies.
    let (store, root) = craft(&[(1, DIR, 0, &[("a", 2), ("b", 2)]), (2, FILE, 2, &[])]);
    let (result, ..) = qualify(&store, root);
    assert_eq!(result.expect("exact count").bindings(), 2);
}

#[test]
fn invalid_inode_rows_and_a_missing_root_inode_are_refused() {
    let invariant = ContentError::InvalidRecord("inode value invariant");
    // The root must be an unbound directory; nothing else may be unbound; only
    // a regular file may be bound more than once.
    for inodes in [
        &[(1, FILE, 0, &[][..])][..],
        &[(1, DIR, 1, &[])],
        &[(1, DIR, 0, &[("a", 2)]), (2, FILE, 0, &[])],
        &[(1, DIR, 0, &[("a", 2)]), (2, InodeKind::Symlink, 2, &[])],
        &[(1, DIR, 0, &[("a", 2)]), (2, DIR, 2, &[])],
    ] {
        let (store, root) = craft(inodes);
        assert_eq!(refused(&store, root), invariant);
    }
    // The root object names a serial its table does not hold.
    let (mut store, _) = craft(&[(2, FILE, 1, &[])]);
    let entries = vec![(2, crafted_value(&mut store, &(2, FILE, 1, &[])))];
    let table = encode_inode_page(&InodePage::Leaf { entries }).expect("inode table");
    let table = store.insert(ObjectRole::InodeLeaf, table);
    let root = crafted_root(&mut store, table, 1);
    let missing = ContentError::InvalidRecord("missing filesystem root inode");
    assert_eq!(refused(&store, root), missing);
    // A table with no row has no canonical encoding to qualify.
    assert_eq!(
        encode_inode_page(&InodePage::Leaf { entries: vec![] }),
        Err(ContentError::NonCanonicalPagePartition)
    );
}

#[test]
fn an_inode_leaf_overlapping_its_left_sibling_is_refused() {
    let mut store = TreeStore::new();
    let listing = crafted_value(&mut store, &(1, DIR, 0, &[]));
    let mut leaf = |serials: std::ops::RangeInclusive<u64>| {
        let entries: Vec<(u64, InodeValue)> = serials
            .map(|serial| match serial {
                1 => (1, listing),
                _ => (
                    serial,
                    crafted_value(&mut TreeStore::new(), &(serial, FILE, 1, &[])),
                ),
            })
            .collect();
        let maximum = entries.last().expect("rows").0;
        let page = encode_inode_page(&InodePage::Leaf { entries }).expect("leaf");
        (maximum, store.insert(ObjectRole::InodeLeaf, page))
    };
    // The right leaf restarts at the left leaf's last serial.
    let children = vec![leaf(1..=60), leaf(60..=120)];
    let branch = InodePage::Branch {
        level: 1,
        subtree_count: 121,
        children,
    };
    let table = store.insert(
        ObjectRole::InodeBranch,
        encode_inode_page(&branch).expect("branch"),
    );
    let root = crafted_root(&mut store, table, 1);
    let (result, work, _) = qualify(&store, root);
    assert_eq!(result, Err(ContentError::NonCanonicalOrdering));
    assert_eq!(work.inodes, 60);
}

#[test]
fn incoming_work_cannot_supply_missing_topology_or_inflate_a_proof() {
    let (store, root) = craft(&[
        (1, DIR, 0, &[("bound", 2)]),
        (2, FILE, 1, &[]),
        (3, FILE, 1, &[]),
    ]);
    let mut work = QualificationWork {
        bindings: 1,
        ..QualificationWork::default()
    };
    assert_eq!(
        qualify_root(&store, &mut Records::default(), &context(root), &mut work),
        Err(ContentError::InvalidRecord("qualified inode binding count"))
    );
    let (session, ..) = built();
    let mut expected = QualificationWork::default();
    let first = qualify_root(
        &session.store,
        &mut Records::default(),
        &context(session.root),
        &mut expected,
    )
    .unwrap();
    let mut previous = expected;
    let second = qualify_root(
        &session.store,
        &mut Records::default(),
        &context(session.root),
        &mut previous,
    )
    .unwrap();
    assert_eq!(first, second);
    assert_eq!(previous, expected);
}

#[test]
fn provider_and_record_failures_return_no_proof() {
    let (session, ..) = built();
    let own = context(session.root);
    // A scope that already holds a pass is not silently reused.
    let (mut records, mut work) = (Records::default(), QualificationWork::default());
    qualify_root(&session.store, &mut records, &own, &mut work).expect("first pass");
    assert_eq!(
        qualify_root(&session.store, &mut records, &own, &mut work),
        Err(ContentError::ProviderFailure {
            what: "filesystem backing precondition"
        })
    );
    // An original record refusal in each phase surfaces unchanged.
    for batch in [1, 2, 9, 14] {
        let mut records = Records {
            fail_batch: Some(batch),
            ..Records::default()
        };
        assert_eq!(
            qualify_root(
                &session.store,
                &mut records,
                &own,
                &mut QualificationWork::default()
            ),
            Err(ContentError::ProviderFailure {
                what: "fixture original batch refusal"
            })
        );
        assert_eq!(records.batches, batch);
    }
    // So does an object the provider cannot serve, wherever the pass meets it.
    for successes in [0, 1, 5, 40] {
        let reader = FailingReader::new(&session.store, successes);
        assert_eq!(
            qualify_root(
                &reader,
                &mut Records::default(),
                &own,
                &mut QualificationWork::default()
            ),
            Err(ContentError::MissingObject)
        );
    }
}
