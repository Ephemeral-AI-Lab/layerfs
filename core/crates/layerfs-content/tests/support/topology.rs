//! External fixtures for the validation topology proofs: a described base tree,
//! one described update and a backed run that keeps everything it was offered.
//!
//! Nothing here is product code. Bases are built by the public resident build,
//! updates run through the public streamed-backed entry with the shared record
//! fixture, and results are read back through the public reader.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use layerfs_content::filesystem::{
    build_filesystem, build_filesystem_streamed_backed, scope_for_seed, update_filesystem,
    update_filesystem_streamed_backed, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemRead, FilesystemResources, FilesystemResult, FilesystemRootId, InodeScope,
    InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
};

use super::filesystem::{name_of, synthetic, value, TreeStore};
use super::reference_records::Records;
use super::reference_rows::{prepared, Rows};

/// Root directory serial of every tree here.
pub const ROOT: u64 = 1;

/// A typed value with placeholder roots.
pub fn typed(kind: InodeKind) -> InodeValue {
    value(
        kind,
        synthetic("topology/content"),
        synthetic("topology/metadata"),
    )
}

/// Sorted final rows of one build or update.
#[derive(Clone, Default)]
pub struct Rowset {
    directories: BTreeMap<u64, BTreeMap<PathName, Option<u64>>>,
    inodes: BTreeMap<u64, InodeValue>,
    new: BTreeSet<u64>,
}

impl Rowset {
    /// No header, value or fresh serial.
    pub fn new() -> Self {
        Self::default()
    }
    /// Declares a header with no change of its own.
    pub fn header(mut self, parent: u64) -> Self {
        self.directories.entry(parent).or_default();
        self
    }
    /// Binds `name` in `parent` to `serial`.
    pub fn bind(mut self, parent: u64, name: &str, serial: u64) -> Self {
        self.directories
            .entry(parent)
            .or_default()
            .insert(name_of(name), Some(serial));
        self
    }
    /// States that `name` is absent from `parent`.
    pub fn unbind(mut self, parent: u64, name: &str) -> Self {
        self.directories
            .entry(parent)
            .or_default()
            .insert(name_of(name), None);
        self
    }
    /// Supplies a typed value for a serial the base holds.
    pub fn value(mut self, serial: u64, value: InodeValue) -> Self {
        self.inodes.insert(serial, value);
        self
    }
    /// Declares a fresh serial with its typed value.
    pub fn fresh(mut self, serial: u64, kind: InodeKind) -> Self {
        self.inodes.insert(serial, typed(kind));
        self.new.insert(serial);
        self
    }
    /// A fresh directory with its own empty header, bound in `parent`.
    pub fn mkdir(self, parent: u64, name: &str, serial: u64) -> Self {
        self.fresh(serial, InodeKind::Directory)
            .header(serial)
            .bind(parent, name, serial)
    }
    /// True when the rows declare a fresh serial.
    pub fn allocates(&self) -> bool {
        !self.new.is_empty()
    }
    /// Changed names over every header.
    pub fn names(&self) -> usize {
        self.directories.values().map(BTreeMap::len).sum()
    }
    fn rows(&self) -> (Vec<DirectoryUpdate>, Vec<InodeUpdate>, Vec<u64>) {
        (
            self.directories
                .iter()
                .map(|(parent, changes)| DirectoryUpdate {
                    parent: *parent,
                    changes: changes
                        .iter()
                        .map(|(name, binding)| (name.clone(), *binding))
                        .collect(),
                })
                .collect(),
            self.inodes
                .iter()
                .map(|(serial, value)| InodeUpdate {
                    serial: *serial,
                    value: *value,
                })
                .collect(),
            self.new.iter().copied().collect(),
        )
    }
}

/// A base tree under description, with a serial allocator.
pub struct Shape {
    rows: Rowset,
    next: u64,
}

impl Default for Shape {
    fn default() -> Self {
        Self::new()
    }
}

impl Shape {
    /// An empty root.
    pub fn new() -> Self {
        Self {
            rows: Rowset::new().fresh(ROOT, InodeKind::Directory).header(ROOT),
            next: ROOT + 1,
        }
    }
    fn add(&mut self, parent: u64, name: &str, kind: InodeKind) -> u64 {
        let serial = self.next;
        self.next += 1;
        let rows = std::mem::take(&mut self.rows)
            .fresh(serial, kind)
            .bind(parent, name, serial);
        self.rows = if kind == InodeKind::Directory {
            rows.header(serial)
        } else {
            rows
        };
        serial
    }
    /// Adds a directory and returns its serial.
    pub fn dir(&mut self, parent: u64, name: &str) -> u64 {
        self.add(parent, name, InodeKind::Directory)
    }
    /// Adds a regular file and returns its serial.
    pub fn file(&mut self, parent: u64, name: &str) -> u64 {
        self.add(parent, name, InodeKind::RegularFile)
    }
    /// Adds a symlink and returns its serial.
    pub fn symlink(&mut self, parent: u64, name: &str) -> u64 {
        self.add(parent, name, InodeKind::Symlink)
    }
    /// Gives an existing regular file one more name.
    pub fn link(&mut self, parent: u64, name: &str, serial: u64) {
        self.rows = std::mem::take(&mut self.rows).bind(parent, name, serial);
    }
    /// Builds the described tree through the public resident build.
    pub fn build(self) -> Tree {
        let scope = scope_for_seed([0x74; 32]);
        let (directories, inodes, new) = self.rows.rows();
        let input = FilesystemInput {
            base: None,
            scope,
            root_serial: ROOT,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &new,
            resources: FilesystemResources::default(),
        };
        let mut store = TreeStore::new();
        let built = build_filesystem(
            &mut FilesystemObjects::new(&TreeStore::new(), &mut store),
            &input,
            None,
        )
        .expect("described base");
        Tree {
            store,
            root: built.root,
            scope,
            next: self.next,
        }
    }
}

/// One built immutable base.
pub struct Tree {
    /// Every object of the base.
    pub store: TreeStore,
    /// Its root identity.
    pub root: FilesystemRootId,
    /// Its allocation scope.
    pub scope: InodeScope,
    /// The first serial the base never used.
    pub next: u64,
}

impl Tree {
    /// A reader over the base itself.
    pub fn read(&self) -> FilesystemRead<'_> {
        FilesystemRead::new(&self.store, self.root).expect("base reader")
    }
    /// The stored value of one base inode.
    pub fn stored(&self, serial: u64) -> InodeValue {
        self.read().resolve_inode(serial).expect("base inode").value
    }
}

struct Shared(Rc<RefCell<TreeStore>>);
impl FinalizedConsumer for Shared {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.borrow_mut().accept(object)
    }
}
/// Reads what the same operation was offered, otherwise the base.
struct SameSave<'a> {
    base: &'a TreeStore,
    emitted: Rc<RefCell<TreeStore>>,
}
impl AuthenticatedObjects for SameSave<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(|id| {
                if self.emitted.borrow().canonical(*id).is_some() {
                    self.emitted.borrow().read_canonical(*id)
                } else {
                    self.base.read_canonical(*id)
                }
            })
            .collect()
    }
}

/// What one backed update returned, was offered and asked of its records.
pub struct Backed {
    /// The operation's own result.
    pub result: ContentResult<FilesystemResult>,
    /// Every object the consumer was offered, in order.
    pub emitted: TreeStore,
    /// The record fixture after the run.
    pub records: Records,
}

impl Backed {
    /// Roles of the offered objects, in the order they were offered.
    pub fn offered(&self) -> Vec<ObjectRole> {
        self.emitted.order().iter().map(|(_, role)| *role).collect()
    }
    /// True when the consumer was offered a filesystem root object.
    pub fn offered_root(&self) -> bool {
        self.offered().contains(&ObjectRole::FilesystemRoot)
    }
    /// The base and everything offered, for reading the result back.
    pub fn folded(&self, tree: &Tree) -> TreeStore {
        let mut all = tree.store.clone();
        all.absorb(&self.emitted);
        all
    }
}

/// Runs one update on the streamed-backed route with a fresh record scope.
pub fn update_backed(tree: &Tree, rows: &Rowset, resources: FilesystemResources) -> Backed {
    let (directories, inodes, new) = rows.rows();
    let input = FilesystemInput {
        base: Some(tree.root),
        scope: tree.scope,
        root_serial: ROOT,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new,
        resources,
    };
    let emitted = Rc::new(RefCell::new(TreeStore::new()));
    let mut consumer = Shared(emitted.clone());
    let accepted = SameSave {
        base: &tree.store,
        emitted: emitted.clone(),
    };
    let mut records = Records::default();
    let streamed = Rows(&input);
    let result = update_filesystem_streamed_backed(
        &mut FilesystemObjects::new_with_accepted(&tree.store, &mut consumer, &accepted),
        &prepared(&streamed),
        &mut records,
        None,
    );
    let emitted = emitted.borrow().clone();
    Backed {
        result,
        emitted,
        records,
    }
}

/// Runs the same update on the resident route.
pub fn update_resident(
    tree: &Tree,
    rows: &Rowset,
    resources: FilesystemResources,
) -> (ContentResult<FilesystemResult>, TreeStore) {
    let (directories, inodes, new) = rows.rows();
    let input = FilesystemInput {
        base: Some(tree.root),
        scope: tree.scope,
        root_serial: ROOT,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new,
        resources,
    };
    let emitted = Rc::new(RefCell::new(TreeStore::new()));
    let mut consumer = Shared(emitted.clone());
    // The resident route reads what it emitted through its one reader.
    let reader = SameSave {
        base: &tree.store,
        emitted: emitted.clone(),
    };
    let result = update_filesystem(
        &mut FilesystemObjects::new(&reader, &mut consumer),
        &input,
        None,
    );
    let emitted = emitted.borrow().clone();
    (result, emitted)
}

/// Builds described rows as a base-less filesystem on the resident route.
pub fn build_resident(rows: &Rowset) -> (ContentResult<FilesystemResult>, TreeStore) {
    let (directories, inodes, new) = rows.rows();
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([0x75; 32]),
        root_serial: ROOT,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new,
        resources: FilesystemResources::default(),
    };
    let mut sink = TreeStore::new();
    let result = build_filesystem(
        &mut FilesystemObjects::new(&TreeStore::new(), &mut sink),
        &input,
        None,
    );
    (result, sink)
}

/// Every binding of one directory in `root`, in name order.
pub fn listing(store: &TreeStore, root: FilesystemRootId, directory: u64) -> Vec<(String, u64)> {
    let mut read = FilesystemRead::new(store, root).expect("result reader");
    let mut entries = Vec::new();
    let mut after: Option<PathName> = None;
    loop {
        let page = read
            .list_inode(directory, after.as_ref(), 64, 65_536)
            .expect("listing page");
        entries.extend(
            page.entries
                .iter()
                .map(|(name, serial)| (name.as_str().to_owned(), *serial)),
        );
        match page.continuation {
            Some(next) => {
                // A cursor that does not advance fails here instead of spinning.
                assert!(
                    !page.entries.is_empty() && after.as_ref().is_none_or(|last| *last < next),
                    "the listing cursor must advance"
                );
                after = Some(next);
            }
            None => return entries,
        }
    }
}

/// The stored value of one inode in `root`, when the root still holds it.
pub fn inode(store: &TreeStore, root: FilesystemRootId, serial: u64) -> Option<InodeValue> {
    FilesystemRead::new(store, root)
        .expect("result reader")
        .lookup_inodes(&[serial])
        .expect("inode lookup")[0]
}

/// Builds described rows as a base-less filesystem on the streamed-backed route.
pub fn build_backed(rows: &Rowset) -> (ContentResult<FilesystemResult>, TreeStore, Records) {
    let (directories, inodes, new) = rows.rows();
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([0x75; 32]),
        root_serial: ROOT,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new,
        resources: FilesystemResources::default(),
    };
    let mut sink = TreeStore::new();
    let mut records = Records::default();
    let streamed = Rows(&input);
    let result = build_filesystem_streamed_backed(
        &mut FilesystemObjects::new(&TreeStore::new(), &mut sink),
        &prepared(&streamed),
        &mut records,
        None,
    );
    (result, sink, records)
}
