//! Public S3 proof over content-built trees; fixture memory is external to product.
use layerfs_content::filesystem::{
    attributes::{
        build_attribute_tree, emit_value, AttributeEntry, AttributeKey, PortableMetadata,
    },
    build_filesystem, scope_for_seed, update_filesystem, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemResources, FilesystemRootId, InodeScope, InodeUpdate, PathName,
    SymlinkTarget,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    construct_bytes, AuthenticatedObjects, ConstructionPolicy, ContentError, ContentResult,
    FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_overlay::{Overlay, ProfileConfig};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{BaseView, CanonicalClient, Workspace};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone, Default)]
struct Store {
    objects: Arc<Mutex<BTreeMap<ObjectId, Vec<u8>>>>,
    demand: Arc<AtomicU64>,
}
impl AuthenticatedObjects for Store {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demand.fetch_add(ids.len() as u64, Ordering::Relaxed);
        let objects = self.objects.lock().unwrap();
        ids.iter()
            .map(|id| objects.get(id).cloned().ok_or(ContentError::MissingObject))
            .collect()
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
fn name(s: &str) -> PathName {
    PathName::new(s).unwrap()
}
fn metadata(store: &Store, kind: InodeKind) -> ObjectId {
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
fn file(store: &Store, bytes: &[u8]) -> ObjectId {
    let mut sink = store.clone();
    let policy = ConstructionPolicy::frozen_default();
    Timing::disabled("file", |scope| {
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
    .root
}
struct Fixture {
    store: Store,
    root: FilesystemRootId,
    scope: InodeScope,
    bytes: Vec<u8>,
}
fn fixture() -> Fixture {
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

#[test]
fn full_root_binding_aliases_metadata_symlinks_and_eof_use_content_apis() {
    let f = fixture();
    let client = Arc::new(CanonicalClient::new(Arc::new(f.store.clone()), 1024 * 1024));
    let before = f.store.demand.load(Ordering::Relaxed);
    let base = BaseView::open(client.clone(), f.root, f.scope).unwrap();
    assert_eq!(
        f.store.demand.load(Ordering::Relaxed) - before,
        1,
        "root binding must not scan"
    );
    let a = base.child(1, &name("file")).unwrap();
    assert_eq!(base.child(1, &name("alias")).unwrap().serial, a.serial);
    assert_eq!(a.value.namespace_ref_count, 2);
    assert_eq!(base.readlink(3).unwrap().as_bytes(), b"../.git/index");
    let stat = base.stat(1).unwrap();
    assert_eq!(stat.metadata.mode, 0o1777);
    assert_eq!(stat.metadata.mtime_seconds, i64::MAX);
    let mut all = Vec::new();
    let mut after = None;
    loop {
        let page = base.list(1, after.as_ref(), 2, 4096).unwrap();
        all.extend(page.entries.iter().map(|(n, _)| n.as_str().to_owned()));
        if page.continuation.is_none() {
            break;
        }
        after = page.continuation;
    }
    assert_eq!(
        all,
        vec![
            ".git",
            "alias",
            "cache",
            "file",
            "node_modules",
            "output",
            "symlink"
        ]
    );
    for (parent, child) in [(4, "index"), (5, "pkg"), (6, "state"), (7, "result")] {
        assert_eq!(base.child(parent, &name(child)).unwrap().serial, 8);
    }
    let plan = base.plan_read(8, 399_980, 128 * 1024).unwrap();
    assert_eq!(plan.length(), 20);
    let mut bytes = Vec::new();
    plan.emit(&mut bytes).unwrap();
    assert_eq!(bytes, f.bytes[399_980..]);
    assert_eq!(base.plan_read(8, u64::MAX, 32).unwrap().length(), 0);
    assert_eq!(
        base.child(1, &name("missing")).unwrap_err(),
        ContentError::PathNotFound
    );
    assert!(BaseView::open(client, f.root, scope_for_seed([19; 32])).is_err());
}

#[test]
fn retained_old_plan_and_new_root_use_exact_immutable_cache_identities() {
    let f = fixture();
    let client = Arc::new(CanonicalClient::new(Arc::new(f.store.clone()), 512));
    let old = BaseView::open(client.clone(), f.root, f.scope).unwrap();
    let retained = old.plan_read(2, 0, 64).unwrap();
    let new_file = file(&f.store, b"changed");
    let mut value = old.inode(2).unwrap().value;
    value.content_root = new_file;
    let inodes = [InodeUpdate { serial: 2, value }];
    let mut sink = f.store.clone();
    let mut objects = FilesystemObjects::new(&f.store, &mut sink);
    let input = FilesystemInput {
        base: Some(f.root),
        scope: f.scope,
        root_serial: 1,
        directories: &[],
        inodes: &inodes,
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let root = update_filesystem(&mut objects, &input, None).unwrap().root;
    let new = BaseView::open(client.clone(), root, f.scope).unwrap();
    let mut bytes = Vec::new();
    retained.emit(&mut bytes).unwrap();
    assert_eq!(bytes, b"original\0\xff");
    bytes.clear();
    new.plan_read(2, 0, 64).unwrap().emit(&mut bytes).unwrap();
    assert_eq!(bytes, b"changed");
    assert_eq!(old.identity(), f.root);
    assert_ne!(new.identity(), f.root);
    assert!(client.diagnostics().unwrap().charged_cache_bytes <= 512);
    assert!(client.diagnostics().unwrap().evictions > 0);
}

#[test]
fn logical_workspace_open_binds_real_root_without_schema_or_namespace_import() {
    let f = fixture();
    let client = Arc::new(CanonicalClient::new(Arc::new(f.store.clone()), 0));
    let path =
        std::env::temp_dir().join(format!("layerfs-base-open-{}.sqlite", std::process::id()));
    let overlay = Overlay::create(&path, ProfileConfig::default()).unwrap();
    let before = f.store.demand.load(Ordering::Relaxed);
    let workspace = Workspace::open(&overlay, client, f.root, f.scope, [91; 32]).unwrap();
    assert_eq!(f.store.demand.load(Ordering::Relaxed) - before, 1);
    assert_eq!(
        overlay.state(workspace.route()).unwrap().base_root,
        f.root.0.to_bytes()
    );
    assert_eq!(overlay.state(workspace.route()).unwrap().dirty_inodes, 0);
    drop(overlay);
    std::fs::remove_file(path).unwrap();
}
