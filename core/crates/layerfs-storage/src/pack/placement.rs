//! Exact append-or-new placement and the retained open-pack tail.
//!
//! Placement is decided before anything is assembled: a group joins the lane's
//! open pack only when the exact assembled length and group count still fit, and
//! otherwise starts a new pack. The retained tail holds framed groups, not
//! assembled bytes, so a write re-assembles only the groups the selected pack
//! actually contains and existing group/record ordinals never move.

use crate::error::{StorageError, StorageResult};
use crate::pack::assemble::{assemble, assemble_consuming};
use crate::pack::layout::{append_fits, directory_entry_len, EncodedGroup, PackLane, HEADER_LEN};

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

/// The exact write selected for one pack.
#[derive(Clone, Debug)]
pub struct SelectedWrite {
    /// Pack receiving the groups.
    pub pack_id: i64,
    /// True when this write creates the pack row.
    pub created: bool,
    /// Exact pack bytes to write.
    pub bytes: Vec<u8>,
    /// One entry per group placed into this pack by this call, in input order.
    pub placed: Vec<PlacedGroup>,
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
        let mut pending: Option<(i64, bool, Vec<PlacedGroup>)> = None;
        for group in groups {
            // Exact fit of the open pack plus this group: the open tail and the
            // incoming group are measured, never copied, and the lane's own group
            // and pack limits decide - not the maxima of any other lane.
            let fits_open = match &self.open {
                Some(open) => append_fits(lane, open.assembled, open.groups.len(), &group)?,
                None => false,
            };
            if !fits_open {
                if let Some((pack_id, created, placed)) = pending.take() {
                    // The open pack is full and is about to be replaced, so its
                    // tail is dead: the assembly consumes the groups and releases
                    // each body as it is copied instead of holding both.
                    writes.push(self.assemble_write(lane, pack_id, created, placed, true)?);
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
                self.open = Some(OpenPack {
                    pack_id,
                    groups: Vec::new(),
                    assembled: HEADER_LEN,
                });
                pending = Some((pack_id, true, Vec::new()));
            }
            if pending.is_none() {
                // The lane already has an open pack from an earlier call and this
                // group fits it: this write appends rather than creates.
                let open = self
                    .open
                    .as_ref()
                    .ok_or(StorageError::Integrity("placement state"))?;
                pending = Some((open.pack_id, false, Vec::new()));
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
                .checked_add(directory_entry_len(lane))
                .and_then(|total| total.checked_add(body))
                .ok_or(StorageError::Integrity("pack size"))?;
            open.groups.push(group);
            let entry = pending
                .as_mut()
                .ok_or(StorageError::Integrity("placement state"))?;
            entry.2.push(PlacedGroup {
                group_number,
                records,
            });
        }
        if let Some((pack_id, created, placed)) = pending {
            // This pack stays open: a later group may still append to it, so the
            // tail is retained and the assembly borrows it.
            writes.push(self.assemble_write(lane, pack_id, created, placed, false)?);
        }
        Ok(writes)
    }

    /// Assembles the write for `pack_id`; `closing` releases the tail it drains.
    fn assemble_write(
        &mut self,
        lane: PackLane,
        pack_id: i64,
        created: bool,
        placed: Vec<PlacedGroup>,
        closing: bool,
    ) -> StorageResult<SelectedWrite> {
        let open = self
            .open
            .as_mut()
            .ok_or(StorageError::Integrity("placement state"))?;
        if open.pack_id != pack_id {
            return Err(StorageError::Integrity("placement pack identity"));
        }
        let bytes = if closing {
            // The tail is consumed, so its running total goes with it: the caller
            // replaces the open pack immediately after a closing assembly. What is
            // left is an empty pack, which assembles to its header alone.
            open.assembled = HEADER_LEN;
            assemble_consuming(lane, std::mem::take(&mut open.groups))?
        } else {
            assemble(lane, &open.groups)?
        };
        Ok(SelectedWrite {
            pack_id,
            created: created && placed.first().is_some_and(|group| group.group_number == 0),
            bytes,
            placed,
        })
    }
}
