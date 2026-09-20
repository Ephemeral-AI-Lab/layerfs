//! Typed product outcomes, separate from transport uncertainty.
use super::Root;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Code {
    InvalidInput = 1,
    Unsupported = 2,
    Denied = 3,
    Capacity = 4,
    Ownership = 5,
    MissingObject = 6,
    PathNotFound = 7,
    Provider = 8,
    Integrity = 9,
    Io = 10,
    Deadline = 11,
    Unknown = 12,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: Code,
    pub unknown: bool,
    pub cleanup: Option<Code>,
}
impl From<Code> for Failure {
    fn from(code: Code) -> Self {
        Self {
            code,
            unknown: code == Code::Unknown,
            cleanup: None,
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} unknown={} cleanup={:?}",
            self.code, self.unknown, self.cleanup
        )
    }
}
impl std::error::Error for Failure {}
impl From<std::io::Error> for Failure {
    fn from(_: std::io::Error) -> Self {
        Code::Io.into()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    Read {
        length: u64,
    },
    Saved {
        root: Root,
        length: u64,
        inserted: u64,
        reused: u64,
    },
    FilesystemSaved {
        root: Root,
        inserted: u64,
        reused: u64,
    },
    File {
        length: u64,
        representation: u8,
    },
    Stat {
        serial: u64,
        kind: u8,
        references: u64,
        content: Root,
        metadata: Root,
        mode: u32,
        mtime: i64,
        nanoseconds: u32,
    },
    List {
        entries: Vec<(Vec<u8>, u64)>,
        continuation: Option<Vec<u8>>,
    },
    Link(Vec<u8>),
}
