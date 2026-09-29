use super::{index::Index, pack::PackedSlot};
use crate::{backing::budget::Charge, WorkspaceError};
use layerfs_bridge::contract::MAX_FILE;
use std::{collections::BTreeMap, mem::size_of};

const VALUE_BYTES: usize = 56;
const MAX_AFFECTED: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExtentKind {
    Base = 0,
    Zero = 1,
    Packed = 2,
    Payload = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    pub start: u64,
    pub end: u64,
    pub kind: ExtentKind,
    pub source_offset: u64,
    pub logical_page: u64,
    pub ordinal: u16,
    pub slot_length: u16,
    pub generation: u64,
    pub revision: u64,
}

pub struct ExtentPlan {
    pub length: u64,
    pub updates: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    _charge: Charge,
}

impl Extent {
    pub fn key(inode: u64, start: u64) -> [u8; 17] {
        let mut key = [0; 17];
        key[0] = b'E';
        key[1..9].copy_from_slice(&inode.to_be_bytes());
        key[9..17].copy_from_slice(&start.to_be_bytes());
        key
    }

    pub fn inverse_key(&self, inode: u64) -> Option<Vec<u8>> {
        let mut key = match self.kind {
            ExtentKind::Packed => {
                let mut key = Vec::with_capacity(27);
                key.push(b'R');
                key.extend_from_slice(&self.logical_page.to_be_bytes());
                key.extend_from_slice(&self.ordinal.to_be_bytes());
                key
            }
            ExtentKind::Payload => {
                let mut key = Vec::with_capacity(25);
                key.push(b'L');
                key.extend_from_slice(&self.logical_page.to_be_bytes());
                key
            }
            _ => return None,
        };
        key.extend_from_slice(&inode.to_be_bytes());
        key.extend_from_slice(&self.start.to_be_bytes());
        Some(key)
    }

    pub fn base(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            kind: ExtentKind::Base,
            source_offset: start,
            logical_page: 0,
            ordinal: 0,
            slot_length: 0,
            generation: 0,
            revision: 0,
        }
    }

    pub fn zero(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            kind: ExtentKind::Zero,
            source_offset: 0,
            logical_page: 0,
            ordinal: 0,
            slot_length: 0,
            generation: 0,
            revision: 0,
        }
    }

    pub fn packed(start: u64, slot: PackedSlot) -> Result<Self, WorkspaceError> {
        if start != slot.offset {
            return Err(WorkspaceError::InvalidInput);
        }
        let end = start
            .checked_add(slot.length as u64)
            .filter(|end| *end <= MAX_FILE)
            .ok_or(WorkspaceError::Capacity)?;
        Ok(Self {
            start,
            end,
            kind: ExtentKind::Packed,
            source_offset: 0,
            logical_page: slot.logical_page,
            ordinal: slot.ordinal,
            slot_length: slot.length,
            generation: slot.generation,
            revision: slot.revision,
        })
    }

    pub fn payload(
        start: u64,
        length: u64,
        id: u64,
        declared: u64,
    ) -> Result<Self, WorkspaceError> {
        let end = start
            .checked_add(length)
            .filter(|end| *end <= MAX_FILE)
            .ok_or(WorkspaceError::Capacity)?;
        let extent = Self {
            start,
            end,
            kind: ExtentKind::Payload,
            source_offset: 0,
            logical_page: id,
            ordinal: 0,
            slot_length: 0,
            generation: declared,
            revision: 0,
        };
        extent.validate()?;
        Ok(extent)
    }

    pub fn value(&self) -> Result<[u8; VALUE_BYTES], WorkspaceError> {
        self.validate()?;
        let mut bytes = [0; VALUE_BYTES];
        bytes[..8].copy_from_slice(&self.end.to_be_bytes());
        bytes[8] = self.kind as u8;
        bytes[16..24].copy_from_slice(&self.source_offset.to_be_bytes());
        bytes[24..32].copy_from_slice(&self.logical_page.to_be_bytes());
        bytes[32..34].copy_from_slice(&self.ordinal.to_be_bytes());
        bytes[34..36].copy_from_slice(&self.slot_length.to_be_bytes());
        bytes[36..44].copy_from_slice(&self.generation.to_be_bytes());
        bytes[44..52].copy_from_slice(&self.revision.to_be_bytes());
        Ok(bytes)
    }

    pub fn parse(key: &[u8], bytes: &[u8], inode: u64) -> Result<Self, WorkspaceError> {
        if key.len() != 17
            || key[0] != b'E'
            || key[1..9] != inode.to_be_bytes()
            || bytes.len() != VALUE_BYTES
            || bytes[9..16].iter().any(|b| *b != 0)
            || bytes[52..56].iter().any(|b| *b != 0)
        {
            return Err(WorkspaceError::Io);
        }
        let number = |at: usize| -> Result<u64, WorkspaceError> {
            Ok(u64::from_be_bytes(
                bytes[at..at + 8]
                    .try_into()
                    .map_err(|_| WorkspaceError::Io)?,
            ))
        };
        let kind = match bytes[8] {
            0 => ExtentKind::Base,
            1 => ExtentKind::Zero,
            2 => ExtentKind::Packed,
            3 => ExtentKind::Payload,
            _ => return Err(WorkspaceError::Io),
        };
        let extent = Self {
            start: u64::from_be_bytes(key[9..17].try_into().map_err(|_| WorkspaceError::Io)?),
            end: number(0)?,
            kind,
            source_offset: number(16)?,
            logical_page: number(24)?,
            ordinal: u16::from_be_bytes(bytes[32..34].try_into().map_err(|_| WorkspaceError::Io)?),
            slot_length: u16::from_be_bytes(
                bytes[34..36].try_into().map_err(|_| WorkspaceError::Io)?,
            ),
            generation: number(36)?,
            revision: number(44)?,
        };
        extent.validate()?;
        Ok(extent)
    }

    fn validate(&self) -> Result<(), WorkspaceError> {
        if self.start >= self.end || self.end > MAX_FILE {
            return Err(WorkspaceError::Io);
        }
        let length = self.end - self.start;
        let source_end = self
            .source_offset
            .checked_add(length)
            .ok_or(WorkspaceError::Io)?;
        let valid = match self.kind {
            ExtentKind::Base => {
                self.logical_page == 0
                    && self.ordinal == 0
                    && self.slot_length == 0
                    && self.generation == 0
                    && self.revision == 0
            }
            ExtentKind::Zero => {
                self.source_offset == 0
                    && self.logical_page == 0
                    && self.ordinal == 0
                    && self.slot_length == 0
                    && self.generation == 0
                    && self.revision == 0
            }
            ExtentKind::Packed => {
                self.logical_page != 0
                    && self.slot_length != 0
                    && self.generation != 0
                    && self.revision != 0
                    && source_end <= self.slot_length as u64
            }
            ExtentKind::Payload => {
                self.logical_page != 0
                    && self.ordinal == 0
                    && self.slot_length == 0
                    && self.generation != 0
                    && self.revision == 0
                    && source_end <= self.generation
            }
        };
        if valid {
            Ok(())
        } else {
            Err(WorkspaceError::Io)
        }
    }

    pub(super) fn cut(mut self, start: u64, end: u64) -> Result<Self, WorkspaceError> {
        if start < self.start || end > self.end || start >= end {
            return Err(WorkspaceError::Io);
        }
        if self.kind != ExtentKind::Zero {
            self.source_offset = self
                .source_offset
                .checked_add(start - self.start)
                .ok_or(WorkspaceError::Io)?;
        }
        self.start = start;
        self.end = end;
        self.validate()?;
        Ok(self)
    }
}

impl ExtentPlan {
    /// Shrink removes every selected interval beyond the new EOF; extension
    /// owns Zero bytes. Both reuse the ordinary range splice and inverse keys.
    pub fn resize(
        index: &Index,
        inode: u64,
        old_length: u64,
        new_length: u64,
    ) -> Result<Self, WorkspaceError> {
        if inode == 0 || old_length > MAX_FILE || new_length > MAX_FILE {
            return Err(WorkspaceError::InvalidInput);
        }
        if new_length == old_length {
            return Ok(Self {
                length: new_length,
                updates: Vec::new(),
                _charge: index.budget().reserve(0)?,
            });
        }
        let mut planned = Self::replace(
            index,
            inode,
            old_length,
            Extent::zero(new_length.min(old_length), new_length.max(old_length)),
        )?;
        if new_length < old_length {
            let key = Extent::key(inode, new_length);
            let removed = planned
                .updates
                .iter_mut()
                .find(|(current, _)| current.as_slice() == key.as_slice())
                .ok_or(WorkspaceError::Io)?;
            removed.1 = None;
            planned.length = new_length;
        }
        Ok(planned)
    }

    fn remove(updates: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>, inode: u64, extent: Extent) {
        updates.insert(Extent::key(inode, extent.start).to_vec(), None);
        if let Some(key) = extent.inverse_key(inode) {
            updates.insert(key.to_vec(), None);
        }
    }

    fn insert(
        updates: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        inode: u64,
        extent: Extent,
    ) -> Result<(), WorkspaceError> {
        updates.insert(
            Extent::key(inode, extent.start).to_vec(),
            Some(extent.value()?.to_vec()),
        );
        if let Some(key) = extent.inverse_key(inode) {
            updates.insert(key.to_vec(), Some(vec![1]));
        }
        Ok(())
    }

    /// Plans one tiny pwrite against the final indexed view. Initial inherited
    /// content is one implicit Base interval until the first local edit.
    pub fn tiny(
        index: &Index,
        inode: u64,
        old_length: u64,
        slot: PackedSlot,
    ) -> Result<Self, WorkspaceError> {
        if inode != slot.inode {
            return Err(WorkspaceError::InvalidInput);
        }
        let replacement = Extent::packed(slot.offset, slot)?;
        Self::replace(index, inode, old_length, replacement)
    }

    pub fn payload(
        index: &Index,
        inode: u64,
        old_length: u64,
        offset: u64,
        id: u64,
        declared: u64,
    ) -> Result<Self, WorkspaceError> {
        let replacement = Extent::payload(offset, declared, id, declared)?;
        Self::replace(index, inode, old_length, replacement)
    }

    pub(super) fn read_origin(
        index: &Index,
        inode: u64,
        old_length: u64,
        offset: u64,
        length: u64,
        source: u64,
    ) -> Result<Self, WorkspaceError> {
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= MAX_FILE)
            .ok_or(WorkspaceError::Capacity)?;
        let mut replacement = Extent::base(offset, end);
        replacement.source_offset = source;
        replacement.validate()?;
        Self::replace(index, inode, old_length, replacement)
    }

    fn replace(
        index: &Index,
        inode: u64,
        old_length: u64,
        replacement: Extent,
    ) -> Result<Self, WorkspaceError> {
        if inode == 0 || old_length > MAX_FILE {
            return Err(WorkspaceError::InvalidInput);
        }
        let start = replacement.start;
        let end = replacement.end;
        let mut charge = index.budget().reserve(128 * 1024)?;
        let first = Extent::key(inode, 0);
        let lower = Extent::key(inode, start);
        let upper = Extent::key(inode, end);
        let prefix = &first[..9];
        let mut affected = Vec::new();
        if let Some((key, value)) = index.floor(&lower)? {
            if key.starts_with(prefix) {
                let extent = Extent::parse(&key, &value, inode)?;
                if extent.end > start {
                    affected.push(extent);
                }
            }
        }
        let mut cursor = lower.to_vec();
        loop {
            let page = index.scan(&cursor, &upper, MAX_AFFECTED)?;
            let entries = affected
                .len()
                .checked_add(page.entries().len())
                .ok_or(WorkspaceError::Capacity)?;
            charge.resize(
                entries
                    .checked_mul(size_of::<Extent>() + 512)
                    .and_then(|bytes| bytes.checked_add(128 * 1024))
                    .ok_or(WorkspaceError::Capacity)?,
            )?;
            affected
                .try_reserve_exact(page.entries().len())
                .map_err(|_| WorkspaceError::Capacity)?;
            for (key, value) in page.entries() {
                let extent = Extent::parse(key, value, inode)?;
                if affected.last().is_none_or(|last| *last != extent) {
                    affected.push(extent);
                }
            }
            if page.entries().len() < MAX_AFFECTED {
                break;
            }
            cursor = page.entries().last().ok_or(WorkspaceError::Io)?.0.clone();
            cursor.push(0);
            if cursor.as_slice() >= upper.as_slice() {
                break;
            }
        }
        let mut updates = BTreeMap::new();
        if affected.is_empty() && old_length > 0 {
            let any = index.scan(&first, &Extent::key(inode, MAX_FILE), 1)?;
            if any.entries().is_empty() {
                let base = Extent::base(0, old_length);
                if start < old_length {
                    affected.push(base);
                } else {
                    Self::insert(&mut updates, inode, base)?;
                }
            }
        }
        let mut covered = start.min(old_length);
        let required = end.min(old_length);
        for extent in &affected {
            if extent.start > covered || extent.end <= covered {
                return Err(WorkspaceError::Io);
            }
            covered = extent.end.min(required);
        }
        if covered != required {
            return Err(WorkspaceError::Io);
        }
        for extent in affected {
            Self::remove(&mut updates, inode, extent);
            if extent.start < start {
                let left = extent.cut(extent.start, start)?;
                Self::insert(&mut updates, inode, left)?;
            }
            if extent.end > end {
                let right = extent.cut(end, extent.end)?;
                Self::insert(&mut updates, inode, right)?;
            }
        }
        if start > old_length {
            let gap = Extent::zero(old_length, start);
            Self::insert(&mut updates, inode, gap)?;
        }
        Self::insert(&mut updates, inode, replacement)?;
        Ok(Self {
            length: old_length.max(end),
            updates: updates.into_iter().collect(),
            _charge: charge,
        })
    }
}
