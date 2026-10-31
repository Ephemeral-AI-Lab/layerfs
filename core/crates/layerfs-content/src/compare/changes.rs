//! Path-ordered comparison of two roots that reads the changed entries.
//!
//! [`ChangeWalk`] yields every path of either root in component order (children
//! in name order, each directory before its children) with both sides' entries,
//! and descends below a path only through [`ChangeWalk::enter`], so a caller
//! filters and pages the pairs it is handed.
//!
//! [`ChangeWalk::full`] lists both trees completely. [`Plan::make`] reads
//! only what changed: the inodes whose stored values differ come from the
//! page-skipping difference of the two inode tables, and each one's paths from
//! the caller's [`BindingHints`], every hint checked against the tree it is used
//! for, up to the root. The walk then enters a directory only when its listing
//! changed, a changed inode lies below it, or its identity differs between the
//! sides; inside a directory kept on both sides it visits only the changed names
//! and those paths. Work follows the changed entries times their path depth, not
//! the trees. A pair either walk omits is equal on both sides, and so is
//! everything below it.
//!
//! A stale or absent hint cannot change the answer: a hint is used only when the
//! tree binds that name to that inode. When an inode has fewer checked bindings
//! than its reference count, or more than [`MAXIMUM_CHANGED`] inodes changed,
//! [`Plan::make`] answers `None` and the caller uses [`ChangeWalk::full`].
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use super::sorted::DifferenceWork;
use super::tree::{binding_changes, inode_differences, join_exact, EntryFacts, TreeTable};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, lookup, DirectoryReadWork};
use crate::filesystem::path::PathName;
use crate::filesystem::sorted::DirectoryRoot;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Changed inodes beyond which [`Plan::make`] leaves the answer to the
/// full walk.
pub const MAXIMUM_CHANGED: usize = 65_536;

/// Names one directory listing window returns.
const WINDOW: usize = 64;

/// Bytes one directory listing window may return.
const WINDOW_BYTES: usize = 8192;

/// Recorded name bindings: where an inode may be bound, in any tree.
pub trait BindingHints {
    /// Every recorded `(parent directory serial, name)` binding of `child`.
    fn bindings_of(&self, child: u64) -> ContentResult<Vec<(u64, PathName)>>;
}

/// One path and its entry on each side.
pub type Pair = (String, Option<EntryFacts>, Option<EntryFacts>);

/// One child name with its entry on each side.
type Child = (PathName, [Option<EntryFacts>; 2]);

/// One child name with its serial on each side.
type Bound = (PathName, [Option<u64>; 2]);

/// One side's directory listing, read in windows.
struct Listing {
    root: ObjectId,
    after: Option<PathName>,
    exhausted: bool,
    page: VecDeque<(PathName, u64)>,
}

/// One directory pair being visited.
struct Frame {
    path: String,
    ready: VecDeque<Child>,
    /// Both sides' full listings, for a directory a full walk lists, whose
    /// identity differs or that one side lacks; `None` when only `ready`'s
    /// names are visited.
    listings: Option<[Option<Listing>; 2]>,
}

/// The changed entries of two roots: per side, each directory serial on a
/// path to a changed inode and the names on those paths. A plan depends only
/// on the two immutable roots, so one plan serves every page of their
/// difference. Its size follows the changed inodes and their ancestors, at
/// most [`MAXIMUM_CHANGED`] inodes.
#[derive(Debug, Default)]
pub struct Plan {
    dirty: [HashMap<u64, BTreeSet<PathName>>; 2],
}

/// A path-ordered walk over two roots.
pub struct ChangeWalk<'a> {
    reader: &'a dyn AuthenticatedObjects,
    sides: [&'a TreeTable; 2],
    /// The changed entries; `None` lists every directory instead.
    plan: Option<&'a Plan>,
    stack: Vec<Frame>,
}

impl Plan {
    /// The changed entries of `left` to `right`, or `None` when an inode's
    /// bindings are not all recorded or more than [`MAXIMUM_CHANGED`] inodes
    /// changed.
    pub fn make(
        reader: &dyn AuthenticatedObjects,
        hints: &dyn BindingHints,
        left: &TreeTable,
        right: &TreeTable,
    ) -> ContentResult<Option<Self>> {
        let mut changed: BTreeMap<u64, [Option<InodeValue>; 2]> = BTreeMap::new();
        let mut large = false;
        let differed = inode_differences(
            reader,
            Some(left),
            right,
            &mut DifferenceWork::default(),
            |serial, before, after| {
                if changed.len() == MAXIMUM_CHANGED {
                    large = true;
                    return Err(ContentError::OutputRejected);
                }
                changed.insert(serial, [before.copied(), after.copied()]);
                Ok(())
            },
        );
        if large {
            return Ok(None);
        }
        differed?;
        let mut plan = Self::default();
        let mut locator = Locator {
            reader,
            hints,
            recorded: HashMap::new(),
        };
        for (side, tree) in [left, right].into_iter().enumerate() {
            let mut marked = HashSet::new();
            for (serial, values) in &changed {
                let Some(value) = values[side] else { continue };
                let need = value.namespace_ref_count.max(1);
                let dirty = &mut plan.dirty[side];
                if !locator.locate(tree, dirty, &mut marked, *serial, need)? {
                    return Ok(None);
                }
            }
        }
        Ok(Some(plan))
    }
}

/// Climbs recorded bindings from changed inodes to the root, each binding
/// checked against the tree; one child's recorded bindings are asked once.
struct Locator<'r> {
    reader: &'r dyn AuthenticatedObjects,
    hints: &'r dyn BindingHints,
    recorded: HashMap<u64, Vec<(u64, PathName)>>,
}

impl Locator<'_> {
    /// Marks the paths of `serial` in `tree` up to the root into `dirty`;
    /// false when fewer than `need` of its bindings check against the tree.
    fn locate(
        &mut self,
        tree: &TreeTable,
        dirty: &mut HashMap<u64, BTreeSet<PathName>>,
        marked: &mut HashSet<u64>,
        serial: u64,
        need: u64,
    ) -> ContentResult<bool> {
        let mut todo = vec![(serial, need)];
        while let Some((child, need)) = todo.pop() {
            if child == tree.root_serial() || !marked.insert(child) {
                continue;
            }
            let bindings = match self.recorded.entry(child) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(self.hints.bindings_of(child)?)
                }
            };
            let mut found = 0;
            for (parent, name) in bindings.iter() {
                if tree.lookup(self.reader, *parent, name)? == Some(child) {
                    found += 1;
                    dirty.entry(*parent).or_default().insert(name.clone());
                    todo.push((*parent, 1));
                }
            }
            if found < need {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl<'a> ChangeWalk<'a> {
    /// The walk of both whole trees. A resumed walk names the first component
    /// of its resume path as `from`, and the root lists from it.
    pub fn full(
        reader: &'a dyn AuthenticatedObjects,
        left: &'a TreeTable,
        right: &'a TreeTable,
        from: Option<&PathName>,
    ) -> ContentResult<Self> {
        let mut walk = Self::new(reader, left, right, None);
        walk.start(from)?;
        Ok(walk)
    }

    /// The walk of the changed entries `plan` holds. A resumed walk names the
    /// first component of its resume path as `from`, and the root visits only
    /// the names at or after it.
    pub fn planned(
        reader: &'a dyn AuthenticatedObjects,
        left: &'a TreeTable,
        right: &'a TreeTable,
        plan: &'a Plan,
        from: Option<&PathName>,
    ) -> ContentResult<Self> {
        let mut walk = Self::new(reader, left, right, Some(plan));
        walk.start(from)?;
        Ok(walk)
    }

    fn new(
        reader: &'a dyn AuthenticatedObjects,
        left: &'a TreeTable,
        right: &'a TreeTable,
        plan: Option<&'a Plan>,
    ) -> Self {
        Self {
            reader,
            sides: [left, right],
            plan,
            stack: Vec::new(),
        }
    }

    /// Enters the two root directories; the roots themselves are not yielded.
    fn start(&mut self, from: Option<&PathName>) -> ContentResult<()> {
        let root = |tree: &TreeTable| -> ContentResult<Option<EntryFacts>> {
            let serial = tree.root_serial();
            Ok(tree
                .value(self.reader, serial)?
                .map(|value| EntryFacts { serial, value }))
        };
        let pair = ("/".to_owned(), root(self.sides[0])?, root(self.sides[1])?);
        self.enter_from(&pair, from)
    }

    /// Both sides' entries of the bound `names`, values read in one batch per side.
    fn bound(&self, names: Vec<Bound>) -> ContentResult<VecDeque<Child>> {
        let mut values: [Vec<Option<InodeValue>>; 2] = [Vec::new(), Vec::new()];
        for (side, values) in values.iter_mut().enumerate() {
            let serials = names
                .iter()
                .filter_map(|(_, serials)| serials[side])
                .collect::<Vec<_>>();
            *values = self.sides[side].values(self.reader, &serials)?;
        }
        let mut cursor = [values[0].iter(), values[1].iter()];
        let mut out = VecDeque::with_capacity(names.len());
        for (name, serials) in names {
            let mut entries = [None, None];
            for side in 0..2 {
                if let Some(serial) = serials[side] {
                    let value = cursor[side]
                        .next()
                        .copied()
                        .flatten()
                        .ok_or(ContentError::InvalidRecord("bound inode record"))?;
                    entries[side] = Some(EntryFacts { serial, value });
                }
            }
            out.push_back((name, entries));
        }
        Ok(out)
    }

    /// The next window of a full-listing frame, merged by name.
    fn refill(&self, listings: &mut [Option<Listing>; 2]) -> ContentResult<Vec<Bound>> {
        for listing in listings.iter_mut().flatten() {
            if listing.page.is_empty() && !listing.exhausted {
                let page = list_after(
                    self.reader,
                    DirectoryRoot(listing.root),
                    listing.after.as_ref(),
                    WINDOW,
                    WINDOW_BYTES,
                    &mut DirectoryReadWork::default(),
                )?;
                listing.exhausted = page.continuation.is_none();
                listing.after = page.continuation;
                listing.page.extend(page.entries);
            }
        }
        // Names up to the smaller last name of an unfinished side are complete
        // on both sides.
        let bound = listings
            .iter()
            .flatten()
            .filter(|listing| !listing.exhausted)
            .filter_map(|listing| listing.page.back().map(|(name, _)| name.clone()))
            .min();
        let mut merged: BTreeMap<PathName, [Option<u64>; 2]> = BTreeMap::new();
        for (side, listing) in listings.iter_mut().enumerate() {
            let Some(listing) = listing else { continue };
            while let Some((name, _)) = listing.page.front() {
                if bound.as_ref().is_some_and(|bound| name > bound) {
                    break;
                }
                let (name, serial) = listing.page.pop_front().expect("front entry");
                merged.entry(name).or_default()[side] = Some(serial);
            }
        }
        Ok(merged.into_iter().collect())
    }

    /// The next path in component order, with both sides' entries.
    pub fn next_pair(&mut self) -> ContentResult<Option<Pair>> {
        loop {
            let Some(mut frame) = self.stack.pop() else {
                return Ok(None);
            };
            if frame.ready.is_empty() {
                if let Some(listings) = frame.listings.as_mut() {
                    let names = self.refill(listings)?;
                    frame.ready = self.bound(names)?;
                }
            }
            let Some((name, [left, right])) = frame.ready.pop_front() else {
                continue;
            };
            let path = join_exact(&frame.path, &name)?;
            self.stack.push(frame);
            return Ok(Some((path, left, right)));
        }
    }

    /// Descends below `pair`, the pair just yielded, where it can differ.
    pub fn enter(&mut self, pair: &Pair) -> ContentResult<()> {
        self.enter_from(pair, None)
    }

    /// [`Self::enter`] for a resumed walk: only the names at or after `from`
    /// are visited, so a later page does not read the entries earlier pages
    /// listed. A listed directory starts at `from` itself, found by lookup,
    /// then lists after it (#364 item 4).
    pub fn enter_from(
        &mut self,
        (path, left, right): &Pair,
        from: Option<&PathName>,
    ) -> ContentResult<()> {
        let directory = |entry: &Option<EntryFacts>| {
            entry.filter(|entry| entry.value.kind == InodeKind::Directory)
        };
        let frame = match (directory(left), directory(right)) {
            (None, None) => return Ok(()),
            (Some(l), Some(r)) if l.serial == r.serial && self.plan.is_some() => {
                let mut names = BTreeSet::new();
                for dirty in self.plan.iter().flat_map(|plan| &plan.dirty) {
                    if let Some(dirty) = dirty.get(&l.serial) {
                        names.extend(dirty.iter().cloned());
                    }
                }
                if l.value.content_root != r.value.content_root {
                    binding_changes(
                        self.reader,
                        Some(l.value.content_root),
                        Some(r.value.content_root),
                        &mut DifferenceWork::default(),
                        |name, _, _| {
                            names.insert(name.clone());
                            Ok(())
                        },
                    )?;
                }
                if let Some(from) = from {
                    names.retain(|name| name >= from);
                }
                if names.is_empty() {
                    return Ok(());
                }
                let mut bound = Vec::with_capacity(names.len());
                for name in names {
                    let mut serials = [None, None];
                    for (side, entry) in [l, r].into_iter().enumerate() {
                        serials[side] =
                            lookup(self.reader, DirectoryRoot(entry.value.content_root), &name)?;
                    }
                    bound.push((name, serials));
                }
                Frame {
                    path: path.clone(),
                    ready: self.bound(bound)?,
                    listings: None,
                }
            }
            (l, r) => {
                let mut listings = [None, None];
                for (listing, entry) in listings.iter_mut().zip([l, r]) {
                    let Some(entry) = entry else { continue };
                    let root = entry.value.content_root;
                    let mut page = VecDeque::new();
                    if let Some(from) = from {
                        if let Some(serial) = lookup(self.reader, DirectoryRoot(root), from)? {
                            page.push_back((from.clone(), serial));
                        }
                    }
                    *listing = Some(Listing {
                        root,
                        after: from.cloned(),
                        exhausted: false,
                        page,
                    });
                }
                Frame {
                    path: path.clone(),
                    ready: VecDeque::new(),
                    listings: Some(listings),
                }
            }
        };
        self.stack.push(frame);
        Ok(())
    }
}
