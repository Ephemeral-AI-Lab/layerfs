//! Checked owned reply encoding; allocation refusal never consumes a receipt.
use layerfs_bridge::contract::{FrameError, FrameResult};
pub(crate) struct Writer {
    pub bytes: Vec<u8>,
    limit: usize,
    counted: usize,
    counting: bool,
}
impl Writer {
    pub fn count(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            counted: 0,
            counting: true,
        }
    }
    pub fn sized(size: usize, limit: usize) -> FrameResult<Self> {
        if size > limit {
            return Err(FrameError::AdmissionUnavailable);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|error| FrameError::Allocation {
                requested_bytes: size,
                error,
            })?;
        Ok(Self {
            bytes,
            limit: size,
            counted: 0,
            counting: false,
        })
    }
    pub fn len(&self) -> usize {
        if self.counting {
            self.counted
        } else {
            self.bytes.len()
        }
    }
    pub fn raw(&mut self, bytes: &[u8]) -> FrameResult<()> {
        let size = self
            .len()
            .checked_add(bytes.len())
            .ok_or(FrameError::Invalid("reply size"))?;
        if size > self.limit {
            return Err(FrameError::AdmissionUnavailable);
        }
        if self.counting {
            self.counted = size;
            return Ok(());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub fn byte(&mut self, value: u8) -> FrameResult<()> {
        self.raw(&[value])
    }
    pub fn u64(&mut self, value: u64) -> FrameResult<()> {
        self.raw(&value.to_be_bytes())
    }
    pub fn u32(&mut self, value: usize) -> FrameResult<()> {
        self.raw(
            &u32::try_from(value)
                .map_err(|_| FrameError::Invalid("reply u32 width"))?
                .to_be_bytes(),
        )
    }
    pub fn blob(&mut self, bytes: &[u8]) -> FrameResult<()> {
        self.u32(bytes.len())?;
        self.raw(bytes)
    }
    pub fn field(&mut self, tag: u8, kind: u8, bytes: &[u8]) -> FrameResult<()> {
        self.raw(&[tag, kind])?;
        self.blob(bytes)
    }
    pub fn number(&mut self, tag: u8, value: u64) -> FrameResult<()> {
        self.field(tag, 1, &value.to_be_bytes())
    }
    pub fn signed(&mut self, tag: u8, value: i64) -> FrameResult<()> {
        self.field(tag, 2, &value.to_be_bytes())
    }
    pub fn text(&mut self, tag: u8, value: &str) -> FrameResult<()> {
        self.field(tag, 4, value.as_bytes())
    }
    pub fn node_start(&mut self, domain: u8, code: u8) -> FrameResult<usize> {
        self.raw(&[domain, code, 0, 0])?;
        let at = self.len();
        self.u32(0)?;
        Ok(at)
    }
    pub fn end_length(&mut self, at: usize) -> FrameResult<()> {
        let length = self
            .len()
            .checked_sub(at + 4)
            .ok_or(FrameError::Invalid("reply node offset"))?;
        if self.counting {
            return Ok(());
        }
        self.bytes[at..at + 4].copy_from_slice(
            &u32::try_from(length)
                .map_err(|_| FrameError::Invalid("reply node length"))?
                .to_be_bytes(),
        );
        Ok(())
    }
    pub fn child_start(&mut self, tag: u8) -> FrameResult<usize> {
        self.raw(&[tag, 5])?;
        let at = self.len();
        self.u32(0)?;
        Ok(at)
    }
}
