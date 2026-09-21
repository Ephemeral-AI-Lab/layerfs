//! Daemon-targeted control; service grants confer no control authority.
use super::{Code, Failure, Root};

pub const WORKSPACE_STATUS_PROFILE: u16 = 3;
pub const WORKSPACE_STATUS_OPCODE: u8 = 8;
pub const WORKSPACE_STATUS_MAX_MS: u32 = 5_000;
pub const WORKSPACE_ID_BYTES: usize = 63;
pub const WORKSPACE_STATUS_REQUEST_BYTES: usize = 124;
pub const WORKSPACE_STATUS_RESULT_BYTES: usize = 139;
pub const WORKSPACE_UNMOUNT_OPCODE: u8 = 10;
pub const WORKSPACE_UNMOUNT_MAX_MS: u32 = 5_000;
pub const WORKSPACE_UNMOUNT_REQUEST_BYTES: usize = 124;
pub const WORKSPACE_UNMOUNT_RESULT_BYTES: usize = 100;

/// An entered native attempt. Retained preserves its owner and is not unmount
/// success; refusal before native admission uses the ordinary Failure terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceUnmountOutcome {
    Unmounted,
    Retained(Code),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceUnmountWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub outcome: WorkspaceUnmountOutcome,
}
impl WorkspaceUnmountWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        match self.outcome {
            WorkspaceUnmountOutcome::Unmounted
            | WorkspaceUnmountOutcome::Retained(
                Code::Deadline | Code::Io | Code::Busy | Code::Unsupported,
            ) => Ok(()),
            _ => Err(Code::InvalidInput.into()),
        }
    }
}

/// A current local observation, never a receipt for an earlier operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceStatusWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub mounted: bool,
    pub stopping: bool,
    pub closed: bool,
    pub active_operations: u64,
    pub nodes: u64,
    pub handles: u64,
    pub cookies: u64,
    /// Aggregate accounted Workspace allocations across the owning consumer.
    pub consumer_accounted_bytes: u64,
}

impl WorkspaceStatusWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        if self.closed
            && (self.mounted
                || self.active_operations != 0
                || self.nodes != 0
                || self.handles != 0
                || self.cookies != 0)
        {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}

pub(crate) fn check_workspace_identity(
    workspace: &[u8],
    incarnation: &Root,
) -> Result<(), Failure> {
    if workspace.is_empty()
        || workspace.len() > WORKSPACE_ID_BYTES
        || !workspace
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || *incarnation == [0; 32]
    {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}
