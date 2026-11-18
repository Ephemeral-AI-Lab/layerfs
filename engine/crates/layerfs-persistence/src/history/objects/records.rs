//! The fixed-width values one mark sorts in bounded memory ([`super::spill`]):
//! what it reached, in which order it walks it, and what it read.
use super::spill::Item;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::ObjectLocation,
    pack::layout::{GroupCodec, GroupView},
};
use std::cmp::Ordering;

fn number(bytes: &[u8], at: usize) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(word)
}

fn put(out: &mut [u8], at: usize, value: u64) {
    out[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// The reason an identity was reached.
pub(super) const LOGICAL: u8 = 1;
pub(super) const BASE: u8 = 2;

/// One identity a wave reached and has not located yet.
#[derive(Clone, Copy)]
pub(super) struct Edge {
    pub(super) id: ObjectId,
    /// [`LOGICAL`], [`BASE`] or both.
    pub(super) kind: u8,
}

impl Item for Edge {
    const BYTES: usize = 33;
    fn encode(&self, out: &mut [u8]) {
        out[..32].copy_from_slice(self.id.as_bytes());
        out[32] = self.kind;
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            id: ObjectId::from_bytes(&bytes[..32]).ok()?,
            kind: bytes[32],
        })
    }
    fn absorb(&mut self, other: &Self) -> bool {
        self.kind |= other.kind;
        true
    }
}

pub(super) fn edges_by_bytes(left: &Edge, right: &Edge) -> Ordering {
    super::table::by_bytes(&left.id, &right.id)
}

pub(super) fn edges_by_signed_prefix(left: &Edge, right: &Edge) -> Ordering {
    super::table::by_signed_prefix(&left.id, &right.id)
}

/// One located object and its rank in the engine's locator order.
#[derive(Clone, Copy)]
pub(super) struct Row {
    pub(super) location: ObjectLocation,
    pub(super) rank: u64,
}

impl Item for Row {
    const BYTES: usize = 73;
    fn encode(&self, out: &mut [u8]) {
        out[..32].copy_from_slice(self.location.object_id.as_bytes());
        put(out, 32, self.location.pack_id as u64);
        put(out, 40, self.location.group_number as u64);
        put(out, 48, self.location.record_number as u64);
        put(out, 56, self.location.canonical_length as u64);
        put(out, 64, self.rank);
        out[72] = self.location.role.code();
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            location: ObjectLocation {
                object_id: ObjectId::from_bytes(&bytes[..32]).ok()?,
                role: ObjectRole::from_code(bytes[72]).ok()?,
                canonical_length: number(bytes, 56) as usize,
                pack_id: number(bytes, 32) as i64,
                group_number: number(bytes, 40) as usize,
                record_number: number(bytes, 48) as usize,
            },
            rank: number(bytes, 64),
        })
    }
}

/// Locator order: pack, group, record.
pub(super) fn rows_by_locator(left: &Row, right: &Row) -> Ordering {
    let key = |row: &Row| {
        (
            row.location.pack_id,
            row.location.group_number,
            row.location.record_number,
        )
    };
    key(left).cmp(&key(right))
}

/// One record the walk reached: where it is, the stored width a read
/// charges it, and its object's rank.
#[derive(Clone, Copy)]
pub(super) struct Kept {
    pub(super) pack: i64,
    pub(super) group: u64,
    pub(super) record: u64,
    pub(super) width: u64,
    pub(super) rank: u64,
}

impl Item for Kept {
    const BYTES: usize = 40;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, self.pack as u64);
        put(out, 8, self.group);
        put(out, 16, self.record);
        put(out, 24, self.width);
        put(out, 32, self.rank);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: number(bytes, 0) as i64,
            group: number(bytes, 8),
            record: number(bytes, 16),
            width: number(bytes, 24),
            rank: number(bytes, 32),
        })
    }
}

pub(super) fn kept_by_locator(left: &Kept, right: &Kept) -> Ordering {
    (left.pack, left.group, left.record).cmp(&(right.pack, right.group, right.record))
}

/// How many records the Store locates in one pack.
#[derive(Clone, Copy)]
pub(super) struct Count {
    pub(super) pack: i64,
    pub(super) records: u64,
}

impl Item for Count {
    const BYTES: usize = 16;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, self.pack as u64);
        put(out, 8, self.records);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: number(bytes, 0) as i64,
            records: number(bytes, 8),
        })
    }
    fn absorb(&mut self, other: &Self) -> bool {
        self.records = self.records.saturating_add(other.records);
        true
    }
}

pub(super) fn counts_by_pack(left: &Count, right: &Count) -> Ordering {
    left.pack.cmp(&right.pack)
}

/// One group extent of a pack whose directory the walk read.
#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub(super) pack: i64,
    pub(super) number: u64,
    pub(super) view: GroupView,
}

impl Item for Layout {
    const BYTES: usize = 41;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, self.pack as u64);
        put(out, 8, self.number);
        put(out, 16, self.view.start as u64);
        put(out, 24, self.view.end as u64);
        put(out, 32, self.view.decoded_length as u64);
        out[40] = match self.view.codec {
            GroupCodec::Raw => 0,
            GroupCodec::Zstandard => 1,
        };
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: number(bytes, 0) as i64,
            number: number(bytes, 8),
            view: GroupView {
                start: number(bytes, 16) as usize,
                end: number(bytes, 24) as usize,
                decoded_length: number(bytes, 32) as usize,
                codec: if bytes[40] == 0 {
                    GroupCodec::Raw
                } else {
                    GroupCodec::Zstandard
                },
            },
        })
    }
    /// A pack is immutable: a second reading of its directory says the same.
    fn absorb(&mut self, _other: &Self) -> bool {
        true
    }
}

pub(super) fn layouts_by_group(left: &Layout, right: &Layout) -> Ordering {
    (left.pack, left.number).cmp(&(right.pack, right.number))
}

/// One read of pack body bytes, in the order the mark made them.
#[derive(Clone, Copy)]
pub(super) struct BodyRead {
    pub(super) pack: i64,
    pub(super) sequence: u64,
    pub(super) bytes: u64,
    /// The pack's declared length for a scoped read, which caps the sum.
    pub(super) limit: u64,
    /// Whether the whole body was read.
    pub(super) whole: bool,
}

impl Item for BodyRead {
    const BYTES: usize = 33;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, self.pack as u64);
        put(out, 8, self.sequence);
        put(out, 16, self.bytes);
        put(out, 24, self.limit);
        out[32] = u8::from(self.whole);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: number(bytes, 0) as i64,
            sequence: number(bytes, 8),
            bytes: number(bytes, 16),
            limit: number(bytes, 24),
            whole: bytes[32] != 0,
        })
    }
}

pub(super) fn reads_in_order(left: &BodyRead, right: &BodyRead) -> Ordering {
    (left.pack, left.sequence).cmp(&(right.pack, right.sequence))
}
