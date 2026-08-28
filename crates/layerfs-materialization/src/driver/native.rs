//! Native entry and metadata representation.

use super::{DriverError, Result};

pub const MAX_NATIVE_XATTR_BYTES: usize = 1024 * 1024;
const NATIVE_XATTR_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeKind {
    Directory,
    RegularFile,
    Symlink,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeEntry {
    pub name: Vec<u8>,
    pub kind: NativeKind,
    pub token: Vec<u8>,
    pub hard_link_key: Option<Vec<u8>>,
    pub link_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMetadata {
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub xattrs: NativeXattrs,
    pub acl: Option<Vec<u8>>,
    pub bsd_flags: u32,
}

/// Compact native xattrs with no per-entry heap allocation. The accepted
/// name+value population remains one MiB; framing is split into <=1 MiB
/// chunks and is not canonical LayerFS storage.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NativeXattrs {
    chunks: Vec<Vec<u8>>,
    count: usize,
    payload_bytes: usize,
    last_name: Option<Vec<u8>>,
}

impl NativeXattrs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }

    pub fn from_entries(entries: impl IntoIterator<Item = (Vec<u8>, Vec<u8>)>) -> Result<Self> {
        let mut xattrs = Self::new();
        for (name, value) in entries {
            xattrs.push(&name, &value)?;
        }
        Ok(xattrs)
    }

    pub fn push(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        if name.is_empty()
            || name.len() > 127
            || name.contains(&0)
            || value.len() > MAX_NATIVE_XATTR_BYTES
            || self
                .last_name
                .as_deref()
                .is_some_and(|previous| previous >= name)
        {
            return Err(DriverError::Unsupported);
        }
        self.payload_bytes = self
            .payload_bytes
            .checked_add(name.len())
            .and_then(|total| total.checked_add(value.len()))
            .filter(|total| *total <= MAX_NATIVE_XATTR_BYTES)
            .ok_or(DriverError::Unsupported)?;
        append_varint(&mut self.chunks, name.len() as u32);
        append_varint(
            &mut self.chunks,
            u32::try_from(value.len()).map_err(|_| DriverError::Unsupported)?,
        );
        append_chunked(&mut self.chunks, name);
        append_chunked(&mut self.chunks, value);
        self.last_name = Some(name.to_vec());
        self.count = self.count.checked_add(1).ok_or(DriverError::Unsupported)?;
        Ok(())
    }

    pub fn iter(&self) -> NativeXattrIter<'_> {
        NativeXattrIter {
            xattrs: self,
            chunk: 0,
            offset: 0,
            remaining: self.count,
        }
    }

    pub fn names(&self) -> NativeXattrNameIter<'_> {
        NativeXattrNameIter { inner: self.iter() }
    }
}

impl<'a> IntoIterator for &'a NativeXattrs {
    type Item = (Vec<u8>, Vec<u8>);
    type IntoIter = NativeXattrIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct NativeXattrIter<'a> {
    xattrs: &'a NativeXattrs,
    chunk: usize,
    offset: usize,
    remaining: usize,
}

pub struct NativeXattrNameIter<'a> {
    inner: NativeXattrIter<'a>,
}

impl Iterator for NativeXattrNameIter<'_> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.remaining == 0 {
            return None;
        }
        let name_len = self.inner.read_varint()? as usize;
        let value_len = self.inner.read_varint()? as usize;
        let name = self.inner.read_vec(name_len)?;
        self.inner.skip_bytes(value_len)?;
        self.inner.remaining -= 1;
        Some(name)
    }
}

impl Iterator for NativeXattrIter<'_> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let name_len = self.read_varint()? as usize;
        let value_len = self.read_varint()? as usize;
        let name = self.read_vec(name_len)?;
        let value = self.read_vec(value_len)?;
        self.remaining -= 1;
        Some((name, value))
    }
}

impl NativeXattrIter<'_> {
    fn read_byte(&mut self) -> Option<u8> {
        while self
            .xattrs
            .chunks
            .get(self.chunk)
            .is_some_and(|chunk| self.offset == chunk.len())
        {
            self.chunk += 1;
            self.offset = 0;
        }
        let byte = *self.xattrs.chunks.get(self.chunk)?.get(self.offset)?;
        self.offset += 1;
        Some(byte)
    }

    fn read_varint(&mut self) -> Option<u32> {
        let mut value = 0_u32;
        for shift in (0..=28).step_by(7) {
            let byte = self.read_byte()?;
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn read_vec(&mut self, len: usize) -> Option<Vec<u8>> {
        let mut value = Vec::with_capacity(len);
        for _ in 0..len {
            value.push(self.read_byte()?);
        }
        Some(value)
    }

    fn skip_bytes(&mut self, len: usize) -> Option<()> {
        for _ in 0..len {
            self.read_byte()?;
        }
        Some(())
    }
}

fn append_varint(chunks: &mut Vec<Vec<u8>>, mut value: u32) {
    let mut bytes = [0_u8; 5];
    let mut len = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes[len] = byte;
        len += 1;
        if value == 0 {
            append_chunked(chunks, &bytes[..len]);
            return;
        }
    }
}

fn append_chunked(chunks: &mut Vec<Vec<u8>>, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        if chunks
            .last()
            .is_none_or(|chunk| chunk.len() == NATIVE_XATTR_CHUNK_BYTES)
        {
            chunks.push(Vec::with_capacity(
                NATIVE_XATTR_CHUNK_BYTES.min(bytes.len()),
            ));
        }
        let chunk = chunks.last_mut().unwrap();
        let take = bytes
            .len()
            .min(NATIVE_XATTR_CHUNK_BYTES.saturating_sub(chunk.len()));
        chunk.extend_from_slice(&bytes[..take]);
        bytes = &bytes[take..];
    }
}
