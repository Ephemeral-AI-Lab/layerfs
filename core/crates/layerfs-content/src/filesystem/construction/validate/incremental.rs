//! Binding classification: each directory binding is the base's own or a placement.
//!
//! Rows arrive in windows of at most [`CLASSIFICATION_WINDOW_ROWS`]: one row per
//! header and one per bound name. A window's base records come from one grouped
//! demand, its restated names from one grouped point lookup per stored parent,
//! and its placements leave in one guarded batch. Streamed directory headers
//! share a parent demand independently of the name windows; the names still
//! fit in one window.

use std::collections::{BTreeMap, BTreeSet};

use super::backed::{Placement, TopologyRecords};
use super::checked::{
    charge_directory, FilesystemTopology, ValidationState, ValidationWork, BACKED_MEMO_RECORDS,
    CLASSIFICATION_WINDOW_ROWS,
};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::construction::input::view::{DirectoryRow, OperationInput};
use crate::filesystem::tree::directory::read::{lookup_many, DirectoryReadWork};
use crate::filesystem::tree::inode::read::{lookup_many as lookup_inodes, InodeReadWork};
use crate::filesystem::tree::pages::finish::DirectoryRoot;
use crate::filesystem::PathName;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, ObjectId};

/// One header, or one bound name of the header before it.
pub(super) struct WindowRow {
    pub parent: u64,
    pub bound: Option<(PathName, u64)>,
}

/// Where the names of the header being classified are stated.
#[derive(Clone, Copy)]
enum Listing {
    /// A directory this operation allocates, or any directory of a build.
    Fresh,
    /// A stored directory and its base listing root.
    Stored(ObjectId),
    /// The stored root, whose record is demanded only if a binding needs it.
    Root,
}

/// Stored directory children of one stored parent, awaiting the base's answer.
struct Restated {
    listing: ObjectId,
    names: Vec<PathName>,
    children: Vec<u64>,
}

pub(super) struct Classifier<'c, 's, 'b> {
    reader: &'c dyn AuthenticatedObjects,
    input: &'c dyn OperationInput,
    topology: FilesystemTopology,
    state: &'c mut ValidationState,
    records: &'c mut TopologyRecords<'s, 'b>,
    work: &'c mut ValidationWork,
    window: Vec<WindowRow>,
    parent_bases: BTreeMap<u64, Option<InodeValue>>,
    parent: Option<(u64, Listing)>,
    /// Inode pages the grouped window demands read, for the site split.
    pub prefetched_pages: u64,
    /// Rows classified so far.
    pub rows: u64,
}

impl<'c, 's, 'b> Classifier<'c, 's, 'b> {
    pub fn new(
        reader: &'c dyn AuthenticatedObjects,
        input: &'c dyn OperationInput,
        topology: FilesystemTopology,
        state: &'c mut ValidationState,
        records: &'c mut TopologyRecords<'s, 'b>,
        work: &'c mut ValidationWork,
    ) -> Self {
        Self {
            reader,
            input,
            topology,
            state,
            records,
            work,
            window: Vec::new(),
            parent_bases: BTreeMap::new(),
            parent: None,
            prefetched_pages: 0,
            rows: 0,
        }
    }

    /// Room for streamed headers while retaining any old-window headers
    /// that have not yet been classified.
    pub fn directory_room(&self) -> usize {
        BACKED_MEMO_RECORDS - self.window.iter().filter(|row| row.bound.is_none()).count()
    }

    /// Reads existing parent records together, then processes the headers
    /// and their streamed names in their original classification windows.
    pub fn streamed(&mut self, directories: &mut Vec<DirectoryRow<'_>>) -> ContentResult<()> {
        if directories.is_empty() {
            return Ok(());
        }
        let Some(table) = self.topology.table else {
            unreachable!("parent look-ahead requires a base");
        };
        let pending: BTreeSet<_> = self
            .window
            .iter()
            .filter(|row| row.bound.is_none())
            .map(|row| row.parent)
            .collect();
        self.parent_bases
            .retain(|serial, _| pending.contains(serial));
        let root = self.input.root_serial();
        let mut serials = Vec::with_capacity(directories.len());
        for directory in directories.iter() {
            let parent = directory.header.parent;
            if parent != root
                && !self.parent_bases.contains_key(&parent)
                && !self.input.is_new(parent)?
            {
                serials.push(parent);
            }
        }
        let mut work = InodeReadWork::default();
        let bases = lookup_inodes(self.reader, table, &serials, &mut work)?;
        self.work.read_waves = self.work.read_waves.saturating_add(work.read_waves);
        self.work.inode_pages_read = self.work.inode_pages_read.saturating_add(work.pages_read);
        self.prefetched_pages = self.prefetched_pages.saturating_add(work.pages_read);
        self.parent_bases.extend(serials.into_iter().zip(bases));
        for directory in directories.drain(..) {
            self.directory(directory)?;
        }
        Ok(())
    }

    pub fn directory(&mut self, directory: DirectoryRow<'_>) -> ContentResult<()> {
        let parent = directory.header.parent;
        self.row(WindowRow {
            parent,
            bound: None,
        })?;
        for change in directory.changes()? {
            let (name, binding) = change?;
            if let Some(child) = binding {
                self.row(WindowRow {
                    parent,
                    bound: Some((name, child)),
                })?;
            }
        }
        Ok(())
    }

    fn row(&mut self, row: WindowRow) -> ContentResult<()> {
        self.window.push(row);
        if self.window.len() == CLASSIFICATION_WINDOW_ROWS {
            self.classify()?;
        }
        Ok(())
    }

    /// Classifies whatever the last window holds.
    pub fn finish(&mut self) -> ContentResult<()> {
        self.classify()
    }

    fn classify(&mut self) -> ContentResult<()> {
        let window = std::mem::take(&mut self.window);
        if window.is_empty() {
            return Ok(());
        }
        self.rows = self.rows.saturating_add(window.len() as u64);
        self.work.peak_window_rows = self.work.peak_window_rows.max(window.len() as u64);
        self.demand(&window)?;
        let mut restated: BTreeMap<u64, Restated> = BTreeMap::new();
        let mut placed: BTreeMap<u64, Placement> = BTreeMap::new();
        for row in window {
            match row.bound {
                None => self.header(row.parent)?,
                Some((name, child)) => {
                    self.binding(row.parent, name, child, &mut restated, &mut placed)?
                }
            }
        }
        for (parent, candidates) in restated {
            let mut directory = DirectoryReadWork::default();
            let base = lookup_many(
                self.reader,
                DirectoryRoot(candidates.listing),
                &candidates.names,
                &mut directory,
            )?;
            charge_directory(self.work, directory);
            for (child, base) in candidates.children.into_iter().zip(base) {
                // The base already binds this name to this directory: the
                // change restates it and nothing moves.
                if base != Some(child) {
                    place(
                        &mut placed,
                        child,
                        Placement {
                            parent,
                            stored: true,
                            parent_stored: true,
                            in_place: false,
                        },
                    )?;
                }
            }
        }
        for child in placed.keys() {
            if self.records.placed(*child)?.is_some() {
                return Err(ContentError::InvalidRecord("multiple parents"));
            }
        }
        self.work.placements = self.work.placements.saturating_add(placed.len() as u64);
        self.records.place(placed, self.input.root_serial())
    }

    /// One grouped demand for the base records the current window still needs.
    fn demand(&mut self, window: &[WindowRow]) -> ContentResult<()> {
        let Some(table) = self.topology.table else {
            return Ok(());
        };
        let root = self.input.root_serial();
        let mut serials = Vec::with_capacity(window.len() + 1);
        let mut root_binds = false;
        for row in window {
            match &row.bound {
                None => {
                    if row.parent != root
                        && !self.parent_bases.contains_key(&row.parent)
                        && !self.input.is_new(row.parent)?
                    {
                        serials.push(row.parent);
                    }
                }
                Some((_, child)) => {
                    root_binds |= row.parent == root;
                    if *child != root && !self.state.verified_fresh(*child) {
                        serials.push(*child);
                    }
                }
            }
        }
        if root_binds {
            serials.push(root);
        }
        let before = self.work.inode_pages_read;
        self.state
            .prefetch(self.reader, table, serials, self.work)?;
        self.prefetched_pages = self
            .prefetched_pages
            .saturating_add(self.work.inode_pages_read.saturating_sub(before));
        Ok(())
    }

    /// Checks that the directory a header names can hold bindings.
    fn header(&mut self, parent: u64) -> ContentResult<()> {
        let listing = if parent == self.input.root_serial() {
            // The root directory of a new filesystem is built by this operation,
            // and a root directory update is legal; its own count stays zero.
            Listing::Root
        } else if self.input.is_new(parent)? || self.topology.table.is_none() {
            // A directory this operation allocates starts empty; its value must
            // still declare the directory kind it will have.
            let value = self
                .input
                .value_for(parent)?
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
            Listing::Fresh
        } else {
            let table = self.topology.table.expect("base inode table");
            let record = if let Some(base) = self.parent_bases.remove(&parent) {
                self.work.objects_read = self.work.objects_read.saturating_add(1);
                self.work.inode_demands = self.work.inode_demands.saturating_add(1);
                base.ok_or(ContentError::InvalidRecord("missing base inode"))?
            } else {
                self.state
                    .lookup_one(self.reader, table, parent, self.work)?
            };
            if record.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
            Listing::Stored(record.content_root)
        };
        self.parent = Some((parent, listing));
        Ok(())
    }

    /// The base listing root of the current header's directory, when it has one.
    fn listing(&mut self, parent: u64) -> ContentResult<Option<ObjectId>> {
        let Some((current, listing)) = self.parent else {
            return Err(ContentError::InvalidRecord("directory parent"));
        };
        if current != parent {
            return Err(ContentError::InvalidRecord("directory parent"));
        }
        Ok(match (listing, self.topology.table) {
            (Listing::Stored(listing), _) => Some(listing),
            (Listing::Root, Some(table)) => {
                let record = self
                    .state
                    .lookup_one(self.reader, table, parent, self.work)?;
                self.parent = Some((parent, Listing::Stored(record.content_root)));
                Some(record.content_root)
            }
            (Listing::Root, None) | (Listing::Fresh, _) => None,
        })
    }

    fn binding(
        &mut self,
        parent: u64,
        name: PathName,
        child: u64,
        restated: &mut BTreeMap<u64, Restated>,
        placed: &mut BTreeMap<u64, Placement>,
    ) -> ContentResult<()> {
        if child == self.input.root_serial() {
            return Err(ContentError::InvalidRecord("root directory binding"));
        }
        // The kind comes from the stored record for an existing inode and from
        // the caller's typed value for one this operation allocates.
        let stored = match self.topology.table {
            Some(table) => self
                .state
                .lookup_optional(self.reader, table, child, self.work)?,
            None => None,
        };
        let kind = match stored {
            Some(record) => record.kind,
            None => {
                self.input
                    .value_for(child)?
                    .ok_or(ContentError::InvalidRecord("binding kind"))?
                    .kind
            }
        };
        // Regular files and symlinks may carry several bindings; the reducer
        // derives their exact counts. Only a directory can close a cycle, so
        // only a directory's bindings are topology evidence.
        if kind != InodeKind::Directory {
            return Ok(());
        }
        match (stored, self.listing(parent)?) {
            (Some(_), Some(listing)) => {
                let candidates = restated.entry(parent).or_insert_with(|| Restated {
                    listing,
                    names: Vec::new(),
                    children: Vec::new(),
                });
                candidates.names.push(name);
                candidates.children.push(child);
                Ok(())
            }
            (stored, listing) => place(
                placed,
                child,
                Placement {
                    parent,
                    stored: stored.is_some(),
                    parent_stored: listing.is_some(),
                    in_place: false,
                },
            ),
        }
    }
}

/// Adds one placement to its window; a directory placed twice has two parents.
fn place(
    placed: &mut BTreeMap<u64, Placement>,
    child: u64,
    placement: Placement,
) -> ContentResult<()> {
    if placed.insert(child, placement).is_some() {
        return Err(ContentError::InvalidRecord("multiple parents"));
    }
    Ok(())
}
