//! XDR (RFC 4506) primitives over one received record and one reply buffer.
use std::fmt;

/// The record ended before a field, or a length exceeded its declared bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Garbage;
impl fmt::Display for Garbage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("malformed XDR arguments")
    }
}
impl std::error::Error for Garbage {}

/// Sequential decoding of one record. Every read is bounds-checked.
pub struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], Garbage> {
        let end = self.at.checked_add(length).ok_or(Garbage)?;
        let slice = self.data.get(self.at..end).ok_or(Garbage)?;
        self.at = end;
        Ok(slice)
    }
    pub fn u32(&mut self) -> Result<u32, Garbage> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    pub fn u64(&mut self) -> Result<u64, Garbage> {
        Ok((u64::from(self.u32()?) << 32) | u64::from(self.u32()?))
    }
    pub fn bool(&mut self) -> Result<bool, Garbage> {
        match self.u32()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Garbage),
        }
    }
    /// Fixed-length opaque data, followed by its padding.
    pub fn fixed(&mut self, length: usize) -> Result<&'a [u8], Garbage> {
        let value = self.take(length)?;
        self.take(pad(length))?;
        Ok(value)
    }
    /// Variable-length opaque data of at most `limit` bytes.
    pub fn opaque(&mut self, limit: usize) -> Result<&'a [u8], Garbage> {
        let length = self.u32()? as usize;
        if length > limit {
            return Err(Garbage);
        }
        self.fixed(length)
    }
    /// The bytes not yet decoded.
    pub fn remaining(&self) -> usize {
        self.data.len() - self.at
    }
}

/// Bytes of padding after `length` bytes of opaque data.
pub const fn pad(length: usize) -> usize {
    (4 - length % 4) % 4
}
/// Encoded size of variable-length opaque data of `length` bytes.
pub const fn opaque_size(length: usize) -> usize {
    4 + length + pad(length)
}

/// One reply under construction.
#[derive(Default)]
pub struct Writer {
    data: Vec<u8>,
}
impl Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }
    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.data.extend_from_slice(&value.to_be_bytes());
        self
    }
    pub fn u64(&mut self, value: u64) -> &mut Self {
        self.data.extend_from_slice(&value.to_be_bytes());
        self
    }
    pub fn bool(&mut self, value: bool) -> &mut Self {
        self.u32(u32::from(value))
    }
    /// Fixed-length opaque data and its padding.
    pub fn fixed(&mut self, value: &[u8]) -> &mut Self {
        self.data.extend_from_slice(value);
        self.data.extend_from_slice(&[0; 3][..pad(value.len())]);
        self
    }
    /// Variable-length opaque data; callers bound `value` by its protocol limit.
    pub fn opaque(&mut self, value: &[u8]) -> &mut Self {
        self.u32(value.len() as u32);
        self.fixed(value)
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}
