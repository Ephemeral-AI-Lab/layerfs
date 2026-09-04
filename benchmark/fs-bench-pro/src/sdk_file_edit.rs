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
    edit_commit_end_inner(client, edit, None)
}

pub(crate) fn edit_commit_end_budgeted(
    client: &Client,
    edit: WorkspaceFileRangeEdit,
    budget: &super::workspace_bench::ProductBudget,
) -> layerfs_sdk::Result<EditCommitTiming> {
    edit_commit_end_inner(client, edit, Some(budget))
}

fn edit_commit_end_inner(
    client: &Client,
    edit: WorkspaceFileRangeEdit,
    budget: Option<&super::workspace_bench::ProductBudget>,
) -> layerfs_sdk::Result<EditCommitTiming> {
    let workspace_id = edit.workspace_id;
    let t0 = match budget {
        Some(b) => b.start_raw_clock("sdk-edit")?,
        None => super::sdk_edit_clock_ns()?,
    };
    let edit_result = client.edit_workspace_file_range(edit);
    let t1 = match budget {
        Some(b) if edit_result.is_ok() => b.transition_raw_clock(t0, "commit")?,
        Some(b) => b.finish_raw_clock(t0)?,
        None => super::sdk_edit_clock_ns()?,
    };
    edit_result?;
    let t2 = t1;
    let commit_result = client.commit_workspace_session(workspace_id);
    let t3 = match budget {
        Some(b) => b.transition_raw_clock(t2, "end")?,
        None => super::sdk_edit_clock_ns()?,
    };
    let end_result = client.end_workspace_session(workspace_id, EndWorkspaceMode::Clean);
    let t4 = match budget {
        Some(b) => b.finish_raw_clock(t3)?,
        None => super::sdk_edit_clock_ns()?,
    };
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
    validate_visibility_inner(client, branch_id, timing, None)
}

pub(crate) fn validate_visibility_budgeted(
    client: &Client,
    branch_id: BranchId,
    timing: &EditCommitTiming,
    budget: &super::workspace_bench::ProductBudget,
) -> layerfs_sdk::Result<FinishTiming> {
    validate_visibility_inner(client, branch_id, timing, Some(budget))
}

fn validate_visibility_inner(
    client: &Client,
    branch_id: BranchId,
    timing: &EditCommitTiming,
    budget: Option<&super::workspace_bench::ProductBudget>,
) -> layerfs_sdk::Result<FinishTiming> {
    let started = match budget {
        Some(b) => b
            .start_clock("visibility")
            .map_err(|_| layerfs_sdk::SdkError::InvalidRequest("product budget clock"))?,
        None => Instant::now(),
    };
    let result = client.query(Query::new(QueryKind::Branches).limit(512));
    let page = match result {
        Ok(page) => page,
        Err(error) => {
            if let Some(b) = budget {
                let _ = b.finish_clock(started);
            }
            return Err(error);
        }
    };
    let visible = page.items.iter().any(|item| {
        matches!(item, QueryItem::Branch(branch)
            if branch.id == branch_id && branch.head_commit_id == Some(timing.commit_id))
    });
    if !visible || page.continuation.is_some() {
        if let Some(b) = budget {
            let _ = b.finish_clock(started);
        }
        return Err(layerfs_sdk::SdkError::InvalidRequest(
            "SDK edit Commit visibility",
        ));
    }
    Ok(FinishTiming {
        visibility_validation_ns: match budget {
            Some(b) => b
                .finish_clock(started)
                .map_err(|_| layerfs_sdk::SdkError::InvalidRequest("product budget clock"))?,
            None => elapsed_ns(started),
        },
    })
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}
