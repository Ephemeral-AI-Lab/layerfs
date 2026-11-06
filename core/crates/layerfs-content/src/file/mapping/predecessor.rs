//! Positional correspondence between a stored chunked mapping and a new one.
//!
//! A construction that re-chunks a file the Store already holds an earlier version
//! of can say which stored payload each newly emitted chunk continues: the earlier
//! version's mapping tree is walked forward, in logical order, and the extent
//! overlapping a chunk's own byte range is offered as that chunk's advisory
//! predecessor. The correspondence is positional - the base is consulted at the
//! logical offset the new chunk occupies - because the two mappings are two
//! partitions of one logical file and nothing else relates them.
//!
//! The walk is bounded, forward-only and holds no payload bytes: a mapping page is
//! acquired when the walk reaches it and released when the walk leaves it, so the
//! correspondence costs mapping pages and never a payload read. A base that is not
//! chunked offers nothing - admitting a whole-file base for a chunk is the deferred
//! cross-role half of the size-transition paper and is not done here - and a walk
//! that reaches the descriptor ceiling stops offering hints, because running out of
//! correspondence is not an error.

use layerfs_telemetry::timer::{Active, TimingScope};

use crate::error::{ContentError, ContentResult};
use crate::file::mapping::extents::Walk;
use crate::file::mapping::types::ExtentSlice;
use crate::object::{AuthenticatedObjects, ObjectId};

/// Extent descriptors one cursor walks before it stops offering hints.
pub const PREDECESSOR_DESCRIPTOR_LIMIT: usize = 4_096;

/// A stored earlier version offered as a positional delta base.
#[derive(Clone, Copy)]
pub struct PredecessorBase<'a> {
    reader: &'a dyn AuthenticatedObjects,
    root: ObjectId,
}

impl<'a> PredecessorBase<'a> {
    /// Offers the stored object `root`, served by `reader`, as the positional base.
    pub const fn new(reader: &'a dyn AuthenticatedObjects, root: ObjectId) -> Self {
        Self { reader, root }
    }

    /// Identity of the offered root.
    pub const fn root(self) -> ObjectId {
        self.root
    }

    /// The provider that serves the offered root and its mapping.
    pub(crate) fn reader(self) -> &'a dyn AuthenticatedObjects {
        self.reader
    }
}

/// Forward correspondence cursor over one stored chunked mapping.
pub struct PredecessorCursor<'a, 'r, 's> {
    reader: &'a dyn AuthenticatedObjects,
    scope: &'s TimingScope<'r, Active>,
    state: crate::file::mapping::FileState,
    walk: Walk,
    current: Option<(u64, u64, ExtentSlice)>,
    descriptors: usize,
    exhausted: bool,
    last_end: u64,
}

impl<'a, 'r, 's> PredecessorCursor<'a, 'r, 's> {
    /// Opens a cursor over `base`, or `None` when the stored base is not chunked.
    ///
    /// Opening costs one read: the base root's own canonical object. That read is
    /// what establishes the base's representation, so a whole-file base is declined
    /// here rather than at the first chunk, and the deferred cross-role pair is
    /// never offered.
    pub fn open(
        base: PredecessorBase<'a>,
        scope: &'s TimingScope<'r, Active>,
    ) -> ContentResult<Option<Self>> {
        let canonical = base
            .reader
            .read_canonical_scoped(base.root, scope.child("mapping.predecessor"))?;
        let state = match crate::file::classify(&canonical)? {
            crate::file::FileContent::WholeFile { .. } => return Ok(None),
            crate::file::FileContent::Chunked(state) => state,
        };
        Ok(Some(Self {
            reader: base.reader,
            scope,
            state,
            walk: Walk::new(state),
            current: None,
            descriptors: 0,
            exhausted: false,
            last_end: 0,
        }))
    }

    /// The stored payload overlapping the span of `len` bytes at `start`.
    ///
    /// Spans are offered in ascending logical order. `None` means the base holds no
    /// extent there or the walk has run out of descriptors; both are policy
    /// outcomes, and neither is an error.
    pub fn hint(&mut self, start: u64, len: u32) -> ContentResult<Option<ObjectId>> {
        if len == 0 || start < self.last_end {
            return Err(ContentError::InvalidRecord("predecessor span order"));
        }
        let end = start
            .checked_add(u64::from(len))
            .ok_or(ContentError::LengthOverflow)?;
        self.last_end = end;
        if self.exhausted {
            return Ok(None);
        }
        loop {
            if let Some((from, to, extent)) = self.current {
                if from >= end {
                    return Ok(None);
                }
                if to > start {
                    return Ok(Some(extent.payload_object_id()));
                }
                self.current = None;
            }
            if self.descriptors >= PREDECESSOR_DESCRIPTOR_LIMIT {
                self.exhausted = true;
                return Ok(None);
            }
            self.current = self.next_extent(start)?;
            if self.current.is_none() {
                return Ok(None);
            }
        }
    }

    /// The base's branch levels, collected once for positional page hints (#92).
    ///
    /// Reads at most [`crate::file::mapping::PAGE_HINT_BRANCH_READS`] branch pages
    /// and never a leaf or a payload.
    pub fn page_hints(&self) -> ContentResult<crate::file::mapping::PageHints> {
        let (reader, scope) = (self.reader, self.scope);
        crate::file::mapping::PageHints::collect(&self.state, &mut |id, _| {
            reader.read_canonical_scoped(id, scope.child("mapping.predecessor"))
        })
    }

    /// Advances the frontier to the next extent at or after `start`.
    fn next_extent(&mut self, start: u64) -> ContentResult<Option<(u64, u64, ExtentSlice)>> {
        let (reader, scope) = (self.reader, self.scope);
        let found = self.walk.next(start, &mut |id, _| {
            reader.read_canonical_scoped(id, scope.child("mapping.predecessor"))
        })?;
        if found.is_some() {
            self.descriptors += 1;
        }
        Ok(found)
    }
}
