//! Checked borrowed binary reads; no receive-body or error-tree mirror.
use super::RemoteFailure;
use layerfs_bridge::contract::{FrameError, FrameResult};
use layerfs_content::ObjectId;
pub(crate) struct Reader<'a> {
    pub bytes: &'a [u8],
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
    pub fn take(&mut self, count: usize) -> FrameResult<&'a [u8]> {
        let (head, tail) = self
            .bytes
            .split_at_checked(count)
            .ok_or(FrameError::Invalid("reply truncated"))?;
        self.bytes = tail;
        Ok(head)
    }
    pub fn byte(&mut self) -> FrameResult<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn flag(&mut self) -> FrameResult<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(FrameError::Invalid("reply flag")),
        }
    }
    pub fn u64(&mut self) -> FrameResult<u64> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().expect("fixed u64"),
        ))
    }
    pub fn u32(&mut self) -> FrameResult<usize> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("fixed u32")) as usize)
    }
    pub fn object(&mut self) -> FrameResult<ObjectId> {
        ObjectId::from_bytes(self.take(32)?).map_err(|_| FrameError::Invalid("reply object"))
    }
    pub fn blob(&mut self) -> FrameResult<&'a [u8]> {
        let count = self.u32()?;
        self.take(count)
    }
    pub fn failure(&mut self) -> FrameResult<RemoteFailure<'a>> {
        if self.bytes.len() < 8 {
            return Err(FrameError::Invalid("failure truncated"));
        }
        let count = u32::from_be_bytes(self.bytes[4..8].try_into().expect("node length")) as usize;
        RemoteFailure::decode(
            self.take(
                count
                    .checked_add(8)
                    .ok_or(FrameError::Invalid("failure length"))?,
            )?,
        )
    }
    pub fn end(&self) -> FrameResult<()> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(FrameError::Invalid("reply trailing bytes"))
        }
    }
}
