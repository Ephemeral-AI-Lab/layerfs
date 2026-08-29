use layerfs_content::CanonicalPath;
use std::fmt;
use std::io::{Read, Write};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Copy, Debug)]
pub struct Attr {
    pub node: NodeId,
    pub kind: Kind,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: Vec<u8>,
    pub attr: Attr,
}

#[derive(Debug)]
pub enum MaterializationError {
    Io(std::io::Error),
    Invalid(&'static str),
    Port(&'static str),
}

pub type Result<T> = std::result::Result<T, MaterializationError>;

impl fmt::Display for MaterializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MaterializationError {}

impl From<std::io::Error> for MaterializationError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<layerfs_content::CoreError> for MaterializationError {
    fn from(_: layerfs_content::CoreError) -> Self {
        Self::Invalid("canonical path")
    }
}

pub trait MaterializationSource {
    fn root(&self) -> Attr;
    fn entries(&self, node: NodeId) -> Result<Vec<Entry>>;
    fn read(&self, node: NodeId, sink: &mut dyn Write) -> Result<()>;
    fn readlink(&self, node: NodeId) -> Result<Vec<u8>>;
}

pub trait CaptureSink {
    fn reset(&mut self, mode: u32, mtime_seconds: i64, mtime_nanoseconds: u32) -> Result<()>;
    fn directory(
        &mut self,
        path: &CanonicalPath,
        mode: u32,
        mtime_seconds: i64,
        mtime_nanoseconds: u32,
    ) -> Result<()>;
    fn file(
        &mut self,
        path: &CanonicalPath,
        source: &mut dyn Read,
        mode: u32,
        mtime_seconds: i64,
        mtime_nanoseconds: u32,
    ) -> Result<()>;
    fn symlink(
        &mut self,
        path: &CanonicalPath,
        target: Vec<u8>,
        mtime_seconds: i64,
        mtime_nanoseconds: u32,
    ) -> Result<()>;
    fn hard_link(&mut self, source: &CanonicalPath, target: &CanonicalPath) -> Result<()>;
}
