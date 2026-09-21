//! Shared external helpers for the storage-side filesystem targets.
//!
//! Nothing here is product code: it builds real trees through the C1 API, saves
//! them through the real admission path, and reads storage bookkeeping out of
//! band so a test can assert what an attempt actually left behind.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;

use layerfs_content::filesystem::attributes::codec::AttributeEntry;
use layerfs_content::filesystem::attributes::keys::AttributeKey;
use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::attributes::value::emit_value;
use layerfs_content::filesystem::symlink::{emit_symlink, SymlinkTarget};
use layerfs_content::filesystem::{
    build_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
    InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_storage::{SaveOutcome, StorageError, Store};

/// Consumer and provider for objects a test builds in memory.
#[derive(Clone, Debug, Default)]
pub struct Bag {
    /// Every object this test holds, by identity.
    pub objects: BTreeMap<ObjectId, (ObjectRole, Vec<u8>)>,
}

impl Bag {
    /// Reads one canonical object.
    pub fn get(&self, id: ObjectId) -> ContentResult<Vec<u8>> {
        self.objects
            .get(&id)
            .map(|(_, bytes)| bytes.clone())
            .ok_or(ContentError::MissingObject)
    }

    /// Objects with one role.
    pub fn role_count(&self, role: ObjectRole) -> usize {
        self.objects
            .values()
            .filter(|(seen, _)| *seen == role)
            .count()
    }

    /// Merges another bag into this one.
    pub fn absorb(&mut self, other: &Bag) {
        self.objects
            .extend(other.objects.iter().map(|(id, value)| (*id, value.clone())));
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

/// A provider that fails after a declared number of successful reads.
pub struct FailingReader<'a> {
    inner: &'a Bag,
    remaining: std::cell::Cell<u64>,
}

impl<'a> FailingReader<'a> {
    /// Fails every read once `successes` reads have returned.
    pub fn new(inner: &'a Bag, successes: u64) -> Self {
        Self {
            inner,
            remaining: std::cell::Cell::new(successes),
        }
    }
}

impl AuthenticatedObjects for FailingReader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let remaining = self.remaining.get();
        if remaining == 0 {
            return Err(ContentError::MissingObject);
        }
        self.remaining.set(remaining - 1);
        self.inner.read_canonical_batch(ids)
    }
}

/// A tree that was built through C1 and is ready to save.
pub struct Built {
    /// Every object the construction emitted, with the objects it referenced.
    pub bag: Bag,
    /// Root identity of the tree.
    pub root: ObjectId,
    /// Allocation scope.
    pub scope: layerfs_content::filesystem::InodeScope,
    /// The file whose content is a real constructed payload.
    pub file: u64,
    /// Its content root.
    pub file_root: ObjectId,
    /// Its attribute root, with portable and generic values.
    pub attribute_root: ObjectId,
    /// A stored symlink target.
    pub symlink: u64,
    /// Its content root.
    pub symlink_root: ObjectId,
    /// A second binding of `file`, exercising a derived count above one.
    pub alias: u64,
    /// A directory holding `entries` files.
    pub directory: u64,
    /// The files inside that directory.
    pub entries: Vec<u64>,
}

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn synthetic(label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("layerfs/stage5-fixture/{label}").as_bytes())
}

/// Builds a real tree: `/f` (also bound as `/alias`), `/d` with `entries` files,
/// `/s` and one attribute root holding portable and generic values.
pub fn build_tree(entries: usize) -> Built {
    let scope = layerfs_content::filesystem::scope_for_seed([0x77; 32]);
    let mut bag = Bag::default();
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let file_root = {
        let mut sink = Bag::default();
        let constructed =
            layerfs_telemetry::timer::Timing::disabled("fixture.construct", |scope| {
                layerfs_content::construct_bytes(
                    policy,
                    &policy.capacities(),
                    b"filesystem pipeline payload",
                    &mut sink,
                    scope.child("construct"),
                )
            })
            .0
            .expect("file construction");
        bag.absorb(&sink);
        constructed.root
    };
    let symlink_root = {
        let reader = bag.clone();
        let mut sink = Bag::default();
        let id = {
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            emit_symlink(
                &mut objects,
                SymlinkTarget::new(b"target/path".to_vec()).expect("target"),
            )
            .expect("symlink")
        };
        bag.absorb(&sink);
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
            let mode = emit_value(
                &mut objects,
                &metadata.mode_bytes(InodeKind::RegularFile).expect("mode"),
            )
            .expect("mode value");
            let mtime = emit_value(&mut objects, &metadata.mtime_bytes().expect("mtime"))
                .expect("mtime value");
            let generic =
                emit_value(&mut objects, b"opaque attribute bytes").expect("generic value");
            layerfs_content::filesystem::attributes::build::build_attribute_tree(
                &mut objects,
                vec![
                    Ok(AttributeEntry {
                        key: AttributeKey::new("portable".to_owned(), b"mode".to_vec())
                            .expect("key"),
                        value_root: mode,
                    }),
                    Ok(AttributeEntry {
                        key: AttributeKey::new("portable".to_owned(), b"mtime".to_vec())
                            .expect("key"),
                        value_root: mtime,
                    }),
                    Ok(AttributeEntry {
                        key: AttributeKey::new("user.example".to_owned(), b"note".to_vec())
                            .expect("key"),
                        value_root: generic,
                    }),
                ]
                .into_iter(),
            )
            .expect("attribute tree")
            .0
        };
        bag.absorb(&sink);
        root
    };
    let directory = 2_u64;
    let entries = (0..entries as u64)
        .map(|index| 6 + index)
        .collect::<Vec<_>>();
    let file = 3_u64;
    let alias = 4_u64;
    let symlink = 5_u64;
    let mut root_changes = vec![
        (name("alias"), Some(alias)),
        (name("d"), Some(directory)),
        (name("f"), Some(file)),
        (name("s"), Some(symlink)),
    ];
    root_changes.sort_by(|left, right| left.0.cmp(&right.0));
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: root_changes,
        },
        DirectoryUpdate {
            parent: directory,
            changes: entries
                .iter()
                .enumerate()
                .map(|(index, serial)| (name(&format!("e{index:04}")), Some(*serial)))
                .collect(),
        },
    ];
    let mut inodes = vec![
        InodeUpdate {
            serial: 1,
            value: directory_value("root"),
        },
        InodeUpdate {
            serial: directory,
            value: directory_value("d"),
        },
        InodeUpdate {
            serial: file,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: file_root,
                metadata_root: attribute_root,
            },
        },
        InodeUpdate {
            serial: alias,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: file_root,
                metadata_root: attribute_root,
            },
        },
        InodeUpdate {
            serial: symlink,
            value: InodeValue {
                kind: InodeKind::Symlink,
                namespace_ref_count: 0,
                content_root: symlink_root,
                metadata_root: synthetic("pipeline/link-meta"),
            },
        },
    ];
    for (index, serial) in entries.iter().enumerate() {
        inodes.push(InodeUpdate {
            serial: *serial,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: synthetic(&format!("pipeline/entry-{index:04}")),
                metadata_root: synthetic("pipeline/entry-meta"),
            },
        });
    }
    inodes.sort_by_key(|update| update.serial);
    let mut new_inodes = vec![1_u64, directory, file, alias, symlink];
    new_inodes.extend(entries.iter().copied());
    new_inodes.sort_unstable();

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
        build_filesystem(&mut objects, &input, None).expect("tree build")
    };
    bag.absorb(&sink);
    Built {
        bag,
        root: result.root.0,
        scope,
        file,
        file_root,
        attribute_root,
        symlink,
        symlink_root,
        alias,
        directory,
        entries,
    }
}

fn directory_value(label: &str) -> InodeValue {
    InodeValue {
        kind: InodeKind::Directory,
        namespace_ref_count: 0,
        content_root: synthetic("pipeline/unused"),
        metadata_root: synthetic(&format!("pipeline/{label}-meta")),
    }
}

/// Saves every object a fixture holds through the real admission path.
pub fn save_bag(store: &Store, bag: &Bag) -> Result<SaveOutcome, StorageError> {
    let objects = bag
        .objects
        .iter()
        .map(|(_, (role, bytes))| {
            FinalizedObject::new(*role, bytes.clone()).expect("finalized object")
        })
        .collect::<Vec<_>>();
    layerfs_telemetry::timer::Timing::disabled("fixture.save", |scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in objects {
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    })
    .0
}

/// Provider over one reopened Store.
pub struct StoreReader<'a> {
    store: &'a Store,
}

impl<'a> StoreReader<'a> {
    /// Wraps one store.
    pub const fn new(store: &'a Store) -> Self {
        Self { store }
    }
}

impl AuthenticatedObjects for StoreReader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        layerfs_telemetry::timer::Timing::disabled("fixture.read", |scope| {
            self.store.read_batch(ids, scope.child("storage.read"))
        })
        .0
        .map(|(values, _)| values)
        .map_err(|_| ContentError::MissingObject)
    }
}

/// Objects, packs and the published watermark, read out of band.
pub fn database_state(path: &Path) -> (i64, i64, i64) {
    let connection = rusqlite::Connection::open(path).expect("open out of band");
    let objects: i64 = connection
        .query_row("SELECT COUNT(*) FROM objects", [], |row| row.get(0))
        .expect("object count");
    let packs: i64 = connection
        .query_row("SELECT COUNT(*) FROM object_packs", [], |row| row.get(0))
        .expect("pack count");
    let watermark: i64 = connection
        .query_row(
            "SELECT retained_pack_ceiling FROM store_policy WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .expect("watermark");
    (objects, packs, watermark)
}

/// Installs an external trigger that rejects locator inserts once `after` rows
/// exist. Ordinary database behaviour, not a product fault switch.
pub fn reject_locators_after(path: &Path, after: i64) {
    let connection = rusqlite::Connection::open(path).expect("open for trigger");
    connection
        .execute_batch(&format!(
            "CREATE TRIGGER reject_late_locators BEFORE INSERT ON objects \
             WHEN (SELECT COUNT(*) FROM objects) >= {after} \
             BEGIN SELECT RAISE(ABORT, 'externally rejected late locator'); END;"
        ))
        .expect("install trigger");
}

/// Corrupts the pack that holds `id`, so a read of it cannot decode.
///
/// The fixture is a disposable copy owned by the test; no dependency or live
/// Store is modified.
pub fn corrupt_pack_containing(path: &Path, id: ObjectId) {
    let connection = rusqlite::Connection::open(path).expect("open for corruption");
    let pack: i64 = connection
        .query_row(
            "SELECT pack_id FROM objects WHERE object_id = ?1",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("the object has a locator");
    let mut data = super::read_pack_row(&connection, pack);
    let start = data.len() / 3;
    for index in (start..data.len()).step_by(23) {
        data[index] ^= 0xff;
    }
    super::write_pack_row(&connection, pack, &data);
}
