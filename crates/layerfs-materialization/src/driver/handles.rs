//! Object-safe native handle contracts.

use std::any::Any;
use std::io::{Read, Seek, Write};

use super::Result;

pub trait DirectoryHandle: Send {
    fn as_any(&self) -> &dyn Any;
}
pub trait RegularFileHandle: Read + Write + Seek + Send {
    fn as_any(&self) -> &dyn Any;
}
pub trait OwnedTempHandle: Read + Write + Seek + Send {
    fn as_any(&self) -> &dyn Any;
    fn set_len(&mut self, len: u64) -> Result<()>;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}
pub trait NamePreflight: Send {
    fn add(&mut self, name: &[u8]) -> Result<()>;
    fn finish(self: Box<Self>) -> Result<()>;
}
