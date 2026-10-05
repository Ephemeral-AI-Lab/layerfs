//! Stored cell codec: bytes trimmed to the last valid one, optional validity.
use crate::{OverlayError, OverlayResult, CELL_BYTES, MASK_BYTES};

/// One cell row as stored. `validity` absent means every stored byte is valid.
pub(crate) struct Stored {
    pub epoch: i64,
    pub data: Vec<u8>,
    pub validity: Option<Vec<u8>>,
}
/// One cell expanded to its full logical window; mask bits are little-order.
pub(crate) struct Window {
    pub data: Box<[u8; CELL_BYTES]>,
    pub mask: Box<[u8; MASK_BYTES]>,
}
impl Stored {
    pub fn decode(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            epoch: row.get(first)?,
            data: row.get(first + 1)?,
            validity: row.get(first + 2)?,
        })
    }
    /// Whether stored byte `index` is a written byte.
    pub fn valid(&self, index: usize) -> bool {
        match &self.validity {
            None => index < self.data.len(),
            Some(mask) => mask
                .get(index / 8)
                .is_some_and(|b| b & (1 << (index % 8)) != 0),
        }
    }
    pub fn expand(&self) -> OverlayResult<Window> {
        if self.data.is_empty()
            || self.data.len() > CELL_BYTES
            || self
                .validity
                .as_ref()
                .is_some_and(|mask| mask.len() != self.data.len().div_ceil(8))
        {
            return Err(OverlayError::Invalid("stored cell"));
        }
        let mut window = Window::empty();
        window.data[..self.data.len()].copy_from_slice(&self.data);
        match &self.validity {
            Some(mask) => window.mask[..mask.len()].copy_from_slice(mask),
            None => window.set(0, self.data.len()),
        }
        Ok(window)
    }
}
impl Window {
    pub fn empty() -> Self {
        Self {
            data: Box::new([0; CELL_BYTES]),
            mask: Box::new([0; MASK_BYTES]),
        }
    }
    /// Marks `[from, to)` written.
    pub fn set(&mut self, from: usize, to: usize) {
        for bit in from..to {
            self.mask[bit / 8] |= 1 << (bit % 8);
        }
    }
    /// Drops every byte at or above `from`.
    pub fn cut(&mut self, from: usize) {
        for bit in from..CELL_BYTES {
            self.mask[bit / 8] &= !(1 << (bit % 8));
        }
    }
    /// Stored form, or None when no byte is valid. Unwritten bytes inside the
    /// stored prefix are zeroed so a row never carries stale data.
    pub fn trim(&self) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        let last = (0..CELL_BYTES)
            .rev()
            .find(|bit| self.mask[bit / 8] & (1 << (bit % 8)) != 0)?;
        let length = last + 1;
        let mut data = self.data[..length].to_vec();
        let mut dense = true;
        for (index, byte) in data.iter_mut().enumerate() {
            if self.mask[index / 8] & (1 << (index % 8)) == 0 {
                *byte = 0;
                dense = false;
            }
        }
        let validity = (!dense).then(|| {
            let mut mask = self.mask[..length.div_ceil(8)].to_vec();
            if length % 8 != 0 {
                let tail = mask.len() - 1;
                mask[tail] &= (1 << (length % 8)) - 1;
            }
            mask
        });
        Some((data, validity))
    }
}
