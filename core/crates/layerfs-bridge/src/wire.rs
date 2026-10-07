//! Checked fields for a single bounded control record.
use crate::control::ControlError;
pub(crate) const LIMIT: usize = 8192;
pub(crate) struct Writer(pub Vec<u8>);
impl Writer {
    pub fn new(magic: &[u8]) -> Self {
        Self(magic.to_vec())
    }
    pub fn put(&mut self, bytes: &[u8]) -> Result<(), ControlError> {
        if self.0.len().saturating_add(bytes.len()) > LIMIT {
            return Err(ControlError("control record limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    pub fn byte(&mut self, value: u8) -> Result<(), ControlError> {
        self.put(&[value])
    }
    pub fn blob(&mut self, value: &[u8]) -> Result<(), ControlError> {
        let size = u16::try_from(value.len()).map_err(|_| ControlError("control field limit"))?;
        self.put(&size.to_be_bytes())?;
        self.put(value)
    }
}
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8], magic: &[u8]) -> Result<Self, ControlError> {
        if bytes.len() > LIMIT || !bytes.starts_with(magic) {
            return Err(ControlError("control version or length"));
        }
        Ok(Self {
            bytes,
            position: magic.len(),
        })
    }
    pub fn take(&mut self, size: usize) -> Result<&'a [u8], ControlError> {
        let end = self
            .position
            .checked_add(size)
            .ok_or(ControlError("control field length"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(ControlError("truncated control field"))?;
        self.position = end;
        Ok(value)
    }
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], ControlError> {
        self.take(N)?
            .try_into()
            .map_err(|_| ControlError("control field width"))
    }
    pub fn byte(&mut self) -> Result<u8, ControlError> {
        Ok(self.array::<1>()?[0])
    }
    pub fn blob(&mut self, maximum: usize) -> Result<&'a [u8], ControlError> {
        let size = u16::from_be_bytes(self.array()?) as usize;
        if size > maximum {
            return Err(ControlError("control field limit"));
        }
        self.take(size)
    }
    pub fn text(&mut self, maximum: usize) -> Result<String, ControlError> {
        std::str::from_utf8(self.blob(maximum)?)
            .map(str::to_owned)
            .map_err(|_| ControlError("control UTF-8"))
    }
    pub fn finish(self) -> Result<(), ControlError> {
        if self.position != self.bytes.len() {
            return Err(ControlError("trailing control bytes"));
        }
        Ok(())
    }
}
