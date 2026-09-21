//! Typed product outcomes, separate from transport uncertainty.
use super::{HistoryResult, Root};
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
    /// Immediate metadata admission was refused; retrying later is allowed.
    Busy = 13,
    /// A named history record does not exist.
    NotFound = 14,
    /// The Branch moved away from the expected head or base.
    HeadMoved = 15,
    /// The exact stage token does not match the stage that is present.
    StageChanged = 16,
    /// Writable history authority was not established by this process.
    ContinuityUnavailable = 17,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: Code,
    pub unknown: bool,
    pub cleanup: Option<Code>,
    /// Typed context for history-only failures.
    pub history: Option<Box<super::HistoryFailure>>,
}
impl From<Code> for Failure {
    fn from(code: Code) -> Self {
        Self {
            code,
            unknown: code == Code::Unknown,
            cleanup: None,
            history: None,
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} unknown={} cleanup={:?} history={:?}",
            self.code, self.unknown, self.cleanup, self.history
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
    /// Checked inode facts at the requested immutable filesystem root.
    /// Directory size is explicitly zero; other sizes are logical byte lengths.
    Attributes {
        serial: u64,
        kind: u8,
        references: u64,
        content: Root,
        metadata: Root,
        mode: u32,
        mtime: i64,
        nanoseconds: u32,
        size: u64,
    },
    List {
        entries: Vec<(Vec<u8>, u64)>,
        continuation: Option<Vec<u8>>,
    },
    Link(Vec<u8>),
    /// One history reply. The closed wire union is boxed so a legacy reply does
    /// not pay for the widest history record it can never carry.
    History(Box<HistoryResult>),
    WorkspaceStatus(Box<super::WorkspaceStatusWire>),
    /// Inspect the outcome: a retained entered attempt is not a successful detach.
    WorkspaceUnmount(Box<super::WorkspaceLifecycleWire>),
    /// Inspect the outcome: a retained entered attempt is not a clean close.
    WorkspaceCloseClean(Box<super::WorkspaceLifecycleWire>),
    /// Inspect the outcome: a retained entered attempt is not a ready mount.
    WorkspaceMount(Box<super::WorkspaceLifecycleWire>),
    /// Inspect the outcome: a retained entered attempt is not an attached Workspace.
    WorkspaceAttach(Box<super::WorkspaceAttachWire>),
    /// Status of an attachment that has not yielded a usable Workspace.
    WorkspaceAttachment(Box<super::WorkspaceAttachmentWire>),
    MetadataSaved {
        base: Root,
        kind: u8,
        mode: u32,
        mtime_seconds: i64,
        mtime_nanoseconds: u32,
        metadata: Root,
        inserted: u64,
        reused: u64,
    },
}

impl Response {
    pub fn validate_metadata_saved(&self) -> Result<(), Failure> {
        let Self::MetadataSaved {
            kind,
            mode,
            mtime_nanoseconds,
            ..
        } = self
        else {
            return Err(Code::InvalidInput.into());
        };
        super::metadata::check_portable_metadata(*kind, *mode, *mtime_nanoseconds)
    }
    /// Checks the complete-attribute shape, including root versus descendant
    /// identity when a request path is available. Other result shapes are refused.
    pub fn validate_attributes(&self, is_root: Option<bool>) -> Result<(), Failure> {
        let Self::Attributes {
            serial,
            kind,
            references,
            mode,
            nanoseconds,
            size,
            ..
        } = self
        else {
            return Err(Code::InvalidInput.into());
        };
        super::metadata::check_portable_metadata(*kind, *mode, *nanoseconds)?;
        let valid_kind = match kind {
            1 => *references >= 1 && *size <= super::MAX_FILE,
            2 => *references <= 1 && *size == 0,
            3 => *references == 1 && *size <= 4096,
            _ => false,
        };
        let valid_position = match is_root {
            Some(true) => *kind == 2 && *references == 0,
            Some(false) => *references >= 1,
            None => true,
        };
        if *serial == 0 || *serial > i64::MAX as u64 || !valid_kind || !valid_position {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}
