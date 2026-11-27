//! Narrow bounded external receipt encoder, not a product serializer.
use std::io;
pub const WINDOW: usize = 65_536;

pub struct Json {
    bytes: Vec<u8>,
}
impl Json {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    pub fn raw(&mut self, value: &str) -> io::Result<()> {
        if self
            .bytes
            .len()
            .checked_add(value.len())
            .is_none_or(|size| size > WINDOW)
        {
            return Err(io::Error::other("E2 receipt encoding window exceeded"));
        }
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }
    pub fn string(&mut self, value: &str) -> io::Result<()> {
        self.raw("\"")?;
        for character in value.chars() {
            match character {
                '"' => self.raw("\\\"")?,
                '\\' => self.raw("\\\\")?,
                '\n' => self.raw("\\n")?,
                '\r' => self.raw("\\r")?,
                '\t' => self.raw("\\t")?,
                value if value < ' ' => self.raw(&format!("\\u{:04x}", value as u32))?,
                value => self.raw(value.encode_utf8(&mut [0; 4]))?,
            }
        }
        self.raw("\"")
    }
    pub fn field(&mut self, name: &str, value: impl std::fmt::Display) -> io::Result<()> {
        self.string(name)?;
        self.raw(":")?;
        self.raw(&value.to_string())
    }
    pub fn text(&mut self, name: &str, value: &str) -> io::Result<()> {
        self.string(name)?;
        self.raw(":")?;
        self.string(value)
    }
    pub fn finish(mut self) -> io::Result<Vec<u8>> {
        self.raw("\n")?;
        Ok(self.bytes)
    }
}
