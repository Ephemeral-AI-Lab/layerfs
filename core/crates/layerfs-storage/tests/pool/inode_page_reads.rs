//! A merge's point lookups read delta-stored inode branches at one chain per
//! demanded branch page (#360 item 3, check 3).
//!
//! A 20,000-file tree is saved whole, then ours and theirs each change one
//! different file's content root in their own Save, so both root branches are deltas against
//! the base's. `merge_roots` runs over a Store reader, the resolver every
//! Store read (Reader, read cache, Save) shares, through a wrapper that counts
//! the inode-branch pages demanded from the Store. The proof: every demanded
//! branch page reconstructs exactly one chain (`branch_requests` equals the
//! branch pages demanded), each chain at most the depth cap long, and the
//! merged tree holds both changes.
use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::inode::{lookup_many, InodeReadWork, InodeTable};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemResources, FilesystemRootId, InodeScope, InodeUpdate, PathName,
};
use layerfs_content::merge::{merge_roots, MergeInput, RulesFrom, Side};
use layerfs_content::object::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ConstructionPolicy, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_storage::{Reader, Storage, StoragePolicy, WriteOutcome};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[path = "../support/memory_metadata.rs"]
mod metadata;

const FILES: u64 = 20_000;

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

fn metadata(version: u64) -> InodeMetadata {
    InodeMetadata::Inline(PortableMetadata {
        mode: 0o644,
        mtime_seconds: 1_700_000_000 + version as i64,
        mtime_nanoseconds: 0,
    })
}

fn file(serial: u64, version: u64) -> InodeValue {
    InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&[serial, version].map(u64::to_le_bytes).concat()),
        metadata_root: metadata(version),
    }
}

fn root_value() -> InodeValue {
    InodeValue {
        kind: InodeKind::Directory,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(b"merge-reads/root"),
        metadata_root: InodeMetadata::Inline(PortableMetadata {
            mode: 0o755,
            mtime_seconds: 1_700_000_000,
            mtime_nanoseconds: 0,
        }),
    }
}

fn name(serial: u64) -> PathName {
    PathName::new(&format!("f{serial:06}")).unwrap()
}

/// Saves `objects` re-made without logical references (content roots are
/// synthetic), keeping their advisory predecessors; records branch ids.
fn save(
    storage: &Storage,
    objects: Vec<FinalizedObject>,
    branches: &mut BTreeSet<ObjectId>,
) -> WriteOutcome {
    let save = storage.begin_save().unwrap();
    for object in objects {
        let predecessors = object.predecessors().clone();
        let parts = object.into_parts();
        if parts.role == ObjectRole::InodeBranch {
            branches.insert(ObjectId::for_bytes(&parts.canonical));
        }
        save.accept(
            FinalizedObject::new(parts.role, parts.canonical)
                .unwrap()
                .with_predecessors(predecessors),
        )
        .unwrap();
    }
    save.finish().unwrap()
}

fn input<'a>(
    base: Option<ObjectId>,
    scope: InodeScope,
    directories: &'a [DirectoryUpdate],
    inodes: &'a [InodeUpdate],
    new_inodes: &'a [u64],
) -> FilesystemInput<'a> {
    FilesystemInput {
        base: base.map(FilesystemRootId),
        scope,
        root_serial: 1,
        directories,
        inodes,
        new_inodes,
        resources: FilesystemResources::default(),
    }
}

/// Changes file `serial` to `version` on `base` and saves the new tree.
fn change(
    storage: &Storage,
    scope: InodeScope,
    base: ObjectId,
    serial: u64,
    version: u64,
    branches: &mut BTreeSet<ObjectId>,
) -> (ObjectId, WriteOutcome) {
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let inodes = [InodeUpdate {
        serial,
        value: file(serial, version),
    }];
    let root = {
        let mut objects = FilesystemObjects::new(&reader, &mut emitted);
        update_filesystem(
            &mut objects,
            &input(Some(base), scope, &[], &inodes, &[]),
            None,
        )
        .unwrap()
        .root
        .0
    };
    drop(reader);
    (root, save(storage, emitted.0, branches))
}

/// Serves objects the merge emitted, else demands them from the Store and
/// counts the inode-branch pages among them.
struct Counting<'a> {
    store: &'a Reader<'a>,
    accepted: &'a RefCell<BTreeMap<ObjectId, Vec<u8>>>,
    branches: &'a BTreeSet<ObjectId>,
    branch_demands: Cell<u64>,
}
impl AuthenticatedObjects for Counting<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let accepted = self.accepted.borrow();
        let wanted: Vec<ObjectId> = ids
            .iter()
            .filter(|id| !accepted.contains_key(id))
            .copied()
            .collect();
        let demanded = wanted
            .iter()
            .filter(|id| self.branches.contains(id))
            .count() as u64;
        self.branch_demands
            .set(self.branch_demands.get() + demanded);
        let mut stored = self.store.read_canonical_batch(&wanted)?.into_iter();
        Ok(ids
            .iter()
            .map(|id| match accepted.get(id) {
                Some(bytes) => bytes.clone(),
                None => stored.next().expect("demanded"),
            })
            .collect())
    }
}
struct Accepting<'a>(&'a RefCell<BTreeMap<ObjectId, Vec<u8>>>);
impl FinalizedConsumer for Accepting<'_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0
            .borrow_mut()
            .insert(object.id(), object.canonical().to_vec());
        Ok(())
    }
}

#[test]
fn a_merge_reconstructs_one_branch_chain_per_demanded_branch_page() {
    let shared = Arc::new(metadata::MemoryMetadata::default());
    shared.state.lock().unwrap().policy = StoragePolicy::default();
    let storage = Storage::new(shared).unwrap();
    let scope = scope_for_seed([0x36; 32]);
    let mut branches = BTreeSet::new();
    // The base: FILES files bound directly under the root.
    let serials: Vec<u64> = (1..=FILES + 1).collect();
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: (2..=FILES + 1)
            .map(|serial| (name(serial), Some(serial)))
            .collect(),
    }];
    let inodes: Vec<InodeUpdate> = std::iter::once(InodeUpdate {
        serial: 1,
        value: root_value(),
    })
    .chain((2..=FILES + 1).map(|serial| InodeUpdate {
        serial,
        value: file(serial, 0),
    }))
    .collect();
    let base = {
        let reader = storage.reader().unwrap();
        let mut emitted = Emitted::default();
        let root = {
            let mut objects = FilesystemObjects::new(&reader, &mut emitted);
            build_filesystem(
                &mut objects,
                &input(None, scope, &directories, &inodes, &serials),
                None,
            )
            .unwrap()
            .root
            .0
        };
        drop(reader);
        save(&storage, emitted.0, &mut branches);
        root
    };
    let (ours_serial, theirs_serial) = (2, FILES + 1);
    let (ours, ours_saved) = change(&storage, scope, base, ours_serial, 1, &mut branches);
    let (theirs, theirs_saved) = change(&storage, scope, base, theirs_serial, 1, &mut branches);
    for saved in [&ours_saved, &theirs_saved] {
        assert!(
            saved.delta.branch.delta_branches >= 1,
            "{:?}",
            saved.delta.branch
        );
    }
    // The merge, over a fresh Store reader.
    let reader = storage.reader().unwrap();
    let accepted = RefCell::new(BTreeMap::new());
    let counting = Counting {
        store: &reader,
        accepted: &accepted,
        branches: &branches,
        branch_demands: Cell::new(0),
    };
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut next = FILES + 2;
    let mut fresh = || {
        next += 1;
        Ok(next - 1)
    };
    let merged = layerfs_telemetry::timer::Timing::disabled("merge", |timing| {
        merge_roots(
            &counting,
            &mut Accepting(&accepted),
            &MergeInput {
                base: FilesystemRootId(base),
                ours: FilesystemRootId(ours),
                theirs: FilesystemRootId(theirs),
                labels: ["ours", "theirs"],
                rules_from: RulesFrom::Theirs,
                policy,
                capacities: &capacities,
                hints: None,
                decide_from: Side::Ours,
                bound: None,
            },
            &mut fresh,
            None,
            timing,
        )
    })
    .0
    .unwrap();
    assert!(merged.conflicts.is_empty(), "{:?}", merged.conflicts);
    let pool = reader.pooled_read_counters();
    let demanded = counting.branch_demands.get();
    println!(
        "merge: branch pages demanded {demanded} chains {} edges {} applies {} leaves {}",
        pool.branch_requests,
        pool.branch_chain_edges,
        pool.branch_delta_applies,
        pool.leaf_requests
    );
    assert!(demanded >= 1, "the merge read inode branches");
    assert_eq!(
        pool.branch_requests, demanded,
        "one chain per demanded branch page: {pool:?}"
    );
    assert!(
        pool.branch_chain_edges <= 8 * pool.branch_requests,
        "{pool:?}"
    );
    assert!(
        pool.branch_delta_applies >= 1,
        "delta branches were rebuilt: {pool:?}"
    );
    // The merged tree holds both changes and nothing else changed.
    let canonical = counting.read_canonical_batch(&[merged.root.0]).unwrap();
    let merged_root = layerfs_content::filesystem::FilesystemRoot::decode(&canonical[0]).unwrap();
    let wanted: Vec<u64> = (2..=FILES + 1).collect();
    let values = lookup_many(
        &counting,
        InodeTable {
            root: merged_root.inode_table(),
            root_serial: 1,
        },
        &wanted,
        &mut InodeReadWork::default(),
    )
    .unwrap();
    for (serial, value) in wanted.iter().zip(values) {
        let version = u64::from(*serial == ours_serial || *serial == theirs_serial);
        assert_eq!(value, Some(file(*serial, version)), "serial {serial}");
    }
}
