//! Test-only public SDK Exec→Commit gate shared by #241 functional routes.
use layerfs_api_core::{ExecResult, WorkspaceError, WorkspaceId};
use layerfs_bridge::contract::WorkspaceCommitReportWire;
use layerfs_sdk::WorkspaceApi;

pub fn confirmed_exit(status: Option<i32>) -> bool {
    status == Some(0)
}

pub fn exec_and_commit_on_success(
    api: &WorkspaceApi<'_>,
    id: &WorkspaceId,
    command: &str,
) -> Result<(ExecResult, Option<WorkspaceCommitReportWire>), WorkspaceError> {
    let exec = api.exec(id, command)?;
    let commit = if confirmed_exit(exec.exit_status) {
        Some(api.commit(id)?)
    } else {
        None
    };
    Ok((exec, commit))
}
