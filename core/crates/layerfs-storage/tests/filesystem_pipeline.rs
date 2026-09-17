//! Real C2 composition: build a tree through C1, save it, reopen and read it.
//!
//! Every emitted tree object goes through the real admission path (including the
//! pooled inode lane), and the reopened Store serves the same root, directory
//! bindings, aliases, symlink target and attribute values. The integrated root is
//! also compared with an independent in-memory run of the same operation.

mod support;

use std::collections::BTreeMap;

use support::filesystem::{
    build_tree as build_shared_tree, database_state, save_bag, StoreReader as SharedStoreReader,
};
use support::{create_store, disabled, open_store, TempDir};

use layerfs_content::filesystem::attributes::build::build_attribute_tree;
use layerfs_content::filesystem::attributes::codec::AttributeEntry;
use layerfs_content::filesystem::attributes::keys::AttributeKey;
use layerfs_content::filesystem::attributes::patch::{apply_patches, AttributePatch};
use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::attributes::read::read_portable;
use layerfs_content::filesystem::attributes::value::emit_value;
use layerfs_content::filesystem::directory::codec::{decode_directory_page, DirectoryPage};
use layerfs_content::filesystem::inode::codec::{decode_inode_page, InodePage};
use layerfs_content::filesystem::symlink::{emit_symlink, SymlinkTarget};
use layerfs_content::filesystem::{
    build_filesystem, update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemRead, FilesystemResources, FilesystemRoot, FilesystemRootId, InodeUpdate,
    LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn synthetic(label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("layerfs/stage5-fixture/{label}").as_bytes())
}

/// Consumer and provider for objects a test builds in memory.
#[derive(Clone, Debug, Default)]
struct Bag {
    objects: BTreeMap<ObjectId, (ObjectRole, Vec<u8>, Vec<ObjectId>)>,
}

impl Bag {
    fn get(&self, id: ObjectId) -> ContentResult<Vec<u8>> {
        self.objects
            .get(&id)
            .map(|(_, bytes, _)| bytes.clone())
            .ok_or(ContentError::MissingObject)
    }
}

impl FinalizedConsumer for Bag {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let (id, role, bytes, references) = object.into_parts();
        self.objects.insert(id, (role, bytes, references));
        Ok(())
    }
}

impl AuthenticatedObjects for Bag {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter().map(|id| self.get(*id)).collect()
    }
}

/// Provider over one reopened Store: the product bridge, not a copy of it.
use layerfs_storage::StoreProvider as StoreReader;

/// A built tree: the root, the independent object set and the serials it used.
struct Fixture {
    root: ObjectId,
    scope: layerfs_content::filesystem::InodeScope,
    bag: Bag,
    file_root: ObjectId,
    symlink_root: ObjectId,
    attribute_root: ObjectId,
    serials: Vec<u64>,
}

/// Builds `/f` (aliased as `/alias`), `/d` and `/s` with a real file payload.
fn build_tree() -> ContentResult<Fixture> {
    let scope = layerfs_content::filesystem::scope_for_seed([0x77; 32]);
    let mut bag = Bag::default();
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let file_root = {
        let mut sink = Bag::default();
        let constructed = disabled(|scope| {
            layerfs_content::construct_bytes(
                policy,
                &policy.capacities(),
                b"filesystem pipeline payload",
                &mut sink,
                scope.child("construct"),
            )
        })?;
        bag.objects.extend(sink.objects);
        constructed.root
    };
    let symlink_root = {
        let reader = bag.clone();
        let mut sink = Bag::default();
        let id = {
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            emit_symlink(&mut objects, SymlinkTarget::new(b"target/path".to_vec())?)?
        };
        bag.objects.extend(sink.objects);
        id
    };
    let attribute_root = {
        let reader = bag.clone();
        let mut sink = Bag::default();
        let root = {
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            let metadata = PortableMetadata {
                mode: 0o644,
                mtime_seconds: 1_700_000_000,
                mtime_nanoseconds: 7,
            };
            let mode = emit_value(&mut objects, &metadata.mode_bytes(InodeKind::RegularFile)?)?;
            let mtime = emit_value(&mut objects, &metadata.mtime_bytes()?)?;
            let generic = emit_value(&mut objects, b"opaque attribute bytes")?;
            build_attribute_tree(
                &mut objects,
                vec![
                    Ok(AttributeEntry {
                        key: AttributeKey::new("portable".to_owned(), b"mode".to_vec())?,
                        value_root: mode,
                    }),
                    Ok(AttributeEntry {
                        key: AttributeKey::new("portable".to_owned(), b"mtime".to_vec())?,
                        value_root: mtime,
                    }),
                    Ok(AttributeEntry {
                        key: AttributeKey::new("user.example".to_owned(), b"note".to_vec())?,
                        value_root: generic,
                    }),
                ]
                .into_iter(),
            )?
            .0
        };
        bag.objects.extend(sink.objects);
        root
    };
    let serials = vec![1_u64, 2, 3, 5];
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![
                (name("alias"), Some(3)),
                (name("d"), Some(2)),
                (name("f"), Some(3)),
                (name("s"), Some(5)),
            ],
        },
        DirectoryUpdate {
            parent: 2,
            changes: Vec::new(),
        },
    ];
    let inodes = [
        InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("pipeline/unused"),
                metadata_root: synthetic("pipeline/root-meta"),
            },
        },
        InodeUpdate {
            serial: 2,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("pipeline/unused"),
                metadata_root: synthetic("pipeline/dir-meta"),
            },
        },
        InodeUpdate {
            serial: 3,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: file_root,
                metadata_root: attribute_root,
            },
        },
        InodeUpdate {
            serial: 5,
            value: InodeValue {
                kind: InodeKind::Symlink,
                namespace_ref_count: 0,
                content_root: symlink_root,
                metadata_root: synthetic("pipeline/link-meta"),
            },
        },
    ];
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &serials,
        resources: FilesystemResources::default(),
    };
    let reader = bag.clone();
    let mut sink = Bag::default();
    let result = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        build_filesystem(&mut objects, &input, None)?
    };
    bag.objects.extend(sink.objects);
    Ok(Fixture {
        root: result.root.0,
        scope,
        bag,
        file_root,
        symlink_root,
        attribute_root,
        serials,
    })
}

/// The direct references the global object-id order does not carry.
///
/// Only a branch page declares its children; a directory or inode leaf carries
/// `child: None` per row, and an inode value's content and metadata roots are
/// logical pointers the caller authorizes rather than declared references. This
/// helper therefore returns exactly what the product's own `references()` does.
fn references_of(canonical: &[u8]) -> Vec<ObjectId> {
    match decode_directory_page(canonical) {
        Ok(DirectoryPage::Branch { children, .. }) => {
            return children.into_iter().map(|(_, child)| child).collect();
        }
        Ok(DirectoryPage::Leaf { .. }) => return Vec::new(),
        Err(_) => {}
    }
    match decode_inode_page(canonical) {
        Ok(InodePage::Branch { children, .. }) => {
            children.into_iter().map(|(_, child)| child).collect()
        }
        Ok(InodePage::Leaf { .. }) | Err(_) => Vec::new(),
    }
}

/// Inserts objects in dependency order: every object is accepted after the ones
/// it directly references that this bag also holds.
///
/// The admission path validates each accepted object's direct references against
/// the objects already inserted by the same save, so a batch that mentions a
/// child after its parent fails with `MissingDependency` for a reason that has
/// nothing to do with the object graph: it is an ordering property of the batch.
fn save_in_dependency_order(
    store: &layerfs_storage::Store,
    fixture: &Fixture,
) -> Result<layerfs_storage::SaveOutcome, layerfs_storage::StorageError> {
    let mut remaining = fixture
        .bag
        .objects
        .iter()
        .map(|(id, value)| (*id, value.clone()))
        .collect::<Vec<_>>();
    let mut inserted: Vec<ObjectId> = Vec::new();
    let mut ordered = Vec::new();
    while !remaining.is_empty() {
        let mut progressed = false;
        let mut next = Vec::new();
        for (id, (role, bytes, references)) in remaining {
            let ready = references.iter().all(|reference| {
                inserted.contains(reference) || !fixture.bag.objects.contains_key(reference)
            });
            if ready {
                ordered.push(
                    FinalizedObject::new(role, bytes.clone())
                        .expect("finalized")
                        .with_references(references.clone()),
                );
                inserted.push(id);
                progressed = true;
            } else {
                next.push((id, (role, bytes, references)));
            }
        }
        assert!(
            progressed,
            "the dependency graph must be acyclic for this helper"
        );
        remaining = next;
    }
    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in ordered {
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    })
}

/// Saves every object a fixture holds through the real admission path.
fn save_fixture(store: &layerfs_storage::Store, fixture: &Fixture) -> u64 {
    let objects = fixture
        .bag
        .objects
        .iter()
        .map(|(_, (role, bytes, _))| FinalizedObject::new(*role, bytes.clone()).expect("finalized"))
        .collect::<Vec<_>>();
    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in objects {
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    })
    .expect("save");
    outcome.inserted + outcome.reused
}

#[test]
fn a_real_tree_saves_reopens_and_reads_back_exactly() {
    let temp = TempDir::new("stage5-filesystem-pipeline");
    let store = create_store(&temp.store_path("filesystem"));
    let fixture = build_tree().expect("build");
    let saved = save_fixture(&store, &fixture);
    assert!(saved >= 6, "the tree reached the store: {saved} objects");
    drop(store);

    let store = open_store(&temp.store_path("filesystem"));
    let reader = StoreReader::new(&store);
    let root_bytes = reader
        .read_canonical(fixture.root)
        .expect("root object is stored");
    let root = FilesystemRoot::decode(&root_bytes).expect("decode");
    assert_eq!(root.scope(), fixture.scope);
    assert_eq!(root.root_inode().serial(), 1);
    let mut read = FilesystemRead::new(&reader, FilesystemRootId(fixture.root)).expect("reader");
    let listing = read
        .list(&LogicalPath::root(), None, 16, 4096)
        .expect("list");
    assert_eq!(
        listing
            .entries
            .iter()
            .map(|(name, serial)| (name.as_str().to_owned(), *serial))
            .collect::<Vec<_>>(),
        vec![
            ("alias".to_owned(), 3),
            ("d".to_owned(), 2),
            ("f".to_owned(), 3),
            ("s".to_owned(), 5),
        ]
    );
    let stat = read.stat(&LogicalPath::new("f").unwrap()).expect("stat");
    assert_eq!(stat.namespace_ref_count, 2, "the alias survives the reopen");
    assert_eq!(stat.content_root, fixture.file_root);
    assert_eq!(stat.metadata_root, fixture.attribute_root);
    let alias = read
        .resolve(&LogicalPath::new("alias").unwrap())
        .expect("alias");
    assert_eq!(alias.serial, 3, "the alias is the same inode, not a copy");
    assert_eq!(alias.value.content_root, fixture.file_root);
    assert_eq!(alias.value.namespace_ref_count, 2);
    let mut payload = Vec::new();
    disabled(|scope| {
        layerfs_content::read_all(
            &reader,
            fixture.file_root,
            &mut payload,
            scope.child("read"),
        )
    })
    .expect("file read");
    assert_eq!(payload, b"filesystem pipeline payload");
    assert_eq!(
        read.readlink(&LogicalPath::new("s").unwrap())
            .expect("readlink")
            .as_bytes(),
        b"target/path"
    );
    let portable = read_portable(
        &reader,
        fixture.attribute_root,
        InodeKind::RegularFile,
        &mut Default::default(),
    )
    .expect("portable attributes");
    assert_eq!(portable.mode, 0o644);
    assert_eq!(portable.mtime_nanoseconds, 7);
    let _ = fixture.symlink_root;
    let _ = fixture.serials;
}

#[test]
fn a_patch_saved_to_the_store_keeps_untouched_attribute_roots() {
    let temp = TempDir::new("stage5-filesystem-patch");
    let store = create_store(&temp.store_path("filesystem"));
    let fixture = build_tree().expect("build");
    save_fixture(&store, &fixture);
    drop(store);
    let store = open_store(&temp.store_path("filesystem"));
    let reader = StoreReader::new(&store);
    let mut sink = Bag::default();
    let patches = vec![AttributePatch::Set {
        key: AttributeKey::new("user.example".to_owned(), b"note".to_vec()).unwrap(),
        value: b"patched".to_vec(),
    }];
    let (patch_root, work) = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        apply_patches(&reader, &mut objects, fixture.attribute_root, &patches).expect("patch")
    };
    assert_eq!(work.set, 1);
    assert_eq!(work.preserved, 2);
    assert_ne!(patch_root, fixture.attribute_root);
    for (id, (role, bytes, _)) in &sink.objects {
        let _ = id;
        let object = FinalizedObject::new(*role, bytes.clone()).expect("finalized");
        disabled(|scope| {
            let mut operation = store.begin_save(scope.child("storage.begin"))?;
            operation.accept(object)?;
            operation.finish(scope.child("storage.finish"))
        })
        .expect("save patch");
    }
    let reader2 = StoreReader::new(&store);
    let portable = read_portable(
        &reader2,
        patch_root,
        InodeKind::RegularFile,
        &mut Default::default(),
    )
    .expect("portable after patch");
    assert_eq!(portable.mode, 0o644);
}

#[test]
fn a_reopened_store_serves_an_update_of_the_saved_tree() {
    let temp = TempDir::new("stage5-filesystem-update");
    let store = create_store(&temp.store_path("filesystem"));
    let fixture = build_tree().expect("build");
    save_fixture(&store, &fixture);
    drop(store);
    let store = open_store(&temp.store_path("filesystem"));
    let reader = StoreReader::new(&store);
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![
            (name("alias"), None),
            (name("d"), Some(2)),
            (name("f"), Some(3)),
            (name("s"), Some(5)),
        ],
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(fixture.root)),
        scope: fixture.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let mut sink = Bag::default();
    let updated = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        update_filesystem(&mut objects, &input, None).expect("update on the reopened tree")
    };
    assert_ne!(updated.root.0, fixture.root);
    let mut merged = fixture.bag.clone();
    merged.objects.extend(sink.objects.clone());
    let mut read = FilesystemRead::new(&merged, FilesystemRootId(updated.root.0)).expect("reader");
    let stat = read.stat(&LogicalPath::new("f").unwrap()).expect("stat");
    assert_eq!(stat.namespace_ref_count, 1, "the removed alias is gone");
    assert!(matches!(
        read.stat(&LogicalPath::new("alias").unwrap()),
        Err(ContentError::MissingObject)
    ));
}

#[test]
fn the_saved_tree_reports_its_physical_footprint_and_reuses_it_unchanged() {
    let temp = TempDir::new("stage5-filesystem-footprint");
    let path = temp.store_path("filesystem");
    let store = create_store(&path);
    let built = build_shared_tree(1_200);
    let first = save_bag(&store, &built.bag).expect("first save");
    assert!(
        first.pool.leaves > 0,
        "the inode leaves went through the pooled lane"
    );
    assert!(first.packs_created >= 1, "the tree created packs");
    let after_first = database_state(&path);
    assert!(after_first.0 > 0 && after_first.1 >= 1);
    assert_eq!(
        after_first.2, after_first.1,
        "the watermark names the packs created"
    );
    let bytes_on_disk = std::fs::metadata(&path).expect("db metadata").len();
    assert!(bytes_on_disk > 0, "the Store has a real on-disk footprint");

    // The same objects save again as a pure reuse: no new rows, no new packs.
    let again = save_bag(&store, &built.bag).expect("reuse save");
    assert_eq!(again.inserted, 0, "every object is still catalogued");
    assert!(again.reused > 0);
    assert_eq!(again.packs_created, 0);
    assert_eq!(
        database_state(&path).0,
        after_first.0,
        "a reuse save adds no locator rows"
    );
    assert_eq!(database_state(&path).1, after_first.1);

    // Every role the tree used is readable back through a fresh connection.
    drop(store);
    let store = open_store(&path);
    let reader = SharedStoreReader::new(&store);
    let mut roles = std::collections::HashMap::new();
    for role in [
        ObjectRole::DirectoryLeaf,
        ObjectRole::InodeLeaf,
        ObjectRole::FilesystemRoot,
        ObjectRole::Symlink,
        ObjectRole::AttributeLeaf,
        ObjectRole::Chunk,
        ObjectRole::ExtentLeaf,
        ObjectRole::FileState,
    ] {
        roles.insert(role, built.bag.role_count(role));
    }
    for (role, count) in &roles {
        if *count == 0 {
            continue;
        }
        let id = built
            .bag
            .objects
            .iter()
            .find(|(_, (seen, _))| seen == role)
            .map(|(id, _)| *id)
            .expect("an object of this role");
        let bytes = reader
            .read_canonical(id)
            .unwrap_or_else(|error| panic!("role {role:?} did not read back: {error}"));
        assert_eq!(
            ObjectId::for_bytes(&bytes),
            id,
            "role {role:?} read back under its stored identity"
        );
    }
    // The pooled lane stores one leaf per 50-100 inodes, the profile's declared
    // occupancy: 1,205 inodes cannot need fewer than 13 or more than 25 leaves.
    let inodes = built.entries.len() + 5;
    let leaves = roles.get(&ObjectRole::InodeLeaf).copied().unwrap_or(0);
    assert!(
        leaves >= inodes.div_ceil(100) && leaves <= inodes.div_ceil(50),
        "{leaves} pooled leaves for {inodes} inodes is outside the declared occupancy"
    );
    assert_eq!(
        database_state(&path).2,
        after_first.2,
        "the reuse save leaves the watermark where it was"
    );
}

#[test]
fn a_caller_authorized_value_root_is_not_an_object_dependency() {
    // `admission-and-persistence.md` states the exclusion this test pins: a value
    // root inside a 73-byte inode value is a logical pointer the caller
    // authorizes, not a declared reference, so the Store accepts a tree whose
    // file inode names an object it does not hold. The save is acknowledged, the
    // value is stored, and reading that content root fails. Both outcomes are
    // asserted, so the documented behaviour cannot drift silently either way.
    let temp = TempDir::new("stage5-filesystem-authorized-root");
    let store = create_store(&temp.store_path("filesystem"));
    // The tree itself is real: a root directory with one binding, emitted and
    // saved through the real admission path. Only the file inode's value roots
    // are caller-supplied identities that no operation emitted.
    let scope = layerfs_content::filesystem::scope_for_seed([0x5c; 32]);
    let authorized_content = synthetic("pipeline-authorized/content");
    let authorized_metadata = synthetic("pipeline-authorized/metadata");
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("f"), Some(2))],
    }];
    let inodes = [
        InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("pipeline-authorized/unused"),
                metadata_root: authorized_metadata,
            },
        },
        InodeUpdate {
            serial: 2,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: authorized_content,
                metadata_root: authorized_metadata,
            },
        },
    ];
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[1, 2],
        resources: FilesystemResources::default(),
    };
    let mut bag = Bag::default();
    let reader = Bag::default();
    let mut sink = Bag::default();
    let result = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        build_filesystem(&mut objects, &input, None).expect("build")
    };
    for (id, (role, bytes, _)) in &sink.objects {
        bag.objects
            .insert(*id, (*role, bytes.clone(), references_of(bytes)));
    }
    assert!(
        !bag.objects.contains_key(&authorized_content),
        "no emitted object holds the authorized content root"
    );
    let fixture = Fixture {
        root: result.root.0,
        scope,
        bag,
        file_root: authorized_content,
        symlink_root: authorized_metadata,
        attribute_root: authorized_metadata,
        serials: vec![1, 2],
    };
    let outcome = save_in_dependency_order(&store, &fixture).expect("save");
    assert!(outcome.inserted >= 3, "the tree reached the store");
    drop(store);

    let store = open_store(&temp.store_path("filesystem"));
    let reader = StoreReader::new(&store);
    let root_bytes = reader
        .read_canonical(fixture.root)
        .expect("the root object is stored");
    let root = FilesystemRoot::decode(&root_bytes).expect("decode");
    let stored_table = reader
        .read_canonical(root.inode_table())
        .expect("the inode table is stored");
    let InodePage::Leaf { entries } = decode_inode_page(&stored_table).expect("inode table") else {
        panic!("the inode table is a leaf at this size");
    };
    let stored = entries
        .into_iter()
        .find(|(serial, _)| *serial == 2)
        .expect("the file inode is stored");
    assert_eq!(
        stored.1.content_root, authorized_content,
        "the stored value names the caller-authorized root"
    );
    assert!(
        matches!(
            reader.read_canonical(authorized_content),
            Err(ContentError::MissingObject)
        ),
        "the Store does not hold the object the value root names"
    );
    let mut read = FilesystemRead::new(&reader, FilesystemRootId(fixture.root)).expect("reader");
    let stat = read
        .stat(&LogicalPath::new("f").unwrap())
        .expect("the inode value is readable through a path");
    assert_eq!(stat.content_root, authorized_content);
    assert_eq!(stat.namespace_ref_count, 1);
}
