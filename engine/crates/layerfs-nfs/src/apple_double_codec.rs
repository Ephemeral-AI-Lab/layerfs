//! The AppleDouble image a macOS NFSv3 client writes as `._<name>`, and its
//! decoded form.
//!
//! The client lays an image out as an AppleDouble v2 header with two entries:
//! the FinderInfo, whose entry runs on into an `ATTR` table of extended
//! attributes (each name with its NUL, padded to four bytes, in entry order;
//! the values packed in the same order), and a resource fork at the end of
//! the image. The image length is a whole number of [`IMAGE_UNIT`] bytes, and
//! the bytes the layout leaves unused are zero.
//!
//! [`Image::parse`] reads those fields. [`Image::compact`] additionally
//! requires that [`Image::encode`] gives back the very same bytes, so a
//! companion held as an [`Image`] encodes again to exactly the image its
//! client wrote: no attribute, value or byte the layout carries is dropped.
//! Each attribute entry keeps the alignment padding the client left after its
//! name, because the client does not clear the bytes a shorter name replaced.
//!
//! Anything else - a resource fork with data, still another layout, a
//! half-written image - yields `None` and is stored as the client wrote it.

use std::fmt;

const MAGIC: u32 = 0x0005_1607;
const VERSION: u32 = 0x0002_0000;
const FILLER: &[u8; 16] = b"Mac OS X        ";
const FINDER_TAG: u32 = 9;
const FORK_TAG: u32 = 2;
/// Bytes of FinderInfo.
pub const FINDER_INFO: usize = 32;
/// The FinderInfo's offset: after the header, the entry count and two entries.
const FINDER_AT: usize = 50;
/// The `ATTR` header: the FinderInfo and two bytes of padding follow the
/// header of the image.
const ATTR_AT: usize = FINDER_AT + FINDER_INFO + 2;
const ATTR_MAGIC: u32 = 0x4154_5452;
/// The first attribute entry, after the 36-byte `ATTR` header.
const ENTRIES_AT: usize = ATTR_AT + 36;
/// An attribute entry before its name: value offset, value length, flags,
/// name length in bytes including the NUL.
const ENTRY_FIXED: usize = 11;
/// The step an image grows by.
pub const IMAGE_UNIT: usize = 4096;
/// The empty resource fork the client writes at the end of an image.
const BLANK_FORK: usize = 286;
const BLANK_FORK_TAG: &[u8] = b"This resource fork intentionally left blank   ";
/// Index bytes counted for one decoded image, besides its names and values.
const IMAGE_OVERHEAD: usize = 96;
/// Index bytes counted for one decoded attribute, besides its name and value.
const ATTR_OVERHEAD: usize = 56;

/// One extended attribute of a decoded image.
#[derive(Clone, Eq, PartialEq)]
pub struct Attribute {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
    /// The bytes the client left between the name's NUL and the next entry.
    pad: [u8; 3],
}
impl Attribute {
    /// An attribute the client would write with a cleared pad.
    pub fn new(name: &[u8], value: &[u8]) -> Self {
        Self {
            name: name.to_vec(),
            value: value.to_vec(),
            pad: [0; 3],
        }
    }
}
impl fmt::Debug for Attribute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Attribute")
            .field("name", &String::from_utf8_lossy(&self.name))
            .field("bytes", &self.value.len())
            .field("pad", &self.pad)
            .finish()
    }
}

/// The resource-fork entry of an image that has no resource-fork data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fork {
    /// A zero-length entry at the end of the image.
    None,
    /// The client's own empty resource fork, [`BLANK_FORK`] bytes.
    Blank,
}
impl Fork {
    fn len(self) -> usize {
        match self {
            Fork::None => 0,
            Fork::Blank => BLANK_FORK,
        }
    }
}

/// A decoded image: its FinderInfo, tag, extended attributes in entry order
/// and resource-fork entry, at its image length.
#[derive(Clone, Eq, PartialEq)]
pub struct Image {
    finder: [u8; FINDER_INFO],
    tag: u32,
    attrs: Vec<Attribute>,
    fork: Fork,
    len: usize,
}
impl fmt::Debug for Image {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Image")
            .field("tag", &self.tag)
            .field("attrs", &self.attrs)
            .field("fork", &self.fork)
            .field("len", &self.len)
            .finish()
    }
}
impl Image {
    /// The image a client would write for these attributes from scratch: the
    /// smallest whole number of [`IMAGE_UNIT`] bytes with its empty resource
    /// fork, and cleared padding. `None` for an empty name, a name holding a
    /// NUL, a name an entry cannot describe, or more attributes than an entry
    /// count can say.
    pub fn new(finder: [u8; FINDER_INFO], tag: u32, attrs: Vec<Attribute>) -> Option<Self> {
        let named = |name: &Vec<u8>| !name.is_empty() && name.len() < 255 && !name.contains(&0);
        if attrs.len() > usize::from(u16::MAX) || !attrs.iter().all(|attr| named(&attr.name)) {
            return None;
        }
        let mut image = Self {
            finder,
            tag,
            attrs,
            fork: Fork::Blank,
            len: 0,
        };
        let used = image.data_start() + image.data_len() + BLANK_FORK;
        image.len = used.div_ceil(IMAGE_UNIT) * IMAGE_UNIT;
        u32::try_from(image.len).ok()?;
        Some(image)
    }
    /// Its FinderInfo.
    pub fn finder_info(&self) -> [u8; FINDER_INFO] {
        self.finder
    }
    /// Its attributes.
    pub fn attributes(&self) -> &[Attribute] {
        &self.attrs
    }
    /// Its resource-fork entry.
    pub fn fork(&self) -> Fork {
        self.fork
    }
    /// The image length the client wrote, in bytes.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether no image has an empty length; here for `len`'s sake only.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Index bytes this decoded form is counted for.
    pub fn cost(&self) -> usize {
        IMAGE_OVERHEAD
            + self
                .attrs
                .iter()
                .map(|attr| attr.name.len() + attr.value.len() + ATTR_OVERHEAD)
                .sum::<usize>()
    }
    fn entry_end(&self, at: usize) -> usize {
        ENTRIES_AT
            + self
                .attrs
                .iter()
                .take(at)
                .map(|attr| entry_len(attr.name.len() + 1))
                .sum::<usize>()
    }
    fn data_start(&self) -> usize {
        self.entry_end(self.attrs.len())
    }
    fn data_len(&self) -> usize {
        self.attrs.iter().map(|attr| attr.value.len()).sum()
    }
    /// The image these fields describe, as the client lays one out.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.len];
        let fork_at = self.len - self.fork.len();
        let data_start = self.data_start();
        put32(&mut out, 0, MAGIC);
        put32(&mut out, 4, VERSION);
        out[8..24].copy_from_slice(FILLER);
        put16(&mut out, 24, 2);
        put32(&mut out, 26, FINDER_TAG);
        put32(&mut out, 30, FINDER_AT as u32);
        put32(&mut out, 34, (fork_at - FINDER_AT) as u32);
        put32(&mut out, 38, FORK_TAG);
        put32(&mut out, 42, fork_at as u32);
        put32(&mut out, 46, self.fork.len() as u32);
        out[FINDER_AT..FINDER_AT + FINDER_INFO].copy_from_slice(&self.finder);
        put32(&mut out, ATTR_AT, ATTR_MAGIC);
        put32(&mut out, ATTR_AT + 4, self.tag);
        put32(&mut out, ATTR_AT + 8, fork_at as u32);
        put32(&mut out, ATTR_AT + 12, data_start as u32);
        put32(&mut out, ATTR_AT + 16, self.data_len() as u32);
        put16(&mut out, ATTR_AT + 34, self.attrs.len() as u16);
        let (mut entry, mut value_at) = (ENTRIES_AT, data_start);
        for attr in &self.attrs {
            put32(&mut out, entry, value_at as u32);
            put32(&mut out, entry + 4, attr.value.len() as u32);
            out[entry + 10] = (attr.name.len() + 1) as u8;
            let name_at = entry + ENTRY_FIXED;
            let name_end = name_at + attr.name.len();
            out[name_at..name_end].copy_from_slice(&attr.name);
            let next = entry + entry_len(attr.name.len() + 1);
            out[name_end + 1..next].copy_from_slice(&attr.pad[..next - name_end - 1]);
            out[value_at..value_at + attr.value.len()].copy_from_slice(&attr.value);
            entry = next;
            value_at += attr.value.len();
        }
        if self.fork == Fork::Blank {
            out[fork_at..].copy_from_slice(&blank_fork());
        }
        out
    }
    /// The fields of `image`, laid out as the client writes one; [`Self::compact`]
    /// also requires that they encode back to the same bytes.
    pub fn parse(image: &[u8]) -> Option<Self> {
        let len = image.len();
        u32::try_from(len).ok()?;
        if len < ENTRIES_AT
            || get32(image, 0)? != MAGIC
            || get32(image, 4)? != VERSION
            || image[8..24] != FILLER[..]
            || get16(image, 24)? != 2
            || get32(image, 26)? != FINDER_TAG
            || get32(image, 30)? as usize != FINDER_AT
            || get32(image, 38)? != FORK_TAG
        {
            return None;
        }
        let fork = match get32(image, 46)? as usize {
            0 => Fork::None,
            BLANK_FORK => Fork::Blank,
            _ => return None,
        };
        let fork_at = get32(image, 42)? as usize;
        if fork_at.checked_add(fork.len())? != len
            || fork_at < ENTRIES_AT
            || get32(image, 34)? as usize != fork_at - FINDER_AT
            || get32(image, ATTR_AT)? != ATTR_MAGIC
            || get32(image, ATTR_AT + 8)? as usize != fork_at
        {
            return None;
        }
        let tag = get32(image, ATTR_AT + 4)?;
        let data_start = get32(image, ATTR_AT + 12)? as usize;
        let data_len = get32(image, ATTR_AT + 16)? as usize;
        let count = get16(image, ATTR_AT + 34)?;
        let (mut entry, mut value_at) = (ENTRIES_AT, data_start);
        let mut attrs = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let value_len = get32(image, entry + 4)? as usize;
            let name_len = usize::from(*image.get(entry + 10)?);
            let name_at = entry + ENTRY_FIXED;
            let name_end = entry + ENTRY_FIXED + name_len;
            let name = image.get(name_at..name_end)?;
            let (last, name) = name.split_last()?;
            if get32(image, entry)? as usize != value_at || *last != 0 || name.is_empty() {
                return None;
            }
            let value = image.get(value_at..value_at.checked_add(value_len)?)?;
            let next = entry + entry_len(name_len);
            let mut pad = [0u8; 3];
            pad[..next - name_end].copy_from_slice(image.get(name_end..next)?);
            attrs.push(Attribute {
                name: name.to_vec(),
                value: value.to_vec(),
                pad,
            });
            entry = next;
            value_at += value_len;
        }
        if entry != data_start
            || value_at != data_start.checked_add(data_len)?
            || value_at > fork_at
        {
            return None;
        }
        let mut finder = [0u8; FINDER_INFO];
        finder.copy_from_slice(image.get(FINDER_AT..FINDER_AT + FINDER_INFO)?);
        Some(Self {
            finder,
            tag,
            attrs,
            fork,
            len,
        })
    }
    /// The decoded form of `image` when it encodes back to exactly `image`.
    pub fn compact(image: &[u8]) -> Option<Self> {
        let decoded = Self::parse(image)?;
        (decoded.encode() == image).then_some(decoded)
    }
}

/// An attribute entry's bytes for a name of `name_len` bytes with its NUL,
/// rounded up to four.
fn entry_len(name_len: usize) -> usize {
    (ENTRY_FIXED + name_len + 3) & !3
}
/// The empty resource fork: a resource map with no types, after the tag.
fn blank_fork() -> [u8; BLANK_FORK] {
    let mut fork = [0u8; BLANK_FORK];
    for (at, value) in [
        (0, 0x100),
        (4, 0x100),
        (12, 0x1e),
        (256, 0x100),
        (260, 0x100),
    ] {
        put32(&mut fork, at, value);
    }
    put32(&mut fork, 268, 0x1e);
    put16(&mut fork, 280, 0x1c);
    put16(&mut fork, 282, 0x1e);
    put16(&mut fork, 284, 0xffff);
    fork[16..16 + BLANK_FORK_TAG.len()].copy_from_slice(BLANK_FORK_TAG);
    fork
}
fn put32(out: &mut [u8], at: usize, value: u32) {
    out[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn put16(out: &mut [u8], at: usize, value: u16) {
    out[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn get32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}
fn get16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}
