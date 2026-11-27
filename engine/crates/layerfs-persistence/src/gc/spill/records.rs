//! The fixed-width values one mark sorts in bounded memory ([`super::spill`]):
//! what it reached, in which order it walks it, and what it read.
use super::sort::Item;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::ObjectLocation,
    pack::layout::{GroupCodec, GroupView},
};
use std::cmp::Ordering;

/// The `width`-byte little-endian number at `at`.
fn number(bytes: &[u8], at: usize, width: usize) -> u64 {
    let mut word = [0u8; 8];
    word[..width].copy_from_slice(&bytes[at..at + width]);
    u64::from_le_bytes(word)
}

/// Writes the low `width` bytes of `value` at `at`. A value that needs more
/// is refused before this by [`Item::fits`].
fn put(out: &mut [u8], at: usize, width: usize, value: u64) {
    out[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
}

/// Whether `value` fits `width` bytes.
fn within(value: u64, width: usize) -> bool {
    value >> (8 * width) == 0
}

/// The reason an identity was reached.
pub(in crate::gc) const LOGICAL: u8 = 1;
pub(in crate::gc) const BASE: u8 = 2;

/// One identity a wave reached and has not located yet.
#[derive(Clone, Copy)]
pub(in crate::gc) struct Edge {
    pub(in crate::gc) id: ObjectId,
    /// [`LOGICAL`], [`BASE`] or both.
    pub(in crate::gc) kind: u8,
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

pub(in crate::gc) fn edges_by_bytes(left: &Edge, right: &Edge) -> Ordering {
    super::super::mark::table::by_bytes(&left.id, &right.id)
}

pub(in crate::gc) fn edges_by_signed_prefix(left: &Edge, right: &Edge) -> Ordering {
    super::super::mark::table::by_signed_prefix(&left.id, &right.id)
}

/// One located object and its rank in the engine's locator order.
#[derive(Clone, Copy)]
pub(in crate::gc) struct Row {
    pub(in crate::gc) location: ObjectLocation,
    pub(in crate::gc) rank: u64,
    /// Why the walk first reached it ([`LOGICAL`], [`BASE`] or both); zero
    /// in a list of what a pack holds.
    pub(in crate::gc) kind: u8,
}

// A file holds a locator in the widths the Store's own limits allow: 255
// groups a pack, 8,190 records a group, 16 MiB an object. Anything wider is
// not a locator this format stores, and is refused, never cut.
impl Item for Row {
    const BYTES: usize = 54;
    fn encode(&self, out: &mut [u8]) {
        out[..32].copy_from_slice(self.location.object_id.as_bytes());
        put(out, 32, 8, self.location.pack_id as u64);
        put(out, 40, 1, self.location.group_number as u64);
        put(out, 41, 2, self.location.record_number as u64);
        put(out, 43, 4, self.location.canonical_length as u64);
        put(out, 47, 6, self.rank);
        // A role's code leaves the two high bits free.
        out[53] = self.location.role.code() | (self.kind << 6);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            location: ObjectLocation {
                object_id: ObjectId::from_bytes(&bytes[..32]).ok()?,
                role: ObjectRole::from_code(bytes[53] & 0x3f).ok()?,
                canonical_length: number(bytes, 43, 4) as usize,
                pack_id: number(bytes, 32, 8) as i64,
                group_number: number(bytes, 40, 1) as usize,
                record_number: number(bytes, 41, 2) as usize,
                base: None,
            },
            rank: number(bytes, 47, 6),
            kind: bytes[53] >> 6,
        })
    }
    fn fits(&self) -> bool {
        within(self.location.group_number as u64, 1)
            && within(self.location.record_number as u64, 2)
            && within(self.location.canonical_length as u64, 4)
            && within(self.rank, 6)
            && self.location.role.code() < 0x40
            && self.kind < 4
    }
}

/// The pack a row is stored in: the key its sealed run is read by.
pub(in crate::gc) fn row_pack(row: &Row) -> i64 {
    row.location.pack_id
}

/// Locator order: pack, group, record.
pub(in crate::gc) fn rows_by_locator(left: &Row, right: &Row) -> Ordering {
    let key = |row: &Row| {
        (
            row.location.pack_id,
            row.location.group_number,
            row.location.record_number,
        )
    };
    key(left).cmp(&key(right))
}

/// One record the walk reached: where it is and the stored width a read
/// charges it.
#[derive(Clone, Copy)]
pub(in crate::gc) struct Kept {
    pub(in crate::gc) pack: i64,
    pub(in crate::gc) group: u64,
    pub(in crate::gc) record: u64,
    pub(in crate::gc) width: u64,
}

impl Item for Kept {
    const BYTES: usize = 16;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, 8, self.pack as u64);
        put(out, 8, 1, self.group);
        put(out, 9, 2, self.record);
        put(out, 11, 5, self.width);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: number(bytes, 0, 8) as i64,
            group: number(bytes, 8, 1),
            record: number(bytes, 9, 2),
            width: number(bytes, 11, 5),
        })
    }
    fn fits(&self) -> bool {
        within(self.group, 1) && within(self.record, 2) && within(self.width, 5)
    }
}

/// One object the walk first reached as a delta base and not through a
/// logical reference: its rank and the stored width a read charges it. If
/// no logical reference reaches it later either, it is kept only as a base.
#[derive(Clone, Copy)]
pub(in crate::gc) struct Based {
    pub(in crate::gc) rank: u64,
    pub(in crate::gc) width: u64,
}

impl Item for Based {
    const BYTES: usize = 11;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, 6, self.rank);
        put(out, 6, 5, self.width);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            rank: number(bytes, 0, 6),
            width: number(bytes, 6, 5),
        })
    }
    fn fits(&self) -> bool {
        within(self.rank, 6) && within(self.width, 5)
    }
}

pub(in crate::gc) fn based_by_rank(left: &Based, right: &Based) -> Ordering {
    left.rank.cmp(&right.rank)
}

/// One number of a list a mark made: a pack it retains.
#[derive(Clone, Copy)]
pub(in crate::gc) struct Number(pub(in crate::gc) u64);

impl Item for Number {
    const BYTES: usize = 8;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, 8, self.0);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self(number(bytes, 0, 8)))
    }
    fn absorb(&mut self, _other: &Self) -> bool {
        true
    }
}

pub(in crate::gc) fn numbers_ascending(left: &Number, right: &Number) -> Ordering {
    left.0.cmp(&right.0)
}

/// An inclusive run of numbers a mark noted: inode value ordinals it read.
#[derive(Clone, Copy)]
pub(in crate::gc) struct Span {
    pub(in crate::gc) first: u32,
    pub(in crate::gc) last: u32,
}

impl Item for Span {
    const BYTES: usize = 8;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, 4, u64::from(self.first));
        put(out, 4, 4, u64::from(self.last));
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            first: number(bytes, 0, 4) as u32,
            last: number(bytes, 4, 4) as u32,
        })
    }
    /// Two runs that start together are the longer one.
    fn absorb(&mut self, other: &Self) -> bool {
        self.last = self.last.max(other.last);
        true
    }
}

pub(in crate::gc) fn spans_by_first(left: &Span, right: &Span) -> Ordering {
    left.first.cmp(&right.first)
}

pub(in crate::gc) fn kept_by_locator(left: &Kept, right: &Kept) -> Ordering {
    (left.pack, left.group, left.record).cmp(&(right.pack, right.group, right.record))
}

/// How many records the Store locates in one pack.
#[derive(Clone, Copy)]
pub(in crate::gc) struct Count {
    pub(in crate::gc) pack: i64,
    pub(in crate::gc) records: u64,
}

impl Item for Count {
    const BYTES: usize = 16;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, 8, self.pack as u64);
        put(out, 8, 8, self.records);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: number(bytes, 0, 8) as i64,
            records: number(bytes, 8, 8),
        })
    }
    fn absorb(&mut self, other: &Self) -> bool {
        self.records = self.records.saturating_add(other.records);
        true
    }
}

pub(in crate::gc) fn counts_by_pack(left: &Count, right: &Count) -> Ordering {
    left.pack.cmp(&right.pack)
}

/// One group extent of a pack whose directory the walk read.
#[derive(Clone, Copy)]
pub(in crate::gc) struct Layout {
    pub(in crate::gc) pack: i64,
    pub(in crate::gc) number: u64,
    pub(in crate::gc) view: GroupView,
}

impl Item for Layout {
    const BYTES: usize = 22;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, 8, self.pack as u64);
        put(out, 8, 1, self.number);
        put(out, 9, 4, self.view.start as u64);
        put(out, 13, 4, self.view.end as u64);
        put(out, 17, 4, self.view.decoded_length as u64);
        out[21] = match self.view.codec {
            GroupCodec::Raw => 0,
            GroupCodec::Zstandard => 1,
        };
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: number(bytes, 0, 8) as i64,
            number: number(bytes, 8, 1),
            view: GroupView {
                start: number(bytes, 9, 4) as usize,
                end: number(bytes, 13, 4) as usize,
                decoded_length: number(bytes, 17, 4) as usize,
                codec: if bytes[21] == 0 {
                    GroupCodec::Raw
                } else {
                    GroupCodec::Zstandard
                },
            },
        })
    }
    fn fits(&self) -> bool {
        within(self.number, 1)
            && within(self.view.start as u64, 4)
            && within(self.view.end as u64, 4)
            && within(self.view.decoded_length as u64, 4)
    }
    /// A pack is immutable: a second reading of its directory says the same.
    fn absorb(&mut self, _other: &Self) -> bool {
        true
    }
}

pub(in crate::gc) fn layouts_by_group(left: &Layout, right: &Layout) -> Ordering {
    (left.pack, left.number).cmp(&(right.pack, right.number))
}

/// One read of pack body bytes, in the order the mark made them.
#[derive(Clone, Copy)]
pub(in crate::gc) struct BodyRead {
    pub(in crate::gc) pack: i64,
    pub(in crate::gc) sequence: u64,
    pub(in crate::gc) bytes: u64,
    /// The pack's declared length for a scoped read, which caps the sum.
    pub(in crate::gc) limit: u64,
    /// Whether the whole body was read.
    pub(in crate::gc) whole: bool,
}

impl Item for BodyRead {
    const BYTES: usize = 28;
    fn encode(&self, out: &mut [u8]) {
        put(out, 0, 8, self.pack as u64);
        put(out, 8, 6, self.sequence);
        put(out, 14, 5, self.bytes);
        // A read with no cap carries the largest value: all eight bytes.
        put(out, 19, 8, self.limit);
        out[27] = u8::from(self.whole);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: number(bytes, 0, 8) as i64,
            sequence: number(bytes, 8, 6),
            bytes: number(bytes, 14, 5),
            limit: number(bytes, 19, 8),
            whole: bytes[27] != 0,
        })
    }
    fn fits(&self) -> bool {
        within(self.sequence, 6) && within(self.bytes, 5)
    }
}

pub(in crate::gc) fn reads_in_order(left: &BodyRead, right: &BodyRead) -> Ordering {
    (left.pack, left.sequence).cmp(&(right.pack, right.sequence))
}
