//! Delta bases for the chunks of an edited chunked file, by byte range.
//!
//! An edit keeps the base's extents on both sides of each replaced range and
//! scans only the replacement into new chunks. Each new chunk is offered, as its
//! advisory delta base, the old payload whose byte range overlaps the chunk's,
//! measured from an alignment between new and old positions. The alignment is
//! anchored at every unchanged neighbour: each edit's two boundaries, and every
//! new chunk equal to an old payload (content-defined chunking resynchronises
//! after an insert or a delete, so the chunks after it repeat the old ones at a
//! shifted position). A chunk with no overlapping old payload is offered the
//! successor of the nearest unchanged neighbour. Chunk index plays no part: an
//! early insert shifts every later index.
//!
//! The base's mapping is walked forward once, holding at most [`WINDOW`] old
//! extents and one mapping page; nothing grows with the file. A hint is
//! advisory: Storage decides whether a delta against it is stored.

use std::collections::VecDeque;

use crate::error::ContentResult;
use crate::file::cdc::MAXIMUM_CHUNK_BYTES;
use crate::file::mapping::extents::Walk;
use crate::file::mapping::node::FileState;
use crate::object::ObjectId;

/// Acquires one mapping page: its identity, and whether it is the tree's root.
pub(crate) type ReadPage<'r> = &'r mut dyn FnMut(ObjectId, bool) -> ContentResult<Vec<u8>>;

/// Old extents held ahead of the alignment, for finding the next unchanged
/// chunk after an insert or a delete.
pub const WINDOW: usize = 64;

/// One old extent: its byte range in the base and its payload.
type Old = (u64, u64, ObjectId);

/// Byte-range correspondence between a stored chunked base and its edit.
pub(crate) struct Aligner {
    walk: Walk,
    /// Old extents read and not yet passed, in base order.
    window: VecDeque<Old>,
    /// Base offset the walk has read up to.
    walked: u64,
    exhausted: bool,
    /// New position minus old position at the last unchanged neighbour.
    shift: i128,
    /// Old end of the last unchanged neighbour, and its payload.
    boundary: u64,
    neighbour: Option<ObjectId>,
}

impl Aligner {
    pub(crate) fn new(base: FileState) -> Self {
        Self {
            walk: Walk::new(base),
            window: VecDeque::new(),
            walked: 0,
            exhausted: false,
            shift: 0,
            boundary: 0,
            neighbour: None,
        }
    }

    /// An edit boundary: new position `new` follows the same bytes as old
    /// position `old`. Boundaries are given in ascending order.
    pub(crate) fn anchor(&mut self, new: u64, old: u64, read: ReadPage<'_>) -> ContentResult<()> {
        self.shift = i128::from(new) - i128::from(old);
        self.boundary = old;
        self.read(old, 0, read)?;
        // The neighbour is the old extent holding the byte before the boundary.
        if let Some((_, _, id)) = self.window.iter().rev().find(|(from, _, _)| *from < old) {
            self.neighbour = Some(*id);
        }
        while self.window.front().is_some_and(|(_, to, _)| *to <= old) {
            self.window.pop_front();
        }
        Ok(())
    }

    /// The advisory base of the new chunk `id`, `len` bytes (a repeated run's
    /// whole length) at new position `start`; `None` when it repeats an old payload or nothing corresponds.
    pub(crate) fn offer(
        &mut self,
        start: u64,
        len: u64,
        id: ObjectId,
        read: ReadPage<'_>,
    ) -> ContentResult<Option<ObjectId>> {
        let from = u64::try_from((i128::from(start) - self.shift).max(0)).unwrap_or(u64::MAX);
        // A repeated run is matched by its first chunk's range, so a long hole
        // never walks the base it spans.
        let end = from.saturating_add(len.min(MAXIMUM_CHUNK_BYTES as u64));
        self.read(end, WINDOW, read)?;
        // An unchanged chunk re-anchors the alignment where it is found.
        if let Some(index) = self.window.iter().position(|(_, _, old)| *old == id) {
            let (old_from, old_to, _) = self.window[index];
            self.shift = i128::from(start) - i128::from(old_from);
            self.boundary = old_to;
            self.neighbour = Some(id);
            self.window.drain(..=index);
            return Ok(None);
        }
        // The old payload sharing the most bytes of this range.
        let overlap =
            |(old_from, old_to, _): &&Old| old_to.min(&end).saturating_sub(*old_from.max(&from));
        if let Some(best) = self.window.iter().max_by_key(overlap) {
            if overlap(&best) > 0 {
                return Ok(Some(best.2));
            }
        }
        // Nothing overlaps: the successor of the nearest unchanged neighbour.
        Ok(self
            .window
            .iter()
            .find(|(old_from, _, _)| *old_from >= self.boundary)
            .map(|(_, _, old)| *old)
            .or(self.neighbour))
    }

    /// Reads old extents until the walk reaches `until` and the window holds
    /// `ahead` extents, or the base ends. The window never holds more than
    /// twice [`WINDOW`]: reaching further drops its oldest extents.
    fn read(&mut self, until: u64, ahead: usize, read: ReadPage<'_>) -> ContentResult<()> {
        while !self.exhausted && (self.walked < until || self.window.len() < ahead) {
            if self.window.len() >= 2 * WINDOW {
                if self.walked >= until {
                    break;
                }
                self.window.pop_front();
            }
            // From the byte before the boundary, so its neighbour is read too.
            let start = self.walked.max(self.boundary.saturating_sub(1));
            match self.walk.next(start, read)? {
                Some((from, to, extent)) => {
                    self.walked = self.walked.max(to);
                    self.window
                        .push_back((from, to, extent.payload_object_id()));
                }
                None => self.exhausted = true,
            }
        }
        Ok(())
    }
}
