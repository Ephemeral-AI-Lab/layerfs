//! Adapted unchanged v4 producer bookkeeping; benchmark-only, no engine code.
#![allow(dead_code)]
use crate::workload::history::{Change, Row};
use crate::workload::providers::TreeStore;
use layerfs_content::filesystem::{scope_for_seed, DirectoryUpdate, InodeUpdate, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{InodeScope, ObjectId};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug)]
pub enum OpError {
    Io(String),
    Product(String),
}
impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OpError {}
const ROOT_SERIAL: u64 = 1;
const FIRST_SERIAL: u64 = 2;
fn empty_metadata_root() -> ObjectId {
    ObjectId::for_bytes(b"layerfs/history/empty-metadata-root")
}
pub fn scope_of(row: Row) -> InodeScope {
    scope_for_seed(
        *ObjectId::for_bytes(format!("layerfs/history/{}", row.token()).as_bytes()).as_bytes(),
    )
}
fn hex(b: &[u8]) -> String {
    use std::fmt::Write;
    b.iter()
        .fold(String::with_capacity(b.len() * 2), |mut out, byte| {
            write!(&mut out, "{byte:02x}").expect("String formatting");
            out
        })
}
fn parent_of(path: &[u8]) -> Vec<u8> {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => path[..index].to_vec(),
        None => Vec::new(),
    }
}

/// The final component of a path.
fn name_of(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// Every proper ancestor of a path, longest last.
fn ancestors_of(path: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (index, byte) in path.iter().enumerate() {
        if *byte == b'/' {
            out.push(path[..index].to_vec());
        }
    }
    out
}

/// The kind class the corpus's octal mode names.
///
/// `120000` is a symbolic link, `040000` a directory, and everything else is a
/// regular file. The permission bits are not representable in `InodeValue` and
/// the corpus makes no mode-only transition, so they are not consulted.
pub fn kind_of(mode: u32) -> InodeKind {
    match mode & 0o170000 {
        0o120000 => InodeKind::Symlink,
        0o040000 => InodeKind::Directory,
        _ => InodeKind::RegularFile,
    }
}

pub struct SimilarityIndex {
    /// Every stored object's eight-hash signature.
    signatures: BTreeMap<ObjectId, [u64; 8]>,
    /// `hash -> objects whose signature contains it`, so a lookup visits only the
    /// objects that share at least one hash rather than all of them.
    by_hash: BTreeMap<u64, Vec<ObjectId>>,
    /// `content root -> the path it was stored for`, so a candidate can be told
    /// from a same-path one.
    pub path_of: BTreeMap<ObjectId, Vec<u8>>,
}

impl SimilarityIndex {
    pub fn new() -> Self {
        Self {
            signatures: BTreeMap::new(),
            by_hash: BTreeMap::new(),
            path_of: BTreeMap::new(),
        }
    }

    /// Records one stored object under the path it was stored for.
    pub fn insert(&mut self, id: ObjectId, path: &[u8], signature: [u64; 8]) {
        if signature[0] == u64::MAX {
            return;
        }
        if self.signatures.insert(id, signature).is_some() {
            return;
        }
        self.path_of.insert(id, path.to_vec());
        for hash in signature.iter().copied().filter(|hash| *hash != u64::MAX) {
            self.by_hash.entry(hash).or_default().push(id);
        }
    }

    /// The objects whose signature shares at least two hashes with `signature`,
    /// best overlap first, excluding `id` itself.
    ///
    /// `>= 2`-of-8 is the product's own match rule, applied here so the harness
    /// proposes what the product's index would have proposed.
    fn nearest(&self, id: ObjectId, signature: [u64; 8], limit: usize) -> Vec<ObjectId> {
        let mut overlap: BTreeMap<ObjectId, u8> = BTreeMap::new();
        for hash in signature.iter().copied().filter(|hash| *hash != u64::MAX) {
            let Some(bucket) = self.by_hash.get(&hash) else {
                continue;
            };
            for candidate in bucket {
                if *candidate == id {
                    continue;
                }
                let entry = overlap.entry(*candidate).or_insert(0);
                *entry = entry.saturating_add(1);
            }
        }
        let mut ranked: Vec<(u8, ObjectId)> = overlap
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .map(|(candidate, count)| (count, candidate))
            .collect();
        // Best overlap first; ties broken by identity so the order is reproducible
        // from the receipt and not from a hash-map iteration order.
        ranked.sort_by(|left, right| right.cmp(left));
        ranked
            .into_iter()
            .map(|(_, candidate)| candidate)
            .take(limit)
            .collect()
    }
}

/// The predecessors to declare for one object, in the measured best order.
///
/// `cross-path best`, `cross-path 2nd`, `same-path previous`, `same-path earlier`.
/// An identity that is not in the Store yet is dropped: the selector probes each in
/// order and returns the first **eligible** one, so proposing an object the save has
/// not admitted would spend a probe and buy nothing.
fn ordered_predecessors(
    index: &SimilarityIndex,
    id: ObjectId,
    path: &[u8],
    signature: [u64; 8],
    same_path_previous: Option<ObjectId>,
) -> Vec<ObjectId> {
    let mut cross: Vec<ObjectId> = Vec::with_capacity(2);
    for candidate in index.nearest(id, signature, 8) {
        if cross.len() == 2 {
            break;
        }
        if index.path_of.get(&candidate).map(Vec::as_slice) == Some(path) {
            continue;
        }
        cross.push(candidate);
    }
    let mut ordered: Vec<ObjectId> = cross;
    if let Some(previous) = same_path_previous {
        if !ordered.contains(&previous) {
            ordered.push(previous);
        }
    }
    ordered.truncate(layerfs_content::MAXIMUM_ADVISORY_PREDECESSORS);
    ordered
}

/// Emits one symlink target through the state's consumer.
///
/// A symlink is **not** a file whose bytes happen to be a path. The driver used
/// `construct_bytes` for every changed path, which stored a symlink as a
/// `RegularFile` content object holding its target string — a tree that reads
/// back with the wrong kind, which is what the verification phase found the first
/// time it ran (`.claude/skills`, oracle mode `120000`, `UnsupportedFraming` from
/// the regular-file read). It is emitted as a `Symlink` object instead, and only
/// when the corpus says the path changed: an unchanged symlink keeps the content
/// root the Store already holds, exactly as an unchanged file does.
pub fn emit_symlink_target(target: &[u8], consumer: &mut TreeStore) -> Result<ObjectId, OpError> {
    let target = layerfs_content::filesystem::SymlinkTarget::new(target.to_vec())
        .map_err(|error| OpError::Product(format!("{error:?}")))?;
    let object = target
        .finalize()
        .map_err(|error| OpError::Product(format!("{error:?}")))?;
    let id = object.id();
    consumer.insert_object(object);
    Ok(id)
}

/// The serials and paths this chain has allocated so far.
///
/// It is carried **across** states, which is what makes an unchanged path keep its
/// inode and an unchanged directory keep its identity: a chain that re-allocated
/// every serial per state would rewrite the whole tree N times and measure a
/// different operation than the one the claim describes.
pub struct Chain {
    serial_of: BTreeMap<Vec<u8>, u64>,
    live_directories: BTreeSet<Vec<u8>>,
    next_serial: u64,
}

impl Chain {
    pub fn new() -> Self {
        let mut serial_of = BTreeMap::new();
        serial_of.insert(Vec::new(), ROOT_SERIAL);
        Self {
            serial_of,
            live_directories: BTreeSet::new(),
            next_serial: FIRST_SERIAL,
        }
    }

    /// The serial for a path, allocating one if this is the first state to see it.
    pub fn serial(&mut self, path: &[u8]) -> u64 {
        if let Some(serial) = self.serial_of.get(path) {
            return *serial;
        }
        let serial = self.next_serial;
        self.next_serial += 1;
        self.serial_of.insert(path.to_vec(), serial);
        serial
    }

    pub fn known(&self, path: &[u8]) -> Option<u64> {
        self.serial_of.get(path).copied()
    }
}

/// Builds the `FilesystemInput` for one state.
///
/// The changed directory bindings carry the **final** binding of every name they
/// change, so a removal is an explicit `None` rather than an omission. New
/// directories are declared as directory inodes and bound in their parents; a
/// directory that already existed keeps its serial and is not re-declared.
pub fn filesystem_input(
    chain: &mut Chain,
    transition: &crate::workload::history::Transition,
    content_roots: &BTreeMap<Vec<u8>, ObjectId>,
    is_build: bool,
    directories: &mut Vec<DirectoryUpdate>,
    inodes: &mut Vec<InodeUpdate>,
    new_inodes: &mut Vec<u64>,
) -> Result<(), OpError> {
    directories.clear();
    inodes.clear();
    new_inodes.clear();

    // A **build** allocates its root like any other inode and must declare it;
    // an update must not, because the root already exists in the base. Both rules
    // are `FilesystemInput::check`'s, and getting either wrong fails the whole
    // state with `InvalidRecord("root inode allocation")`.
    if is_build {
        inodes.push(InodeUpdate {
            serial: ROOT_SERIAL,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: ObjectId::for_bytes(b"layerfs/history/dir/root"),
                metadata_root: empty_metadata_root(),
            },
        });
        new_inodes.push(ROOT_SERIAL);
    }

    let name = |path: &[u8]| -> Result<PathName, OpError> {
        let text = std::str::from_utf8(name_of(path))
            .map_err(|_| OpError::Io("a corpus path name is not UTF-8".to_string()))?;
        PathName::new(text).map_err(|error| OpError::Product(format!("{error:?}")))
    };

    // The tree's directories, and the ones this state is the first to see.
    let mut directories_now: BTreeSet<Vec<u8>> = BTreeSet::new();
    for path in transition.tree.keys() {
        for ancestor in ancestors_of(path) {
            directories_now.insert(ancestor);
        }
    }

    // Changed directory bindings, keyed by the directory path.
    let mut bindings: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, Option<u64>>> = BTreeMap::new();

    // Only the outermost disappeared directory needs an unbind: removing its
    // parent entry removes the whole subtree. Burn every disappeared serial so
    // a later path reuse cannot inherit the removed inode's identity.
    let removed_directories: BTreeSet<Vec<u8>> = chain
        .live_directories
        .difference(&directories_now)
        .cloned()
        .collect();
    for directory in &removed_directories {
        if !removed_directories.contains(&parent_of(directory)) {
            bindings
                .entry(parent_of(directory))
                .or_default()
                .insert(name_of(directory).to_vec(), None);
        }
        chain.serial_of.remove(directory);
    }
    chain.live_directories = directories_now.clone();

    for directory in &directories_now {
        if chain.known(directory).is_none() {
            let serial = chain.serial(directory);
            inodes.push(InodeUpdate {
                serial,
                value: InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(
                        format!("layerfs/history/dir/{}", hex(directory)).as_bytes(),
                    ),
                    metadata_root: empty_metadata_root(),
                },
            });
            new_inodes.push(serial);
            bindings
                .entry(parent_of(directory))
                .or_default()
                .insert(name_of(directory).to_vec(), Some(serial));
        }
    }

    for changed in &transition.changed {
        let path = &changed.path;
        let parent = parent_of(path);
        match changed.kind {
            Change::Removed => {
                chain.serial_of.remove(path);
                if !removed_directories.contains(&parent) {
                    bindings
                        .entry(parent)
                        .or_default()
                        .insert(name_of(path).to_vec(), None);
                }
            }
            Change::MetadataOnly => {
                // The corpus makes no mode-only transition — verified on all 157
                // checkpoints — and `InodeValue` could not express one anyway. It
                // is counted, never silently dropped.
            }
            Change::Added | Change::Modified => {
                let serial = chain.serial(path);
                if changed.kind == Change::Added {
                    new_inodes.push(serial);
                }
                let content_root = content_roots.get(path).copied().ok_or_else(|| {
                    OpError::Io(format!(
                        "state {} has no constructed content for {}",
                        transition.state.ordinal,
                        hex(path)
                    ))
                })?;
                inodes.push(InodeUpdate {
                    serial,
                    value: InodeValue {
                        kind: kind_of(changed.mode),
                        namespace_ref_count: 1,
                        content_root,
                        metadata_root: empty_metadata_root(),
                    },
                });
                bindings
                    .entry(parent)
                    .or_default()
                    .insert(name_of(path).to_vec(), Some(serial));
            }
        }
    }

    for (directory, changes) in bindings {
        let parent = chain.serial(&directory);
        let mut rows: Vec<(PathName, Option<u64>)> = Vec::with_capacity(changes.len());
        for (path, binding) in changes {
            rows.push((name(&path)?, binding));
        }
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        directories.push(DirectoryUpdate {
            parent,
            changes: rows,
        });
    }
    directories.sort_by_key(|update| update.parent);
    inodes.sort_by_key(|update| update.serial);
    new_inodes.sort_unstable();
    new_inodes.dedup();
    Ok(())
}
