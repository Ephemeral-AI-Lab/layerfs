//! One path resolved across many versions' roots, level by level (#818 H5).
//!
//! A history call (blame, revision) resolves the same path in every version
//! it walks. Walking each version alone waits for one read per inode level
//! and per directory page level at every path component, version after
//! version. Here a window of versions is resolved together:
//!
//! 1. every root of the window is read in one demand;
//! 2. the serial chain the previous window resolved (root, then each
//!    component) predicts this window's: all predicted serials of all
//!    versions' inode tables are read in one descent, one demand per inode
//!    level ([`lookup_tables`]);
//! 3. each version's directory lookups are answered by the caller's
//!    [`DirectoryMemo`] when that directory's content root was seen before
//!    (directory contents are immutable, so an equal root gives an equal
//!    binding); the misses of every version and every level are read in one
//!    descent, one demand per directory page level;
//! 4. a version whose actual binding differs from the prediction (a rename,
//!    a replaced file, a kind change) reads its actual serials in a further
//!    round, and only that version waits for it.
//!
//! A window whose bindings all match the prediction therefore costs
//! 1 + h + m demands: the roots, the inode table height and the directory
//! page height of its memo misses (0 when no directory on the path changed),
//! whatever the window's width. The first window has no prediction and walks
//! level by level, all versions together.
//!
//! The answers are exactly the per-version walk's: every page is still read
//! through the caller's authenticated reader and checked where it was found,
//! as [`lookup_many`] and the point directory lookup check it, and a version's
//! answer is taken only from its own bindings, never from a prediction.
//! A provider without round-trip cost keeps today's per-version reads: pass
//! `batched = false` (the read port's `batched_chain_reads()`), and each
//! version is opened and walked alone, as the store service's strict path
//! walk does it.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::tree::{EntryFacts, TreeTable, DIRECTORY_WINDOW};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::decoded::{self, Role};
use crate::filesystem::directory::read::{lookup_counted, DirectoryReadWork};
use crate::filesystem::inode::read::{lookup_many, lookup_tables, InodeReadWork, InodeTable};
use crate::filesystem::limits::{MAXIMUM_PAGE_BYTES, MAXIMUM_TREE_LEVEL};
use crate::filesystem::path::PathName;
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::sorted::format::{CompactDirectory, Format, Wire};
use crate::filesystem::sorted::DirectoryRoot;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Objects one demand carries at most: the directory window's 128 pages, at
/// most 1 MiB, which is also one prepared window of pooled inode leaves in
/// the storage reader (`PREPARED_LEAVES`), and the roots demand's bound.
const DEMAND: usize = DIRECTORY_WINDOW;
const _: () = assert!(DEMAND * MAXIMUM_PAGE_BYTES <= 1024 * 1024);

/// One version's answer for the path, as a per-version walk gives it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathAnswer {
    /// The entry the path names.
    Entry(EntryFacts),
    /// No entry: a name is unbound, a component before the last is not a
    /// directory (and, for a strict walk, not a symlink), or the last bound
    /// serial has no inode record.
    Absent,
    /// Strict walks only: the component at this index, before the last, is a
    /// symlink, which a strict walk refuses instead of reading past it.
    Symlink {
        /// Index of the symlink component in the path's names.
        component: usize,
    },
    /// Strict walks only: the component at this index, before the last, is
    /// bound but its inode record is absent (the walk's "bound inode record"
    /// failure).
    Unbound {
        /// Index of the unbound component in the path's names.
        component: usize,
    },
}

/// One version of a resolved window: its opened root and its answer.
#[derive(Clone, Copy, Debug)]
pub struct VersionPath {
    /// The version's decoded root and inode table.
    pub tree: TreeTable,
    /// The path's answer in this version.
    pub answer: PathAnswer,
}

/// Work one or more resolutions performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AcrossWork {
    /// Demands that read filesystem roots.
    pub root_demands: u64,
    /// Inode reads; `read_waves` counts their demands.
    pub inodes: InodeReadWork,
    /// Directory reads; `read_waves` counts their demands.
    pub directories: DirectoryReadWork,
    /// Evaluation rounds of the batched path.
    pub rounds: u64,
}

impl AcrossWork {
    /// Every demand counted: roots, inode and directory waves.
    pub fn demands(&self) -> u64 {
        self.root_demands
            .saturating_add(self.inodes.read_waves)
            .saturating_add(self.directories.read_waves)
    }
}

/// A bounded memo of directory bindings, `(directory root, name) → serial`,
/// absence included, for one call.
///
/// A directory's content is immutable, so an equal root gives an equal
/// binding in every version. The memo keeps at most `capacity` entries and
/// then stops adding; a lookup it cannot answer is read again, never guessed.
/// It holds no Branch, head or visibility state and is meant to be dropped
/// with the call.
#[derive(Default)]
pub struct DirectoryMemo {
    entries: HashMap<ObjectId, HashMap<PathName, Option<u64>>>,
    len: usize,
    capacity: usize,
}

impl DirectoryMemo {
    /// An empty memo of at most `capacity` bindings; 0 keeps none.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            ..Self::default()
        }
    }

    /// The memoized binding of `name` in `directory`: `None` when unknown,
    /// `Some(None)` when known to be unbound.
    pub fn get(&self, directory: DirectoryRoot, name: &PathName) -> Option<Option<u64>> {
        self.entries.get(&directory.0)?.get(name).copied()
    }

    /// Records one binding read from `directory`; false when the memo is
    /// full and the binding was not kept.
    pub fn insert(
        &mut self,
        directory: DirectoryRoot,
        name: PathName,
        binding: Option<u64>,
    ) -> bool {
        if self.get(directory, &name).is_some() {
            return true;
        }
        // Full: nothing is added, not even an empty map for a new directory,
        // so the memo stays bounded by its capacity.
        if self.len >= self.capacity {
            return false;
        }
        self.entries
            .entry(directory.0)
            .or_default()
            .insert(name, binding);
        self.len += 1;
        true
    }

    /// Bindings kept.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether none is kept.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// One path to resolve across consecutive windows of versions.
///
/// It carries the serial chain the last window resolved, which predicts the
/// next window's: callers pass windows in walk order (newest first for a
/// history walk), so the prediction comes from the adjacent versions.
#[derive(Clone, Debug)]
pub struct PathAcross {
    names: Vec<PathName>,
    strict: bool,
    chain: Vec<u64>,
}

impl PathAcross {
    /// The path given as its checked components; `strict` refuses an
    /// intermediate symlink (and a bound intermediate with no inode record)
    /// as the store service's strict walk does, otherwise both read as absent.
    ///
    /// A caller whose path has an invalid component resolves the valid
    /// prefix before it: a version where the prefix's answer is an entry
    /// would reach the invalid name, except that a strict walk refuses an
    /// entry that is a symlink first.
    pub fn new(names: Vec<PathName>, strict: bool) -> Self {
        Self {
            names,
            strict,
            chain: Vec::new(),
        }
    }

    /// The path's components.
    pub fn names(&self) -> &[PathName] {
        &self.names
    }

    /// The serial chain predicted for the next window: the root serial, then
    /// the serial bound to each component, as far as known.
    pub fn chain(&self) -> &[u64] {
        &self.chain
    }

    /// Resolves the path in each of `roots`, answers in the same order.
    ///
    /// With `batched` (the read port reports that each demand costs a round
    /// trip) the window is resolved level by level as the module describes;
    /// pass at most 128 roots for the stated demand bound (a larger window
    /// splits each wave into demands of at most 128 objects). Without it each
    /// root is opened and walked alone, the same reads in the same order as a
    /// per-version strict walk, except that bindings `memo` holds are not
    /// read again (a `DirectoryMemo::new(0)` reads every one).
    ///
    /// Bindings read are added to `memo` while it has room. Nothing else is
    /// kept: the values and bindings of the window are dropped on return, so
    /// memory is bounded by the window, the path depth and the memo.
    ///
    /// An error fails the whole window. Integrity failures are those of the
    /// per-version walk, but in a batched window they surface for any version
    /// of it, also one after the version where the caller would have stopped.
    pub fn resolve(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        roots: &[FilesystemRootId],
        memo: &mut DirectoryMemo,
        batched: bool,
        work: &mut AcrossWork,
    ) -> ContentResult<Vec<VersionPath>> {
        if !batched {
            return roots
                .iter()
                .map(|root| {
                    let tree = TreeTable::open(reader, *root)?;
                    work.root_demands = work.root_demands.saturating_add(1);
                    let answer = self.walk(reader, &tree, memo, work)?;
                    Ok(VersionPath { tree, answer })
                })
                .collect();
        }
        let trees = open_roots(reader, roots, work)?;
        let mut window = Window {
            path: self,
            values: HashMap::new(),
            bindings: HashMap::new(),
        };
        let walks = window.run(reader, &trees, memo, work)?;
        let mut chain = Vec::new();
        let mut answers = Vec::with_capacity(trees.len());
        for (tree, (answer, serials)) in trees.into_iter().zip(walks) {
            for (level, serial) in serials.into_iter().enumerate() {
                match chain.get_mut(level) {
                    Some(slot) => *slot = serial,
                    None => chain.push(serial),
                }
            }
            answers.push(VersionPath { tree, answer });
        }
        if !chain.is_empty() {
            self.chain = chain;
        }
        Ok(answers)
    }

    /// One version walked alone: the store service's strict or lenient walk.
    fn walk(
        &self,
        reader: &dyn AuthenticatedObjects,
        tree: &TreeTable,
        memo: &mut DirectoryMemo,
        work: &mut AcrossWork,
    ) -> ContentResult<PathAnswer> {
        let value = |serial: u64, work: &mut AcrossWork| -> ContentResult<Option<InodeValue>> {
            Ok(
                lookup_many(reader, tree.table, &[serial], &mut work.inodes)?
                    .pop()
                    .flatten(),
            )
        };
        let mut serial = tree.root_serial();
        for (index, name) in self.names.iter().enumerate() {
            let child = match value(serial, work)? {
                Some(directory) if directory.kind == InodeKind::Directory => {
                    let root = DirectoryRoot(directory.content_root);
                    match memo.get(root, name) {
                        Some(binding) => binding,
                        None => {
                            let binding =
                                lookup_counted(reader, root, name, &mut work.directories)?
                                    .map(|(_, serial)| serial);
                            memo.insert(root, name.clone(), binding);
                            binding
                        }
                    }
                }
                _ => None,
            };
            let Some(child) = child else {
                return Ok(PathAnswer::Absent);
            };
            serial = child;
            if self.strict && index + 1 < self.names.len() {
                match value(serial, work)? {
                    None => return Ok(PathAnswer::Unbound { component: index }),
                    Some(value) if value.kind == InodeKind::Symlink => {
                        return Ok(PathAnswer::Symlink { component: index })
                    }
                    Some(_) => {}
                }
            }
        }
        Ok(value(serial, work)?.map_or(PathAnswer::Absent, |value| {
            PathAnswer::Entry(EntryFacts { serial, value })
        }))
    }
}

/// The reads one evaluation round could not answer.
#[derive(Default)]
struct Needs {
    /// `(inode table root, serial)`, with the table to read it from.
    inodes: HashMap<(ObjectId, u64), InodeTable>,
    /// `(directory root, component index)`.
    bindings: BTreeSet<(ObjectId, usize)>,
}

/// One batched window: the values and bindings it has read.
struct Window<'p> {
    path: &'p PathAcross,
    /// `(inode table root, serial)` → value, absence included.
    values: HashMap<(ObjectId, u64), Option<InodeValue>>,
    /// `(directory root, component index)` → binding, absence included.
    bindings: HashMap<(ObjectId, usize), Option<u64>>,
}

/// A finished version: its answer and the serials its walk reached.
type Walked = (PathAnswer, Vec<u64>);

impl Window<'_> {
    fn run(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        trees: &[TreeTable],
        memo: &mut DirectoryMemo,
        work: &mut AcrossWork,
    ) -> ContentResult<Vec<Walked>> {
        let mut needs = Needs::default();
        for tree in trees {
            let predicted = self.path.chain.iter().skip(1);
            for serial in std::iter::once(&tree.root_serial()).chain(predicted) {
                needs.inodes.insert((tree.table.root, *serial), tree.table);
            }
        }
        let mut walked: Vec<Option<Walked>> = vec![None; trees.len()];
        loop {
            if !needs.inodes.is_empty() {
                self.read_values(reader, &needs, work)?;
            } else if !needs.bindings.is_empty() {
                self.read_bindings(reader, &needs, memo, work)?;
            }
            needs = Needs::default();
            work.rounds = work.rounds.saturating_add(1);
            for (tree, slot) in trees.iter().zip(walked.iter_mut()) {
                if slot.is_none() {
                    *slot = self.step(tree, memo, &mut needs);
                }
            }
            if walked.iter().all(Option::is_some) {
                return Ok(walked.into_iter().flatten().collect());
            }
            if needs.inodes.is_empty() && needs.bindings.is_empty() {
                return Err(ContentError::InvalidRecord("path resolution progress"));
            }
        }
    }

    /// One version's walk over what is read so far: its answer, or `None`
    /// with what it waits for added to `needs`. Past a binding it has not
    /// read, the walk goes on along the predicted chain, so the deeper misses
    /// of the same version join the same directory demand; such a
    /// speculative walk only collects bindings and never answers.
    fn step(&self, tree: &TreeTable, memo: &DirectoryMemo, needs: &mut Needs) -> Option<Walked> {
        let names = &self.path.names;
        let mut speculative = false;
        let mut serial = tree.root_serial();
        let mut serials = vec![serial];
        let value = |serial: u64, speculative: bool, needs: &mut Needs| {
            let found = self.values.get(&(tree.table.root, serial)).copied();
            if found.is_none() && !speculative {
                needs.inodes.insert((tree.table.root, serial), tree.table);
            }
            found
        };
        for (index, name) in names.iter().enumerate() {
            let directory = value(serial, speculative, needs)?
                .filter(|value| value.kind == InodeKind::Directory);
            let Some(directory) = directory else {
                return (!speculative).then_some((PathAnswer::Absent, serials));
            };
            let slot = self.slot(index);
            let known = self
                .bindings
                .get(&(directory.content_root, slot))
                .copied()
                .or_else(|| memo.get(DirectoryRoot(directory.content_root), name));
            serial = match known {
                Some(Some(child)) => child,
                Some(None) => return (!speculative).then_some((PathAnswer::Absent, serials)),
                None => {
                    needs.bindings.insert((directory.content_root, slot));
                    speculative = true;
                    *self.path.chain.get(index + 1)?
                }
            };
            serials.push(serial);
            if self.path.strict && index + 1 < names.len() {
                match value(serial, speculative, needs)? {
                    None => {
                        let answer = PathAnswer::Unbound { component: index };
                        return (!speculative).then_some((answer, serials));
                    }
                    Some(child) if child.kind == InodeKind::Symlink => {
                        let answer = PathAnswer::Symlink { component: index };
                        return (!speculative).then_some((answer, serials));
                    }
                    Some(_) => {}
                }
            }
        }
        let last = value(serial, speculative, needs)?;
        if speculative {
            return None;
        }
        let answer = last.map_or(PathAnswer::Absent, |value| {
            PathAnswer::Entry(EntryFacts { serial, value })
        });
        Some((answer, serials))
    }

    /// The first component index with this component's name, so a repeated
    /// name in one directory is read once.
    fn slot(&self, index: usize) -> usize {
        let names = &self.path.names;
        names
            .iter()
            .position(|name| *name == names[index])
            .unwrap_or(index)
    }

    fn read_values(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        needs: &Needs,
        work: &mut AcrossWork,
    ) -> ContentResult<()> {
        let mut demands = needs
            .inodes
            .iter()
            .map(|(&(_, serial), table)| (*table, serial))
            .collect::<Vec<_>>();
        demands.sort_unstable_by_key(|(table, serial)| (table.root, *serial));
        let values = lookup_tables(reader, &demands, DEMAND, &mut work.inodes)?;
        for ((table, serial), value) in demands.into_iter().zip(values) {
            self.values.insert((table.root, serial), value);
        }
        Ok(())
    }

    fn read_bindings(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        needs: &Needs,
        memo: &mut DirectoryMemo,
        work: &mut AcrossWork,
    ) -> ContentResult<()> {
        let keys = needs
            .bindings
            .iter()
            .map(|&(root, slot)| (root, &self.path.names[slot]))
            .collect::<Vec<_>>();
        let found = lookup_directories(reader, &keys, &mut work.directories)?;
        for (&(root, slot), binding) in needs.bindings.iter().zip(found) {
            self.bindings.insert((root, slot), binding);
            memo.insert(DirectoryRoot(root), self.path.names[slot].clone(), binding);
        }
        Ok(())
    }
}

/// The distinct roots read in demands of at most [`DEMAND`] objects, then
/// opened in the caller's order.
fn open_roots(
    reader: &dyn AuthenticatedObjects,
    roots: &[FilesystemRootId],
    work: &mut AcrossWork,
) -> ContentResult<Vec<TreeTable>> {
    let mut seen = HashSet::new();
    let ids = roots
        .iter()
        .map(|root| root.0)
        .filter(|id| seen.insert(*id))
        .collect::<Vec<_>>();
    let mut opened = HashMap::with_capacity(ids.len());
    for wave in ids.chunks(DEMAND) {
        let bodies = reader.read_canonical_batch(wave)?;
        work.root_demands = work.root_demands.saturating_add(1);
        if bodies.len() != wave.len() {
            return Err(ContentError::BatchCardinality {
                requested: wave.len(),
                returned: bodies.len(),
            });
        }
        for (id, body) in wave.iter().zip(bodies) {
            opened.insert(*id, TreeTable::from_root(FilesystemRoot::decode(&body)?));
        }
    }
    roots
        .iter()
        .map(|root| {
            opened
                .get(&root.0)
                .copied()
                .ok_or(ContentError::MissingObject)
        })
        .collect()
}

/// One name's descent in one directory tree.
struct Descent<'n> {
    page: ObjectId,
    root: bool,
    /// The level and largest name the parent recorded for this page.
    expected: Option<(u8, PathName)>,
    /// The largest name left of this page, which every row must exceed.
    minimum: Option<PathName>,
    name: &'n PathName,
    index: usize,
}

/// Looks up `name` in each `(directory root, name)`, all directory trees
/// descending together: one demand per page level (of at most [`DEMAND`]
/// distinct pages), each page checked where it was found exactly as the
/// point directory lookup checks it.
fn lookup_directories(
    reader: &dyn AuthenticatedObjects,
    keys: &[(ObjectId, &PathName)],
    work: &mut DirectoryReadWork,
) -> ContentResult<Vec<Option<u64>>> {
    let mut answers = vec![None; keys.len()];
    let mut level = keys
        .iter()
        .enumerate()
        .map(|(index, (root, name))| Descent {
            page: *root,
            root: true,
            expected: None,
            minimum: None,
            name,
            index,
        })
        .collect::<Vec<_>>();
    let mut depth = 0_u8;
    while !level.is_empty() {
        if depth > MAXIMUM_TREE_LEVEL {
            return Err(ContentError::MappingDepthExceeded);
        }
        let mut seen = HashSet::new();
        let ids = level
            .iter()
            .map(|descent| descent.page)
            .filter(|id| seen.insert(*id))
            .collect::<Vec<_>>();
        let mut pages = HashMap::with_capacity(ids.len());
        for wave in ids.chunks(DEMAND) {
            let read = decoded::pages(reader, wave, Role::Directory)?;
            work.read_waves = work.read_waves.saturating_add(1);
            pages.extend(wave.iter().copied().zip(read));
        }
        let mut next = Vec::new();
        for descent in level {
            work.pages_read = work.pages_read.saturating_add(1);
            let decoded = pages
                .get(&descent.page)
                .ok_or(ContentError::MissingObject)?;
            let wire = decoded.directory_wire()?;
            checked(wire, &descent)?;
            if wire.level == 0 {
                work.names_resolved = work.names_resolved.saturating_add(1);
                answers[descent.index] =
                    match wire.entries.iter().find(|entry| entry.key == *descent.name) {
                        Some(entry) => Some(
                            entry
                                .value
                                .ok_or(ContentError::InvalidRecord("directory leaf value"))?,
                        ),
                        None => None,
                    };
                continue;
            }
            let position = wire
                .entries
                .partition_point(|entry| entry.key < *descent.name);
            let Some(entry) = wire.entries.get(position) else {
                continue;
            };
            let minimum = position
                .checked_sub(1)
                .map(|previous| wire.entries[previous].key.clone())
                .or(descent.minimum);
            next.push(Descent {
                page: entry
                    .child
                    .ok_or(ContentError::InvalidRecord("directory child"))?,
                root: false,
                expected: Some((
                    wire.level
                        .checked_sub(1)
                        .ok_or(ContentError::MappingDepthExceeded)?,
                    entry.key.clone(),
                )),
                minimum,
                name: descent.name,
                index: descent.index,
            });
        }
        level = next;
        depth = depth.saturating_add(1);
    }
    Ok(answers)
}

/// The point directory lookup's checks on one page where it was found: the
/// fill rule off the root, at least two rows in a branch, the level and
/// largest name the parent recorded, and every row after the left boundary.
fn checked(wire: &Wire<PathName, u64>, descent: &Descent<'_>) -> ContentResult<()> {
    if (!descent.root
        && !<CompactDirectory as Format>::filled(wire.size, wire.entries.len(), wire.level))
        || (wire.level > 0 && wire.entries.len() < 2)
    {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    if let Some((level, maximum)) = &descent.expected {
        if wire.level != *level || wire.entries.last().map(|entry| &entry.key) != Some(maximum) {
            return Err(ContentError::InvalidRecord("directory child summary"));
        }
    }
    if descent.minimum.as_ref().is_some_and(|minimum| {
        wire.entries
            .first()
            .is_some_and(|entry| &entry.key <= minimum)
    }) {
        return Err(ContentError::NonCanonicalOrdering);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full memo adds nothing, not even an empty entry for a directory it
    /// has not seen: it stays bounded by its capacity.
    #[test]
    fn a_full_memo_adds_no_directory() {
        let mut memo = DirectoryMemo::new(1);
        let name = PathName::new("a").unwrap();
        let first = DirectoryRoot(ObjectId::for_bytes(b"first"));
        assert!(memo.insert(first, name.clone(), Some(1)));
        assert!(memo.insert(first, name.clone(), Some(1)), "already kept");
        for n in 0..64u8 {
            let other = DirectoryRoot(ObjectId::for_bytes(&[n]));
            assert!(!memo.insert(other, name.clone(), None));
            assert_eq!(memo.get(other, &name), None);
        }
        assert_eq!((memo.len(), memo.entries.len()), (1, 1));
    }
}
