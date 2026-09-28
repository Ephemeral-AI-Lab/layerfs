//! Selected 64-slot hot directory: one fixed table of tagged node targets.
//!
//! The directory is authoritative selected state; a charged in-memory copy is
//! resolution state, never the source of a frozen target. Members are 32-byte
//! records, so a full table is `8 + 64*32 = 2,056` body bytes and the remaining
//! body/tail bytes stay zero under the ordinary page framing.
use super::{
    keyed::{HOT_SLOTS, MAX_LEVEL},
    page::{Kind, Page, PageRef},
};
use crate::WorkspaceError;

const PREFIX_BYTES: usize = 8;
const ENTRY_BYTES: usize = 32;
const RESERVED_BYTES: usize = 6;
pub const DIRECTORY_RECORDS: u16 = HOT_SLOTS as u16;
pub const DIRECTORY_BODY: usize = PREFIX_BYTES + HOT_SLOTS * ENTRY_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub epoch: u64,
    pub page: PageRef,
    pub level: u8,
    pub kind: Kind,
}

#[derive(Clone)]
pub struct Directory {
    slots: [Option<Entry>; HOT_SLOTS],
}

fn level_kind(level: u8, kind: Kind) -> bool {
    level <= MAX_LEVEL && kind.indexed() && (level == 0) == (kind == Kind::IndexLeaf)
}

impl Entry {
    fn validate(&self) -> Result<(), WorkspaceError> {
        if self.epoch == 0
            || self.page.id == 0
            || self.page.epoch == 0
            || !level_kind(self.level, self.kind)
        {
            return Err(WorkspaceError::Io);
        }
        Ok(())
    }
}

impl Default for Directory {
    fn default() -> Self {
        Self::empty()
    }
}

impl Directory {
    pub fn empty() -> Self {
        Self {
            slots: [None; HOT_SLOTS],
        }
    }

    pub fn entry(&self, slot: usize) -> Option<Entry> {
        self.slots.get(slot).copied().flatten()
    }

    pub fn occupied(&self) -> usize {
        self.slots.iter().filter(|entry| entry.is_some()).count()
    }

    pub fn clear(&mut self, slot: usize) -> bool {
        match self.slots.get_mut(slot) {
            Some(target) => target.take().is_some(),
            None => false,
        }
    }

    pub fn set(&mut self, slot: usize, entry: Entry) -> Result<(), WorkspaceError> {
        entry.validate()?;
        let target = self
            .slots
            .get_mut(slot)
            .ok_or(WorkspaceError::InvalidInput)?;
        if target.is_some_and(|current| current.epoch == entry.epoch) {
            *target = Some(entry);
            return Ok(());
        }
        if target.is_some() {
            return Err(WorkspaceError::InvalidInput);
        }
        *target = Some(entry);
        Ok(())
    }

    /// Resolution checks the requested reuse epoch, node kind and level. An
    /// absent slot, a mismatched epoch or a superseded kind refuses.
    pub fn resolve(
        &self,
        slot: usize,
        epoch: u64,
        requested: Kind,
    ) -> Result<Entry, WorkspaceError> {
        let entry = self.entry(slot).ok_or(WorkspaceError::Io)?;
        if entry.epoch != epoch || entry.kind != requested {
            return Err(WorkspaceError::Io);
        }
        Ok(entry)
    }

    pub fn encode(&self) -> Result<Vec<u8>, WorkspaceError> {
        let mut body = vec![0; DIRECTORY_BODY];
        body[..2].copy_from_slice(&(HOT_SLOTS as u16).to_be_bytes());
        body[2..4].copy_from_slice(&(self.occupied() as u16).to_be_bytes());
        for (slot, entry) in self.slots.iter().enumerate() {
            let Some(entry) = entry else { continue };
            entry.validate()?;
            let at = PREFIX_BYTES + slot * ENTRY_BYTES;
            body[at..at + 8].copy_from_slice(&entry.epoch.to_be_bytes());
            body[at + 8..at + 16].copy_from_slice(&entry.page.id.to_be_bytes());
            body[at + 16..at + 24].copy_from_slice(&entry.page.epoch.to_be_bytes());
            body[at + 24] = entry.level;
            body[at + 25] = entry.kind as u8;
            body[at + 26..at + 26 + RESERVED_BYTES].fill(0);
        }
        Ok(body)
    }

    pub fn decode(
        page: &Page,
        incarnation: [u8; 32],
        reference: PageRef,
    ) -> Result<Self, WorkspaceError> {
        let body = page.verify(Kind::HotDirectory, incarnation, reference)?;
        if body.len() != DIRECTORY_BODY
            || page.records() != DIRECTORY_RECORDS
            || u16::from_be_bytes([body[0], body[1]]) as usize != HOT_SLOTS
            || body[4..PREFIX_BYTES].iter().any(|byte| *byte != 0)
        {
            return Err(WorkspaceError::Io);
        }
        let mut directory = Self::empty();
        for slot in 0..HOT_SLOTS {
            let at = PREFIX_BYTES + slot * ENTRY_BYTES;
            let number = |offset: usize| -> Result<u64, WorkspaceError> {
                Ok(u64::from_be_bytes(
                    body[at + offset..at + offset + 8]
                        .try_into()
                        .map_err(|_| WorkspaceError::Io)?,
                ))
            };
            let level = body[at + 24];
            let kind = body[at + 25];
            if body[at + 26..at + ENTRY_BYTES]
                .iter()
                .any(|byte| *byte != 0)
            {
                return Err(WorkspaceError::Io);
            }
            let epoch = number(0)?;
            if epoch == 0 {
                if body[at + 8..at + 24].iter().any(|byte| *byte != 0) || level != 0 || kind != 0 {
                    return Err(WorkspaceError::Io);
                }
                continue;
            }
            let kind = match kind {
                2 => Kind::IndexLeaf,
                3 => Kind::IndexBranch,
                _ => return Err(WorkspaceError::Io),
            };
            let entry = Entry {
                epoch,
                page: PageRef {
                    id: number(8)?,
                    epoch: number(16)?,
                },
                level,
                kind,
            };
            entry.validate()?;
            directory.slots[slot] = Some(entry);
        }
        if directory.occupied() != u16::from_be_bytes([body[2], body[3]]) as usize {
            return Err(WorkspaceError::Io);
        }
        Ok(directory)
    }
}
