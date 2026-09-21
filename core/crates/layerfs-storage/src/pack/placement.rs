//! Exact append-or-new placement and the retained open-pack tail.
//!
//! Placement is decided before anything is written: a group joins the lane's open
//! pack only when the exact assembled length and group count still fit, and
//! otherwise starts a new pack. A write is an **increment** - the directory
//! entries and the bodies of the groups it places, and nothing else - because the
//! directory region is reserved by the format and a body's offset never depends
//! on how many groups precede it (`pack::layout`). The retained tail holds framed
//! groups; it is kept because a closing assembly needed it, and because the
//! lane's retained bytes are a published observable, but no write copies it.

use crate::error::{StorageError, StorageResult};
use crate::pack::assemble::{body_bytes, control_area, directory_entries};
use crate::pack::layout::{
    append_fits, body_area_offset, directory_entry_len, pack_capacity, EncodedGroup, PackLane,
    HEADER_LEN,
};

/// One pack this save created and may still append to.
///
/// `assembled` is the running total of what `groups` would assemble to - the
/// header, one directory entry per group and every body - maintained as groups
/// land. It is the state the fit decision reads, so appending does not re-measure
/// the tail; `assembled_length` is the canonical predicate it agrees with, and
/// the placement case in `tests/pack_locator.rs` checks the two against each
/// other at every boundary probe.
#[derive(Clone, Debug)]
struct OpenPack {
    pack_id: i64,
    groups: Vec<EncodedGroup>,
    assembled: usize,
}

/// The exact increment selected for one pack.
///
/// Nothing here is the pack: it is what this write *adds* to one. The control
/// area is a whole 24-byte block because the two fields an append can move - the
/// group count and the assembled length - are in it, and writing the block in one
/// call is cheaper than two writes of four bytes each.
#[derive(Clone, Debug)]
pub struct SelectedWrite {
    /// Pack receiving the groups.
    pub pack_id: i64,
    /// True when this write creates the pack row.
    pub created: bool,
    /// Ordinal of the first group this write places.
    pub first_group: usize,
    /// Groups the pack holds after this write.
    pub group_count: usize,
    /// Assembled length of the pack after this write.
    pub used: usize,
    /// Bytes the pack row allocates, which is what a created row is zero-filled
    /// to. Every lane but Singleton allocates its full pack limit.
    pub capacity: usize,
    /// Control area of the pack as this write leaves it.
    pub control: [u8; HEADER_LEN],
    /// Directory entries for the placed groups.
    pub directory: Vec<u8>,
    /// Offset the entries land at: `HEADER_LEN + entry_len * first_group`.
    pub directory_offset: usize,
    /// Concatenated bodies of the placed groups.
    pub bodies: Vec<u8>,
    /// Offset the bodies land at: the pack's assembled length before this write.
    pub body_offset: usize,
    /// One entry per group placed into this pack by this call, in input order.
    pub placed: Vec<PlacedGroup>,
}

/// One pack a call is accumulating groups for, before its increment is built.
#[derive(Clone, Debug)]
struct PendingWrite {
    pack_id: i64,
    created: bool,
    first_group: usize,
    body_offset: usize,
    placed: Vec<PlacedGroup>,
}

/// Where one input group landed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacedGroup {
    /// Group ordinal inside the pack.
    pub group_number: usize,
    /// Records in the group.
    pub records: usize,
}

/// Per-lane placement state: at most one open pack.
#[derive(Clone, Debug, Default)]
pub struct LanePlacement {
    open: Option<OpenPack>,
}

impl LanePlacement {
    /// Empty placement state.
    pub fn new() -> Self {
        Self { open: None }
    }

    /// Bytes retained by this lane's open tail.
    ///
    /// The open state already carries the assembled length of its groups, so this
    /// is a read rather than a re-measurement; the lane is the state's own, which
    /// is why the caller no longer passes one.
    pub fn retained_bytes(&self) -> StorageResult<usize> {
        Ok(self.open.as_ref().map_or(0, |open| open.assembled))
    }

    /// Places `groups`, deciding append or new pack for each one in order.
    ///
    /// Every pack that receives a group in this call produces exactly one write,
    /// assembled once, after the last group that landed in it.
    pub fn select_many(
        &mut self,
        lane: PackLane,
        groups: Vec<EncodedGroup>,
        next_pack_id: &mut i64,
    ) -> StorageResult<Vec<SelectedWrite>> {
        let mut writes: Vec<SelectedWrite> = Vec::new();
        let mut pending: Option<PendingWrite> = None;
        for group in groups {
            // Exact fit of the open pack plus this group: the open tail and the
            // incoming group are measured, never copied, and the lane's own group
            // and pack limits decide - not the maxima of any other lane.
            let fits_open = match &self.open {
                Some(open) => append_fits(lane, open.assembled, open.groups.len(), &group)?,
                None => false,
            };
            if !fits_open {
                if let Some(entry) = pending.take() {
                    // The open pack is full and is about to be replaced, so its
                    // tail is dead: the increment is built from it and the groups
                    // are released with it.
                    writes.push(self.increment(lane, entry, true)?);
                }
                let pack_id = *next_pack_id;
                if pack_id <= 0 {
                    return Err(StorageError::Integrity("pack identifier"));
                }
                *next_pack_id = pack_id
                    .checked_add(1)
                    .ok_or(StorageError::Integrity("pack identifier overflow"))?;
                // A fresh pack already assembles to its control area. The running
                // total is the canonical assembled length - header, one directory
                // entry per group and every body - so it starts at the header, not
                // at zero: a total without the header lets a pack assemble up to
                // `HEADER_LEN` bytes past the lane limit, which `assemble` then
                // refuses with `CapacityExceeded { pack.assembled_length }`.
                // A fresh pack already assembles to its control area plus its
                // whole reserved directory region. The running total is the
                // canonical assembled length the fit probe is compared against,
                // so it starts there and not at zero: a total without them lets a
                // pack assemble past its lane limit, which `assemble` then refuses
                // with `CapacityExceeded { pack.assembled_length }`.
                let base = body_area_offset(lane);
                self.open = Some(OpenPack {
                    pack_id,
                    groups: Vec::new(),
                    assembled: base,
                });
                pending = Some(PendingWrite {
                    pack_id,
                    created: true,
                    first_group: 0,
                    body_offset: base,
                    placed: Vec::new(),
                });
            }
            if pending.is_none() {
                // The lane already has an open pack from an earlier call and this
                // group fits it: this write appends rather than creates.
                let open = self
                    .open
                    .as_ref()
                    .ok_or(StorageError::Integrity("placement state"))?;
                pending = Some(PendingWrite {
                    pack_id: open.pack_id,
                    created: false,
                    first_group: open.groups.len(),
                    body_offset: open.assembled,
                    placed: Vec::new(),
                });
            }
            let open = self
                .open
                .as_mut()
                .ok_or(StorageError::Integrity("placement state"))?;
            let group_number = open.groups.len();
            let records = group.records;
            let body = group.body_size(lane)?;
            open.assembled = open
                .assembled
                .checked_add(body)
                .ok_or(StorageError::Integrity("pack size"))?;
            open.groups.push(group);
            let entry = pending
                .as_mut()
                .ok_or(StorageError::Integrity("placement state"))?;
            entry.placed.push(PlacedGroup {
                group_number,
                records,
            });
        }
        if let Some(entry) = pending {
            // This pack stays open: a later group may still append to it, so the
            // tail is retained and the next increment borrows it.
            writes.push(self.increment(lane, entry, false)?);
        }
        Ok(writes)
    }

    /// Builds the increment for one pending write; `closing` releases the tail.
    fn increment(
        &mut self,
        lane: PackLane,
        entry: PendingWrite,
        closing: bool,
    ) -> StorageResult<SelectedWrite> {
        let open = self
            .open
            .as_mut()
            .ok_or(StorageError::Integrity("placement state"))?;
        if open.pack_id != entry.pack_id {
            return Err(StorageError::Integrity("placement pack identity"));
        }
        let used = open.assembled;
        let group_count = open.groups.len();
        if group_count > lane.group_count_limit() {
            return Err(StorageError::Integrity("pack group count"));
        }
        let placed_groups = open
            .groups
            .get(entry.first_group..)
            .ok_or(StorageError::Integrity("placement group ordinal"))?;
        let directory = directory_entries(lane, placed_groups, entry.body_offset)?;
        let bodies = body_bytes(lane, placed_groups)?;
        let directory_offset = HEADER_LEN
            .checked_add(directory_entry_len(lane) * entry.first_group)
            .ok_or(StorageError::Integrity("pack directory"))?;
        let control = control_area(lane, group_count, used)?;
        let capacity = pack_capacity(lane, used);
        if closing {
            // The tail is consumed, so its running total goes with it: the caller
            // replaces the open pack immediately after a closing increment. What
            // is left is an empty pack, which assembles to its control area and
            // its whole reserved directory region.
            open.groups.clear();
            open.assembled = body_area_offset(lane);
        }
        Ok(SelectedWrite {
            pack_id: entry.pack_id,
            created: entry.created
                && entry
                    .placed
                    .first()
                    .is_some_and(|group| group.group_number == 0),
            first_group: entry.first_group,
            group_count,
            used,
            capacity,
            control,
            directory,
            directory_offset,
            bodies,
            body_offset: entry.body_offset,
            placed: entry.placed,
        })
    }
}
