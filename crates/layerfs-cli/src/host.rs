use crate::{
    BranchCommand, BranchDiff, BranchFork, CliError, CliResult, Command, CommandResult,
    ContextCommand, DbCommand, LayerStackCommand, MonitorCommand, Projection, QueryKind, StoreRole,
    WorkspaceCommand,
};
use layerfs_sdk::{
    BranchStore, Client, CreateWorkspaceSession, DiffRequest, EndWorkspaceMode,
    LayerStackInitialization, LayerStackStore, LocalForkSource, Query, QueryKind as SdkQueryKind,
    RemotePlacement, WorkspacePlacement, WorkspaceProjection,
};
use std::path::Path;

pub(crate) fn execute(
    context_path: &Path,
    client: Option<Client>,
    command: Command,
    error_context: String,
    emit: &mut dyn FnMut(crate::CliEvent) -> CliResult<()>,
) -> CliResult<CommandResult> {
    let decorate_error = client.is_some();
    let result = match command {
        Command::Db { command } => database(command),
        Command::Context { command } => context(context_path, command),
        command => execute_client(
            client.ok_or_else(|| CliError::Context("Client unavailable".to_owned()))?,
            command,
            emit,
        ),
    };
    match (result, decorate_error) {
        (Err(source), true) => Err(CliError::Operation {
            context: error_context,
            source: Box::new(source),
        }),
        (result, _) => result,
    }
}

fn database(command: DbCommand) -> CliResult<CommandResult> {
    match command {
        DbCommand::Create {
            role: StoreRole::Layerstack,
            path,
            parent: None,
        } => {
            let store = LayerStackStore::create(path)?;
            Ok(CommandResult::Text(store.store_id().to_string()))
        }
        DbCommand::Create {
            role: StoreRole::Branch,
            path,
            parent: Some(parent),
        } => {
            let authority = LayerStackStore::connect(parent)?;
            let store = BranchStore::create(path, authority.store_id())?;
            Ok(CommandResult::Text(store.store_id().to_string()))
        }
        DbCommand::Connect {
            role: StoreRole::Layerstack,
            location,
            parent: None,
        } => {
            let store = LayerStackStore::connect(location)?;
            Ok(CommandResult::Text(store.store_id().to_string()))
        }
        DbCommand::Connect {
            role: StoreRole::Branch,
            location,
            parent: Some(parent),
        } => {
            let authority = LayerStackStore::connect(parent)?;
            let store = BranchStore::connect(location, authority.store_id())?;
            Ok(CommandResult::Text(store.store_id().to_string()))
        }
        _ => Err(CliError::Context("invalid Store parent".to_owned())),
    }
}

fn context(context_path: &Path, command: ContextCommand) -> CliResult<CommandResult> {
    match command {
        ContextCommand::Use { layerstack, branch } => {
            let authority = LayerStackStore::connect(&layerstack)?;
            BranchStore::connect(&branch, authority.store_id())?;
            crate::context::save(
                context_path,
                &crate::context::SavedContext { layerstack, branch },
            )?;
            Ok(CommandResult::Text("context updated".to_owned()))
        }
        ContextCommand::Show => Ok(CommandResult::Text(format!(
            "{:?}",
            crate::context::load(context_path)?
        ))),
    }
}

fn execute_client(
    client: Client,
    command: Command,
    emit: &mut dyn FnMut(crate::CliEvent) -> CliResult<()>,
) -> CliResult<CommandResult> {
    let text = match command {
        Command::Layerstack { command } => match command {
            LayerStackCommand::Init(request) => client
                .initialize_layerstack(
                    parse(&request.name)?,
                    if request.empty {
                        LayerStackInitialization::Empty
                    } else {
                        LayerStackInitialization::Directory(request.directory.ok_or_else(|| {
                            CliError::Parse("LayerStack source required".to_owned())
                        })?)
                    },
                )
                .map(|result| format!("{result:?}"))?,
            LayerStackCommand::Pull(request) => format!(
                "{:?}",
                client.pull_layer(
                    parse(&request.through)?,
                    if request.reference {
                        RemotePlacement::Reference
                    } else {
                        RemotePlacement::Replica
                    },
                )?
            ),
            LayerStackCommand::Diff { from, to } => {
                emit_diff(
                    client.diff(DiffRequest::Layers {
                        from_layer_id: parse(&from)?,
                        to_layer_id: parse(&to)?,
                    })?,
                    emit,
                )?;
                return Ok(CommandResult::Empty);
            }
            LayerStackCommand::Add { branch_id } => {
                format!("{:?}", client.add_layer(parse(&branch_id)?)?)
            }
        },
        Command::Branch { command } => match command {
            BranchCommand::Pull(request) => format!(
                "{:?}",
                client.pull_branch(
                    parse(&request.branch_id)?,
                    parse(&request.through)?,
                    if request.reference {
                        RemotePlacement::Reference
                    } else {
                        RemotePlacement::Replica
                    },
                )?
            ),
            BranchCommand::Fork(request) => client
                .fork_branch(parse(&request.name)?, fork_source(request)?)?
                .to_string(),
            BranchCommand::Diff(request) => {
                emit_diff(client.diff(branch_diff(request)?)?, emit)?;
                return Ok(CommandResult::Empty);
            }
            BranchCommand::Push { branch_id } => {
                format!("{:?}", client.push_branch(parse(&branch_id)?)?)
            }
        },
        Command::Workspace { command } => match command {
            WorkspaceCommand::Create {
                branch_id,
                root,
                container,
                projection,
            } => client
                .create_workspace_session(CreateWorkspaceSession {
                    branch_id: parse(&branch_id)?,
                    placement: match container {
                        Some(container_id) => WorkspacePlacement::Container {
                            container_id: layerfs_sdk::ContainerId(container_id),
                            root,
                        },
                        None => WorkspacePlacement::Host { root },
                    },
                    projection: projection.map(|projection| match projection {
                        Projection::Fuse => WorkspaceProjection::Fuse,
                        Projection::Materialize => WorkspaceProjection::Materialize,
                    }),
                })?
                .to_string(),
            WorkspaceCommand::Exec { workspace_id, argv } => client
                .exec_workspace_session(parse(&workspace_id)?, layerfs_sdk::NonEmpty::new(argv)?)?
                .id
                .to_string(),
            WorkspaceCommand::Shell { workspace_id } => client
                .shell_workspace_session(parse(&workspace_id)?)?
                .id
                .to_string(),
            WorkspaceCommand::Output {
                execution_id,
                follow,
            } => {
                let reader = client.workspace_output(parse(&execution_id)?)?;
                let mut after = 0;
                loop {
                    let page = reader.read(after, follow)?;
                    after = page.next_sequence;
                    for chunk in page.chunks {
                        emit(crate::CliEvent::Output(chunk.bytes))?;
                    }
                    if !follow || page.exited {
                        break;
                    }
                    emit(crate::CliEvent::Progress {
                        phase: crate::OperationPhase::Execute,
                        value: crate::ProgressValue {
                            current: after,
                            total: None,
                        },
                    })?;
                }
                return Ok(CommandResult::Empty);
            }
            WorkspaceCommand::Stop { execution_id } => {
                client.stop_workspace_execution(parse(&execution_id)?)?;
                "stopped".to_owned()
            }
            WorkspaceCommand::Conflicts {
                workspace_id,
                after,
            } => format!(
                "{:?}",
                client.workspace_conflicts(
                    parse(&workspace_id)?,
                    after.as_deref().map(parse).transpose()?,
                )?
            ),
            WorkspaceCommand::Resolve(request) => format!(
                "{:?}",
                client.resolve_workspace_conflict(
                    parse(&request.workspace_id)?,
                    parse(&request.conflict_id)?,
                    if request.branch {
                        layerfs_sdk::ResolveChoice::Branch
                    } else if request.layer {
                        layerfs_sdk::ResolveChoice::Layer
                    } else {
                        layerfs_sdk::ResolveChoice::WorkingTree
                    },
                )?
            ),
            WorkspaceCommand::Commit { workspace_id } => format!(
                "{:?}",
                client.commit_workspace_session(parse(&workspace_id)?)?
            ),
            WorkspaceCommand::End {
                workspace_id,
                discard,
            } => {
                client.end_workspace_session(
                    parse(&workspace_id)?,
                    if discard {
                        EndWorkspaceMode::Discard
                    } else {
                        EndWorkspaceMode::Clean
                    },
                )?;
                "ended".to_owned()
            }
        },
        Command::Monitor { command } => {
            return Ok(match command {
                MonitorCommand::Snapshot => CommandResult::Monitor(client.monitor_snapshot()?),
                MonitorCommand::AnalyzeDedup => CommandResult::Dedup(client.analyze_dedup()?),
            });
        }
        Command::Query { kind } => {
            let mut request = query(kind);
            loop {
                let page = client.query(request.clone())?;
                let next = page
                    .continuation
                    .clone()
                    .map(|continuation| request.clone().after(continuation));
                emit(crate::CliEvent::Snapshot(page))?;
                let Some(next) = next else {
                    break;
                };
                request = next;
            }
            return Ok(CommandResult::Empty);
        }
        Command::Db { .. } | Command::Context { .. } => unreachable!(),
    };
    Ok(CommandResult::Text(text))
}

fn emit_diff(
    handle: layerfs_sdk::OperationHandle,
    emit: &mut dyn FnMut(crate::CliEvent) -> CliResult<()>,
) -> CliResult<()> {
    while let Some(page) = handle.next_diff_page()? {
        emit(crate::CliEvent::Diff(page))?;
    }
    Ok(())
}

fn fork_source(request: BranchFork) -> CliResult<LocalForkSource> {
    if let Some(layer_id) = request.layer {
        return Ok(LocalForkSource::Layer {
            layer_id: parse(&layer_id)?,
        });
    }
    Ok(LocalForkSource::Branch {
        branch_id: parse(
            &request
                .branch
                .ok_or_else(|| CliError::Parse("Branch source required".to_owned()))?,
        )?,
        commit_id: parse(
            &request
                .commit
                .ok_or_else(|| CliError::Parse("Commit source required".to_owned()))?,
        )?,
    })
}

fn branch_diff(request: BranchDiff) -> CliResult<DiffRequest> {
    let branch_id = parse(&request.branch)?;
    if let Some(layer_id) = request.layer {
        Ok(DiffRequest::BranchLayer {
            branch_id,
            layer_id: parse(&layer_id)?,
        })
    } else {
        Ok(DiffRequest::BranchCommits {
            branch_id,
            from_commit_id: parse(
                &request
                    .from
                    .ok_or_else(|| CliError::Parse("from Commit required".to_owned()))?,
            )?,
            to_commit_id: parse(
                &request
                    .to
                    .ok_or_else(|| CliError::Parse("to Commit required".to_owned()))?,
            )?,
        })
    }
}

fn query(kind: QueryKind) -> Query {
    Query::new(match kind {
        QueryKind::Layerstacks => SdkQueryKind::LayerStacks,
        QueryKind::AuthorityLayerstacks => SdkQueryKind::AuthorityLayerStacks,
        QueryKind::Layers => SdkQueryKind::Layers,
        QueryKind::AuthorityLayers => SdkQueryKind::AuthorityLayers,
        QueryKind::AuthorityBranches => SdkQueryKind::AuthorityBranches,
        QueryKind::AuthorityCommits => SdkQueryKind::AuthorityCommits,
        QueryKind::Branches => SdkQueryKind::Branches,
        QueryKind::Commits => SdkQueryKind::Commits,
        QueryKind::Workspaces => SdkQueryKind::Workspaces,
        QueryKind::Monitor => SdkQueryKind::Monitor,
    })
}

fn parse<T>(value: &str) -> CliResult<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|error| CliError::Parse(error.to_string()))
}
