//! Bounded daemon Exec request and result contract.
use super::{control::check_workspace_identity, Code, Failure, Root};

pub const WORKSPACE_EXEC_OPCODE: u8 = 19;
pub const WORKSPACE_EXEC_REQUEST_BYTES: usize = 4250;
pub const WORKSPACE_EXEC_MAX_MS: u32 = 30_000;
pub const WORKSPACE_EXEC_OUTPUT_BYTES: usize = 8192;
pub const WORKSPACE_EXEC_RESULT_BYTES: usize = 16500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceExecWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub exit_status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}
impl WorkspaceExecWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        if self.stdout.len() > WORKSPACE_EXEC_OUTPUT_BYTES
            || self.stderr.len() > WORKSPACE_EXEC_OUTPUT_BYTES
        {
            return Err(Code::Capacity.into());
        }
        Ok(())
    }
}
