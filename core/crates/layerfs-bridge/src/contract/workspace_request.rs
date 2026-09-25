//! Admission checks for the daemon-only control request family.
use super::{control::check_workspace_identity, history::tag, *};

pub(crate) fn validate(request: &Request) -> Result<(), Failure> {
    let maximum = match &request.operation {
        Operation::WorkspaceUnmount { .. } => WORKSPACE_UNMOUNT_MAX_MS,
        Operation::WorkspaceCloseClean { .. } => WORKSPACE_CLOSE_CLEAN_MAX_MS,
        Operation::WorkspaceMount { .. } => WORKSPACE_MOUNT_MAX_MS,
        Operation::WorkspaceAttach { .. } => WORKSPACE_ATTACH_MAX_MS,
        Operation::WorkspaceCommit { .. } => WORKSPACE_COMMIT_MAX_MS,
        Operation::WorkspaceOpen { .. } => WORKSPACE_OPEN_MAX_MS,
        Operation::WorkspaceExec { .. } => WORKSPACE_EXEC_MAX_MS,
        _ => WORKSPACE_STATUS_MAX_MS,
    };
    let response_bytes = if matches!(&request.operation, Operation::WorkspaceExec { .. }) {
        u64::from(request.deadline_ms.div_ceil(1_000))
    } else {
        0
    };
    if request.store != 0
        || request.generation != 0
        || request.response_bytes != response_bytes
        || request.deadline_ms > maximum
    {
        return Err(Code::InvalidInput.into());
    }
    let (workspace, incarnation) = match &request.operation {
        Operation::SandboxHello => return Ok(()),
        Operation::WorkspaceStatus {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceUnmount {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceCloseClean {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceMount {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceAttach {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceCommit {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceOpen {
            workspace,
            incarnation,
            ..
        }
        | Operation::WorkspaceExec {
            workspace,
            incarnation,
            ..
        } => (workspace, incarnation),
        _ => return Err(Code::InvalidInput.into()),
    };
    check_workspace_identity(workspace, incarnation)?;
    match &request.operation {
        Operation::WorkspaceOpen {
            instance,
            project,
            branch,
            commit,
            ..
        } => {
            if *instance == [0; 32] {
                return Err(Code::InvalidInput.into());
            }
            tag(project, 0x31)?;
            tag(branch, 0x11)?;
            if let Some(commit) = commit {
                tag(commit, 0x12)?;
            }
        }
        Operation::WorkspaceExec { command, .. } => {
            if command.is_empty()
                || command.len() > 4096
                || command.contains(&0)
                || std::str::from_utf8(command).is_err()
            {
                return Err(Code::InvalidInput.into());
            }
        }
        _ => {}
    }
    Ok(())
}
