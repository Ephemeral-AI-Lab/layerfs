//! Bounded read-only Workspace view-lease requests and their results.
use super::{control::check_workspace_identity, Code, Failure, Root};

pub const WORKSPACE_PIN_VIEW_OPCODE: u8 = 21;
pub const WORKSPACE_VIEW_LOOKUP_OPCODE: u8 = 22;
pub const WORKSPACE_VIEW_LIST_OPCODE: u8 = 23;
pub const WORKSPACE_VIEW_READ_OPCODE: u8 = 24;
pub const WORKSPACE_VIEW_READLINK_OPCODE: u8 = 25;
pub const WORKSPACE_VIEW_STATUS_OPCODE: u8 = 26;
pub const WORKSPACE_RELEASE_VIEW_OPCODE: u8 = 27;
/// Lease tokens: one tag byte plus 32 unguessable bytes.
pub const VIEW_LEASE_TOKEN_BYTES: usize = 33;
/// Largest one name component may be.
pub const VIEW_NAME_BYTES: usize = 255;
/// Largest one bounded listing page may return.
pub const VIEW_LIST_ENTRIES: usize = 128;
/// Largest one bounded read may return.
pub const VIEW_READ_BYTES: usize = 128 * 1024;
/// Largest one symlink target may be.
pub const VIEW_SYMLINK_BYTES: usize = 4_096;
pub const WORKSPACE_VIEW_MAX_MS: u32 = 5_000;
/// Fixed bound for every view request frame: identity (63), incarnation (32),
/// token (33), one serial (8), one name (255), one option flag and the frame
/// header comfortably fit.
pub const WORKSPACE_VIEW_REQUEST_BYTES: usize = 512;
/// Fixed bound for one entry-carrying lease result: identity, incarnation,
/// token, root entry fields and generation/revision/base.
pub const WORKSPACE_VIEW_LEASE_RESULT_BYTES: usize = 288;
/// Fixed bound for one listing result frame header; entries are charged by
/// the encoder's own capacity accounting.
pub const WORKSPACE_VIEW_LIST_RESULT_BYTES: usize = 65_536;
/// Fixed bound for one read result: identity, incarnation, token, serial,
/// flags and up to `VIEW_READ_BYTES` of payload.
pub const WORKSPACE_VIEW_READ_RESULT_BYTES: usize = VIEW_READ_BYTES + 192;
/// Fixed bound for one readlink result.
pub const WORKSPACE_VIEW_READLINK_RESULT_BYTES: usize = VIEW_SYMLINK_BYTES + 192;
/// Fixed bound for one status result.
pub const WORKSPACE_VIEW_STATUS_RESULT_BYTES: usize = 192;
/// Fixed bound for one release result.
pub const WORKSPACE_VIEW_RELEASE_RESULT_BYTES: usize = 128;
pub const WORKSPACE_RELEASE_VIEW_MAX_MS: u32 = 15_000;

/// One entry a view lease resolved: identity and attributes only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspaceViewEntryWire {
    pub serial: u64,
    /// 1 file, 2 directory, 3 symlink.
    pub kind: u8,
    pub size: u64,
    pub references: u64,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}
impl WorkspaceViewEntryWire {
    pub fn validate(&self) -> Result<(), Failure> {
        if !matches!(self.kind, 1..=3) {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}

/// The pinned lease a `WorkspacePinView` returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewLeaseWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub view: Vec<u8>,
    pub root: WorkspaceViewEntryWire,
    pub generation: u64,
    pub revision: u64,
    pub base: Root,
}
impl WorkspaceViewLeaseWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        if self.view.len() != VIEW_LEASE_TOKEN_BYTES || self.view[0] != 1 {
            return Err(Code::InvalidInput.into());
        }
        self.root.validate()?;
        if self.base == [0; 32] {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}

/// One bounded listing page from one selected view revision. Entries are
/// `(name, serial)`; a full entry comes from one component lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewListWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub view: Vec<u8>,
    pub entries: Vec<(Vec<u8>, u64)>,
    pub continuation: Option<Vec<u8>>,
}
impl WorkspaceViewListWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        if self.view.len() != VIEW_LEASE_TOKEN_BYTES || self.view[0] != 1 {
            return Err(Code::InvalidInput.into());
        }
        if self.entries.len() > VIEW_LIST_ENTRIES {
            return Err(Code::InvalidInput.into());
        }
        let mut prior: &[u8] = &[];
        for (name, serial) in &self.entries {
            if name.is_empty()
                || name.len() > VIEW_NAME_BYTES
                || name.contains(&0)
                || name.as_slice() <= prior
            {
                return Err(Code::InvalidInput.into());
            }
            prior = name;
            if *serial == 0 {
                return Err(Code::InvalidInput.into());
            }
        }
        if let Some(name) = &self.continuation {
            if name.len() > VIEW_NAME_BYTES
                || name.contains(&0)
                || self.entries.last().is_none_or(|last| &last.0 != name)
            {
                return Err(Code::InvalidInput.into());
            }
        }
        Ok(())
    }
}

/// Bounded pinned bytes with end-of-file and the pinned logical size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewReadWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub view: Vec<u8>,
    pub serial: u64,
    pub bytes: Vec<u8>,
    pub eof: bool,
    pub size: u64,
}
impl WorkspaceViewReadWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        if self.view.len() != VIEW_LEASE_TOKEN_BYTES || self.view[0] != 1 {
            return Err(Code::InvalidInput.into());
        }
        if self.bytes.len() > VIEW_READ_BYTES || self.bytes.len() as u64 > self.size {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}

/// Exact pinned symlink target bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewReadlinkWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub view: Vec<u8>,
    pub target: Vec<u8>,
}
impl WorkspaceViewReadlinkWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        if self.view.len() != VIEW_LEASE_TOKEN_BYTES || self.view[0] != 1 {
            return Err(Code::InvalidInput.into());
        }
        if self.target.len() > VIEW_SYMLINK_BYTES || self.target.contains(&0) {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}

/// Read-only custody observation of one held lease; never a pin or release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewStatusWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub view: Vec<u8>,
    pub generation: u64,
    pub revision: u64,
    pub entries: u64,
    pub held_leases: u64,
}
impl WorkspaceViewStatusWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        if self.view.len() != VIEW_LEASE_TOKEN_BYTES || self.view[0] != 1 {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}

/// The checked lease release terminal. Completed means the retirement
/// selector finished and the charge refunded; Retained means physical custody
/// stayed (the Workspace stopped) and the lease was not silently retried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceViewReleaseOutcome {
    Completed,
    Retained(Code),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewReleaseWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub view: Vec<u8>,
    pub outcome: WorkspaceViewReleaseOutcome,
}
impl WorkspaceViewReleaseWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        if self.view.len() != VIEW_LEASE_TOKEN_BYTES || self.view[0] != 1 {
            return Err(Code::InvalidInput.into());
        }
        match self.outcome {
            WorkspaceViewReleaseOutcome::Completed
            | WorkspaceViewReleaseOutcome::Retained(
                Code::Deadline | Code::Io | Code::Busy | Code::Unsupported,
            ) => Ok(()),
            _ => Err(Code::InvalidInput.into()),
        }
    }
}

/// Checks one view token a request carried.
pub(crate) fn check_view_token(view: &[u8]) -> Result<(), Failure> {
    if view.len() != VIEW_LEASE_TOKEN_BYTES || view[0] != 1 {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}

/// Checks one name component a request carried.
pub(crate) fn check_view_name(name: &[u8]) -> Result<(), Failure> {
    if name.is_empty() || name.len() > VIEW_NAME_BYTES || name.contains(&0) || name.contains(&b'/')
    {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}
