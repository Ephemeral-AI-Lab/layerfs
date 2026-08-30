use crate::fixture::{commit_id, BranchRecord, MockState};
use crate::{
    CliError, CliResult, CommandResult, ConflictId, ConflictView, LayerId, WorkspaceId,
    WorkspaceState, WorkspaceView,
};

pub(super) fn create_reconciliation_workspace(
    state: &mut MockState,
    branch: &BranchRecord,
    old_base: LayerId,
    current_layer: LayerId,
) -> CliResult<CommandResult> {
    if state
        .workspaces
        .iter()
        .any(|workspace| workspace.branch_id == branch.id)
    {
        return Err(CliError::WorkspaceBusy(branch.name.to_string()));
    }
    let project = state
        .projects
        .iter()
        .find(|project| project.id == branch.project_id)
        .expect("fixture project");
    let id = WorkspaceId::new(format!("W{}", state.next_workspace));
    state.next_workspace += 1;
    let conflicts = vec![
        ConflictView {
            id: ConflictId::from("conflict-1"),
            path: "src/model.rs".into(),
            kind: "Content".into(),
        },
        ConflictView {
            id: ConflictId::from("conflict-2"),
            path: "src/schema.rs".into(),
            kind: "Directory".into(),
        },
    ];
    state.workspaces.push(WorkspaceView {
        id: id.clone(),
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        branch_id: branch.id.clone(),
        branch_name: branch.name.clone(),
        branch_relation: branch.relation.clone(),
        anchor_commit: branch
            .work_head
            .map(|number| commit_id(branch.id.as_str(), number)),
        anchor_layer: Some(current_layer.clone()),
        state: WorkspaceState::Dirty,
        projection: "FUSE".into(),
        placement: "host".into(),
        mount: format!("/workspaces/reconcile-{id}"),
        changed_paths: conflicts.len() as u16,
        output_bytes: 0,
        execution: None,
        output: vec![format!(
            "reconcile Branch base {old_base} against current {current_layer}"
        )],
        conflicts,
    });
    Ok(CommandResult::NeedsResolution {
        workspace_id: id,
        old_base,
        current_layer,
        conflict_count: 2,
    })
}
