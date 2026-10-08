//! Independent namespace model and complete canonical walk for the captured
//! namespace proofs. The model advances by this file's own rules over paths.
//! It reads no captured row, producer record or producer counter; the walk
//! reads a constructed root only to be compared against the model. The walk
//! also decodes the whole inode table and every attribute tree itself, so an
//! inode no name reaches and an attribute key nobody asked for are both seen.
#![allow(dead_code)]
use crate::common::Store;
use layerfs_content::{
    filesystem::{
        attributes::{decode_attribute_page, read_value, AttributeEntry, AttributePage},
        inode::codec::{decode_inode_page, InodePage},
        FilesystemRead, FilesystemRootId, PathName,
    },
    object::inode_leaf::{InodeKind, InodeValue},
    read_all, AuthenticatedObjects, ContentError, ObjectId,
};
use layerfs_telemetry::timer::Timing;
use std::collections::{BTreeMap, BTreeSet};

pub type Stamp = (i64, u32);
/// One attribute outside the two typed portable keys: domain, key, value.
pub type Extra = (String, Vec<u8>, Vec<u8>);
/// A bound no decoded attribute value of these proofs approaches.
const VALUE_BYTES: usize = 65_536;
/// The shared fixture's one metadata time.
pub const BASE: Stamp = (i64::MAX, 999_999_999);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    File,
    Directory,
    Symlink,
}
#[derive(Clone, Debug)]
enum Body {
    File(Vec<u8>),
    Directory(BTreeMap<Vec<u8>, usize>),
    Symlink(Vec<u8>),
}
#[derive(Clone, Debug)]
struct Node {
    body: Body,
    mode: u32,
    mtime: Stamp,
    /// Every other attribute, in key order. No operation changes them.
    extra: Vec<Extra>,
}
/// One path of a complete comparison. `links` is the canonical count: zero
/// for the root, the number of names for everything else. `identity` is the
/// least path naming the same inode, so equal maps mean equal alias classes.
/// `extra` is every attribute beside mode and mtime, so equal rows mean the
/// same complete attribute key set with the same values.
#[derive(Clone, Eq, PartialEq)]
pub struct Seen {
    pub kind: Kind,
    pub mode: u32,
    pub mtime: Stamp,
    pub links: u64,
    pub payload: Vec<u8>,
    pub identity: Vec<u8>,
    pub extra: Vec<Extra>,
}
impl std::fmt::Debug for Seen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sum = self
            .payload
            .iter()
            .fold(0_u64, |sum, byte| sum.wrapping_mul(131) ^ u64::from(*byte));
        write!(
            f,
            "{:?} mode={:o} mtime={:?} links={} bytes={} sum={sum:x} head={:?} identity={:?} extra={:?}",
            self.kind,
            self.mode,
            self.mtime,
            self.links,
            self.payload.len(),
            String::from_utf8_lossy(&self.payload[..self.payload.len().min(16)]),
            short(&self.identity),
            self.extra,
        )
    }
}
pub type Flat = BTreeMap<Vec<u8>, Seen>;

fn short(path: &[u8]) -> String {
    let text = String::from_utf8_lossy(path);
    if text.len() > 48 {
        format!("{}..({} bytes)", &text[..text.len().min(24)], path.len())
    } else {
        text.into_owned()
    }
}
fn split(path: &str) -> Vec<&[u8]> {
    path.split('/')
        .filter(|name| !name.is_empty())
        .map(str::as_bytes)
        .collect()
}
pub fn join(path: &[u8], name: &[u8]) -> Vec<u8> {
    let mut joined = path.to_vec();
    if !joined.is_empty() {
        joined.push(b'/');
    }
    joined.extend_from_slice(name);
    joined
}

/// A rooted tree of nodes. Node 0 is the root; a node no name reaches is not
/// part of the tree, whatever it still holds.
#[derive(Clone, Debug)]
pub struct Model {
    nodes: Vec<Node>,
}
impl Model {
    pub fn empty(mode: u32, mtime: Stamp) -> Self {
        Self {
            nodes: vec![Node {
                body: Body::Directory(BTreeMap::new()),
                mode,
                mtime,
                extra: Vec::new(),
            }],
        }
    }
    /// The shared content-built fixture, restated by hand.
    pub fn fixture(large: &[u8]) -> Self {
        let mut model = Self::empty(0o1777, BASE);
        for directory in [".git", "cache", "node_modules", "output"] {
            model.insert(directory, Body::Directory(BTreeMap::new()), 0o1777);
        }
        model.insert("file", Body::File(b"original\0\xff".to_vec()), 0o644);
        model.bind("file", "alias");
        model.insert("symlink", Body::Symlink(b"../.git/index".to_vec()), 0o777);
        model.insert(".git/index", Body::File(large.to_vec()), 0o644);
        for alias in ["node_modules/pkg", "cache/state", "output/result"] {
            model.bind(".git/index", alias);
        }
        model
    }
    fn insert(&mut self, path: &str, body: Body, mode: u32) -> usize {
        let (parent, name) = self.parent(path);
        let id = self.nodes.len();
        self.nodes.push(Node {
            body,
            mode,
            mtime: BASE,
            extra: Vec::new(),
        });
        assert!(self.names(parent).insert(name.to_vec(), id).is_none());
        id
    }
    fn bind(&mut self, existing: &str, path: &str) {
        let id = self.id(existing);
        let (parent, name) = self.parent(path);
        assert!(self.names(parent).insert(name.to_vec(), id).is_none());
    }
    fn names(&mut self, id: usize) -> &mut BTreeMap<Vec<u8>, usize> {
        match &mut self.nodes[id].body {
            Body::Directory(names) => names,
            other => panic!("model: not a directory: {other:?}"),
        }
    }
    fn find(&self, path: &str) -> Option<usize> {
        let mut id = 0;
        for name in split(path) {
            match &self.nodes[id].body {
                Body::Directory(names) => id = *names.get(name)?,
                _ => return None,
            }
        }
        Some(id)
    }
    fn id(&self, path: &str) -> usize {
        self.find(path)
            .unwrap_or_else(|| panic!("model: missing {path:?}"))
    }
    fn parent<'p>(&self, path: &'p str) -> (usize, &'p [u8]) {
        let names = split(path);
        let (last, before) = names.split_last().expect("model: the root has no parent");
        let mut id = 0;
        for name in before {
            match &self.nodes[id].body {
                Body::Directory(names) => id = names[*name],
                other => panic!("model: not a directory: {other:?}"),
            }
        }
        (id, *last)
    }
    pub fn exists(&self, path: &str) -> bool {
        self.find(path).is_some()
    }
    pub fn kind(&self, path: &str) -> Kind {
        match &self.nodes[self.id(path)].body {
            Body::File(_) => Kind::File,
            Body::Directory(_) => Kind::Directory,
            Body::Symlink(_) => Kind::Symlink,
        }
    }
    pub fn bytes(&self, path: &str) -> &[u8] {
        match &self.nodes[self.id(path)].body {
            Body::File(bytes) => bytes,
            other => panic!("model: not a file: {other:?}"),
        }
    }
    fn add(&mut self, path: &str, body: Body, mode: u32, now: Stamp) {
        let (parent, name) = self.parent(path);
        let id = self.nodes.len();
        self.nodes.push(Node {
            body,
            mode,
            mtime: now,
            extra: Vec::new(),
        });
        assert!(
            self.names(parent).insert(name.to_vec(), id).is_none(),
            "model: {path:?} exists"
        );
        self.nodes[parent].mtime = now;
    }
    // Every operation returns whether it changed anything.
    pub fn create(&mut self, path: &str, mode: u32, now: Stamp) -> bool {
        self.add(path, Body::File(Vec::new()), mode, now);
        true
    }
    pub fn mkdir(&mut self, path: &str, mode: u32, now: Stamp) -> bool {
        self.add(path, Body::Directory(BTreeMap::new()), mode, now);
        true
    }
    pub fn symlink(&mut self, path: &str, target: &[u8], now: Stamp) -> bool {
        self.add(path, Body::Symlink(target.to_vec()), 0o777, now);
        true
    }
    /// A new name of an existing file: the file's own time is unchanged.
    pub fn link(&mut self, existing: &str, path: &str, now: Stamp) -> bool {
        let id = self.id(existing);
        assert!(matches!(self.nodes[id].body, Body::File(_)));
        let (parent, name) = self.parent(path);
        assert!(self.names(parent).insert(name.to_vec(), id).is_none());
        self.nodes[parent].mtime = now;
        true
    }
    /// Unlink of a file or symlink, or removal of an empty directory.
    pub fn remove(&mut self, path: &str, now: Stamp) -> bool {
        let (parent, name) = self.parent(path);
        let id = self.names(parent).remove(name).expect("model: remove");
        if let Body::Directory(names) = &self.nodes[id].body {
            assert!(names.is_empty(), "model: removed a nonempty directory");
        }
        self.nodes[parent].mtime = now;
        true
    }
    /// Replacing rename. A name moved onto itself or onto another name of the
    /// same inode changes nothing; the moved inode keeps its own time.
    pub fn rename(&mut self, from: &str, to: &str, now: Stamp) -> bool {
        let (old_parent, old_name) = self.parent(from);
        let (new_parent, new_name) = self.parent(to);
        let id = self.names(old_parent)[old_name];
        if self.names(new_parent).get(new_name) == Some(&id) {
            return false;
        }
        self.names(old_parent).remove(old_name);
        self.names(new_parent).insert(new_name.to_vec(), id);
        self.nodes[old_parent].mtime = now;
        self.nodes[new_parent].mtime = now;
        true
    }
    /// An empty write changes nothing, not even the time.
    pub fn write(&mut self, path: &str, offset: u64, data: &[u8], now: Stamp) -> bool {
        let id = self.id(path);
        self.write_node(id, offset, data, now)
    }
    fn write_node(&mut self, id: usize, offset: u64, data: &[u8], now: Stamp) -> bool {
        if data.is_empty() {
            return false;
        }
        let node = &mut self.nodes[id];
        let Body::File(bytes) = &mut node.body else {
            panic!("model: write to a non-file")
        };
        let (start, end) = (offset as usize, offset as usize + data.len());
        if bytes.len() < end {
            bytes.resize(end, 0);
        }
        bytes[start..end].copy_from_slice(data);
        node.mtime = now;
        true
    }
    pub fn append(&mut self, path: &str, data: &[u8], now: Stamp) -> bool {
        let id = self.id(path);
        let offset = self.bytes(path).len() as u64;
        self.write_node(id, offset, data, now)
    }
    /// A size that is already current changes nothing; bytes cut off never
    /// return when the file grows again.
    pub fn resize(&mut self, path: &str, size: u64, now: Stamp) -> bool {
        let id = self.id(path);
        let node = &mut self.nodes[id];
        let Body::File(bytes) = &mut node.body else {
            panic!("model: resize of a non-file")
        };
        if bytes.len() as u64 == size {
            return false;
        }
        bytes.resize(size as usize, 0);
        node.mtime = now;
        true
    }
    /// A mapped store: only its bytes below the current size land, so the
    /// size never changes; a store wholly beyond it changes nothing.
    pub fn store(&mut self, path: &str, offset: u64, data: &[u8], now: Stamp) -> bool {
        let size = self.bytes(path).len() as u64;
        let kept = size.saturating_sub(offset).min(data.len() as u64) as usize;
        let id = self.id(path);
        self.write_node(id, offset, &data[..kept], now)
    }
    /// An attribute of the base that no operation of these proofs changes.
    pub fn attribute(&mut self, path: &str, domain: &str, key: &[u8], value: &[u8]) {
        let id = self.id(path);
        let extra = &mut self.nodes[id].extra;
        extra.push((domain.to_owned(), key.to_vec(), value.to_vec()));
        extra.sort();
    }
    pub fn chmod(&mut self, path: &str, mode: u32) -> bool {
        let id = self.id(path);
        let changed = self.nodes[id].mode != mode;
        self.nodes[id].mode = mode;
        changed
    }
    pub fn utimens(&mut self, path: &str, mtime: Stamp) -> bool {
        let id = self.id(path);
        let changed = self.nodes[id].mtime != mtime;
        self.nodes[id].mtime = mtime;
        changed
    }
    /// Every path of the tree, the root as the empty path.
    pub fn flat(&self) -> Flat {
        let mut paths = Vec::new();
        let mut pending = vec![(Vec::new(), 0_usize)];
        while let Some((path, id)) = pending.pop() {
            if let Body::Directory(names) = &self.nodes[id].body {
                for (name, child) in names {
                    pending.push((join(&path, name), *child));
                }
            }
            paths.push((path, id));
        }
        let mut least = BTreeMap::<usize, (Vec<u8>, u64)>::new();
        for (path, id) in &paths {
            let entry = least.entry(*id).or_insert_with(|| (path.clone(), 0));
            entry.1 += 1;
            if *path < entry.0 {
                entry.0 = path.clone();
            }
        }
        paths
            .into_iter()
            .map(|(path, id)| {
                let node = &self.nodes[id];
                let (identity, names) = least[&id].clone();
                let (kind, payload) = match &node.body {
                    Body::File(bytes) => (Kind::File, bytes.clone()),
                    Body::Directory(_) => (Kind::Directory, Vec::new()),
                    Body::Symlink(target) => (Kind::Symlink, target.clone()),
                };
                assert!(
                    kind == Kind::File || names == 1,
                    "model: {names} names of one {kind:?}"
                );
                let seen = Seen {
                    kind,
                    mode: node.mode,
                    mtime: node.mtime,
                    links: if id == 0 { 0 } else { names },
                    payload,
                    identity,
                    extra: node.extra.clone(),
                };
                (path, seen)
            })
            .collect()
    }
}

/// One complete walk of a canonical root through Content's ordinary reader.
pub struct Walked {
    pub flat: Flat,
    /// The serial each path resolves to.
    pub serials: BTreeMap<Vec<u8>, u64>,
    /// Every regular file's content root with its exact byte length.
    pub files: Vec<(ObjectId, u64)>,
    /// Every row of the inode table, decoded page by page. Its serials are
    /// exactly the serials the walk reached from the root.
    pub table: BTreeMap<u64, InodeValue>,
}
impl Walked {
    pub fn serial(&self, path: &str) -> u64 {
        self.serials[path.as_bytes()]
    }
    pub fn has(&self, path: &str) -> bool {
        self.serials.contains_key(path.as_bytes())
    }
}
/// Every row of the root's inode table, read by decoding its pages directly:
/// the table itself, not what a name happens to reach.
pub fn table(store: &Store, root: FilesystemRootId) -> BTreeMap<u64, InodeValue> {
    let reader = FilesystemRead::new(store, root).unwrap();
    let mut rows = BTreeMap::new();
    let mut pending = vec![reader.root().inode_table()];
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        assert!(visited.insert(id), "an inode page is referenced twice");
        let canonical = store.read_canonical_batch(&[id]).unwrap().remove(0);
        match decode_inode_page(&canonical).unwrap() {
            InodePage::Leaf { entries } => {
                for (serial, value) in entries {
                    assert!(
                        rows.insert(serial, value).is_none(),
                        "serial {serial} twice"
                    );
                }
            }
            InodePage::Branch { children, .. } => {
                pending.extend(children.into_iter().map(|(_, child)| child));
            }
        }
    }
    rows
}
/// Every entry of one attribute tree in key order, by decoding its pages.
pub fn attributes(store: &Store, root: ObjectId) -> Vec<AttributeEntry> {
    let mut all = Vec::new();
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        assert!(visited.insert(id), "an attribute page is referenced twice");
        let canonical = store.read_canonical_batch(&[id]).unwrap().remove(0);
        match decode_attribute_page(&canonical).unwrap() {
            AttributePage::Leaf { entries, .. } => all.extend(entries),
            AttributePage::Branch { children, .. } => {
                pending.extend(children.into_iter().map(|(_, child)| child));
            }
        }
    }
    all.sort_by(|left, right| left.key.cmp(&right.key));
    all
}
/// The complete key set of one inode: exactly one mode, exactly one mtime,
/// and every other attribute with its value.
fn extra(store: &Store, metadata_root: ObjectId, what: &str) -> Vec<Extra> {
    let entries = attributes(store, metadata_root);
    let (mut modes, mut mtimes, mut others) = (0, 0, Vec::new());
    for entry in entries {
        if entry.key.is_mode() {
            modes += 1;
        } else if entry.key.is_mtime() {
            mtimes += 1;
        } else {
            let value = read_value(store, entry.value_root, VALUE_BYTES).unwrap();
            others.push((
                entry.key.domain().to_owned(),
                entry.key.key().to_vec(),
                value,
            ));
        }
    }
    assert_eq!((modes, mtimes), (1, 1), "{what}: portable keys");
    others
}
pub fn walk(store: &Store, root: FilesystemRootId) -> Walked {
    let mut reader = FilesystemRead::new(store, root).unwrap();
    let mut rows = Vec::new();
    let mut files = Vec::new();
    let mut pending = vec![(Vec::new(), reader.root().root_inode().serial())];
    while let Some((path, serial)) = pending.pop() {
        let value = reader
            .resolve_inode(serial)
            .unwrap_or_else(|error| panic!("{}: serial {serial}: {error:?}", short(&path)))
            .value;
        let metadata = reader.read_portable_inode(serial).unwrap();
        let (kind, payload) = match value.kind {
            InodeKind::Directory => {
                let mut after: Option<PathName> = None;
                loop {
                    let page = reader
                        .list_inode(serial, after.as_ref(), 64, 32_768)
                        .unwrap();
                    assert!(page.entries.len() <= 64);
                    for (name, child) in &page.entries {
                        pending.push((join(&path, name.as_bytes()), *child));
                    }
                    match page.continuation {
                        Some(next) => {
                            // A listing that does not advance fails here.
                            assert!(
                                after
                                    .as_ref()
                                    .is_none_or(|last| last.as_bytes() < next.as_bytes()),
                                "{}: the listing did not advance past {:?}",
                                short(&path),
                                after
                            );
                            after = Some(next);
                        }
                        None => break,
                    }
                }
                (Kind::Directory, Vec::new())
            }
            InodeKind::Symlink => (
                Kind::Symlink,
                reader.readlink_inode(serial).unwrap().as_bytes().to_vec(),
            ),
            InodeKind::RegularFile => {
                let mut bytes = Vec::new();
                Timing::disabled("oracle", |scope| {
                    read_all(store, value.content_root, &mut bytes, scope.child("file"))
                })
                .0
                .unwrap();
                files.push((value.content_root, bytes.len() as u64));
                (Kind::File, bytes)
            }
        };
        let seen = Seen {
            kind,
            mode: metadata.mode,
            mtime: (metadata.mtime_seconds, metadata.mtime_nanoseconds),
            links: value.namespace_ref_count,
            payload,
            identity: Vec::new(),
            extra: extra(store, value.metadata_root, &short(&path)),
        };
        rows.push((path, serial, seen));
    }
    let mut least = BTreeMap::<u64, Vec<u8>>::new();
    for (path, serial, _) in &rows {
        let entry = least.entry(*serial).or_insert_with(|| path.clone());
        if *path < *entry {
            *entry = path.clone();
        }
    }
    let serials: BTreeMap<Vec<u8>, u64> = rows
        .iter()
        .map(|(path, serial, _)| (path.clone(), *serial))
        .collect();
    // The inode table holds exactly what the walk reached: an inode that no
    // name reaches is a difference, not something the walk cannot see.
    let table = table(store, root);
    let reached: BTreeSet<u64> = serials.values().copied().collect();
    let stored: BTreeSet<u64> = table.keys().copied().collect();
    assert_eq!(
        stored,
        reached,
        "the inode table and the walk from the root differ: unreachable {:?}, missing {:?}",
        stored.difference(&reached).collect::<Vec<_>>(),
        reached.difference(&stored).collect::<Vec<_>>()
    );
    let flat = rows
        .into_iter()
        .map(|(path, serial, mut seen)| {
            seen.identity = least[&serial].clone();
            (path, seen)
        })
        .collect();
    Walked {
        flat,
        serials,
        files,
        table,
    }
}
/// Reports the first differing paths instead of two whole trees.
pub fn assert_same(expected: &Flat, actual: &Flat, what: &str) {
    let mut wrong = Vec::new();
    for (path, row) in expected {
        match actual.get(path) {
            Some(other) if other == row => {}
            other => wrong.push(format!(
                "{}: expected {row:?} actual {other:?}",
                short(path)
            )),
        }
    }
    for (path, row) in actual {
        if !expected.contains_key(path) {
            wrong.push(format!("{}: unexpected {row:?}", short(path)));
        }
    }
    assert!(
        wrong.is_empty(),
        "{what}: {} differing paths of {} expected, {} actual; first: {:#?}",
        wrong.len(),
        expected.len(),
        actual.len(),
        &wrong[..wrong.len().min(8)]
    );
}
/// Every name, kind, mode, time, link count, byte, target, alias class and
/// attribute key of the constructed root equals the model, and its inode
/// table holds no inode beyond the ones the model names.
pub fn assert_tree(store: &Store, root: FilesystemRootId, model: &Model, what: &str) -> Walked {
    let walked = walk(store, root);
    assert_same(&model.flat(), &walked.flat, what);
    walked
}
/// Whether the root's inode table has no value at all for this serial: both
/// Content's point read and the decoded table say so.
pub fn absent(store: &Store, root: FilesystemRootId, serial: u64) -> bool {
    let mut reader = FilesystemRead::new(store, root).unwrap();
    let point = match reader.resolve_inode(serial) {
        Err(ContentError::PathNotFound) => true,
        Ok(_) => false,
        Err(error) => panic!("serial {serial}: {error:?}"),
    };
    assert_eq!(
        point,
        !table(store, root).contains_key(&serial),
        "serial {serial}: the point read and the decoded table disagree"
    );
    point
}
/// The stored inode value of one serial, for root-identity comparisons.
pub fn value(store: &Store, root: FilesystemRootId, serial: u64) -> InodeValue {
    let mut reader = FilesystemRead::new(store, root).unwrap();
    reader.resolve_inode(serial).unwrap().value
}
