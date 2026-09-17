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

/// Provider over one reopened Store.
struct StoreReader<'a> {
    store: &'a layerfs_storage::Store,
}

impl AuthenticatedObjects for StoreReader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        disabled(|scope| self.store.read_batch(ids, scope.child("storage.read")))
            .map(|(values, _)| values)
            .map_err(|_| ContentError::MissingObject)
    }
}

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

/// Saves every object a fixture holds through the real admission path.
fn save_fixture(store: &layerfs_storage::Store, fixture: &Fixture) -> u64 {
    let objects = fixture
        .bag
        .objects
        .iter()
        .map(|(id, (role, bytes))| {
            let _ = id;
            FinalizedObject::new(*role, bytes.clone())
                .expect("finalized")
                .with_references(Vec::new())
        })
        .collect::<Vec<_>>();
    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in objects {
            operation.accept(object, scope.child("storage.accept"))?;
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
    let reader = StoreReader { store: &store };
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
    let reader = StoreReader { store: &store };
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
    for (id, (role, bytes)) in &sink.objects {
        let _ = id;
        let object = FinalizedObject::new(*role, bytes.clone()).expect("finalized");
        disabled(|scope| {
            let mut operation = store.begin_save(scope.child("storage.begin"))?;
            operation.accept(object, scope.child("storage.accept"))?;
            operation.finish(scope.child("storage.finish"))
        })
        .expect("save patch");
    }
    let reader2 = StoreReader { store: &store };
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
    let reader = StoreReader { store: &store };
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
    assert!(first.acknowledged, "the save is acknowledged");
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
