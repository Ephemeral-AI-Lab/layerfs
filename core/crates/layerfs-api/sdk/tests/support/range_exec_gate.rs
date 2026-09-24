//! Test-only public SDK Exec→Commit gate shared by #241 functional routes.
use layerfs_api_core::{ExecResult, WorkspaceError, WorkspaceId};
use layerfs_bridge::contract::WorkspaceCommitReportWire;
use layerfs_sdk::WorkspaceApi;

pub fn confirmed_tool_result(exec: &ExecResult, expected_final_bytes: u64) -> bool {
    if exec.exit_status != Some(0) || exec.stdout_truncated || exec.stderr_truncated {
        return false;
    }
    let expected = format!(
        "{{\"status\":\"PASS\",\"operation\":\"splice\",\"direction\":\"-\",\"final_bytes\":{expected_final_bytes},\"shifted_bytes\":0}}\n"
    );
    exec.stdout == expected.as_bytes()
}

pub fn exec_and_commit_on_success(
    api: &WorkspaceApi<'_>,
    id: &WorkspaceId,
    command: &str,
    expected_final_bytes: u64,
) -> Result<(ExecResult, Option<WorkspaceCommitReportWire>), WorkspaceError> {
    let exec = api.exec(id, command)?;
    let commit = if confirmed_tool_result(&exec, expected_final_bytes) {
        Some(api.commit(id)?)
    } else {
        None
    };
    Ok((exec, commit))
}
