//! Topology evidence of one validation: one small type with two containers.
//!
//! With serial state backed, every record is a short point job in the caller's
//! filesystem scope and nothing here is resident beyond two queue ordinals and
//! three counters. Without a backing the same records are resident maps, each
//! held under the caller's ordering budget at the rate every other resident
//! container of this module is charged; exceeding it refuses on the container's
//! size, never on the work a walk examined.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Bound;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::state::{
    change, key, serial, SerialState, TOPOLOGY_PLACED, TOPOLOGY_QUEUE, TOPOLOGY_ROOTED,
    TOPOLOGY_SCANNED, TOPOLOGY_TERRITORY,
};
use crate::object::ObjectId;
use crate::ConstructionRecordChange;

const VERSION: u8 = 1;
const STORED: u8 = 1;
const PARENT_STORED: u8 = 2;
const IN_PLACE: u8 = 4;
/// Keys one enumeration window returns and records one guarded batch changes.
pub(super) const RECORD_WINDOW: usize = 64;

/// One directory binding this operation states that the base does not have.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Placement {
    /// The directory the binding is stated in.
    pub parent: u64,
    /// The placed directory has a base record.
    pub stored: bool,
    /// The parent has a base record.
    pub parent_stored: bool,
    /// The parent is the directory's own base parent: it was renamed, not
    /// moved, and keeps its base ancestry.
    pub in_place: bool,
}

impl Placement {
    fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(10);
        let flags = if self.stored { STORED } else { 0 }
            | if self.parent_stored { PARENT_STORED } else { 0 }
            | if self.in_place { IN_PLACE } else { 0 };
        bytes.extend_from_slice(&[VERSION, flags]);
        bytes.extend_from_slice(&self.parent.to_be_bytes());
        bytes
    }
    fn decode(bytes: &[u8]) -> ContentResult<Self> {
        let invalid = ContentError::InvalidRecord("filesystem placement state");
        if bytes.len() != 10
            || bytes[0] != VERSION
            || bytes[1] > (STORED | PARENT_STORED | IN_PLACE)
        {
            return Err(invalid);
        }
        let parent = word(&bytes[2..]).ok_or(invalid)?;
        Ok(Self {
            parent,
            stored: bytes[1] & STORED != 0,
            parent_stored: bytes[1] & PARENT_STORED != 0,
            in_place: bytes[1] & IN_PLACE != 0,
        })
    }
}

fn word(bytes: &[u8]) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(..8)?.try_into().ok()?))
}

fn versioned(serial: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(9);
    bytes.push(VERSION);
    bytes.extend_from_slice(&serial.to_be_bytes());
    bytes
}

/// Placements, territory marks, rooted marks, the territory queue and the
/// parents whose removed names were already resolved.
pub(super) struct TopologyRecords<'s, 'b> {
    state: &'s SerialState<'b>,
    limit: usize,
    placed: BTreeMap<u64, Placement>,
    territory: BTreeMap<u64, u64>,
    rooted: BTreeSet<u64>,
    queue: VecDeque<(u64, ObjectId)>,
    head: u64,
    tail: u64,
    scanned: BTreeSet<u64>,
    placements: u64,
    stored: u64,
    in_place: u64,
    anchored: bool,
}

impl<'s, 'b> TopologyRecords<'s, 'b> {
    /// `limit` bounds each resident container; a backed state never reads it.
    pub fn new(state: &'s SerialState<'b>, limit: usize) -> Self {
        Self {
            state,
            limit,
            placed: BTreeMap::new(),
            territory: BTreeMap::new(),
            rooted: BTreeSet::new(),
            queue: VecDeque::new(),
            head: 0,
            tail: 0,
            scanned: BTreeSet::new(),
            placements: 0,
            stored: 0,
            in_place: 0,
            anchored: false,
        }
    }

    fn held(&self, entries: usize) -> ContentResult<()> {
        if entries > self.limit {
            return Err(ContentError::ObjectLimitExceeded {
                limit: self.limit,
                actual: entries,
            });
        }
        Ok(())
    }

    /// Placements recorded so far.
    pub fn placements(&self) -> u64 {
        self.placements
    }

    /// True once a directory with a base record has been placed.
    pub fn stored_placed(&self) -> bool {
        self.stored > 0
    }

    /// True when a placed stored directory is not known to be in place.
    pub fn moved(&self) -> bool {
        self.stored > self.in_place
    }

    /// True once a placement names a stored parent other than the root.
    pub fn anchored(&self) -> bool {
        self.anchored
    }

    /// The placement of one directory, when this operation recorded one.
    pub fn placed(&self, directory: u64) -> ContentResult<Option<Placement>> {
        if self.state.backed() {
            self.state
                .read(TOPOLOGY_PLACED, directory)?
                .as_deref()
                .map(Placement::decode)
                .transpose()
        } else {
            Ok(self.placed.get(&directory).copied())
        }
    }

    /// Records one window's placements, each guarded on its own absence.
    pub fn place(&mut self, window: BTreeMap<u64, Placement>, root: u64) -> ContentResult<()> {
        if window.is_empty() {
            return Ok(());
        }
        let placements = self
            .placements
            .checked_add(window.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        let stored = window.values().filter(|placement| placement.stored).count() as u64;
        let anchored = window
            .values()
            .any(|placement| placement.parent_stored && placement.parent != root);
        if self.state.backed() {
            let mut changes = Vec::with_capacity(window.len().min(RECORD_WINDOW));
            for (directory, placement) in window {
                changes.push(change(TOPOLOGY_PLACED, directory, None, placement.encode()));
                self.flush(&mut changes, false)?;
            }
            self.flush(&mut changes, true)?;
        } else {
            self.placed.extend(window);
            self.held(self.placed.len())?;
        }
        self.placements = placements;
        self.stored = self.stored.saturating_add(stored);
        self.anchored |= anchored;
        Ok(())
    }

    /// Sets the in-place flag on placements read as given, each guarded on the
    /// record it was read from.
    pub fn mark_in_place(&mut self, mut kept: Vec<(u64, Placement)>) -> ContentResult<()> {
        kept.sort_unstable_by_key(|(directory, _)| *directory);
        kept.dedup_by_key(|(directory, _)| *directory);
        if kept.is_empty() {
            return Ok(());
        }
        let marked = kept.len() as u64;
        if self.state.backed() {
            self.state.apply(
                kept.into_iter()
                    .map(|(directory, placement)| {
                        let after = Placement {
                            in_place: true,
                            ..placement
                        };
                        change(
                            TOPOLOGY_PLACED,
                            directory,
                            Some(placement.encode()),
                            after.encode(),
                        )
                    })
                    .collect(),
            )?;
        } else {
            for (directory, _) in kept {
                self.placed
                    .get_mut(&directory)
                    .ok_or(ContentError::InvalidRecord("filesystem placement missing"))?
                    .in_place = true;
            }
        }
        self.in_place = self.in_place.saturating_add(marked);
        Ok(())
    }

    /// True when this parent's removed names were already resolved.
    pub fn scanned(&self, parent: u64) -> ContentResult<bool> {
        if !self.state.backed() {
            return Ok(self.scanned.contains(&parent));
        }
        match self.state.read(TOPOLOGY_SCANNED, parent)?.as_deref() {
            None => Ok(false),
            Some([VERSION]) => Ok(true),
            Some(_) => Err(ContentError::InvalidRecord("filesystem scanned state")),
        }
    }

    /// Records that this parent's removed names were resolved, guarded on absence.
    pub fn mark_scanned(&mut self, parent: u64) -> ContentResult<()> {
        if self.state.backed() {
            self.state
                .apply(vec![change(TOPOLOGY_SCANNED, parent, None, vec![VERSION])])
        } else {
            self.scanned.insert(parent);
            self.held(self.scanned.len())
        }
    }

    /// Applies a full window, or whatever is left when `last` is set.
    fn flush(&self, changes: &mut Vec<ConstructionRecordChange>, last: bool) -> ContentResult<()> {
        if changes.is_empty() || (!last && changes.len() < RECORD_WINDOW) {
            return Ok(());
        }
        let window = std::mem::replace(changes, Vec::with_capacity(RECORD_WINDOW));
        self.state.apply(window)
    }

    /// At most one window of placements strictly after `after`, in serial order.
    pub fn placements_after(&self, after: Option<u64>) -> ContentResult<Vec<(u64, Placement)>> {
        if !self.state.backed() {
            let from = after.map_or(Bound::Unbounded, Bound::Excluded);
            return Ok(self
                .placed
                .range((from, Bound::Unbounded))
                .take(RECORD_WINDOW)
                .map(|(directory, placement)| (*directory, *placement))
                .collect());
        }
        let keys = self.state.keys_after(
            TOPOLOGY_PLACED,
            after.map(|after| key(TOPOLOGY_PLACED, after).key),
        )?;
        let mut window = Vec::with_capacity(keys.len());
        for key in keys {
            let directory = serial(key)?;
            let placement = self
                .placed(directory)?
                .ok_or(ContentError::InvalidRecord("filesystem placement missing"))?;
            window.push((directory, placement));
        }
        Ok(window)
    }

    /// The moved directory whose surviving subtree holds this directory.
    pub fn territory(&self, directory: u64) -> ContentResult<Option<u64>> {
        if self.state.backed() {
            let bytes = self.state.read(TOPOLOGY_TERRITORY, directory)?;
            match bytes.as_deref() {
                None => Ok(None),
                Some([VERSION, owner @ ..]) if owner.len() == 8 => Ok(word(owner)),
                Some(_) => Err(ContentError::InvalidRecord("filesystem territory state")),
            }
        } else {
            Ok(self.territory.get(&directory).copied())
        }
    }

    /// Marks a directory a territory walk listed, guarded on absence.
    pub fn mark_territory(&mut self, directory: u64, owner: u64) -> ContentResult<()> {
        if self.state.backed() {
            self.state.apply(vec![change(
                TOPOLOGY_TERRITORY,
                directory,
                None,
                versioned(owner),
            )])
        } else {
            self.territory.insert(directory, owner);
            self.held(self.territory.len())
        }
    }

    /// True when an earlier walk proved this directory reaches the root.
    pub fn rooted(&self, directory: u64) -> ContentResult<bool> {
        if !self.state.backed() {
            return Ok(self.rooted.contains(&directory));
        }
        match self.state.read(TOPOLOGY_ROOTED, directory)?.as_deref() {
            None => Ok(false),
            Some([VERSION]) => Ok(true),
            Some(_) => Err(ContentError::InvalidRecord("filesystem rooted state")),
        }
    }

    /// Marks the distinct directories of one proven walk as rooted.
    pub fn mark_rooted(&mut self, mut directories: Vec<u64>) -> ContentResult<()> {
        if directories.is_empty() {
            return Ok(());
        }
        if self.state.backed() {
            directories.sort_unstable();
            self.state.apply(
                directories
                    .into_iter()
                    .map(|directory| change(TOPOLOGY_ROOTED, directory, None, vec![VERSION]))
                    .collect(),
            )
        } else {
            self.rooted.extend(directories);
            self.held(self.rooted.len())
        }
    }

    /// Appends directories still to list, with their base listing roots.
    pub fn push(&mut self, directories: Vec<(u64, ObjectId)>) -> ContentResult<()> {
        if directories.is_empty() {
            return Ok(());
        }
        if !self.state.backed() {
            self.queue.extend(directories);
            return self.held(self.queue.len());
        }
        let mut tail = self.tail;
        let mut changes = Vec::with_capacity(directories.len());
        for (directory, listing) in directories {
            let mut value = versioned(directory);
            value.extend_from_slice(listing.as_bytes());
            changes.push(change(TOPOLOGY_QUEUE, tail, None, value));
            tail = tail.checked_add(1).ok_or(ContentError::LengthOverflow)?;
        }
        self.state.apply(changes)?;
        self.tail = tail;
        Ok(())
    }

    /// The next queued directory and its base listing root, in queue order.
    pub fn pop(&mut self) -> ContentResult<Option<(u64, ObjectId)>> {
        if !self.state.backed() {
            return Ok(self.queue.pop_front());
        }
        if self.head == self.tail {
            return Ok(None);
        }
        let invalid = ContentError::InvalidRecord("filesystem territory queue state");
        let bytes =
            self.state
                .read(TOPOLOGY_QUEUE, self.head)?
                .ok_or(ContentError::InvalidRecord(
                    "filesystem territory queue missing",
                ))?;
        if bytes.len() != 41 || bytes[0] != VERSION {
            return Err(invalid);
        }
        let directory = word(&bytes[1..]).ok_or(invalid)?;
        let listing = ObjectId::from_bytes(&bytes[9..])?;
        self.head += 1;
        Ok(Some((directory, listing)))
    }
}
