use crate::{CliResult, Command};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandEffect {
    Read,
    Write,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPlan {
    pub effect: CommandEffect,
    pub summary: String,
}

pub(crate) fn plan(
    command: &Command,
    client: Option<&layerfs_sdk::Client>,
) -> CliResult<CommandPlan> {
    let effect = match command {
        Command::Context {
            command: crate::ContextCommand::Show,
        }
        | Command::Monitor { .. }
        | Command::Query { .. }
        | Command::Layerstack {
            command: crate::LayerStackCommand::Diff { .. },
        }
        | Command::Branch {
            command: crate::BranchCommand::Diff(_),
        } => CommandEffect::Read,
        _ => CommandEffect::Write,
    };
    let mut summary = format!("{command:?}");
    if let Some(client) = client {
        if let Some(label) = command_label(client, command)? {
            summary.push_str(" [");
            summary.push_str(&label);
            summary.push(']');
        }
    }
    Ok(CommandPlan { effect, summary })
}

fn command_label(client: &layerfs_sdk::Client, command: &Command) -> CliResult<Option<String>> {
    use crate::{BranchCommand, LayerStackCommand, WorkspaceCommand};
    let branch = match command {
        Command::Layerstack {
            command: LayerStackCommand::Add { branch_id },
        }
        | Command::Branch {
            command: BranchCommand::Push { branch_id },
        }
        | Command::Branch {
            command:
                BranchCommand::Diff(crate::BranchDiff {
                    branch: branch_id, ..
                }),
        }
        | Command::Workspace {
            command: WorkspaceCommand::Create { branch_id, .. },
        } => Some((branch_id, false)),
        Command::Branch {
            command: BranchCommand::Pull(request),
        } => Some((&request.branch_id, true)),
        Command::Branch {
            command: BranchCommand::Fork(request),
        } => request.branch.as_ref().map(|branch| (branch, false)),
        _ => None,
    };
    if let Some((branch, authority)) = branch {
        let id = branch.parse().map_err(|error: layerfs_sdk::StorageError| {
            crate::CliError::Parse(error.to_string())
        })?;
        return branch_label(client, id, authority);
    }
    let workspace = match command {
        Command::Workspace {
            command:
                WorkspaceCommand::Exec { workspace_id, .. }
                | WorkspaceCommand::Shell { workspace_id }
                | WorkspaceCommand::Conflicts { workspace_id, .. }
                | WorkspaceCommand::Commit { workspace_id }
                | WorkspaceCommand::End { workspace_id, .. },
        } => Some(workspace_id),
        Command::Workspace {
            command: WorkspaceCommand::Resolve(request),
        } => Some(&request.workspace_id),
        _ => None,
    };
    let Some(workspace) = workspace else {
        return Ok(None);
    };
    let workspace_id = workspace.parse().map_err(crate::CliError::Workspace)?;
    let mut query = layerfs_sdk::Query::new(layerfs_sdk::QueryKind::Workspaces);
    loop {
        let page = client.query(query.clone())?;
        for item in page.items {
            if let layerfs_sdk::QueryItem::Workspace(value) = item {
                if value.summary.id == workspace_id {
                    return Ok(Some(format!(
                        "{}/{} ({})",
                        value.layer_stack_name, value.branch_name, value.summary.branch_id
                    )));
                }
            }
        }
        let Some(continuation) = page.continuation else {
            return Ok(None);
        };
        query = query.after(continuation);
    }
}

fn branch_label(
    client: &layerfs_sdk::Client,
    branch_id: layerfs_sdk::BranchId,
    authority: bool,
) -> CliResult<Option<String>> {
    let mut query = layerfs_sdk::Query::new(if authority {
        layerfs_sdk::QueryKind::AuthorityBranches
    } else {
        layerfs_sdk::QueryKind::Branches
    });
    loop {
        let page = client.query(query.clone())?;
        for item in page.items {
            let branch = match item {
                layerfs_sdk::QueryItem::Branch(branch)
                | layerfs_sdk::QueryItem::BranchScope(branch, _)
                    if branch.id == branch_id =>
                {
                    branch
                }
                _ => continue,
            };
            let stack = stack_name(client, branch.layer_stack_id, authority)?;
            return Ok(Some(format!("{stack}/{} ({branch_id})", branch.name)));
        }
        let Some(continuation) = page.continuation else {
            return Ok(None);
        };
        query = query.after(continuation);
    }
}

fn stack_name(
    client: &layerfs_sdk::Client,
    stack_id: layerfs_sdk::LayerStackId,
    authority: bool,
) -> CliResult<String> {
    let mut query = layerfs_sdk::Query::new(if authority {
        layerfs_sdk::QueryKind::AuthorityLayerStacks
    } else {
        layerfs_sdk::QueryKind::LayerStacks
    });
    loop {
        let page = client.query(query.clone())?;
        for item in page.items {
            match item {
                layerfs_sdk::QueryItem::LayerStack(value) if value.id == stack_id => {
                    return Ok(value.name.to_string())
                }
                layerfs_sdk::QueryItem::LayerStackScope(value, _) if value.id == stack_id => {
                    return Ok(value.name.to_string())
                }
                _ => {}
            }
        }
        let Some(continuation) = page.continuation else {
            return Ok("?".to_owned());
        };
        query = query.after(continuation);
    }
}
