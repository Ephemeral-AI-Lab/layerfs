//! Independent namespace model for captured-construction proofs. The model is
//! advanced by this file's own POSIX rules and never reads product output; the
//! canonical walker reads a published root only to be compared against it.
use layerfs_content::{
    filesystem::{
        inode::codec::{decode_inode_page, InodePage},
        FilesystemRead, FilesystemRootId, PathName,
    },
    object::inode_leaf::InodeKind,
    AuthenticatedObjects, ObjectId,
};
use layerfs_telemetry::timer::Timing;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
};

pub type Stamp = (i64, u32);
pub const FILE: u8 = 1;
pub const DIRECTORY: u8 = 2;
pub const SYMLINK: u8 = 3;
/// The canonical format's fixed counts: the root is named by nothing and every
/// other directory or symlink by exactly one binding.
pub const ROOT_LINKS: u64 = 0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Body {
    File(Vec<u8>),
    Directory(BTreeMap<Vec<u8>, usize>),
    Symlink(Vec<u8>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    pub body: Body,
    pub mode: u32,
    pub mtime: Stamp,
    pub links: u64,
}
/// One path of a complete comparison. Link identity is the least path naming
/// the same inode, so equal maps mean equal alias classes without serials.
#[derive(Clone, Eq, PartialEq)]
pub struct Seen {
    pub kind: u8,
    pub mode: u32,
    pub mtime: Stamp,
    pub links: u64,
    pub payload: Vec<u8>,
    pub identity: Vec<u8>,
}
impl std::fmt::Debug for Seen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "kind={} mode={:o} mtime={:?} links={} bytes={} head={:?} identity={:?}",
            self.kind,
            self.mode,
            self.mtime,
            self.links,
            self.payload.len(),
            String::from_utf8_lossy(&self.payload[..self.payload.len().min(24)]),
            String::from_utf8_lossy(&self.identity)
        )
    }
}
pub type Flat = BTreeMap<Vec<u8>, Seen>;

/// Reports the first differing paths instead of two whole trees.
pub fn assert_same(expected: &Flat, actual: &Flat, what: &str) {
    let mut wrong = Vec::new();
    for (path, row) in expected {
        match actual.get(path) {
            Some(other) if other == row => {}
            other => wrong.push(format!(
                "{:?}: expected {row:?} actual {other:?}",
                String::from_utf8_lossy(path)
            )),
        }
    }
    for (path, row) in actual {
        if !expected.contains_key(path) {
            wrong.push(format!(
                "{:?}: unexpected {row:?}",
                String::from_utf8_lossy(path)
            ));
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

#[derive(Clone, Debug)]
pub struct Model {
    nodes: Vec<Option<Node>>,
}
fn split(path: &[u8]) -> Vec<&[u8]> {
    path.split(|b| *b == b'/')
        .filter(|n| !n.is_empty())
        .collect()
}
impl Model {
    pub fn new(mode: u32, mtime: Stamp) -> Self {
        Self {
            nodes: vec![Some(Node {
                body: Body::Directory(BTreeMap::new()),
                mode,
                mtime,
                links: ROOT_LINKS,
            })],
        }
    }
    /// The model of a native source tree, read through `std::fs` alone. Hard
    /// links are one node; a symlink's portable mode is 0777.
    pub fn native(source: &Path) -> (Self, usize) {
        let stamp = |m: &fs::Metadata| (m.mtime(), m.mtime_nsec() as u32);
        let root = fs::symlink_metadata(source).unwrap();
        let mut model = Self::new(root.mode() & 0o1777, stamp(&root));
        let mut aliases = BTreeMap::<(u64, u64), usize>::new();
        let mut pending = vec![(source.to_path_buf(), 0usize)];
        let mut names = 0;
        while let Some((directory, parent)) = pending.pop() {
            for entry in fs::read_dir(&directory).unwrap() {
                let entry = entry.unwrap();
                let m = entry.metadata().unwrap();
                let name = entry.file_name().as_bytes().to_vec();
                names += 1;
                let known = aliases.get(&(m.dev(), m.ino())).copied();
                let id = known.unwrap_or(model.nodes.len());
                if let Some(id) = known {
                    model.node(id).links += 1;
                } else {
                    let (body, mode) = if m.file_type().is_symlink() {
                        let target = fs::read_link(entry.path()).unwrap();
                        (Body::Symlink(target.as_os_str().as_bytes().to_vec()), 0o777)
                    } else if m.is_dir() {
                        pending.push((entry.path(), id));
                        (Body::Directory(BTreeMap::new()), m.mode() & 0o1777)
                    } else {
                        aliases.insert((m.dev(), m.ino()), id);
                        (
                            Body::File(fs::read(entry.path()).unwrap()),
                            m.mode() & 0o777,
                        )
                    };
                    model.nodes.push(Some(Node {
                        body,
                        mode,
                        mtime: stamp(&m),
                        links: 1,
                    }));
                }
                model.names(parent).insert(name, id);
            }
        }
        (model, names)
    }
    fn node(&mut self, id: usize) -> &mut Node {
        self.nodes[id].as_mut().unwrap()
    }
    fn names(&mut self, id: usize) -> &mut BTreeMap<Vec<u8>, usize> {
        match &mut self.node(id).body {
            Body::Directory(names) => names,
            other => panic!("model: not a directory: {other:?}"),
        }
    }
    fn find(&self, path: &[u8]) -> Option<usize> {
        let mut id = 0;
        for name in split(path) {
            match &self.nodes[id].as_ref().unwrap().body {
                Body::Directory(names) => id = *names.get(name)?,
                _ => return None,
            }
        }
        Some(id)
    }
    fn id(&self, path: &[u8]) -> usize {
        self.find(path)
            .unwrap_or_else(|| panic!("model: missing {:?}", String::from_utf8_lossy(path)))
    }
    fn parent<'a>(&self, path: &'a [u8]) -> (usize, &'a [u8]) {
        let names = split(path);
        let (last, before) = names.split_last().expect("model: root has no parent");
        let mut id = 0;
        for name in before {
            match &self.nodes[id].as_ref().unwrap().body {
                Body::Directory(names) => id = names[*name],
                other => panic!("model: not a directory: {other:?}"),
            }
        }
        (id, last)
    }
    fn add(&mut self, path: &[u8], body: Body, mode: u32, now: Stamp) {
        let (parent, name) = self.parent(path);
        let id = self.nodes.len();
        self.nodes.push(Some(Node {
            body,
            mode,
            mtime: now,
            links: 1,
        }));
        assert!(self.names(parent).insert(name.to_vec(), id).is_none());
        self.node(parent).mtime = now;
    }
    pub fn create(&mut self, path: impl AsRef<[u8]>, mode: u32, now: Stamp) {
        self.add(path.as_ref(), Body::File(Vec::new()), mode, now);
    }
    pub fn mkdir(&mut self, path: impl AsRef<[u8]>, mode: u32, now: Stamp) {
        self.add(path.as_ref(), Body::Directory(BTreeMap::new()), mode, now);
    }
    pub fn symlink(&mut self, path: impl AsRef<[u8]>, target: &[u8], now: Stamp) {
        self.add(path.as_ref(), Body::Symlink(target.to_vec()), 0o777, now);
    }
    pub fn link(&mut self, existing: impl AsRef<[u8]>, path: impl AsRef<[u8]>, now: Stamp) {
        let id = self.id(existing.as_ref());
        assert!(matches!(self.node(id).body, Body::File(_)));
        let (parent, name) = self.parent(path.as_ref());
        assert!(self.names(parent).insert(name.to_vec(), id).is_none());
        self.node(id).links += 1;
        self.node(parent).mtime = now;
    }
    fn unbind(&mut self, id: usize) {
        let node = self.node(id);
        node.links -= 1;
        if node.links == 0 {
            if let Body::Directory(names) = &node.body {
                assert!(names.is_empty(), "model: removed a nonempty directory");
            }
            self.nodes[id] = None;
        }
    }
    /// Unlink of a file or symlink, or removal of an empty directory.
    pub fn remove(&mut self, path: impl AsRef<[u8]>, now: Stamp) {
        let (parent, name) = self.parent(path.as_ref());
        let id = self.names(parent).remove(name).expect("model: remove");
        self.unbind(id);
        self.node(parent).mtime = now;
    }
    /// Replacing rename. Renaming one alias onto another of the same inode, or
    /// a name onto itself, changes nothing.
    pub fn rename(&mut self, from: impl AsRef<[u8]>, to: impl AsRef<[u8]>, now: Stamp) {
        let (old_parent, old_name) = self.parent(from.as_ref());
        let (new_parent, new_name) = self.parent(to.as_ref());
        let id = self.names(old_parent)[old_name];
        if self.names(new_parent).get(new_name) == Some(&id) {
            return;
        }
        self.names(old_parent).remove(old_name);
        if let Some(replaced) = self.names(new_parent).insert(new_name.to_vec(), id) {
            self.unbind(replaced);
        }
        self.node(old_parent).mtime = now;
        self.node(new_parent).mtime = now;
    }
    pub fn write(&mut self, path: impl AsRef<[u8]>, offset: usize, data: &[u8], now: Stamp) {
        let id = self.id(path.as_ref());
        let node = self.node(id);
        let Body::File(bytes) = &mut node.body else {
            panic!("model: write to a non-file")
        };
        if !data.is_empty() && bytes.len() < offset + data.len() {
            bytes.resize(offset + data.len(), 0);
        }
        bytes[offset..offset + data.len()].copy_from_slice(data);
        // An empty write changes nothing, its time included.
        if !data.is_empty() {
            node.mtime = now;
        }
    }
    pub fn truncate(&mut self, path: impl AsRef<[u8]>, size: usize, now: Stamp) {
        let id = self.id(path.as_ref());
        let node = self.node(id);
        let Body::File(bytes) = &mut node.body else {
            panic!("model: truncate of a non-file")
        };
        // Only a changed size sets the time.
        if bytes.len() != size {
            bytes.resize(size, 0);
            node.mtime = now;
        }
    }
    pub fn chmod(&mut self, path: impl AsRef<[u8]>, mode: u32) {
        let id = self.id(path.as_ref());
        self.node(id).mode = mode;
    }
    pub fn utimens(&mut self, path: impl AsRef<[u8]>, mtime: Stamp) {
        let id = self.id(path.as_ref());
        self.node(id).mtime = mtime;
    }
    /// Every path of the tree, the root as the empty path.
    pub fn flat(&self) -> Flat {
        let mut paths = Vec::new();
        let mut pending = vec![(Vec::new(), 0usize)];
        while let Some((path, id)) = pending.pop() {
            if let Body::Directory(names) = &self.nodes[id].as_ref().unwrap().body {
                for (name, child) in names {
                    pending.push((join(&path, name), *child));
                }
            }
            paths.push((path, id));
        }
        let mut least = BTreeMap::<usize, Vec<u8>>::new();
        for (path, id) in &paths {
            let entry = least.entry(*id).or_insert_with(|| path.clone());
            if path < entry {
                *entry = path.clone();
            }
        }
        paths
            .into_iter()
            .map(|(path, id)| {
                let node = self.nodes[id].as_ref().unwrap();
                let (kind, payload) = match &node.body {
                    Body::File(bytes) => (FILE, bytes.clone()),
                    Body::Directory(_) => (DIRECTORY, Vec::new()),
                    Body::Symlink(target) => (SYMLINK, target.clone()),
                };
                let seen = Seen {
                    kind,
                    mode: node.mode,
                    mtime: node.mtime,
                    links: node.links,
                    payload,
                    identity: least[&id].clone(),
                };
                (path, seen)
            })
            .collect()
    }
}
pub fn join(path: &[u8], name: &[u8]) -> Vec<u8> {
    let mut joined = path.to_vec();
    if !joined.is_empty() {
        joined.push(b'/');
    }
    joined.extend_from_slice(name);
    joined
}

/// Rows read from one walker before link identity is known.
pub struct Raw {
    pub path: Vec<u8>,
    pub serial: u64,
    pub seen: Seen,
}
/// Replaces serials by least-path identity.
pub fn identified(rows: Vec<Raw>) -> Flat {
    let mut least = BTreeMap::<u64, Vec<u8>>::new();
    for row in &rows {
        let entry = least.entry(row.serial).or_insert_with(|| row.path.clone());
        if row.path < *entry {
            *entry = row.path.clone();
        }
    }
    rows.into_iter()
        .map(|mut row| {
            row.seen.identity = least[&row.serial].clone();
            (row.path, row.seen)
        })
        .collect()
}

/// Complete walk of one published canonical root through the ordinary reader:
/// every name, kind, portable metadata, link count, byte and symlink target.
pub fn canonical(objects: &dyn AuthenticatedObjects, root: ObjectId) -> Flat {
    let mut reader = FilesystemRead::new(objects, FilesystemRootId(root)).unwrap();
    let mut rows = Vec::new();
    let mut pending = vec![(Vec::new(), reader.root().root_inode().serial())];
    while let Some((path, serial)) = pending.pop() {
        let value = reader.resolve_inode(serial).unwrap().value;
        let metadata = reader.read_portable_inode(serial).unwrap();
        let (kind, payload) = match value.kind {
            InodeKind::Directory => {
                let mut after: Option<PathName> = None;
                loop {
                    let page = reader
                        .list_inode(serial, after.as_ref(), 64, 32768)
                        .unwrap();
                    for (name, child) in &page.entries {
                        pending.push((join(&path, name.as_bytes()), *child));
                    }
                    match page.continuation {
                        Some(next) => {
                            assert!(after.as_ref() < Some(&next), "listing did not advance");
                            after = Some(next);
                        }
                        None => break,
                    }
                }
                (DIRECTORY, Vec::new())
            }
            InodeKind::Symlink => (
                SYMLINK,
                reader.readlink_inode(serial).unwrap().as_bytes().to_vec(),
            ),
            InodeKind::RegularFile => {
                let mut bytes = Vec::new();
                Timing::disabled("model.read", |scope| {
                    layerfs_content::read_all(
                        objects,
                        value.content_root,
                        &mut bytes,
                        scope.child("file"),
                    )
                })
                .0
                .unwrap();
                (FILE, bytes)
            }
        };
        rows.push(Raw {
            path,
            serial,
            seen: Seen {
                kind,
                mode: metadata.mode,
                mtime: (metadata.mtime_seconds, metadata.mtime_nanoseconds),
                links: value.namespace_ref_count,
                payload,
                identity: Vec::new(),
            },
        });
    }
    // Nothing may hide outside the tree: the inode table holds exactly the
    // inodes the walk reached, so a released subtree left in it is seen.
    let mut walked = rows.iter().map(|row| row.serial).collect::<Vec<_>>();
    walked.sort_unstable();
    walked.dedup();
    let mut stored = Vec::new();
    let mut pages = vec![reader.root().inode_table()];
    while let Some(page) = pages.pop() {
        match decode_inode_page(&objects.read_canonical(page).unwrap()).unwrap() {
            InodePage::Leaf { entries } => stored.extend(entries.iter().map(|(serial, _)| *serial)),
            InodePage::Branch { children, .. } => {
                pages.extend(children.iter().map(|(_, child)| *child))
            }
        }
    }
    stored.sort_unstable();
    assert_eq!(
        stored, walked,
        "inode table differs from the reachable tree"
    );
    identified(rows)
}
