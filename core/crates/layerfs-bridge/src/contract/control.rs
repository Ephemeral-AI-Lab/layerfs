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
pub const WORKSPACE_CLOSE_CLEAN_OPCODE: u8 = 11;
pub const WORKSPACE_CLOSE_CLEAN_MAX_MS: u32 = 5_000;
pub const WORKSPACE_CLOSE_CLEAN_REQUEST_BYTES: usize = 124;
pub const WORKSPACE_CLOSE_CLEAN_RESULT_BYTES: usize = 100;
pub const WORKSPACE_MOUNT_OPCODE: u8 = 12;
pub const WORKSPACE_MOUNT_MAX_MS: u32 = 5_000;
pub const WORKSPACE_MOUNT_REQUEST_BYTES: usize = 124;
pub const WORKSPACE_MOUNT_RESULT_BYTES: usize = 100;
pub const WORKSPACE_ATTACH_OPCODE: u8 = 13;
pub const WORKSPACE_ATTACH_MAX_MS: u32 = 5_000;
pub const WORKSPACE_ATTACH_REQUEST_BYTES: usize = 124;
pub const WORKSPACE_ATTACH_RESULT_BYTES: usize = 100;
pub const WORKSPACE_ATTACHMENT_RESULT_BYTES: usize = 103;
pub const SANDBOX_HELLO_OPCODE: u8 = 17;
pub const WORKSPACE_OPEN_OPCODE: u8 = 18;
pub const WORKSPACE_OPEN_REQUEST_BYTES: usize = 244;
pub const WORKSPACE_OPEN_MAX_MS: u32 = 15_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxHelloWire {
    pub sandbox: [u8; 16],
    pub instance: Root,
}
impl SandboxHelloWire {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.sandbox == [0; 16] || self.instance == [0; 32] {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}

/// Attachment can retain any native or upstream failure classification.
/// This does not widen the existing Mount/Unmount/CloseClean outcome profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceAttachOutcome {
    Completed,
    Retained(Code),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceAttachWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub outcome: WorkspaceAttachOutcome,
}
impl WorkspaceAttachWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)
    }
}

/// A current attachment observation, not a terminal for a previous Attach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceAttachmentWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub state: WorkspaceAttachmentState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceAttachmentState {
    Attaching,
    Failed {
        cause: Code,
        cleanup: Option<Code>,
        progress: WorkspaceAttachmentProgress,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceAttachmentProgress {
    Running,
    Retained {
        mount_directory: bool,
        metadata_arena: bool,
        backing_directory: bool,
    },
}
impl WorkspaceAttachmentWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)
    }
}

/// Result of the requested daemon lifecycle operation. Completed means that
/// operation finished; Retained means an entered native attempt preserved its
/// owner rather than completing. Pre-admission refusals use ordinary Failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceLifecycleOutcome {
    Completed,
    Retained(Code),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceLifecycleWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub outcome: WorkspaceLifecycleOutcome,
}
impl WorkspaceLifecycleWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        match self.outcome {
            WorkspaceLifecycleOutcome::Completed
            | WorkspaceLifecycleOutcome::Retained(
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
