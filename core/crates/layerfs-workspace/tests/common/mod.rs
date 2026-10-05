//! Shared content-built full-root fixture; test memory is external to product.
#![allow(dead_code)]
use layerfs_content::filesystem::{
    attributes::{
        build_attribute_tree, emit_value, AttributeEntry, AttributeKey, PortableMetadata,
    },
    build_filesystem, scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemResources, FilesystemRootId, InodeScope, InodeUpdate, PathName, SymlinkTarget,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    construct_bytes, AuthenticatedObjects, ConstructionPolicy, ContentError, ContentResult,
    FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_telemetry::timer::Timing;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone, Default)]
pub struct Store {
    pub objects: Arc<Mutex<BTreeMap<ObjectId, Vec<u8>>>>,
    pub demand: Arc<AtomicU64>,
    pub lengths: Arc<Mutex<BTreeMap<ObjectId, u64>>>,
    /// Canonical bytes served upstream, to tell metadata demand from payload.
    pub served: Arc<AtomicU64>,
}
impl AuthenticatedObjects for Store {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demand.fetch_add(ids.len() as u64, Ordering::Relaxed);
        let objects = self.objects.lock().unwrap();
        ids.iter()
            .map(|id| {
                let value = objects
                    .get(id)
                    .cloned()
                    .ok_or(ContentError::MissingObject)?;
                self.served.fetch_add(value.len() as u64, Ordering::Relaxed);
                Ok(value)
            })
            .collect()
    }
}
impl layerfs_workspace::FileLengths for Store {
    fn file_length(&self, id: ObjectId) -> layerfs_workspace::WorkspaceResult<u64> {
        self.lengths
            .lock()
            .unwrap()
            .get(&id)
            .copied()
            .ok_or(ContentError::MissingObject.into())
    }
}
impl FinalizedConsumer for Store {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.objects
            .lock()
            .unwrap()
            .insert(object.id(), object.canonical().to_vec());
        Ok(())
    }
}
pub fn name(s: &str) -> PathName {
    PathName::new(s).unwrap()
}
pub fn metadata(store: &Store, kind: InodeKind) -> ObjectId {
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let portable = PortableMetadata {
        mode: match kind {
            InodeKind::Directory => 0o1777,
            InodeKind::Symlink => 0o777,
            _ => 0o644,
        },
        mtime_seconds: i64::MAX,
        mtime_nanoseconds: 999_999_999,
    };
    let mode = emit_value(&mut objects, &portable.mode_bytes(kind).unwrap()).unwrap();
    let time = emit_value(&mut objects, &portable.mtime_bytes().unwrap()).unwrap();
    build_attribute_tree(
        &mut objects,
        [
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_root: mode,
            }),
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_root: time,
            }),
        ]
        .into_iter(),
    )
    .unwrap()
    .0
}
pub fn file(store: &Store, bytes: &[u8]) -> ObjectId {
    let mut sink = store.clone();
    let policy = ConstructionPolicy::frozen_default();
    let root = Timing::disabled("file", |scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            bytes,
            &mut sink,
            scope.child("construct"),
        )
    })
    .0
    .unwrap()
    .root;
    store
        .lengths
        .lock()
        .unwrap()
        .insert(root, bytes.len() as u64);
    root
}
pub struct Fixture {
    pub store: Store,
    pub root: FilesystemRootId,
    pub scope: InodeScope,
    pub bytes: Vec<u8>,
}
pub fn fixture() -> Fixture {
    let store = Store::default();
    let scope = scope_for_seed([11; 32]);
    let bytes: Vec<u8> = (0..400_000)
        .map(|i| ((i * 17 + i / 128) % 251) as u8)
        .collect();
    let small = file(&store, b"original\0\xff");
    let large = file(&store, &bytes);
    let symlink = SymlinkTarget::new(b"../.git/index".to_vec())
        .unwrap()
        .finalize()
        .unwrap();
    let target = symlink.id();
    store.clone().accept(symlink).unwrap();
    let dir_meta = metadata(&store, InodeKind::Directory);
    let file_meta = metadata(&store, InodeKind::RegularFile);
    let link_meta = metadata(&store, InodeKind::Symlink);
    let inodes: Vec<_> = (1..=8)
        .map(|serial| {
            let (kind, content_root, metadata_root) = match serial {
                2 => (InodeKind::RegularFile, small, file_meta),
                3 => (InodeKind::Symlink, target, link_meta),
                8 => (InodeKind::RegularFile, large, file_meta),
                _ => (
                    InodeKind::Directory,
                    ObjectId::for_bytes(b"new-directory"),
                    dir_meta,
                ),
            };
            InodeUpdate {
                serial,
                value: InodeValue {
                    kind,
                    content_root,
                    metadata_root,
                    namespace_ref_count: 0,
                },
            }
        })
        .collect();
    let directories = vec![
        DirectoryUpdate {
            parent: 1,
            changes: vec![
                (name(".git"), Some(4)),
                (name("alias"), Some(2)),
                (name("cache"), Some(6)),
                (name("file"), Some(2)),
                (name("node_modules"), Some(5)),
                (name("output"), Some(7)),
                (name("symlink"), Some(3)),
            ],
        },
        DirectoryUpdate {
            parent: 4,
            changes: vec![(name("index"), Some(8))],
        },
        DirectoryUpdate {
            parent: 5,
            changes: vec![(name("pkg"), Some(8))],
        },
        DirectoryUpdate {
            parent: 6,
            changes: vec![(name("state"), Some(8))],
        },
        DirectoryUpdate {
            parent: 7,
            changes: vec![(name("result"), Some(8))],
        },
    ];
    let new: Vec<_> = (1..=8).collect();
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(&store, &mut sink);
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new,
        resources: FilesystemResources::default(),
    };
    let root = build_filesystem(&mut objects, &input, None).unwrap().root;
    Fixture {
        store,
        root,
        scope,
        bytes,
    }
}
