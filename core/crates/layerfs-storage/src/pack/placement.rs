//! Exact append-or-new placement and the retained open-pack tail.
//!
//! Placement is decided before anything is assembled: a group joins the lane's
//! open pack only when the exact assembled length and group count still fit, and
//! otherwise starts a new pack. The retained tail holds framed groups, not
//! assembled bytes, so a write re-assembles only the groups the selected pack
//! actually contains and existing group/record ordinals never move.

use crate::error::{StorageError, StorageResult};
use crate::pack::assemble;
use crate::pack::layout::{append_fits, assembled_length, EncodedGroup, PackLane};

/// One pack this save created and may still append to.
#[derive(Clone, Debug)]
struct OpenPack {
    pack_id: i64,
    groups: Vec<EncodedGroup>,
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

    /// Pack the lane is currently appending to, if any.
    pub fn open_pack_id(&self) -> Option<i64> {
        self.open.as_ref().map(|open| open.pack_id)
    }

    /// Bytes retained by the open tail for this lane.
    pub fn retained_bytes(&self, lane: PackLane) -> StorageResult<usize> {
        match &self.open {
            Some(open) => assembled_length(lane, &open.groups),
            None => Ok(0),
        }
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
                Some(open) => append_fits(lane, &open.groups, &group)?,
                None => false,
            };
            if !fits_open {
                if let Some((pack_id, created, placed)) = pending.take() {
                    writes.push(self.assemble_write(lane, pack_id, created, placed)?);
                }
                let pack_id = *next_pack_id;
                if pack_id <= 0 {
                    return Err(StorageError::Integrity("pack identifier"));
                }
                *next_pack_id = pack_id
                    .checked_add(1)
                    .ok_or(StorageError::Integrity("pack identifier overflow"))?;
                self.open = Some(OpenPack {
                    pack_id,
                    groups: Vec::new(),
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
            writes.push(self.assemble_write(lane, pack_id, created, placed)?);
        }
        Ok(writes)
    }

    fn assemble_write(
        &self,
        lane: PackLane,
        pack_id: i64,
        created: bool,
        placed: Vec<PlacedGroup>,
    ) -> StorageResult<SelectedWrite> {
        let open = self
            .open
            .as_ref()
            .ok_or(StorageError::Integrity("placement state"))?;
        if open.pack_id != pack_id {
            return Err(StorageError::Integrity("placement pack identity"));
        }
        Ok(SelectedWrite {
            pack_id,
            created: created && placed.first().is_some_and(|group| group.group_number == 0),
            bytes: assemble(lane, &open.groups)?,
            placed,
        })
    }
}
