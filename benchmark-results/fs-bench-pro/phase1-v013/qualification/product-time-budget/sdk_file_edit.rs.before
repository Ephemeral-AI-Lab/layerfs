use layerfs_sdk::{
    BranchId, Client, CommitId, EndWorkspaceMode, Query, QueryItem, QueryKind,
    WorkspaceCommitResult, WorkspaceFileRangeEdit,
};
use std::time::Instant;

pub(crate) struct EditCommitTiming {
    pub(crate) commit_id: CommitId,
    pub(crate) edit_call_ns: u64,
    pub(crate) commit_call_ns: u64,
    pub(crate) edit_commit_ns: u64,
    pub(crate) workspace_end_ns: u64,
    pub(crate) t0_clock_ns: u64,
    pub(crate) t3_clock_ns: u64,
}

pub(crate) struct FinishTiming {
    pub(crate) visibility_validation_ns: u64,
}

pub(crate) fn edit_commit_end(
    client: &Client,
    edit: WorkspaceFileRangeEdit,
) -> layerfs_sdk::Result<EditCommitTiming> {
    let workspace_id = edit.workspace_id;
    let t0 = super::sdk_edit_clock_ns()?;
    client.edit_workspace_file_range(edit)?;
    let t1 = super::sdk_edit_clock_ns()?;
    let t2 = t1;
    let commit_result = client.commit_workspace_session(workspace_id);
    let t3 = super::sdk_edit_clock_ns()?;
    let end_result = client.end_workspace_session(workspace_id, EndWorkspaceMode::Clean);
    let t4 = super::sdk_edit_clock_ns()?;
    let commit_id = match commit_result? {
        WorkspaceCommitResult::Created { commit_id, .. } => commit_id,
        _ => {
            return Err(layerfs_sdk::SdkError::InvalidRequest(
                "SDK edit Commit result",
            ))
        }
    };
    end_result?;
    let timing = EditCommitTiming {
        commit_id,
        edit_call_ns: t1 - t0,
        commit_call_ns: t3 - t2,
        edit_commit_ns: t3 - t0,
        workspace_end_ns: t4 - t3,
        t0_clock_ns: t0,
        t3_clock_ns: t3,
    };
    if timing.edit_commit_ns != timing.edit_call_ns.saturating_add(timing.commit_call_ns) {
        return Err(layerfs_sdk::SdkError::InvalidRequest(
            "SDK edit timing equation",
        ));
    }
    Ok(timing)
}

pub(crate) fn validate_visibility(
    client: &Client,
    branch_id: BranchId,
    timing: &EditCommitTiming,
) -> layerfs_sdk::Result<FinishTiming> {
    let started = Instant::now();
    let page = client.query(Query::new(QueryKind::Branches).limit(512))?;
    let visible = page.items.iter().any(|item| {
        matches!(item, QueryItem::Branch(branch)
            if branch.id == branch_id && branch.head_commit_id == Some(timing.commit_id))
    });
    if !visible || page.continuation.is_some() {
        return Err(layerfs_sdk::SdkError::InvalidRequest(
            "SDK edit Commit visibility",
        ));
    }
    Ok(FinishTiming {
        visibility_validation_ns: elapsed_ns(started),
    })
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}
