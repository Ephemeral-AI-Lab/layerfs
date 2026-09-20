//! Closed operation profile. Root bytes are logical identities, never paths.
use super::{Code, Failure};
pub const FRAME_BYTES: usize = 16384;
pub const METADATA_BYTES: usize = 32768;
pub const MAX_FILE: u64 = 64 * 1024 * 1024;
pub const MAX_REPLAY: u64 = 8 * 1024 * 1024;
/// Persistent/handshaking/closing sessions; the acceptor owns one extra refusal slot.
pub const MAX_SESSIONS: usize = 4;
/// Total application connection slots, including the synchronous accept/refusal owner.
pub const MAX_CONNECTIONS: usize = MAX_SESSIONS + 1;
pub const MAX_FRAMES: usize = 8192;
pub type Root = [u8; 32];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: u64,
    pub generation: u64,
    pub store: u32,
    pub profile: u16,
    pub deadline_ms: u32,
    pub response_bytes: u64,
    pub operation: Operation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    ReadFile {
        root: Root,
        start: u64,
        end: u64,
    },
    Inspect {
        root: Root,
        query: Inspect,
    },
    ConstructFile {
        length: u64,
    },
    EditFile {
        root: Root,
        base_length: u64,
        edits: Vec<Edit>,
    },
    UpdatePreparedFilesystem {
        base: Root,
        scope: Root,
        root_serial: u64,
        directories: Vec<DirectoryChange>,
        inodes: Vec<InodeChange>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inspect {
    File,
    Stat {
        path: Vec<u8>,
    },
    List {
        path: Vec<u8>,
        after: Vec<u8>,
        entries: u16,
        bytes: u32,
    },
    Readlink {
        path: Vec<u8>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: u64,
    pub end: u64,
    pub replacement: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryChange {
    pub parent: u64,
    pub changes: Vec<(Vec<u8>, Option<u64>)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InodeChange {
    pub serial: u64,
    pub kind: u8,
    pub content: Root,
    pub metadata: Root,
}
impl Operation {
    pub const fn opcode(&self) -> u8 {
        match self {
            Self::ReadFile { .. } => 1,
            Self::Inspect { .. } => 2,
            Self::ConstructFile { .. } => 3,
            Self::EditFile { .. } => 4,
            Self::UpdatePreparedFilesystem { .. } => 5,
        }
    }
    pub const fn label(&self) -> &'static str {
        match self {
            Self::ReadFile { .. } => "ReadFile",
            Self::Inspect { .. } => "Inspect",
            Self::ConstructFile { .. } => "ConstructFile",
            Self::EditFile { .. } => "EditFile",
            Self::UpdatePreparedFilesystem { .. } => "UpdatePreparedFilesystem",
        }
    }
    pub const fn mutation(&self) -> bool {
        self.opcode() >= 3
    }
    pub fn input_length(&self) -> Result<u64, Failure> {
        match self {
            Self::ConstructFile { length } => Ok(*length),
            Self::EditFile { edits, .. } => edits.iter().try_fold(0u64, |sum, e| {
                sum.checked_add(e.replacement).ok_or(Code::Capacity.into())
            }),
            _ => Ok(0),
        }
    }
}
impl Request {
    pub fn validate(&self) -> Result<(), Failure> {
        let invalid = || Failure::from(Code::InvalidInput);
        if self.profile != 1 {
            return Err(Code::Unsupported.into());
        }
        if self.id == 0 || self.deadline_ms == 0 || self.deadline_ms > 10000 {
            return Err(invalid());
        }
        if self.response_bytes > MAX_FILE || self.operation.input_length()? > MAX_FILE {
            return Err(Code::Capacity.into());
        }
        match &self.operation {
            Operation::ReadFile { start, end, .. } => {
                if start > end || end - start > self.response_bytes {
                    return Err(invalid());
                }
            }
            Operation::EditFile {
                base_length, edits, ..
            } => {
                if edits.len() > 256
                    || *base_length > MAX_FILE
                    || self.operation.input_length()? > MAX_REPLAY
                {
                    return Err(Code::Capacity.into());
                }
                let mut length = *base_length;
                let mut previous = 0;
                for edit in edits {
                    if edit.start < previous || edit.start > edit.end || edit.end > length {
                        return Err(invalid());
                    }
                    length = length
                        .checked_sub(edit.end - edit.start)
                        .and_then(|n| n.checked_add(edit.replacement))
                        .ok_or_else(invalid)?;
                    previous = edit
                        .start
                        .checked_add(edit.replacement)
                        .ok_or_else(invalid)?;
                    if length > MAX_FILE {
                        return Err(Code::Capacity.into());
                    }
                }
            }
            Operation::Inspect { query, .. } => match query {
                Inspect::File => {}
                Inspect::Stat { path } | Inspect::Readlink { path } => check_path(path)?,
                Inspect::List {
                    path,
                    after,
                    entries,
                    bytes,
                } => {
                    check_path(path)?;
                    if after.len() > 255
                        || *entries == 0
                        || *entries > 128
                        || *bytes == 0
                        || *bytes > 16384
                    {
                        return Err(Code::Capacity.into());
                    }
                }
            },
            Operation::UpdatePreparedFilesystem {
                root_serial,
                directories,
                inodes,
                ..
            } => {
                if *root_serial == 0 || *root_serial > i64::MAX as u64 {
                    return Err(invalid());
                }
                if directories.len() > 128 || inodes.len() > 128 {
                    return Err(Code::Capacity.into());
                }
                let mut count = 0usize;
                for directory in directories {
                    count = count
                        .checked_add(directory.changes.len())
                        .ok_or(Code::Capacity)?;
                    if count > 128 {
                        return Err(Code::Capacity.into());
                    }
                    for (name, _) in &directory.changes {
                        if name.is_empty() || name.len() > 255 {
                            return Err(invalid());
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
fn check_path(path: &[u8]) -> Result<(), Failure> {
    if path.len() > 4096 {
        Err(Code::Capacity.into())
    } else {
        Ok(())
    }
}
