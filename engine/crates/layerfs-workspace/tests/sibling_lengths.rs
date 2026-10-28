//! #356 F-032: a looked-up regular file's length is asked together with the
//! lengths of the files listed after it in the same base directory, in one
//! provider demand bounded by a window, so a tree walk's later lookups are
//! answered from memory. Public API over a content-built base.
mod common;
use common::{file, metadata, name, Store};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemResources, InodeUpdate,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::ObjectId;
use layerfs_workspace::{
    BaseView, CanonicalCache, CanonicalClient, FileLengths, Need, VisitFacts, WorkspaceResult,
};
use std::sync::{Arc, Mutex};

const FILES: u64 = 300;
/// The window: the looked-up file and this many listed after it.
const WINDOW: u64 = 128;

struct Counting {
    store: Store,
    /// Ids per demand, in order.
    demands: Mutex<Vec<usize>>,
}
impl FileLengths for Counting {
    fn file_length(&self, id: ObjectId) -> WorkspaceResult<u64> {
        self.demands.lock().unwrap().push(1);
        self.store.file_length(id)
    }
    fn file_lengths(&self, ids: &[ObjectId]) -> WorkspaceResult<Vec<u64>> {
        self.demands.lock().unwrap().push(ids.len());
        ids.iter().map(|id| self.store.file_length(*id)).collect()
    }
}

/// Root 1 holds directory 2, which holds files f000..f299 (serials 3..),
/// each with its own content.
fn base() -> (Store, BaseView, Arc<CanonicalCache>, Arc<Counting>) {
    let store = Store::default();
    let scope = scope_for_seed([12; 32]);
    let dir_meta = metadata(&store, InodeKind::Directory);
    let file_meta = metadata(&store, InodeKind::RegularFile);
    let mut inodes = vec![];
    for serial in 1..=2 {
        inodes.push(InodeUpdate {
            serial,
            value: InodeValue {
                kind: InodeKind::Directory,
                content_root: ObjectId::for_bytes(b"new-directory"),
                metadata_root: InodeMetadata::Root(dir_meta),
                namespace_ref_count: 0,
            },
        });
    }
    let mut changes = vec![];
    for n in 0..FILES {
        let root = file(&store, format!("file {n} of the sibling window").as_bytes());
        inodes.push(InodeUpdate {
            serial: 3 + n,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                content_root: root,
                metadata_root: InodeMetadata::Root(file_meta),
                namespace_ref_count: 0,
            },
        });
        changes.push((name(&format!("f{n:03}")), Some(3 + n)));
    }
    let directories = vec![
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("dir"), Some(2))],
        },
        DirectoryUpdate { parent: 2, changes },
    ];
    let new: Vec<_> = (1..=2 + FILES).collect();
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
    let lengths = Arc::new(Counting {
        store: store.clone(),
        demands: Mutex::new(vec![]),
    });
    let cache = Arc::new(CanonicalCache::new(8 * 1024 * 1024));
    let client = Arc::new(CanonicalClient::with_cache(
        Arc::new(store.clone()),
        Some(lengths.clone()),
        cache.clone(),
    ));
    let view = BaseView::open(client, root, scope).unwrap();
    (store, view, cache, lengths)
}

#[test]
fn a_lookup_supplies_its_siblings_lengths_in_one_demand() {
    let (_store, base, cache, lengths) = base();
    let resident = base.with_client(Arc::new(CanonicalClient::resident(cache.clone())));
    let mut facts = VisitFacts::default();
    facts
        .supply(&base, &[Need::Name(2, name("f000"))], None)
        .unwrap();
    // One demand: f000 and the WINDOW files listed after it.
    assert_eq!(*lengths.demands.lock().unwrap(), vec![1 + WINDOW as usize]);
    // Those answer from memory, with their own lengths; the next does not.
    for n in 0..=WINDOW {
        let stat = resident.stat(3 + n).unwrap();
        assert_eq!(
            stat.logical_len,
            format!("file {n} of the sibling window").len() as u64
        );
    }
    assert!(resident.stat(3 + WINDOW + 1).is_err());
    // A lookup whose length is remembered asks nothing more.
    let mut facts = VisitFacts::default();
    facts
        .supply(&base, &[Need::Name(2, name("f005"))], None)
        .unwrap();
    assert_eq!(lengths.demands.lock().unwrap().len(), 1);
    // Past the window, one more demand covers the next window, and the
    // listing's end bounds the last one.
    let mut facts = VisitFacts::default();
    facts
        .supply(&base, &[Need::Name(2, name("f129"))], None)
        .unwrap();
    let mut facts = VisitFacts::default();
    facts
        .supply(&base, &[Need::Name(2, name("f258"))], None)
        .unwrap();
    assert_eq!(
        *lengths.demands.lock().unwrap(),
        vec![129, 129, (FILES - 258) as usize]
    );
}
